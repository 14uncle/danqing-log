//! @author 十四叔
//! @date 2026/09/05
//!
//! 丹青日志 —— 大文件日志/JSONL 查看分析器 (第四件产品)。
//!
//! 已落地: 开枪前提①(性能) + 前提②(JSONL 列化 demo) + core-viewer T1–T5
//! (步进索引/编码三件套/截断轮转原语/PageUp-Down/正则搜索)。
//! 无窗口基准走 bin/logbench, 测试数据生成走 bin/genlog, mmap 行为实验走 bin/mmap_lab。
//!
//! 键盘总览:
//! - 滚动: 滚轮/方向键/PageUp/PageDown/Space(Shift 反向)/Home/End; 点击选中
//! - 搜索: `/` (原始模式) 或 Ctrl+F (任意模式) 开栏; Enter 应用, 栏空后
//!   Enter/Shift+Enter 下/上一命中 (环绕); Esc 关闭
//! - 表格模式 (JSONL 检出): 字符直捕进过滤框, Enter 应用, Esc 清除, Ctrl+T 切原始

#![windows_subsystem = "windows"]

mod view;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use danqing::widget::{Node, node};
use danqing::{
    AnimationCtx, App, Color, Event, ImeEvent, Key, NamedKey, Size, WindowConfig,
    WindowEventSender, run_app,
};

use danqing_log::encoding::{self, Encoding};
use danqing_log::expand::{self, ExpandMap};
use danqing_log::jsonl::{self, Schema, SubRow};
use danqing_log::logfile::LogFile;
use danqing_log::search::{AsyncJob, SearchNav, bytes_as_literal_regex};

/// 空格/PageUp-Down 翻页的行数: POC 定值。正式版由组件回报视口行数。
const PAGE_ROWS: f64 = 25.0;
/// 搜索命中行号收集上限 (防命中过密内存爆; 总数如实报告)。
const SEARCH_HIT_CAP: usize = 1_000_000;

/// 视图模式 (仅 JSONL 检出后可进表格; Ctrl+T 互切)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewMode {
    /// 原始文本 (级别着色)。
    Raw,
    /// JSONL 列化表格 (前提②)。
    Table,
}

/// 过滤作业产物。
pub(crate) struct FilterOutcome {
    lines: Vec<u64>,
    elapsed: Duration,
}

/// 搜索作业产物。
pub(crate) struct SearchOutcome {
    hits: Vec<u64>,
    total: u64,
    elapsed: Duration,
    /// 实际编译的正则模式 (GBK 文件为 \xNN 转义串, 供渲染侧编译高亮)。
    pattern: String,
    /// 用户输入原文 (展示)。
    query: String,
}

/// 应用状态本体 (danqing App)。
pub(crate) struct LogApp {
    file: Arc<LogFile>,
    /// 首可见显示行 (行锚定, 小数 = 行内偏移, 任意文件大小无损; 见 view.rs 注释)。
    top_row: f64,
    /// 点击选中显示行 (过滤模式下经 filtered 映射到文件行号)。
    selected: u64,
    /// 打开统计 (不变部分)。
    base_status: String,
    /// 底栏合成文本 (base + 模式 + 过滤 + 搜索)。
    status: String,
    mode: ViewMode,
    /// JSONL 列定义 (检出才有; Ctrl+T 切换的前置条件)。
    schema: Option<Arc<Schema>>,
    /// 过滤命中的文件行号 (升序); None = 全量。
    filtered: Option<Arc<Vec<u64>>>,
    /// 正在编辑的过滤查询 (表格模式下字符键直捕)。
    filter_input: String,
    /// 已应用的过滤查询。
    filter_applied: String,
    filter_elapsed: Option<Duration>,
    filter_job: AsyncJob<FilterOutcome>,
    /// 搜索栏开闭 (开着时键盘优先归搜索)。
    search_open: bool,
    search_input: String,
    /// 已应用搜索的导航态 (命中表 + 当前位置)。
    search: Option<SearchNav>,
    search_query: String,
    /// 已应用的正则模式 (view 编译高亮用; 与 search 同生同灭)。
    search_pattern: Option<String>,
    search_elapsed: Option<Duration>,
    search_job: AsyncJob<SearchOutcome>,
    /// 书签: 文件行号集合 (会话内有效, 持久化归 v1.x 会话功能)。
    bookmarks: std::collections::BTreeSet<u64>,
    /// 展开态 (jsonl-table T4): 文件行号 → 子行数。
    expanded: ExpandMap,
    /// 展开行的拍平子行 (渲染用; 与 expanded 同生同灭, 惰性 parse)。
    sub_rows: std::collections::BTreeMap<u64, Vec<SubRow>>,
    /// 窗口事件发送器 (粘贴走 read_clipboard 回送 IME Commit)。
    sender: Option<WindowEventSender>,
}

