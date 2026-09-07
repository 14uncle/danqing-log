//! @author 十四叔
//! @date 2026/09/05
//!
//! 丹青日志 LogLens —— 大文件日志/JSONL 查看分析器 (第四件产品)。
//!
//! 已落地：开枪前提①(性能) + 前提②(JSONL 列化 demo) + core-viewer T1–T5
//! (步进索引/编码三件套/截断轮转原语/PageUp-Down/正则搜索)。
//! 无窗口基准走 bin/logbench, 测试数据生成走 bin/genlog, mmap 行为实验走 bin/mmap_lab。
//!
//! 键盘总览 (v1 后栏走焦点系统，栏聚焦时方向键移光标，无焦点时这些全局键生效):
//! - 文件：路径参数可选，无参启动进空态 (欢迎提示); Ctrl+O 打开/换开文件
//! - 滚动：滚轮/方向键/PageUp/PageDown/Space(Shift 反向)/Home/End; 点击选中
//! - 搜索：`/` (原始模式) 或 Ctrl+F (任意模式) 开栏并聚焦; Enter 应用，栏空后
//!   Enter/Shift+Enter 下/上一命中 (环绕); Esc 关闭
//! - 表格模式 (JSONL 检出): 框架自动聚焦过滤栏 (真 TextInput); Enter 应用，Esc 清除并清焦，Ctrl+T 切原始

#![windows_subsystem = "windows"]

mod app_update;
mod settings;
mod view;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use danqing::theme::{ScenePalette, SceneTheme};
use danqing::widget::{Column, LogoKind, Node, Stack, TitleBar, node};
use danqing::{
    AnimationCtx, App, Color, Event, Key, NamedKey, Size, WindowAction, WindowConfig, run_app,
};

use danqing_log::encoding::{self, Encoding};
use danqing_log::expand::{self, ExpandMap};
use danqing_log::jsonl::{self, Schema, SubRow};
use danqing_log::logfile::{FileStat, LogFile};
use danqing_log::search::{AsyncJob, SearchNav, bytes_as_literal_regex};

/// 空格/PageUp-Down 翻页的行数：POC 定值。正式版由组件回报视口行数。
pub(crate) const PAGE_ROWS: f64 = 25.0;
/// 搜索命中行号收集上限 (防命中过密内存爆; 总数如实报告)。
const SEARCH_HIT_CAP: usize = 1_000_000;
/// 空态底栏提示 (无参启动, 未打开文件时)。
const EMPTY_STATUS: &str = "未打开文件 · 按 Ctrl+O 打开日志文件";

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
    /// 实际编译的正则模式 (GBK 文件为 \xNN 转义串，供渲染侧编译高亮)。
    pattern: String,
    /// 用户输入原文 (展示)。
    query: String,
}

/// 标题栏主题：浅色 (匹配白底日志视图), 深色文字。
/// SceneTheme 提供跨明暗 Theme 实现; 背景透明，标题文字/按钮符号用深色。
fn title_theme() -> SceneTheme {
    SceneTheme::new(ScenePalette {
        base: Color::rgb(0.96, 0.96, 0.96),
        accent: Color::rgb(0.18, 0.35, 0.60),
        text_primary: Color::rgb(0.12, 0.12, 0.12),
        text_secondary: Color::rgb(0.40, 0.40, 0.42),
        surface: Color::rgba(0.0, 0.0, 0.0, 0.04),
        surface_input: Color::rgba(0.0, 0.0, 0.0, 0.06),
        backdrop_light: Color::rgb(0.85, 0.85, 0.88),
        backdrop_dark: Color::rgb(0.70, 0.70, 0.74),
    })
}

