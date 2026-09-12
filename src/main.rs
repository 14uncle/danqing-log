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
mod config;
mod histogram;
mod settings;
mod tray;
mod view;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use danqing::theme::{ScenePalette, SceneTheme};
use danqing::widget::{Column, LogoKind, Node, Row, Stack, TitleBar, node};
use danqing::{
    AnimationCtx, App, Color, Event, Key, NamedKey, Size, WindowAction, WindowConfig, run_app,
};

use danqing::encoding::{self, Encoding, bytes_as_literal_regex};
use danqing_log::expand::{self, ExpandMap};
use danqing_log::jsonl::{self, Schema, SubRow};
use danqing_log::levels::{self, Level, LevelCounts, LevelQueries};
use danqing_log::logfile::{FileStat, INDEX_CANCELLED, LogFile};
use danqing_log::open::{OpenJob, OpenKind, OpenOutcome};
use danqing_log::search::{AsyncJob, SearchNav};

/// 空格/PageUp-Down 翻页的行数：POC 定值。正式版由组件回报视口行数。
pub(crate) const PAGE_ROWS: f64 = 25.0;
/// 搜索命中行号收集上限 (防命中过密内存爆; 总数如实报告)。
const SEARCH_HIT_CAP: usize = 1_000_000;
/// tail 追加同步/异步阈值: 新追加字节几乎必在页缓存 (写入方刚产生的写回页),
/// 串行增量索引 ~2.4GB/s → 32MB ≈ 13ms ≤ 16ms 帧预算 (async-open plan D3)。
const APPEND_SYNC_MAX_BYTES: u64 = 32 << 20;

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
fn title_theme(theme: config::AppTheme) -> SceneTheme {
    match theme {
        config::AppTheme::Light => SceneTheme::new(ScenePalette {
            base: Color::rgb(0.96, 0.96, 0.96),
            accent: Color::rgb(0.18, 0.35, 0.60),
            text_primary: Color::rgb(0.12, 0.12, 0.12),
            text_secondary: Color::rgb(0.40, 0.40, 0.42),
            surface: Color::rgba(0.0, 0.0, 0.0, 0.04),
            surface_input: Color::rgba(0.0, 0.0, 0.0, 0.06),
            backdrop_light: Color::rgb(0.85, 0.85, 0.88),
            backdrop_dark: Color::rgb(0.70, 0.70, 0.74),
        }),
        config::AppTheme::Dark => SceneTheme::new(ScenePalette {
            base: Color::rgb(0.10, 0.10, 0.13),
            accent: Color::from_srgb8(26, 158, 138),
            text_primary: Color::rgb(0.90, 0.90, 0.92),
            text_secondary: Color::rgb(0.56, 0.56, 0.58),
            surface: Color::rgba(1.0, 1.0, 1.0, 0.06),
            surface_input: Color::rgba(1.0, 1.0, 1.0, 0.10),
            backdrop_light: Color::rgb(0.16, 0.16, 0.20),
            backdrop_dark: Color::rgb(0.06, 0.06, 0.08),
        }),
    }
}