/// 应用消息。
pub(crate) enum Msg {
    /// 滚轮/键盘滚动 N 显示行 (负 = 向上)。
    ScrollRows(f64),
    /// 点击选中显示行。
    Select(u64),
    /// 展开/折叠某显示行的嵌套 (表格模式)。
    ToggleExpand(u64),
    /// 跳到文件头/尾。
    GotoStart,
    GotoEnd,
}

impl LogApp {
    /// 显示行来源 (全量恒等或过滤命中)。
    fn lines(&self) -> expand::Lines<'_> {
        match &self.filtered {
            Some(hits) => expand::Lines::Filtered(hits),
            None => expand::Lines::All {
                total: self.file.line_count(),
            },
        }
    }

    /// 显示行数: 文件行数 + 展开子行数。
    fn display_count(&self) -> u64 {
        expand::display_count(self.lines(), &self.expanded)
    }

    /// 最大首行: 保守取 count-1 (尾部可滚出少量空白, POC 不追求贴底钳制)。
    fn max_top(&self) -> f64 {
        self.display_count().saturating_sub(1) as f64
    }

    /// 显示行 → (文件行, 子行偏移)。偏移 0 = 文件行本身。
    fn line_at(&self, row: u64) -> (u64, usize) {
        expand::file_line_at(row, self.lines(), &self.expanded).unwrap_or((0, 0))
    }

    /// 显示行 → 文件行号 (书签/跳转用, 子行归父行)。
    fn file_line_of(&self, row: u64) -> u64 {
        self.line_at(row).0
    }

    /// 文件行号 → 显示行 (过滤 + 展开叠加)。越界 (文件行被滤掉) 钳到末行。
    fn display_row_of(&self, file_line: u64) -> u64 {
        let row = expand::display_row_of(file_line, self.lines(), &self.expanded);
        row.min(self.display_count().saturating_sub(1))
    }

    /// 展开/折叠某文件行的嵌套 (惰性 parse, 只 parse 展开的那一行)。
    fn toggle_expand(&mut self, file_line: u64) {
        if self.expanded.is_expanded(file_line) {
            self.expanded.collapse(file_line);
            self.sub_rows.remove(&file_line);
            return;
        }
        let raw = self.file.line(file_line);
        let Some(parsed) = jsonl::parse_line(raw) else {
            return;
        };
        let rows = jsonl::flatten(&parsed);
        if rows.is_empty() {
            return; // 无嵌套可展开
        }
        self.expanded.expand(file_line, rows.len());
        self.sub_rows.insert(file_line, rows);
    }

    /// 合成底栏状态: base + 模式 + 过滤 + 搜索。
    fn refresh_status(&mut self) {
        let mut s = self.base_status.clone();
        if self.mode == ViewMode::Table {
            s.push_str(" · JSONL 表格");
        }
        if !self.filter_applied.is_empty() {
            let elapsed = self
                .filter_elapsed
                .map(|d| format!(" ({} ms)", d.as_millis()))
                .unwrap_or_default();
            s.push_str(&format!(
                " · 过滤 \"{}\" → {}/{}{}",
                self.filter_applied,
                self.display_count(),
                self.file.line_count(),
                elapsed
            ));
        }
        if !self.search_query.is_empty() {
            let nav = self
                .search
                .as_ref()
                .and_then(|n| n.position())
                .map(|(k, n)| format!("第 {k}/{n} 命中"))
                .unwrap_or_else(|| "无命中".into());
            let total = self.search.as_ref().map(|n| n.total()).unwrap_or(0);
            let total_tag = if total > SEARCH_HIT_CAP as u64 {
                format!(" (总命中 {total})")
            } else {
                String::new()
            };
            let elapsed = self
                .search_elapsed
                .map(|d| format!(" ({} ms)", d.as_millis()))
                .unwrap_or_default();
            s.push_str(&format!(
                " · 搜索 \"{}\" → {nav}{total_tag}{elapsed}",
                self.search_query
            ));
        }
        if !self.bookmarks.is_empty() {
            s.push_str(&format!(" · 书签 {}", self.bookmarks.len()));
        }
        self.status = s;
    }

    /// Enter (过滤): 应用。空查询 = 回全量; 非空走 AsyncJob (1GB 亚秒, 不冻界面)。
    fn apply_filter(&mut self) {
        let query = self.filter_input.trim().to_string();
        self.filter_input.clear();
        self.filter_applied = query.clone();
        if query.is_empty() {
            self.filtered = None;
            self.filter_elapsed = None;
            self.top_row = 0.0;
            self.selected = 0;
            self.refresh_status();
            return;
        }
        let clauses = jsonl::parse_query(&query);
        let file = Arc::clone(&self.file);
        self.status = format!("{} · 过滤 \"{query}\" 中…", self.base_status);
        self.filter_job.launch(move || {
            let t = std::time::Instant::now();
            let lines = jsonl::run_filter(&file, &clauses);
            FilterOutcome {
                lines,
                elapsed: t.elapsed(),
            }
        });
    }

    /// Esc (过滤): 清输入 + 清已应用过滤, 回到全量。
    fn clear_filter(&mut self) {
        if self.filter_input.is_empty() && self.filter_applied.is_empty() {
            return;
        }
        self.filter_input.clear();
        self.filter_applied.clear();
        self.filter_elapsed = None;
        self.filtered = None;
        self.top_row = 0.0;
        self.selected = 0;
        self.refresh_status();
    }

    /// 打开搜索栏 (`/` 原始模式 / Ctrl+F 任意模式)。
    fn open_search(&mut self) {
        self.search_open = true;
        self.search_input.clear();
        self.refresh_status();
    }

    /// Esc (搜索): 关栏并清搜索态 (命中高亮同步消失)。
    fn close_search(&mut self) {
        self.search_open = false;
        self.search_input.clear();
        self.search = None;
        self.search_query.clear();
        self.search_pattern = None;
        self.search_elapsed = None;
        self.refresh_status();
    }

    /// Enter (搜索): 栏内有输入 = 应用新搜索; 栏空 = 下一命中。
    fn apply_search(&mut self) {
        let q = self.search_input.trim().to_string();
        if q.is_empty() {
            return;
        }
        let pattern = build_search_pattern(self.file.encoding(), &q);
        let Ok(re) = regex::bytes::Regex::new(&pattern) else {
            self.status = format!("{} · 搜索 \"{q}\" 正则无效", self.base_status);
            return;
        };
        self.search_input.clear();
        self.search_query = q.clone();
        let file = Arc::clone(&self.file);
        self.status = format!("{} · 搜索 \"{q}\" 中…", self.base_status);
        self.search_job.launch(move || {
            let t = std::time::Instant::now();
            let (hits, total, _) = file.search(&re, SEARCH_HIT_CAP);
            SearchOutcome {
                hits,
                total,
                elapsed: t.elapsed(),
                pattern,
                query: q,
            }
        });
    }

    /// 跳到命中行 (置视口中部, 选中跟随)。
    fn jump_to_file_line(&mut self, file_line: u64) {
        let row = self.display_row_of(file_line);
        self.top_row = clamp_top(row as f64 - PAGE_ROWS / 2.0, self.display_count());
        self.selected = row;
    }

    fn next_hit(&mut self) {
        if let Some(line) = self.search.as_mut().and_then(SearchNav::jump_next) {
            self.jump_to_file_line(line);
            self.refresh_status();
        }
    }

    fn prev_hit(&mut self) {
        if let Some(line) = self.search.as_mut().and_then(SearchNav::jump_prev) {
            self.jump_to_file_line(line);
            self.refresh_status();
        }
    }

    /// `b` / Ctrl+B: 切换选中行书签 (按文件行号, 过滤模式下语义不漂移)。
    fn toggle_bookmark(&mut self) {
        let line = self.file_line_of(self.selected);
        if !self.bookmarks.insert(line) {
            self.bookmarks.remove(&line);
        }
        self.refresh_status();
    }

    /// `'` / Ctrl+G: 跳下一书签 (严格大于当前行, 环绕)。
    fn goto_next_bookmark(&mut self) {
        if let Some(line) = next_bookmark(&self.bookmarks, self.file_line_of(self.selected)) {
            self.jump_to_file_line(line);
            self.refresh_status();
        }
    }

    /// Ctrl+V: 请求引擎读剪贴板 (回送为 IME Commit 进搜索/过滤框)。
    /// App 层无剪贴板直连, 走 WindowEventSender::read_clipboard (打磨寄生新增)。
    fn paste(&mut self) {
        if let Some(sender) = &self.sender {
            sender.read_clipboard();
        }
    }
}