/// 应用状态本体 (danqing App)。
pub(crate) struct LogApp {
    file: Arc<LogFile>,
    /// 是否已打开真实文件 (false = 无参启动空态占位: 轮询/键盘导航全门禁,
    /// 仅 Ctrl+O 与设置可用)。
    has_file: bool,
    /// 首可见显示行 (行锚定，小数 = 行内偏移，任意文件大小无损; 见 view.rs 注释)。
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
    /// 已应用的过滤查询。
    filter_applied: String,
    /// 过滤栏清空信号 (Bar::bind_clear_filter 借此原地 clear)。
    filter_clear_rev: u64,
    filter_elapsed: Option<Duration>,
    filter_job: AsyncJob<FilterOutcome>,
    /// 搜索栏清空信号 (Bar::bind_clear_search 借此原地 clear)。
    search_clear_rev: u64,
    /// 一次性焦点请求：开搜索 / 进表格时置 true, `focus_restored` 消费后清除。
    focus_bar: bool,
    /// 已应用搜索的导航态 (命中表 + 当前位置)。
    search: Option<SearchNav>,
    search_query: String,
    /// 已应用的正则模式 (view 编译高亮用; 与 search 同生同灭)。
    search_pattern: Option<String>,
    search_elapsed: Option<Duration>,
    search_job: AsyncJob<SearchOutcome>,
    /// 书签：文件行号集合 (会话内有效，持久化归 v1.x 会话功能)。
    bookmarks: std::collections::BTreeSet<u64>,
    /// 展开态 (jsonl-table T4): 文件行号 → 子行数。
    expanded: ExpandMap,
    /// 展开行的拍平子行 (渲染用; 与 expanded 同生同灭，惰性 parse)。
    sub_rows: std::collections::BTreeMap<u64, Vec<SubRow>>,
    // ---- live-tail (T2) ----
    /// 文件路径 (增长检测轮询用)。
    path: PathBuf,
    /// 跟随模式：新行到达自动滚底 (F 键 toggle)。
    follow: bool,
    /// 上次 stat 轮询时刻 (250ms 节流)。
    last_stat_poll: Instant,
    /// 底栏提示 (截断/轮转等一次性事件)。
    notice: Option<String>,
    /// 窗口是否已最大化 (TitleBar::bind_maximized 读; 框架 Handler 经 maximized_changed 写)。
    maximized: bool,
    /// 设置卡是否打开 (S2)。
    settings_open: bool,
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
    // ---- v1 真 TextInput 栏 (Bar 抛给应用) ----
    /// 应用过滤 (携带当前输入值; 空 = 回到全量)。
    ApplyFilter(String),
    /// 应用搜索 (携带当前输入值; 非空才发)。
    ApplySearch(String),
    /// 空搜索时 Enter=下一命中，Shift+Enter=上一命中。
    SearchNextHit,
    SearchPrevHit,
    /// Esc 清除搜索结果 (栏保持可见)。
    ClearSearch,
    /// Ctrl+F / `/: 聚焦搜索栏。
    FocusSearch,
    /// Esc 清除过滤。
    ClearFilter,
    /// 表格/原始互切 (JSONL 检出才可用; 栏聚焦时经 app_key_filter 前置仍生效)。
    ToggleMode,
    // ---- S2–S4 设置卡 ----
    /// 打开设置卡。
    OpenSettings,
    /// 关闭设置卡。
    CloseSettings,
    /// 打开 URL (反馈链接/发布页)。
    OpenUrl(String),
    /// Ctrl+O / 拖拽文件：打开新文件。
    OpenFile(PathBuf),
    /// 无操作 (事件吞噬用，不触发任何状态变更)。
    Noop,
}

impl LogApp {
    /// 窗口标题：产品名 + 模式指示 (随 Ctrl+T 切换; 文件名在底栏显示)。
    fn make_title(&self) -> String {
        if self.mode == ViewMode::Table {
            "丹青日志 LogLens [JSONL]".to_string()
        } else {
            "丹青日志 LogLens".to_string()
        }
    }