/// 应用状态本体 (danqing App)。
pub(crate) struct LogApp {
    /// 窗口事件发送器 (显隐/退出等)。
    window_sender: Option<danqing::WindowEventSender>,
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
    /// 全文件级别计数 (level-histogram 侧栏)。随文件同批换入 —— worker 算好
    /// 与 file 一起交卷, 故不存在「行数已更新、计数还是旧的」窗口。
    level_counts: Arc<LevelCounts>,
    /// 计数所用的级别类列名 (None = 行口径)。追加时据此选同一条口径 ——
    /// 口径混用会让同一个侧栏里出现两种数法。
    level_column: Option<String>,
    /// 侧栏每桶的点选子句 (None = 该行不可点)。明文/无级别类列时全 None。
    /// 子句只依赖列名 (前缀口径无 per-file 观察值), 故追加换入时无需重算。
    level_queries: LevelQueries,
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
    /// 在途打开作业 (async-open): Some = 打开/重建/追平进行中, UI 全程可响应;
    /// 取消 = 置 None (worker 持 cancel Arc 早退, 见 open.rs drop 语义)。
    open_job: Option<OpenJob>,
    /// Loading 占位文案 (仅 无旧文件 + job 在途 时 Some; view 空态分支呈现)。
    loading_label: Option<(String, String)>,
    /// 设置卡是否打开 (S2)。
    settings_open: bool,
    /// 主题模式 (浅色/深色)。
    theme: config::AppTheme,
    /// 级别计数侧栏是否显示 (`Ctrl+L` 切换, 落 config.toml)。
    histogram_visible: bool,
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
    // ---- level-histogram 侧栏 ----
    /// 点侧栏柱条: 套用该桶的过滤子句; 已是当前生效项则清除 (切换语义)。
    ApplyLevelFilter(Level),
    /// `Ctrl+L`: 侧栏显隐 (落 config.toml)。
    ToggleHistogram,
    // ---- S2–S4 设置卡 ----
    /// 打开设置卡。
    OpenSettings,
    /// 关闭设置卡。
    CloseSettings,
    /// 打开 URL (反馈链接/发布页)。
    OpenUrl(String),
    /// Ctrl+O / 拖拽文件：打开新文件。
    OpenFile(PathBuf),
    /// 底栏一次性提示 (选区超限未复制等, 组件层 → 应用层 notice 通道)。
    Notice(String),
    // ---- 主题下拉 ----
    /// 通过下拉选择器选择主题 (索引)。
    /// 展开/收起/键盘导航/点外关闭均由 `Dropdown` 自管, 不再经应用消息。
    SelectTheme(usize),
    /// 退出应用 (托盘菜单)。
    Quit,
    /// 无操作 (事件吞噬用，不触发任何状态变更)。
    Noop,
}

impl LogApp {
    /// 空态骨架 (run() 启动与测试夹具共享, 字段只许有一份真身)。
    fn new_empty() -> Self {
        let cfg = config::Config::load();
        Self {
            window_sender: None,
            file: Arc::new(LogFile::empty()),
            has_file: false,
            top_row: 0.0,
            selected: 0,
            base_status: EMPTY_STATUS.to_string(),
            status: String::new(),
            mode: ViewMode::Raw,
            schema: None,
            level_counts: Arc::new(LevelCounts::default()),
            level_column: None,
            level_queries: levels::no_level_queries(),
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
            path: PathBuf::new(),
            follow: false,
            last_stat_poll: Instant::now(),
            notice: None,
            maximized: false,
            open_job: None,
            loading_label: None,
            settings_open: false,
            theme: cfg.theme,
            histogram_visible: cfg.histogram,
        }
    }

    /// 采纳一份产物带来的计数口径: 列名与据此生成的点选子句表。
    ///
    /// 抽成一处而非在 `apply_fresh` / `apply_rebuild` 各写一遍 —— 两处的写法
    /// 必须永远一致 (口径与子句表不同步 = 点某行筛到另一行), 重复即隐患。
    fn adopt_level_column(&mut self, column: Option<String>) {
        self.level_queries = column
            .as_deref()
            .map_or_else(levels::no_level_queries, levels::level_queries_for);
        self.level_column = column;
    }