/// 下一书签: 严格大于 current 的最小书签, 无则环绕到最小书签。空集 None。
pub(crate) fn next_bookmark(set: &std::collections::BTreeSet<u64>, current: u64) -> Option<u64> {
    set.range(current.saturating_add(1)..)
        .next()
        .or_else(|| set.first())
        .copied()
}

/// 搜索模式构造: UTF-8 **存储**编码直接用原查询 (完整正则语法); 非 UTF-8 存储
/// (GBK) 查询转字节 → `\xNN` 字面量 (正则语法退化为字面量, 有意边界)。
///
/// 必须用存储编码而非检出编码: UTF-16 文件打开即转 UTF-8 副本, 存储编码是 Utf8,
/// 走完整正则路径; 误用 stats().encoding (检出 Utf16*) 会把它踢进字面量分支,
/// `ERROR|FATAL` 之类交替正则静默失效 (review 当场抓住的回归)。
fn build_search_pattern(enc: Encoding, query: &str) -> String {
    if enc == Encoding::Utf8 {
        query.to_string()
    } else {
        bytes_as_literal_regex(&encoding::encode_query(enc, query))
    }
}

/// top_row 钳制到 [0, max_top]。独立成函数供单测。
fn clamp_top(top: f64, line_count: u64) -> f64 {
    let max = line_count.saturating_sub(1) as f64;
    top.clamp(0.0, max)
}