    /// 显示行来源 (全量恒等或过滤命中)。
    fn lines(&self) -> expand::Lines<'_> {
        match &self.filtered {
            Some(hits) => expand::Lines::Filtered(hits),
            None => expand::Lines::All {
                total: self.file.line_count(),
            },
        }
    }

    /// 显示行数：文件行数 + 展开子行数。
    fn display_count(&self) -> u64 {
        expand::display_count(self.lines(), &self.expanded)
    }

    /// 最大首行：保守取 count-1 (尾部可滚出少量空白，POC 不追求贴底钳制)。
    fn max_top(&self) -> f64 {
        self.display_count().saturating_sub(1) as f64
    }

    /// 显示行 → (文件行，子行偏移)。偏移 0 = 文件行本身。
    fn line_at(&self, row: u64) -> (u64, usize) {
        expand::file_line_at(row, self.lines(), &self.expanded).unwrap_or((0, 0))
    }

    /// 显示行 → 文件行号 (书签/跳转用，子行归父行)。
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

    /// F 键：跟随 toggle。开启时跳到当前底部 (从此跟随新行)。
    fn toggle_follow(&mut self) {
        self.follow = !self.follow;
        if self.follow {
            self.top_row = self.max_top();
            self.selected = self.display_count().saturating_sub(1);
        }
        self.refresh_status();
    }

    /// 增长检测 (live-tail): 文件变长 → `append_from` 增量; 缩容/轮转 → 全量重建。
    fn poll_growth(&mut self) {
        if !self.has_file {
            return; // 空态无文件可轮询
        }
        let Ok(cur) = FileStat::of(&self.path) else {
            return; // 文件暂不可读 (轮转间隙), 下轮再试
        };
        let known = self.file.stat_snapshot();
        if cur == known {
            return; // 未变化
        }
        if cur.head != known.head {
            // 首块变 = 轮转/覆写 (内容换了), 即便新文件更大也全量重建
            self.rebuild_file();
        } else if cur.len > known.len {
            // 同文件增长：增量追加
            let old_line_count = self.file.line_count();
            match LogFile::append_from(&self.file, &self.path) {
                Ok(new) => {
                    self.file = Arc::new(new);
                    self.append_filter_hits(old_line_count);
                    if self.follow {
                        self.top_row = self.max_top();
                        self.selected = self.display_count().saturating_sub(1);
                    }
                    self.refresh_status();
                }
                Err(e) => log::warn!("tail 追加失败：{e:#}"),
            }
        } else {
            // 同文件缩容 (截断): 全量重建
            self.rebuild_file();
        }
    }

    /// 缩容/轮转: 全量重建 + 清失效状态 (书签越界丢弃) + 状态提示。
    fn rebuild_file(&mut self) {
        match LogFile::open(&self.path) {
            Ok(new) => {
                let new_count = new.line_count();
                self.file = Arc::new(new);
                self.bookmarks.retain(|&l| l < new_count);
                self.filtered = None;
                self.filter_applied.clear();
                self.filter_elapsed = None;
                self.search = None;
                self.search_query.clear();
                self.search_pattern = None;
                self.search_elapsed = None;
                self.expanded = ExpandMap::new();
                self.sub_rows.clear();
                self.top_row = 0.0;
                self.selected = 0;
                self.notice = Some("文件已截断/轮转".into());
                self.refresh_status();
            }
            Err(e) => log::warn!("轮转重建失败：{e:#}"),
        }
    }

    /// 热替换文件 (Ctrl+O / 拖拽): 全部状态重建，窗口不重建。
    fn reload_file(&mut self, new_path: PathBuf) {
        let Ok(new_file) = LogFile::open(&new_path) else {
            self.notice = Some(format!(
                "无法打开：{}",
                new_path
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            ));
            self.refresh_status();
            return;
        };
        let base_status = status_text(&new_path, &new_file);
        let new_file = Arc::new(new_file);
        let schema = if jsonl::detect(&new_file) {
            jsonl::discover_schema(&new_file).map(Arc::new)
        } else {
            None
        };
        let mode = if schema.is_some() {
            ViewMode::Table
        } else {
            ViewMode::Raw
        };
        self.file = new_file;
        self.has_file = true;
        self.path = new_path;
        self.base_status = base_status;
        self.mode = mode;
        self.schema = schema;
        self.top_row = 0.0;
        self.selected = 0;
        self.filtered = None;
        self.filter_applied.clear();
        self.filter_clear_rev += 1;
        self.filter_elapsed = None;
        self.search_clear_rev += 1;
        self.search = None;
        self.search_query.clear();
        self.search_pattern = None;
        self.search_elapsed = None;
        self.bookmarks.clear();
        self.expanded = ExpandMap::new();
        self.sub_rows.clear();
        self.follow = false;
        self.notice = None;
        self.refresh_status();
    }

    /// 实时过滤：增量行追加命中表 (只跑新行，不全量重跑)。
    fn append_filter_hits(&mut self, old_line_count: u64) {
        if self.filter_applied.is_empty() {
            return;
        }
        let Some(existing) = &self.filtered else {
            return;
        };
        let clauses = jsonl::parse_query(&self.filter_applied);
        let new_hits = jsonl::run_filter_from(&self.file, &clauses, old_line_count);
        if new_hits.is_empty() {
            return;
        }
        let mut merged = existing.as_ref().clone();
        merged.extend(new_hits);
        self.filtered = Some(Arc::new(merged));
    }

    /// 合成底栏状态：base + 模式 + 过滤 + 搜索。
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
        if self.follow {
            s.push_str(" · FOLLOW");
        }
        if let Some(n) = &self.notice {
            s.push_str(&format!(" · {n}"));
        }
        self.status = s;
    }

    /// Enter (过滤): 应用。空查询 = 回全量; 非空走 AsyncJob (1GB 亚秒，不冻界面)。
    fn apply_filter(&mut self, query: String) {
        self.filter_applied = query.clone();
        self.filter_clear_rev += 1; // 应用后清空输入框 (显示"已应用"占位)
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
            let t = Instant::now();
            let lines = jsonl::run_filter(&file, &clauses);
            FilterOutcome {
                lines,
                elapsed: t.elapsed(),
            }
        });
    }

    /// Esc (过滤): 清已应用过滤，回到全量。
    fn clear_filter(&mut self) {
        self.filter_clear_rev += 1;
        self.filter_applied.clear();
        self.filter_elapsed = None;
        self.filtered = None;
        self.top_row = 0.0;
        self.selected = 0;
        self.refresh_status();
    }

    /// 表格/原始互切 (JSONL 检出才可用): 进表格自动聚焦过滤栏，回原始清 focus_bar。
    fn toggle_mode(&mut self) {
        if self.schema.is_none() {
            return;
        }
        if self.mode == ViewMode::Table {
            self.mode = ViewMode::Raw;
        } else {
            self.mode = ViewMode::Table;
            self.focus_bar = true;
        }
        self.refresh_status();
    }

    /// 聚焦搜索栏 (`/` 原始模式 / Ctrl+F 任意模式)。
    fn open_search(&mut self) {
        self.search_clear_rev += 1; // 聚焦即干净开始
        self.focus_bar = true; // 自动聚焦搜索栏
        self.refresh_status();
    }

    /// Esc (搜索): 清搜索态，栏保持可见 (搜索栏始终显示，不可隐藏)。
    fn clear_search(&mut self) {
        self.search_clear_rev += 1;
        self.search = None;
        self.search_query.clear();
        self.search_pattern = None;
        self.search_elapsed = None;
        self.refresh_status();
    }

    /// Enter (搜索): 栏内有输入 = 应用新搜索; 栏空 = 下一命中。
    fn apply_search(&mut self, q: String) {
        if q.is_empty() {
            return;
        }
        let pattern = build_search_pattern(self.file.encoding(), &q);
        let Ok(re) = regex::bytes::Regex::new(&pattern) else {
            self.status = format!("{} · 搜索 \"{q}\" 正则无效", self.base_status);
            return;
        };
        self.search_clear_rev += 1; // 应用后清空输入框 (显示"已应用"占位)
        self.search_query = q.clone();
        let file = Arc::clone(&self.file);
        self.status = format!("{} · 搜索 \"{q}\" 中…", self.base_status);
        self.search_job.launch(move || {
            let t = Instant::now();
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

    /// 跳到命中行 (置视口中部，选中跟随)。
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

    /// `b` / Ctrl+B: 切换选中行书签 (按文件行号，过滤模式下语义不漂移)。
    fn toggle_bookmark(&mut self) {
        let line = self.file_line_of(self.selected);
        if !self.bookmarks.insert(line) {
            self.bookmarks.remove(&line);
        }
        self.refresh_status();
    }

    /// `'` / Ctrl+G: 跳下一书签 (严格大于当前行，环绕)。
    fn goto_next_bookmark(&mut self) {
        if let Some(line) = next_bookmark(&self.bookmarks, self.file_line_of(self.selected)) {
            self.jump_to_file_line(line);
            self.refresh_status();
        }
    }
}

/// 下一书签：严格大于 current 的最小书签，无则环绕到最小书签。空集 None。
pub(crate) fn next_bookmark(set: &std::collections::BTreeSet<u64>, current: u64) -> Option<u64> {
    set.range(current.saturating_add(1)..)
        .next()
        .or_else(|| set.first())
        .copied()
}

/// 搜索模式构造：UTF-8 **存储**编码直接用原查询 (完整正则语法); 非 UTF-8 存储
/// (GBK) 查询转字节 → `\xNN` 字面量 (正则语法退化为字面量，有意边界)。
///
/// 必须用存储编码而非检出编码：UTF-16 文件打开即转 UTF-8 副本，存储编码是 Utf8,
/// 走完整正则路径; 误用 stats().encoding (检出 Utf16*) 会把它踢进字面量分支，
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
                // 用户向上滚动 → 脱离跟随 (不打扰阅读)
                if d < 0.0 && self.follow {
                    self.follow = false;
                    self.refresh_status();
                }
                self.top_row = clamp_top(self.top_row + d, self.display_count());
                // 方向键滚动时选中跟随首行，底栏读数即当前位置
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
                // End 跳底自动恢复跟随
                if !self.follow {
                    self.follow = true;
                    self.refresh_status();
                }
            }
            // ---- v1 真 TextInput 栏 ----
            Msg::ApplyFilter(q) => self.apply_filter(q),
            Msg::ApplySearch(q) => self.apply_search(q),
            Msg::SearchNextHit => self.next_hit(),
            Msg::SearchPrevHit => self.prev_hit(),
            Msg::ClearSearch => self.clear_search(),
            Msg::FocusSearch => self.open_search(),
            Msg::ClearFilter => self.clear_filter(),
            Msg::ToggleMode => self.toggle_mode(),
            // ---- S2–S4 设置卡 ----
            Msg::OpenSettings => {
                self.settings_open = true;
            }
            Msg::CloseSettings => {
                self.settings_open = false;
            }
            Msg::OpenUrl(url) => {
                if let Err(err) = open::that(&url) {
                    log::warn!("打开链接失败：{err}");
                }
            }
            Msg::OpenFile(path) => {
                self.reload_file(path);
            }
            Msg::Noop => {}
        }
    }

    fn view(&self) -> Node {
        // 顶层：Stack[Column[TitleBar.embed(Bar), LogView.fill], SettingsOverlay]。
        // 设置卡浮层在最上层，关闭时零高不拦截事件。
        node(
            Stack::new()
                .child(
                    Column::new()
                        .child(
                            TitleBar::themed(&title_theme(), self.make_title())
                                .logo_kind(LogoKind::Log)
                                .on_close(|| WindowAction::Close)
                                .on_minimize(|| WindowAction::Minimize)
                                .on_maximize(|| WindowAction::MaximizeOrRestore)
                                .on_drag(|| WindowAction::Drag)
                                .bind_maximized(|app: &LogApp| app.maximized)
                                .embed(
                                    view::Bar::default()
                                        .bind_clear_filter(|app: &LogApp| app.filter_clear_rev)
                                        .bind_clear_search(|app: &LogApp| app.search_clear_rev),
                                ),
                        )
                        .fill(view::LogView::new(), 1),
                )
                .child(settings::settings_overlay()),
        )
    }

    fn event(&mut self, event: &Event) {
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
        // 设置卡打开时 Esc 关闭 (S3)
        if self.settings_open {
            if let Some(msg) = settings::handle_settings_key(key) {
                self.update(msg);
                return;
            }
        }
        // 空态门禁: 仅 Ctrl+O (app_key_filter 前置, 不经此处) 与设置可用, 其余键无文件无意义
        if !self.has_file {
            return;
        }
        // Ctrl 组合全局快捷键 (栏聚焦时键进 TextInput, 不达此处; 无焦点时这些仍工作)。
        if *ctrl {
            if let Key::Character(s) = key {
                // 书签键双模式通用
                if s.eq_ignore_ascii_case("b") {
                    self.toggle_bookmark();
                    return;
                }
                if s.eq_ignore_ascii_case("g") {
                    self.goto_next_bookmark();
                    return;
                }
                if s.eq_ignore_ascii_case("t") && self.schema.is_some() {
                    self.update(Msg::ToggleMode);
                }
            }
            return;
        }
        // 原始模式：`/` 开搜索栏; `b`/`'` 书签; `f` 跟随 (栏聚焦时键进 TextInput, 不达此处)
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
                    "f" => {
                        self.toggle_follow();
                        return;
                    }
                    _ => {}
                }
            }
        }
        match key {
            Key::Named(NamedKey::ArrowUp) => self.update(Msg::ScrollRows(-1.0)),
            Key::Named(NamedKey::ArrowDown) => self.update(Msg::ScrollRows(1.0)),
            // 展开/折叠 (表格模式; → 展开 ← 折叠选中行，子行归父行)
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

    /// 键盘前置过滤 (焦点分发前拦截): 栏聚焦时键进焦点组件，全局 Ctrl 快捷键
    /// 经此仍生效 (如 Ctrl+T 切模式)。仅拦截不破坏输入态的快捷键;
    /// Ctrl+Z/A/Y/C/X/V 等剪辑操作留 TextInput (走框架 clipboard 路由)。
    fn app_key_filter(&mut self, event: &Event) -> Option<Msg> {
        let Event::Key {
            key,
            pressed: true,
            ctrl: true,
            ..
        } = event
        else {
            return None;
        };
        let Key::Character(s) = key else {
            return None;
        };
        if s.eq_ignore_ascii_case("f") {
            return Some(Msg::FocusSearch);
        }
        if s.eq_ignore_ascii_case("t") && self.schema.is_some() {
            return Some(Msg::ToggleMode);
        }
        if s.eq_ignore_ascii_case("o") {
            // Ctrl+O 全局：弹文件选择器，选中返回 OpenFile msg
            if let Some(p) = rfd::FileDialog::new().set_title("选择日志文件").pick_file() {
                return Some(Msg::OpenFile(p));
            }
            return Some(Msg::Noop); // 取消：吞掉事件，不触发副作用
        }
        None
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
            // 首跳：当前选中行之后的第一条命中 (无则环绕回首条)
            let from = self.file_line_of(self.selected);
            let first = nav.jump_first_from(from);
            self.search = Some(nav);
            if let Some(hit) = first {
                self.jump_to_file_line(hit);
            }
            self.refresh_status();
        }
        // 增长检测 (live-tail): 250ms 节流 stat 轮询，增长则增量追加
        if self.last_stat_poll.elapsed() >= Duration::from_millis(250) {
            self.last_stat_poll = Instant::now();
            self.poll_growth();
        }
    }

    /// 窗口最大化状态回调 (框架 Handler 经 set_maximized 触发): 存 `maximized` 供 TitleBar 图标。
    fn maximized_changed(&mut self, is_maximized: bool) {
        self.maximized = is_maximized;
    }

    /// 焦点为空时的一次性恢复请求：开搜索/进表格时把焦点给栏 (via `log-bar`)。
    fn focus_request(&self) -> Option<&'static str> {
        if self.focus_bar {
            Some("log-bar")
        } else {
            None
        }
    }

    /// 消费焦点请求 (逐帧调用，一次性：置位后立即清除，避免 Esc 后误拉回)。
    fn focus_restored(&mut self) {
        self.focus_bar = false;
    }

    fn window_title(&self) -> Option<String> {
        Some(self.make_title())
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
    // 路径参数可选: 无参进空态 (Ctrl+O 打开), 带参直接打开。
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    if let Err(e) = run(path.as_deref()) {
        log::error!("启动失败：{e:#}");
        eprintln!("启动失败：{e:#}");
        std::process::exit(1);
    }
}