    /// 把当前设置写回 `config.toml`。
    ///
    /// 必须走整文件写入 —— [`config::Config`] 的两个键同源, 分头写会让
    /// 「改主题」顺手抹掉侧栏开关 (config.rs 的 `round_trip_preserves_both_keys`
    /// 钉着这条)。
    fn save_config(&self) {
        config::Config {
            theme: self.theme,
            histogram: self.histogram_visible,
        }
        .save();
    }

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
        if self.open_job.is_some() {
            return; // 打开/重建/追平在途: 不叠加 tail 动作 (async-open plan D3)
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
            let delta = cur.len - known.len;
            if delta >= APPEND_SYNC_MAX_BYTES {
                // 巨量追平 (久未轮询后的追平, 如隐藏期间暴涨): 转 worker,
                // 旧快照保持可见可滚 + 底栏「追平中」; 在途期间本函数被门禁 (D3)。
                // 过滤激活时子句随行 (review R2): 增量过滤随 worker 下沉,
                // 落点只合并不扫描 —— 否则 GB 级追平的过滤成本回到 UI 线程。
                let old = Arc::clone(&self.file);
                let filter = if !self.filter_applied.is_empty() && self.filtered.is_some() {
                    Some((jsonl::parse_query(&self.filter_applied), old.line_count()))
                } else {
                    None
                };
                self.open_job = Some(OpenJob::launch_append(
                    &self.path,
                    old,
                    delta,
                    filter,
                    self.level_column.clone(),
                ));
                self.refresh_status();
                return;
            }
            // 同文件常态增长：同步增量追加 (新字节在页缓存, 毫秒级)
            match LogFile::append_from(&self.file, &self.path) {
                Ok(new) => {
                    // 只数新行 —— 这里在 UI 线程上, 全量重算会冻帧。
                    // 口径必须与当前显示的一致 (字段 vs 行), 否则侧栏会混两种数法。
                    let old_count = self.file.line_count();
                    let delta = match &self.level_column {
                        Some(col) => levels::count_levels_field_from(&new, col, old_count),
                        None => levels::count_levels_from(&new, old_count),
                    };
                    self.apply_appended(new, None, delta);
                }
                Err(e) => log::warn!("tail 追加失败：{e:#}"),
            }
        } else {
            // 同文件缩容 (截断): 全量重建
            self.rebuild_file();
        }
    }

    /// 缩容/轮转: 异步全量重建 (旧快照保持可见可滚, spec 裁决);
    /// pickup 走 [`Self::apply_rebuild`] 重置链。
    fn rebuild_file(&mut self) {
        let path = self.path.clone();
        self.open_job = Some(OpenJob::launch(OpenKind::Rebuild, &path));
        self.refresh_status();
    }

    /// Rebuild 换入 (worker 交卷): 清失效状态 (书签越界丢弃) + 状态提示
    /// (原 rebuild_file 重置链; base_status 换新打开统计 —— 同步时代留旧串,
    /// 轮转后底栏数字失真, 异步化顺带修正)。
    fn apply_rebuild(&mut self, path: &Path, out: OpenOutcome) {
        // review C1: 旧内容上的在途 filter/search 结果不得贴到新内容
        self.filter_job.invalidate();
        self.search_job.invalidate();
        let OpenOutcome {
            file,
            schema,
            level_counts,
            level_column,
            ..
        } = out;
        let base_status = status_text(path, &file);
        let new_count = file.line_count();
        self.file = Arc::new(file);
        self.level_counts = Arc::new(level_counts);
        // 格式可能整体换了 → 列名与子句表跟着换 (不沿用旧的)
        self.adopt_level_column(level_column);
        // review R4: 轮转后格式可能变了 (JSONL↔明文), schema/mode 用 worker 新发现
        self.schema = schema.map(Arc::new);
        self.mode = if self.schema.is_some() {
            ViewMode::Table
        } else {
            ViewMode::Raw
        };
        self.base_status = base_status;
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

    /// 热替换文件 (Ctrl+O / 拖拽): 异步管道发起 (在途旧 job 被 drop = 取消);
    /// 旧视图保持至 worker 交卷 (spec 裁决 A), 换入走 [`Self::apply_fresh`]。
    fn reload_file(&mut self, new_path: PathBuf) {
        self.open_job = Some(OpenJob::launch(OpenKind::Fresh, &new_path));
        self.refresh_status();
    }

    /// Fresh 换入 (worker 交卷): 全部状态重建, 窗口不重建 (原 reload_file 重置链)。
    fn apply_fresh(&mut self, new_path: PathBuf, out: OpenOutcome) {
        // review C1: 旧文件上的在途 filter/search 结果不得贴到新文件
        self.filter_job.invalidate();
        self.search_job.invalidate();
        let OpenOutcome {
            file: new_file,
            schema,
            level_counts,
            level_column,
            ..
        } = out;
        let base_status = status_text(&new_path, &new_file);
        log::info!("{base_status}");
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
        let new_file = Arc::new(new_file);
        let schema = schema.map(Arc::new);
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
        self.level_counts = Arc::new(level_counts);
        self.adopt_level_column(level_column);
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

    /// Append 换入 (同步小追加 / 追平 worker 交卷同链): 增量过滤 + follow 滚底。
    /// `worker_hits` = worker 已算好的增量命中 (review R2: 巨量追平过滤下沉);
    /// None = 本地扫 (同步小追加, 毫秒级)。
    fn apply_appended(
        &mut self,
        new: LogFile,
        worker_hits: Option<Vec<u64>>,
        level_delta: LevelCounts,
    ) {
        let old_line_count = self.file.line_count();
        self.file = Arc::new(new);
        // 计数是**增量**: 追加只数了新行, 与旧计数合并 (全量重算在同步热路径上
        // 等于每次追加卡一次全文件扫描)。合并与文件换入同批次, 故侧栏永远
        // 对应此刻这份文件。
        let mut counts = *self.level_counts.as_ref();
        counts.merge(&level_delta);
        self.level_counts = Arc::new(counts);
        match worker_hits {
            Some(hits) => self.merge_filter_hits(hits),
            None => self.append_filter_hits(old_line_count),
        }
        if self.follow {
            self.top_row = self.max_top();
            self.selected = self.display_count().saturating_sub(1);
        }
        self.refresh_status();
    }

    /// 打开管道拾取 (async-open): 完成/失败都收摊 (take → drop 旧 job 语义),
    /// 按 kind 分派换入链; 失败按 kind 分流 (Fresh notice / 余静默待重试)。
    fn pickup_open_job(&mut self) {
        let Some(res) = self.open_job.as_mut().and_then(OpenJob::poll) else {
            return;
        };
        let job = self.open_job.take().expect("在途 job");
        match res {
            Ok(out) => match job.kind() {
                OpenKind::Fresh => self.apply_fresh(job.path().to_path_buf(), out),
                OpenKind::Rebuild => self.apply_rebuild(job.path(), out),
                OpenKind::Append => {
                    if out.rebuilt {
                        // 追加退化全量重建 (UTF-16/缩容, review R3): 走 rebuild 重置链
                        self.apply_rebuild(job.path(), out);
                    } else {
                        let OpenOutcome {
                            file,
                            incremental_hits,
                            level_counts,
                            ..
                        } = out;
                        self.apply_appended(file, incremental_hits, level_counts);
                    }
                }
            },
            Err(e) => {
                // 「索引已取消」= 主动取消, 静默; 失败语义按 kind 分流 (保旧行为):
                // Fresh 失败 notice + 留空态/旧视图; Rebuild/Append 静默,
                // 文件仍过期, 下轮 250ms poll 自然驱动重试。
                if !e.to_string().contains(INDEX_CANCELLED) {
                    match job.kind() {
                        OpenKind::Fresh => {
                            log::warn!("打开失败：{e:#}");
                            self.notice = Some(format!(
                                "无法打开：{}",
                                job.path()
                                    .file_name()
                                    .map(|n| n.to_string_lossy())
                                    .unwrap_or_default()
                            ));
                        }
                        OpenKind::Rebuild => log::warn!("轮转重建失败：{e:#}"),
                        OpenKind::Append => log::warn!("tail 追加失败：{e:#}"),
                    }
                }
                self.refresh_status();
            }
        }
    }

    /// 点侧栏柱条: 套用该桶的过滤子句 (复用既有过滤通路, 零新语法);
    /// 点的已是当前生效项 → 清除 (切换语义)。
    ///
    /// 子句来自 `level_queries` —— 它是**按当前文件的级别类列**生成的,
    /// 不是写死的 `level=X`: 列名可能是 severity/lvl, 值可能是 WARNING。
    /// 无子句的桶 (`其他` / 合并的 DEBUG+TRACE / 明文模式) 在侧栏侧已挡,
    /// 此处再兜一层 —— 消息源不止一处时不会漏。
    fn apply_level_filter(&mut self, level: Level) {
        let Some(q) = self.level_queries[level as usize].clone() else {
            return;
        };
        if self.filter_applied == q {
            self.update(Msg::ApplyFilter(String::new()));
        } else {
            self.update(Msg::ApplyFilter(q));
        }
    }

    /// 实时过滤：增量行追加命中表 (只跑新行，不全量重跑)。
    fn append_filter_hits(&mut self, old_line_count: u64) {
        if self.filter_applied.is_empty() {
            return;
        }
        if self.filtered.is_none() {
            return;
        }
        let clauses = jsonl::parse_query(&self.filter_applied);
        let new_hits = jsonl::run_filter_from(&self.file, &clauses, old_line_count);
        self.merge_filter_hits(new_hits);
    }

    /// 合并增量命中进过滤表 (本地扫描与 worker 下沉共用合并半段, review R2)。
    fn merge_filter_hits(&mut self, new_hits: Vec<u64>) {
        if new_hits.is_empty() {
            return;
        }
        let Some(existing) = &self.filtered else {
            return;
        };
        let mut merged = existing.as_ref().clone();
        merged.extend(new_hits);
        self.filtered = Some(Arc::new(merged));
    }

    /// 合成底栏状态：base + 模式 + 过滤 + 搜索。job 在途时整行被 loading 覆盖。
    /// loading 显示三元 (底栏动词, 文件名, 进度细节): job 在途才有。
    /// refresh_status 的整行覆盖源 (计算与 mutation 分离)。
    fn loading_parts(&self) -> Option<(&'static str, String, String)> {
        let job = self.open_job.as_ref()?;
        let (done, total) = job.progress();
        let name = job
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let verb = match job.kind() {
            OpenKind::Fresh => "正在索引",
            OpenKind::Rebuild => "重建中",
            OpenKind::Append => "追平中",
        };
        // done==0 = 刚发起或 UTF-16 读取/转码段 (无细粒度钩子, 见 plan D2)
        let detail = if done == 0 {
            "读取中…".to_string()
        } else if let Some(pct) = (done * 100).checked_div(total) {
            // 封顶 99: 在途分子可超分母 (索引期间文件增长 / UTF-16 转码口径),
            // 完成时 loading 分支随 job 消失, 永远看不到 100 (review O1)
            format!("{}% · {}/{} MiB", pct.min(99), done >> 20, total >> 20)
        } else {
            "…".to_string()
        };
        Some((verb, name, detail))
    }

    fn refresh_status(&mut self) {
        if let Some((verb, name, detail)) = self.loading_parts() {
            self.status = format!("{verb} {name} · {detail}");
            // 无旧文件才上占位文案 (有旧文件: 列表照画, 进度只上底栏)
            self.loading_label = if self.has_file {
                None
            } else {
                Some((name.clone(), format!("{verb} {detail}")))
            };
            return;
        }
        self.loading_label = None;
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
            // ---- level-histogram 侧栏 ----
            Msg::ApplyLevelFilter(l) => self.apply_level_filter(l),
            Msg::ToggleHistogram => {
                self.histogram_visible = !self.histogram_visible;
                self.save_config();
            }
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
            Msg::Notice(text) => {
                self.notice = Some(text);
                self.refresh_status();
            }
            Msg::SelectTheme(idx) => {
                self.theme = config::AppTheme::from_index(idx);
                self.save_config();
            }
            Msg::Quit => {
                if let Some(sender) = &self.window_sender {
                    sender.quit();
                }
            }
            Msg::Noop => {}
        }
    }

    fn view(&self) -> Node {
        // 顶层：Stack[Column[TitleBar.embed(Bar), Row[Histogram(Fit), LogView.fill]],
        // Overlay(设置卡)]。设置卡浮层在最上层，关闭时零高不拦截事件。
        //
        // 侧栏做成 LogView 的 **sibling** (weight 0 = 取自身 layout 的固定宽),
        // 而非塞进 LogView 内部 —— 后者要改它的 gutter/x 偏移/命中测试/横滚范围
        // 一整套坐标数学, sibling 方案下 LogView 只是拿到一个更窄的 area。
        node(
            Stack::new()
                .child(
                    Column::new()
                        .child(
                            TitleBar::themed(&title_theme(self.theme), self.make_title())
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
                        .fill(
                            Row::new()
                                .fill(histogram::LevelHistogram::new(), 0)
                                .fill(view::LogView::new(), 1),
                            1,
                        ),
                )
                .child(settings::settings_overlay(self.theme)),
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
        // Esc 前置：设置卡 > (后续留给搜索/过滤栏)
        if let Event::Key {
            key: Key::Named(NamedKey::Escape),
            pressed: true,
            ..
        } = event
        {
            if self.settings_open {
                // 本函数在焦点分发前运行 (无论有无焦点)，所以卡内主题下拉展开
                // 时按 Esc 也走这条路径：整卡通关，而非先收下拉。与「设置卡
                // 优先」的次序一致; 组件自身的 Esc 折叠只在该路径之外可达。
                return Some(Msg::CloseSettings);
            }
        }
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
        // Ctrl+L 侧栏显隐: 走前置过滤而非 event(), 故栏聚焦时也生效 (与 Ctrl+T 同级)
        if s.eq_ignore_ascii_case("l") {
            return Some(Msg::ToggleHistogram);
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

    /// LogView 持焦后, 焦点组件未消费的键回退应用层 (danqing opt-in):
    /// 点击日志区后 j/k/翻页/`/`/b 等应用级导航不失灵 (text-selection T4)。
    /// TextInput 栏持焦时其已消费的键不会重复到达 (引擎保证)。
    fn propagate_unhandled_keys(&self) -> bool {
        true
    }

    /// 心跳拾取异步作业结果 (OnDemand 可见态 ~60fps tick, 完成至显示 ≤16ms)。
    fn tick(&mut self, _ctx: &AnimationCtx) {
        self.pickup_open_job();
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
            if self.open_job.is_some() {
                self.refresh_status(); // loading 进度文本随 poll 前进
            }
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

    fn attach_window_sender(&mut self, sender: danqing::WindowEventSender) {
        self.window_sender = Some(sender);
    }

    fn tray_menu(&self) -> danqing::tray_icon::menu::Menu {
        tray::build_menu()
    }

    fn tray_action(&mut self, id: u8) -> Option<Msg> {
        if id == tray::ACTION_SETTINGS {
            Some(Msg::OpenSettings)
        } else if id == tray::ACTION_QUIT {
            Some(Msg::Quit)
        } else {
            None
        }
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
        "{name} · {} · {:.1} MiB · {} 行 · mmap {} us · 索引 {} ms ({:.0} MiB/s)",
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
    // 一律空态骨架开局 (async-open): 带文件启动只发起 OpenJob 便立刻 run_app,
    // 窗口按 GPU 速度出现, 索引在 worker 后台跑 (spec 判据: ≤ 无文件启动 +200ms)。
    let mut app = LogApp::new_empty();
    if let Some(p) = path {
        app.open_job = Some(OpenJob::launch(OpenKind::Fresh, p));
    }
    app.refresh_status();
    let config = WindowConfig {
        title: "丹青日志 LogLens".to_string(),
        size: Size::new(1100.0, 760.0),
        clear_color: Color::rgb(0.98, 0.98, 0.98),
        logo_name: "log".into(),
        maximized: true, // 日志查看器主战场是全屏阅读: 初始最大化
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

    /// 空态 LogApp 测试夹具 (与 run() 的空态骨架同构)。
    fn temp_log(content: &[u8]) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "danqing-log-main-{}-{}.log",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::write(&path, content).unwrap();
        path
    }

    /// 追加换入: 级别计数是**增量合并**, 不是覆盖 —— 写成覆盖会让每次追加
    /// 之后侧栏只剩新行的数字。终值必须等于对新文件全量重算的结果。
    #[test]
    fn apply_appended_merges_level_counts_incrementally() {
        let mut app = LogApp::new_empty();
        // 起点: 已打开的文件含 1 条 ERROR + 1 条 INFO
        let p1 = temp_log(b"2026-09-05 12:00:01 ERROR one\n2026-09-05 12:00:02 INFO two\n");
        let f1 = LogFile::open(&p1).unwrap();
        app.level_counts = Arc::new(levels::count_levels(&f1));
        app.file = Arc::new(f1);
        assert_eq!(app.level_counts.get(Level::Error), 1);

        // 追加后: 第 3 行是新来的 ERROR, 落点只交 [2, 3) 的增量
        let p2 = temp_log(
            b"2026-09-05 12:00:01 ERROR one\n\
              2026-09-05 12:00:02 INFO two\n\
              2026-09-05 12:00:03 ERROR three\n",
        );
        let f2 = LogFile::open(&p2).unwrap();
        let delta = levels::count_levels_from(&f2, 2);
        assert_eq!(delta.total(), 1, "增量只含新行");
        app.apply_appended(f2, None, delta);

        assert_eq!(app.level_counts.get(Level::Error), 2, "旧 1 + 新 1");
        assert_eq!(app.level_counts.get(Level::Info), 1);
        assert_eq!(app.level_counts.total(), 3);
        // 增量结果 == 对新文件全量重算 (这条是增量正确性的定义)
        assert_eq!(*app.level_counts.as_ref(), levels::count_levels(&app.file));
        std::fs::remove_file(&p1).ok();
        std::fs::remove_file(&p2).ok();
    }

    /// 点选联动: 点柱条套用该桶子句; 再点同一行 = 清除; 无子句的桶点不动。
    ///
    /// 「点不动」必须是真的不动 —— 若把过滤改成空串, 用户点在「其他」上会
    /// 意外清掉自己正在看的过滤。
    #[test]
    fn level_filter_toggles_and_ignores_queryless_buckets() {
        let mut app = LogApp::new_empty();
        let p = temp_log(
            b"{\"level\":\"ERROR\",\"msg\":\"a\"}\n\
              {\"level\":\"INFO\",\"msg\":\"b\"}\n",
        );
        let f = LogFile::open(&p).unwrap();
        app.level_counts = Arc::new(levels::count_levels_field(&f, "level"));
        app.level_queries = levels::level_queries_for("level");
        app.file = Arc::new(f);
        app.has_file = true;

        // 首次点击 → 套用前缀通配子句 (不是字节全等的 level=ERROR)
        app.apply_level_filter(Level::Error);
        assert_eq!(app.filter_applied, "level=ERROR*");

        // 再点同一行 → 清除 (切换语义)
        app.apply_level_filter(Level::Error);
        assert_eq!(app.filter_applied, "", "再点生效行 = 清除");

        // 无子句的桶: 不动过滤 (也不误套别的)
        app.apply_level_filter(Level::Error);
        assert_eq!(app.filter_applied, "level=ERROR*");
        app.apply_level_filter(Level::Other);
        assert_eq!(app.filter_applied, "level=ERROR*", "其他桶点不动");
        app.apply_level_filter(Level::DebugTrace);
        assert_eq!(app.filter_applied, "level=ERROR*", "合并桶点不动");

        // 明文模式 (子句表全 None) → 点不动
        app.level_queries = levels::no_level_queries();
        app.apply_level_filter(Level::Info);
        assert_eq!(app.filter_applied, "level=ERROR*", "只读侧栏点不动");

        std::fs::remove_file(&p).ok();
    }

    /// **T5 端到端一致性 (D2 红线的应用层落点)**: 点柱条 → 真实过滤管道 →
    /// 筛出行数 == 柱条数字。
    ///
    /// 引擎层的逐桶相等已由 `levels.rs` 的 `field_counts_equal_filter_hits_...`
    /// 钉住; 这条补的是「应用层真的把对的那个子句发出去了」——
    /// 子句生成、toggle 语义、AsyncJob 管道都不脱节。
    #[test]
    fn clicking_a_bar_filters_to_exactly_the_bar_count() {
        let mut app = LogApp::new_empty();
        // 300 行: ERROR / INFO / WARNING 各 100。WARNING 是关键样本 ——
        // 它验证别名靠前缀通配被吃到 (字节全等的 level=WARN 会筛出 0 行)。
        let mut content = Vec::new();
        for i in 0..300 {
            let lv = match i % 3 {
                0 => "ERROR",
                1 => "INFO",
                _ => "WARNING",
            };
            content.extend_from_slice(format!("{{\"level\":\"{lv}\",\"i\":{i}}}\n").as_bytes());
        }
        let p = temp_log(&content);
        let f = LogFile::open(&p).unwrap();
        app.level_counts = Arc::new(levels::count_levels_field(&f, "level"));
        app.level_queries = levels::level_queries_for("level");
        app.file = Arc::new(f);
        app.has_file = true;

        for level in [Level::Error, Level::Info, Level::Warn] {
            app.filter_applied.clear(); // 避开 toggle 分支, 单纯验「套用后筛多少」
            app.apply_level_filter(level);
            let deadline = Instant::now() + Duration::from_secs(5);
            let lines = loop {
                if let Some(out) = app.filter_job.poll() {
                    break out.lines;
                }
                assert!(Instant::now() < deadline, "过滤 job 5s 未交卷 (悬挂?)");
                std::thread::sleep(Duration::from_millis(5));
            };
            assert_eq!(
                lines.len() as u64,
                app.level_counts.get(level),
                "{level:?}: 筛出行数 != 柱条数字 —— D2 红线在应用层破裂"
            );
            assert_eq!(lines.len(), 100, "{level:?}: 300 行三轮 → 各 100");
        }

        std::fs::remove_file(&p).ok();
    }

    /// 轮转/重建: 计数随重建**全量重算**, 且列名与子句表跟着换 ——
    /// JSONL 变明文后必须降级只读, 不能留着旧列名的子句去点 (会筛出 0 行)。
    #[test]
    fn apply_rebuild_recomputes_counts_and_switches_column() {
        let mut app = LogApp::new_empty();
        let p1 = temp_log(b"{\"level\":\"ERROR\",\"m\":\"a\"}\n{\"level\":\"INFO\",\"m\":\"b\"}\n");
        let f1 = LogFile::open(&p1).unwrap();
        app.level_counts = Arc::new(levels::count_levels_field(&f1, "level"));
        app.level_queries = levels::level_queries_for("level");
        app.level_column = Some("level".into());
        app.file = Arc::new(f1);
        app.has_file = true;
        assert!(
            app.level_queries[Level::Error as usize].is_some(),
            "起点: JSONL 可点"
        );

        // 轮转后内容变明文 → worker 交全量计数 + 无级别列
        let p2 = temp_log(b"2026-09-05 ERROR plain one\n2026-09-05 WARN plain two\n");
        let f2 = LogFile::open(&p2).unwrap();
        let rebuilt_counts = levels::count_levels(&f2);
        let out = OpenOutcome {
            file: f2,
            schema: None,
            incremental_hits: None,
            rebuilt: true,
            level_counts: rebuilt_counts,
            level_column: None,
        };
        app.apply_rebuild(&p2, out);

        assert_eq!(
            *app.level_counts.as_ref(),
            levels::count_levels(&app.file),
            "重建后计数 == 新文件全量重算"
        );
        assert_eq!(app.level_counts.get(Level::Error), 1);
        assert_eq!(app.level_counts.get(Level::Warn), 1);
        assert_eq!(app.level_counts.total(), 2);
        assert!(app.level_column.is_none(), "列名换掉, 不沿用旧的");
        assert!(
            app.level_queries.iter().all(Option::is_none),
            "明文 → 子句表清空 (降级只读)"
        );
        std::fs::remove_file(&p1).ok();
        std::fs::remove_file(&p2).ok();
    }

    #[test]
    fn apply_fresh_invalidates_inflight_filter_and_search_jobs() {
        // review C1 回归: 旧文件上的在途 filter/search 结果, 换入新文件后
        // 不得贴上 (worker 用通道闸门控制交付时序, 无运气成分)
        let mut app = LogApp::new_empty();
        let (f_tx, f_rx) = std::sync::mpsc::channel::<()>();
        app.filter_job.launch(move || {
            f_rx.recv().ok();
            FilterOutcome {
                lines: vec![1, 2],
                elapsed: Duration::ZERO,
            }
        });
        let (s_tx, s_rx) = std::sync::mpsc::channel::<()>();
        app.search_job.launch(move || {
            s_rx.recv().ok();
            SearchOutcome {
                hits: vec![5],
                total: 1,
                elapsed: Duration::ZERO,
                pattern: "x".into(),
                query: "x".into(),
            }
        });
        // 两个 worker 阻塞中 (结果必未到达) → 换入新文件 (invalidate 发生)
        let p = temp_log(b"new\nfile\n");
        let f = LogFile::open(&p).unwrap();
        let level_counts = levels::count_levels(&f);
        let out = OpenOutcome {
            file: f,
            schema: None,
            incremental_hits: None,
            rebuilt: false,
            level_counts,
            level_column: None,
        };
        app.apply_fresh(p.clone(), out);
        // 放行 worker 交付, 长窗轮询: 结果必须永不到达
        f_tx.send(()).unwrap();
        s_tx.send(()).unwrap();
        let mut filter_got = false;
        let mut search_got = false;
        for _ in 0..100 {
            filter_got |= app.filter_job.poll().is_some();
            search_got |= app.search_job.poll().is_some();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!filter_got, "在途过滤结果必须被 invalidate 丢弃");
        assert!(!search_got, "在途搜索结果必须被 invalidate 丢弃");
        assert!(app.filtered.is_none());
        assert!(app.search.is_none());
        std::fs::remove_file(&p).ok();
    }
}