impl App for LogApp {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::ScrollRows(d) => {
                self.top_row = clamp_top(self.top_row + d, self.display_count());
                // 方向键滚动时选中跟随首行, 底栏读数即当前位置
                self.selected = self.top_row as u64;
            }
            Msg::Select(row) => {
                if row < self.display_count() {
                    self.selected = row;
                }
            }
            Msg::ToggleExpand(row) => {
                let file_line = self.file_line_of(row);
                self.toggle_expand(file_line);
            }
            Msg::GotoStart => {
                self.top_row = 0.0;
                self.selected = 0;
            }
            Msg::GotoEnd => {
                self.top_row = self.max_top();
                self.selected = self.display_count().saturating_sub(1);
            }
        }
    }

    fn view(&self) -> Node {
        node(view::LogView::new())
    }

    fn event(&mut self, event: &Event) {
        // IME 提交 (中文输入; 粘贴经引擎 read_clipboard 回送也走这里) 优先归栏:
        // winit 下 IME 合成文本只经 Ime(Commit) 送达, Key::Character 拿不到汉字。
        if let Event::Ime(ImeEvent::Commit { value }) = event {
            if self.search_open {
                self.search_input.push_str(value);
            } else if self.mode == ViewMode::Table {
                self.filter_input.push_str(value);
            }
            return;
        }
        let Event::Key {
            key,
            pressed: true,
            shift,
            ctrl,
            ..
        } = event
        else {
            return;
        };
        // 搜索栏开着: 键盘优先归搜索 (方向键等未拦的键放行去滚动)
        if self.search_open {
            match key {
                Key::Character(s) if !ctrl => {
                    self.search_input.push_str(s);
                    return;
                }
                Key::Named(NamedKey::Space) => {
                    self.search_input.push(' ');
                    return;
                }
                Key::Named(NamedKey::Backspace) => {
                    self.search_input.pop();
                    return;
                }
                Key::Named(NamedKey::Enter) => {
                    if self.search_input.trim().is_empty() {
                        // 栏空: Enter/Shift+Enter = 下/上一命中
                        if *shift {
                            self.prev_hit();
                        } else {
                            self.next_hit();
                        }
                    } else {
                        self.apply_search();
                    }
                    return;
                }
                Key::Named(NamedKey::Escape) => {
                    self.close_search();
                    return;
                }
                _ => {}
            }
        }
        if *ctrl {
            if let Key::Character(s) = key {
                if s.eq_ignore_ascii_case("f") {
                    if !self.search_open {
                        self.open_search();
                    }
                    return;
                }
                // 书签键双模式通用 (表格模式字符键归过滤框, 故走 Ctrl)
                if s.eq_ignore_ascii_case("b") {
                    self.toggle_bookmark();
                    return;
                }
                if s.eq_ignore_ascii_case("g") {
                    self.goto_next_bookmark();
                    return;
                }
                if s.eq_ignore_ascii_case("v") {
                    self.paste();
                    return;
                }
                if s.eq_ignore_ascii_case("t") && self.schema.is_some() {
                    self.mode = match self.mode {
                        ViewMode::Raw => ViewMode::Table,
                        ViewMode::Table => ViewMode::Raw,
                    };
                    self.refresh_status();
                }
            }
            return;
        }
        // 原始模式: `/` 开搜索栏; `b`/`'` 书签 (表格模式字符键归过滤框)
        if self.mode == ViewMode::Raw {
            if let Key::Character(s) = key {
                match s.as_str() {
                    "/" => {
                        self.open_search();
                        return;
                    }
                    "b" => {
                        self.toggle_bookmark();
                        return;
                    }
                    "'" => {
                        self.goto_next_bookmark();
                        return;
                    }
                    _ => {}
                }
            }
        }
        if self.mode == ViewMode::Table {
            match key {
                Key::Character(s) => {
                    self.filter_input.push_str(s);
                    return;
                }
                // danqing 空格产 Named(Space) 而非 Character, 过滤查询需要空格分词
                Key::Named(NamedKey::Space) => {
                    self.filter_input.push(' ');
                    return;
                }
                Key::Named(NamedKey::Backspace) => {
                    self.filter_input.pop();
                    return;
                }
                Key::Named(NamedKey::Enter) => {
                    self.apply_filter();
                    return;
                }
                Key::Named(NamedKey::Escape) => {
                    self.clear_filter();
                    return;
                }
                _ => {}
            }
        }
        match key {
            Key::Named(NamedKey::ArrowUp) => self.update(Msg::ScrollRows(-1.0)),
            Key::Named(NamedKey::ArrowDown) => self.update(Msg::ScrollRows(1.0)),
            // 展开/折叠 (表格模式; → 展开 ← 折叠选中行, 子行归父行)
            Key::Named(NamedKey::ArrowRight) if self.mode == ViewMode::Table => {
                let file_line = self.file_line_of(self.selected);
                if !self.expanded.is_expanded(file_line) {
                    self.toggle_expand(file_line);
                }
            }
            Key::Named(NamedKey::ArrowLeft) if self.mode == ViewMode::Table => {
                let file_line = self.file_line_of(self.selected);
                if self.expanded.is_expanded(file_line) {
                    self.toggle_expand(file_line);
                }
            }
            // PageUp/PageDown = danqing 打磨寄生新增 (T4, 联动改动两仓分别待提交)
            Key::Named(NamedKey::PageUp) => self.update(Msg::ScrollRows(-PAGE_ROWS)),
            Key::Named(NamedKey::PageDown) => self.update(Msg::ScrollRows(PAGE_ROWS)),
            Key::Named(NamedKey::Space) => {
                let d = if *shift { -PAGE_ROWS } else { PAGE_ROWS };
                self.update(Msg::ScrollRows(d));
            }
            Key::Named(NamedKey::Home) => self.update(Msg::GotoStart),
            Key::Named(NamedKey::End) => self.update(Msg::GotoEnd),
            _ => {}
        }
    }

    /// 无焦点应用声明 IME 需求: 表格模式过滤框或搜索栏开启时请求中文输入法
    /// (框架 `update_ime` 无焦点时默认关 IME, 经此钩子放行)。
    fn wants_ime(&self) -> bool {
        self.mode == ViewMode::Table || self.search_open
    }

    /// 心跳拾取异步作业结果 (OnDemand 可见态 ~60fps tick, 完成至显示 ≤16ms)。
    fn tick(&mut self, _ctx: &AnimationCtx) {
        if let Some(out) = self.filter_job.poll() {
            self.filtered = Some(Arc::new(out.lines));
            self.filter_elapsed = Some(out.elapsed);
            self.top_row = 0.0;
            self.selected = 0;
            self.refresh_status();
        }
        if let Some(out) = self.search_job.poll() {
            let mut nav = SearchNav::new(Arc::new(out.hits), out.total);
            self.search_pattern = Some(out.pattern);
            self.search_query = out.query;
            self.search_elapsed = Some(out.elapsed);
            // 首跳: 当前选中行之后的第一条命中 (无则环绕回首条)
            let from = self.file_line_of(self.selected);
            let first = nav.jump_first_from(from);
            self.search = Some(nav);
            if let Some(hit) = first {
                self.jump_to_file_line(hit);
            }
            self.refresh_status();
        }
    }

    fn attach_window_sender(&mut self, sender: WindowEventSender) {
        self.sender = Some(sender);
    }
}