fn run(path: Option<&Path>) -> Result<()> {
    // 启动后台更新检查 (24h TTL 缓存，静默)。
    app_update::init();
    let (file, base_status, schema, has_file, path_buf) = match path {
        Some(p) => {
            let file = LogFile::open(p)?;
            let base_status = status_text(p, &file);
            log::info!("{base_status}");
            // JSONL 自动检测 → 列发现 (采样毫秒级; 检出即表格模式开局)
            let schema = if jsonl::detect(&file) {
                jsonl::discover_schema(&file).map(Arc::new)
            } else {
                None
            };
            if let Some(s) = &schema {
                log::info!(
                    "JSONL 检出，列化 {} 列：{}",
                    s.columns.len(),
                    s.columns
                        .iter()
                        .map(|c| c.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            (Arc::new(file), base_status, schema, true, p.to_path_buf())
        }
        None => (
            Arc::new(LogFile::empty()),
            EMPTY_STATUS.to_string(),
            None,
            false,
            PathBuf::new(),
        ),
    };
    let mode = if schema.is_some() {
        ViewMode::Table
    } else {
        ViewMode::Raw
    };
    let title = if mode == ViewMode::Table {
        "丹青日志 LogLens [JSONL]"
    } else {
        "丹青日志 LogLens"
    }
    .to_string();
    let mut app = LogApp {
        file,
        has_file,
        top_row: 0.0,
        selected: 0,
        base_status,
        status: String::new(),
        mode,
        schema,
        filtered: None,
        filter_applied: String::new(),
        filter_clear_rev: 0,
        filter_elapsed: None,
        filter_job: AsyncJob::new(),
        search_clear_rev: 0,
        search: None,
        search_query: String::new(),
        search_pattern: None,
        search_elapsed: None,
        search_job: AsyncJob::new(),
        bookmarks: std::collections::BTreeSet::new(),
        expanded: ExpandMap::new(),
        sub_rows: std::collections::BTreeMap::new(),
        focus_bar: false,
        path: path_buf,
        follow: false,
        last_stat_poll: Instant::now(),
        notice: None,
        maximized: false,
        settings_open: false,
    };
    app.refresh_status();
    let config = WindowConfig {
        title,
        size: Size::new(1100.0, 760.0),
        clear_color: Color::rgb(0.98, 0.98, 0.98),
        logo_name: "log".into(),
        hotkeys: vec![], // 显式置空：不继承番茄钟默认热键 (danqing WindowConfig 注释)
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
        // UTF-8 存储 (含 UTF-16 转码副本) 保留完整正则语法 —— review 抓的回归：
        // 旧代码查 stats().encoding, 把 UTF-16 文件误踢进字面量分支，`ERROR|FATAL`
        // 变成逐字节字面匹配，命中恒空。
        assert_eq!(
            build_search_pattern(Encoding::Utf8, "ERROR|FATAL"),
            "ERROR|FATAL"
        );
        // GBK 存储：转字节 → \xNN 字面量 (退化为字面量语义)
        assert_eq!(
            build_search_pattern(Encoding::Gbk, "中文"),
            "(?-u)\\xD6\\xD0\\xCE\\xC4"
        );
    }
}