/// 底栏打开统计一行流 (截图弹药)。
fn status_text(path: &Path, file: &LogFile) -> String {
    let s = file.stats();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!(
        "{name} · {} · {:.1} MiB · {} 行 · mmap {} µs · 索引 {} ms ({:.0} MiB/s)",
        s.encoding.label(),
        s.file_bytes as f64 / (1024.0 * 1024.0),
        s.line_count,
        s.map_us,
        s.index.as_millis(),
        s.index_mib_per_s(),
    )
}

fn main() {
    danqing::log::init_log();
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("用法: danqing-log <日志文件路径>");
        std::process::exit(2);
    };
    if let Err(e) = run(&path) {
        log::error!("启动失败: {e:#}");
        eprintln!("启动失败: {e:#}");
        std::process::exit(1);
    }
}

fn run(path: &Path) -> Result<()> {
    let file = Arc::new(LogFile::open(path)?);
    let base_status = status_text(path, &file);
    log::info!("{base_status}");
    // JSONL 自动检测 → 列发现 (采样毫秒级; 检出即表格模式开局)
    let schema = if jsonl::detect(&file) {
        jsonl::discover_schema(&file).map(Arc::new)
    } else {
        None
    };
    let mode = if schema.is_some() {
        ViewMode::Table
    } else {
        ViewMode::Raw
    };
    if let Some(s) = &schema {
        log::info!(
            "JSONL 检出, 列化 {} 列: {}",
            s.columns.len(),
            s.columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = format!(
        "丹青日志 POC{} — {name}",
        if mode == ViewMode::Table {
            " [JSONL]"
        } else {
            ""
        }
    );
    let mut app = LogApp {
        file,
        top_row: 0.0,
        selected: 0,
        base_status,
        status: String::new(),
        mode,
        schema,
        filtered: None,
        filter_input: String::new(),
        filter_applied: String::new(),
        filter_elapsed: None,
        filter_job: AsyncJob::new(),
        search_open: false,
        search_input: String::new(),
        search: None,
        search_query: String::new(),
        search_pattern: None,
        search_elapsed: None,
        search_job: AsyncJob::new(),
        bookmarks: std::collections::BTreeSet::new(),
        expanded: ExpandMap::new(),
        sub_rows: std::collections::BTreeMap::new(),
        sender: None,
    };
    app.refresh_status();
    let config = WindowConfig {
        title,
        size: Size::new(1100.0, 760.0),
        clear_color: Color::rgb(0.118, 0.118, 0.145),
        logo_name: "log".into(),
        hotkeys: vec![], // 显式置空: 不继承番茄钟默认热键 (danqing WindowConfig 注释)
        ..Default::default()
    };
    run_app(config, &mut app).context("事件循环异常退出")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_top_bounds() {
        assert_eq!(clamp_top(-5.0, 100), 0.0, "负数钳到 0");
        assert_eq!(clamp_top(500.0, 100), 99.0, "越界钳到末行");
        assert_eq!(clamp_top(42.5, 100), 42.5, "区间内不变 (保小数偏移)");
        assert_eq!(clamp_top(3.0, 0), 0.0, "空文件归零");
    }

    #[test]
    fn next_bookmark_strictly_after_and_wraps() {
        let mut set = std::collections::BTreeSet::new();
        assert_eq!(next_bookmark(&set, 0), None, "空集 None");
        set.extend([3, 10, 20]);
        assert_eq!(next_bookmark(&set, 0), Some(3));
        assert_eq!(next_bookmark(&set, 3), Some(10), "严格大于 (不跳自身)");
        assert_eq!(next_bookmark(&set, 20), Some(3), "末尾环绕");
        assert_eq!(next_bookmark(&set, u64::MAX), Some(3), "MAX 不溢出");
        assert_eq!(next_bookmark(&set, 15), Some(20));
    }

    #[test]
    fn build_search_pattern_utf8_keeps_regex_gbk_literalizes() {
        // UTF-8 存储 (含 UTF-16 转码副本) 保留完整正则语法 —— review 抓的回归:
        // 旧代码查 stats().encoding, 把 UTF-16 文件误踢进字面量分支, `ERROR|FATAL`
        // 变成逐字节字面匹配, 命中恒空。
        assert_eq!(
            build_search_pattern(Encoding::Utf8, "ERROR|FATAL"),
            "ERROR|FATAL"
        );
        // GBK 存储: 转字节 → \xNN 字面量 (退化为字面量语义)
        assert_eq!(
            build_search_pattern(Encoding::Gbk, "中文"),
            "(?-u)\\xD6\\xD0\\xCE\\xC4"
        );
    }
}
