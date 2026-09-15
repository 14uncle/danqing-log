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
use danqing::theme::{ScenePalette, SceneTheme, Theme};
use danqing::widget::{Column, LogoKind, Node, Row, Stack, TitleBar, Widget, node};
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

/// 标题栏主题 —— 用 `LogTheme` 的 token 组装一个 `SceneTheme`。
///
/// 六项 (`base` / `accent` / `text_primary` / `text_secondary` / `surface` /
/// `surface_input`) **全部取自 `LogTheme`**, 不再手抄。手抄的后果是同一个界面里
/// 出现**两套强调色**: 原先浅色分支的 accent 是蓝 `0.18,0.35,0.60`, 而框架玉色是
/// `#0F766E`; `base` 也手抄成 `0.96` 灰, 与主题真实的 `#F0F8F6` 差一截。
///
/// `backdrop_light` / `backdrop_dark` **保留手写**: 框架没有对应 token ——
/// 它们是场景层的前后景渐变端点, 只服务标题栏这一层场景。
/// 回归锁 `title_theme_derives_tokens_from_log_theme`。
fn title_theme(theme: config::AppTheme) -> SceneTheme {
    let t = theme.theme();
    let (backdrop_light, backdrop_dark) = match theme {
        config::AppTheme::Light => (Color::rgb(0.85, 0.85, 0.88), Color::rgb(0.70, 0.70, 0.74)),
        config::AppTheme::Dark => (Color::rgb(0.16, 0.16, 0.20), Color::rgb(0.06, 0.06, 0.08)),
    };
    SceneTheme::new(ScenePalette {
        base: t.background(),
        accent: t.accent(),
        text_primary: t.text_primary(),
        text_secondary: t.text_secondary(),
        surface: t.surface(),
        surface_input: t.surface_input(),
        backdrop_light,
        backdrop_dark,
    })
}

/// 窗口清屏色 —— **单点定义, 启动与切主题都取它**。
///
/// 为什么必须是单点: 清屏色有两条来路 (启动的 `WindowConfig`、运行时的
/// `set_clear_color`), 各算一份就会漂 —— 本仓已有先例 (设置卡页签序号曾在两个文件
/// 各抄一份、双双漂掉)。
///
/// 为什么它值得存在: 标题栏那条亮带**就是**清屏色。框架 `TitleBar` 的背景是有意的
/// `TRANSPARENT` (`danqing/src/widget/title_bar.rs`, 且有测试锁死), 让窗口底色透出;
/// 内容区反而看不见它 (被不透明的 `th.background()` 盖住)。所以清屏色不跟随主题时,
/// 症状恰好是「暗色下标题栏一条白板」。
fn window_clear_color(theme: config::AppTheme) -> Color {
    theme.theme().background()
}

/// 标题栏 (含嵌入的过滤栏)。
///
/// 抽成函数不只为了整洁 —— **测试必须复用同一份构建代码**才守得住下面这个坑:
/// `view()` 只在启动时求值一次 (`danqing/src/window/mod.rs:207` 的
/// `let tree = app.view();`, 之后整棵交给 Handler, 不再重建), 所以
/// `TitleBar::themed(&title_theme(..))` 烘进去的是**启动那一刻**的主题色。
/// 卡面上其它控件走 `bind_color` 闭包、每帧重读, 于是切主题时**只有标题栏停在旧色**
/// —— 底色换了、文字没换: 切到浅色是浅字压浅底, 切到暗色是暗字压暗底
/// (用户实机报的「标题看不清」, 两个方向都成立)。
///
/// `bind_theme` 是框架**专为这件事**准备的 API (见 `TitleBar` 文档,
/// 「每帧从应用状态重取主题」)。**不许删**。
/// 参数取**值**而非 `&LogApp`: edition 2024 里 `impl Trait` 会捕获全部输入生命周期,
/// 借 `&LogApp` 返回的话这个类型就不是 `'static`, `Column::child` 直接编译不过
/// (E0521 「borrowed data escapes」)。
fn title_bar(theme: config::AppTheme, title: String) -> impl Widget {
    TitleBar::themed(&title_theme(theme), title)
        .bind_theme(|app: &LogApp| title_theme(app.theme))
        .logo_kind(LogoKind::Log)
        .on_close(|| WindowAction::Close)
        .on_minimize(|| WindowAction::Minimize)
        .on_maximize(|| WindowAction::MaximizeOrRestore)
        .on_drag(|| WindowAction::Drag)
        .bind_maximized(|app: &LogApp| app.maximized)
        .embed(
            view::Bar::default()
                .bind_clear_filter(|app: &LogApp| app.filter_clear_rev)
                .bind_clear_search(|app: &LogApp| app.search_clear_rev)
                .bind_refocus_search(|app: &LogApp| app.search_refocus_rev),
        )
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
    /// 后台级别计数作业。
    ///
    /// **计数不在打开管道里** (2026-09-12 用户实机反馈后改): 字段口径的
    /// `extract_field` 要在整行里找 `"level":`, 成本随行内容走; 对某些文件它是
    /// 打开路径上最重的一段, 挡在内容显示之前就是「索引 92ms 却等十几秒」。
    /// 现在打开只交出口径列名, 计数由这里的作业后台完成, 侧栏随后补入。
    levels_job: AsyncJob<levels::LevelsOutcome>,
    /// 计数是否仍在算 —— 侧栏据此显示「计算中」而非把 0 当数读。
    levels_pending: bool,
    /// 过滤命中的文件行号 (升序); None = 全量。
    filtered: Option<Arc<Vec<u64>>>,
    /// 已应用的过滤查询。
    filter_applied: String,
    /// 过滤栏清空信号 (Bar::bind_clear_filter 借此原地 clear)。
    filter_clear_rev: u64,
    filter_elapsed: Option<Duration>,
    filter_job: AsyncJob<FilterOutcome>,
    /// 「回到搜索栏」信号 (T21: Bar::bind_refocus_search 借此全选草稿)。
    search_refocus_rev: u64,
    /// 搜索栏清空信号 (Bar::bind_clear_search 借此原地 clear)。
    search_clear_rev: u64,
    /// 一次性焦点请求：开搜索 / 进表格时置 true, `focus_restored` 消费后清除。
    /// 一次性焦点请求目标 (见 `focus_request`): `"log-bar"` = 过滤/搜索栏,
    /// `"log-view"` = 日志列表。`None` = 不请求。
    ///
    /// 泛化自原先的 `focus_bar: bool` —— 打开文件后也得把焦点送进列表 (T14 之后
    /// 高亮只在持焦时画, 否则打开文件看到的是「一行都没选中」, 而按 ↑↓ 只动底栏
    /// 行号、屏上什么都不动: 本模块自己判据里的「按了没反应」)。
    focus_target: Option<&'static str>,
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
    /// 展开态修订号 (M3): `toggle_expand` 每次实际改动 +1; LogView 据它
    /// 作废旧选区/单元格选中 —— 展开/折叠改变显示行映射, 旧 (显示行, 偏移)
    /// 会指向错误的行。
    expand_rev: u64,
    // ---- live-tail (T2) ----
    /// 文件路径 (增长检测轮询用)。
    path: PathBuf,
    /// 跟随模式：新行到达自动滚底 (F 键 toggle)。
    follow: bool,
    /// 上次 stat 轮询时刻 (250ms 节流)。
    last_stat_poll: Instant,
    /// 底栏提示 (截断/轮转等一次性事件)。
    ///
    /// **改它一律走 [`Self::set_notice`]**, 不直接赋值 —— 直接赋值会漏掉消退期限,
    /// 那条提示就永远赖在底栏上 (T18/Q3 之前是 8 处各写各的)。
    notice: Option<(String, NoticeKind)>,
    /// notice 的消退时刻; `None` = 当前无提示。见 [`Self::set_notice`]。
    notice_until: Option<Instant>,
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
    /// 配置读写路径。`None` = 用户真实配置 (`%APPDATA%\danqing-log\config.toml`)。
    ///
    /// **测试必须给临时路径** —— 见 [`Self::save_config`] 里那条 `#[cfg(test)]` 的
    /// 硬拦。这不是洁癖: `save_to` 是**整文件覆盖写**, 而 `load_from` 对认不出的
    /// `mode` 会取值域默认 (light) —— 一次 `cargo test` 就能把用户的主题**改掉**,
    /// 并抹掉手写注释。
    cfg_path: Option<std::path::PathBuf>,
    /// 设置卡当前页签**下标** —— 序号含义见 `settings.rs` 里 `.tab()` 处 (**唯一真身**,
    /// 别在这里另列一份, 加页签时会漂)。越界值无需在此防御: 框架 `Tabs` 自行钳制
    /// (`clamp_active`), 且 `on_change` 只会回传合法下标。
    /// 留在应用状态里: 重开卡片停在上次那页。
    settings_tab: usize,
}

/// 底栏提示的级别 (2026-09-14 实机 M0 P27): 警示与提示**同屏可辨**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoticeKind {
    /// 警示 (打开失败/轮转/选区超限) —— 用 `danger()` 画。
    Warn,
    /// 提示 (复制回执等) —— 用 `text_secondary()` 画。
    Info,
}

/// 应用消息。
pub(crate) enum Msg {
    /// 滚轮/键盘滚动 N 显示行 (负 = 向上)。
    ScrollRows(f64),
    /// **绝对**滚到某显示行 (T17 滚动条拖拽)。与 `ScrollRows` 的两点不同都是
    /// 有意的: ① 它是绝对定位, 拖拽是「拇指在哪内容就在哪」而不是增量;
    /// ② 它**不动 `selected`** —— 抓滚动条是「看」不是「选」, 见 todo T17 不变量 ③。
    ///
    /// `at_bottom` = 目标就在**条能被拖到的最底**。它存在只为一个理由: 跟随态下
    /// app 的 `top_row` 用 `count-1` 口径, 而条能表达的最大值是 `count-可见行数`
    /// —— 两者不等, 直接比 `top < top_row` 会把「在底部碰一下条」误判成「向上看」
    /// 而**静默脱掉 FOLLOW** (T17 review 抓出来的)。带上这一位, 判断才有据可依。
    ScrollTo {
        top: f64,
        at_bottom: bool,
    },
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
    /// 设置卡切页签 (下标含义见 `settings.rs` 的 `.tab()` 处)。
    SelectSettingsTab(usize),
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
    ///
    /// **级别语义** (2026-09-14 实机 M0 P27): `NoticeKind::Warn` = 警示
    /// (打开失败/轮转/选区超限), 用 `danger()` 画; `NoticeKind::Info` = 提示
    /// (复制回执等), 用 `text_secondary()` 画 —— 两者同屏时**可辨**。
    Notice(String, NoticeKind),
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
        Self::new_empty_at(None)
    }

    /// 同上, 但可指定配置路径 (仅测试用; 见 `cfg_path` 字段)。
    fn new_empty_at(cfg_path: Option<std::path::PathBuf>) -> Self {
        let cfg = match &cfg_path {
            Some(p) => config::Config::load_from(p),
            None => config::Config::load(),
        };
        Self {
            cfg_path,
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
            levels_job: AsyncJob::new(),
            levels_pending: false,
            filtered: None,
            filter_applied: String::new(),
            filter_clear_rev: 0,
            filter_elapsed: None,
            filter_job: AsyncJob::new(),
            search_refocus_rev: 0,
            search_clear_rev: 0,
            search: None,
            search_query: String::new(),
            search_pattern: None,
            search_elapsed: None,
            search_job: AsyncJob::new(),
            bookmarks: std::collections::BTreeSet::new(),
            expanded: ExpandMap::new(),
            sub_rows: std::collections::BTreeMap::new(),
            expand_rev: 0,
            focus_target: None,
            path: PathBuf::new(),
            follow: false,
            last_stat_poll: Instant::now(),
            notice: None,
            notice_until: None,
            maximized: false,
            open_job: None,
            loading_label: None,
            settings_open: false,
            theme: cfg.theme,
            histogram_visible: cfg.histogram,
            settings_tab: 0,
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

    /// 起一个后台计数作业 (换文件 / 重建后调用; 口径列名须已由
    /// [`Self::adopt_level_column`] 定好)。
    ///
    /// 计数未就绪期间侧栏**只读且不显示数字** —— 把 0 显示出来会被读成
    /// 「这个文件真的没有 ERROR」, 那是假信息。
    fn launch_levels_job(&mut self) {
        self.levels_job.invalidate();
        self.levels_pending = true;
        self.level_counts = Arc::new(LevelCounts::default());
        self.level_queries = levels::no_level_queries();
        let file = Arc::clone(&self.file);
        let column = self.level_column.clone();
        self.levels_job
            .launch(move || levels::counts_for(file, column.as_deref()));
    }

    /// 计数作业交付: 与快照对账后换入计数与子句表。
    ///
    /// 作业在算的时候文件可能又增长了 —— 此时**不能重起作业** (持续增长的 tail
    /// 会永远算不完), 而是用 `update_for_append` 把快照之后的那几行按「重叠一行」
    /// 补上。只数增量, 很便宜。
    fn pickup_levels_job(&mut self) {
        let Some(out) = self.levels_job.poll() else {
            return;
        };
        let counts = if out.file.line_count() < self.file.line_count() {
            levels::update_for_append(
                &out.file,
                &self.file,
                out.counts,
                self.level_column.as_deref(),
            )
        } else {
            out.counts
        };
        self.level_counts = Arc::new(counts);
        self.level_queries = out
            .column
            .as_deref()
            .map_or_else(levels::no_level_queries, levels::level_queries_for);
        self.level_column = out.column;
        self.levels_pending = false;
    }

    /// 把当前设置写回 `config.toml`。
    ///
    /// 必须走整文件写入 —— [`config::Config`] 的两个键同源, 分头写会让
    /// 「改主题」顺手抹掉侧栏开关 (config.rs 的 `round_trip_preserves_both_keys`
    /// 钉着这条)。
    fn save_config(&self) {
        let cfg = config::Config {
            theme: self.theme,
            histogram: self.histogram_visible,
        };
        match &self.cfg_path {
            Some(p) => cfg.save_to(p),
            // **测试里不许落到真实配置**: 这条不是洁癖, 是实测过的坑 ——
            // 本批的 T20 单测走 `update(Msg::ToggleHistogram)` → 这里 → 真实路径,
            // 而 `save_to` 是**整文件覆盖写**、`load_from` 对认不出的 `mode` 取默认
            // (light): 一次 `cargo test` 就能把用户的主题改掉、手写注释抹掉。
            // 与其靠「下一个写测试的人记得」, 不如让它**写不出去**。
            None => {
                #[cfg(test)]
                panic!("测试不得写真实配置 —— 请用 LogApp::new_empty_at(临时路径)");
                #[cfg(not(test))]
                cfg.save();
            }
        }
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
            self.expand_rev += 1; // 显示行映射已变 → LogView 选区守卫
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
        self.expand_rev += 1;
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
                    Some((self.parse_filter(&self.filter_applied), old.line_count()))
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
                Ok(new) => self.apply_appended(new, None),
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
            level_column,
            ..
        } = out;
        let base_status = status_text(path, &file);
        let new_count = file.line_count();
        self.file = Arc::new(file);
        // 格式可能整体换了 → 列名与子句表跟着换 (不沿用旧的); 计数重新后台算
        self.adopt_level_column(level_column);
        self.launch_levels_job();
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
        self.set_notice("文件已截断/轮转".into(), NoticeKind::Warn);
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
        self.adopt_level_column(level_column);
        self.launch_levels_job();
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
        self.notice_until = None;
        // **把焦点送进列表** (T14 之后高亮只在持焦时画): 不送的话, 打开文件看到的
        // 是「一行都没选中」, 而按 ↑↓ 只动底栏行号、屏上什么都不动 —— 正是本模块
        // 自己那条判据要消灭的「按了没反应」。只在 **Fresh** (换了文件) 时送:
        // rebuild/append 走的是 `apply_rebuild`/`apply_appended`, 不动焦点, 免得
        // 轮转或追长时把正在栏里打字的用户拽走。
        self.focus_target = Some("log-view");
        self.refresh_status();
    }

    /// Append 换入 (同步小追加 / 追平 worker 交卷同链): 增量过滤 + follow 滚底。
    /// `worker_hits` = worker 已算好的增量命中 (review R2: 巨量追平过滤下沉);
    /// None = 本地扫 (同步小追加, 毫秒级)。
    fn apply_appended(&mut self, new: LogFile, worker_hits: Option<Vec<u64>>) {
        let old_line_count = self.file.line_count();
        // 重算起点**退一行**: 旧快照末行可能以无换行结尾、被本次追加补全改判
        // (review R1)。计数与过滤必须同起点同区间, 否则柱条数字与筛选结果
        // 当场分岔 —— 即 D2 红线破裂, 而这正是 review 前两侧同步漂移掩盖掉的那个形态。
        //
        // 这里与 worker 各自独立算出同一个 `from` (worker 用发起时的旧行数, 这里用
        // 落地时的) —— 二者能相等，靠的是 `poll_growth` 开头的 `open_job.is_some()`
        // 门禁: 在途期间不叠加任何 tail 动作, 故 `self.file` 不会在 launch 与落地
        // 之间被别的追加换掉。**若将来允许并发追加, 这个摘/补对称会静默失效**
        // (摘多了漏行、摘少了重计), 届时须把 `from` 随产物一起交回来。
        let from = old_line_count.saturating_sub(1);
        // 计数: 已就绪 → 在 UI 线程做「重叠一行」的绝对量更新 (KB 级增量, 便宜),
        // **先算再换入** (update_for_append 需要旧快照)。
        //
        // 未就绪 → **什么都不做**: 计数作业交付时会拿它自己的快照与当时的文件
        // 对账 (见 `pickup_levels_job`)。这里若重起作业, 一个持续增长的 tail
        // 会把计数一遍遍从头来过 —— 永远算不完, 侧栏永远挂在「…」。
        let recomputed = (!self.levels_pending).then(|| {
            levels::update_for_append(
                &self.file,
                &new,
                *self.level_counts,
                self.level_column.as_deref(),
            )
        });
        self.file = Arc::new(new);
        if let Some(c) = recomputed {
            self.level_counts = Arc::new(c);
        }
        // 过滤: 先摘掉将被重算区间的旧命中, 再合并新命中 (否则重叠行出现两次)
        self.drop_filter_hits_from(from);
        match worker_hits {
            Some(hits) => self.merge_filter_hits(hits),
            None => self.append_filter_hits(from),
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
            Ok(out) => {
                // 落地耗时 (自发起): 与 worker 的 `perf open_phases` 对照 ——
                // 两者相减即「交付 + 拾取」的延迟; 若落地很快而用户仍等很久,
                // 瓶颈就在落地之后的渲染, 不在这条管道。
                log::info!(
                    "perf open_landed: {:?} 自发起 (kind={:?})",
                    job.elapsed_since_launch(),
                    job.kind()
                );
                match job.kind() {
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
                                ..
                            } = out;
                            self.apply_appended(file, incremental_hits);
                        }
                    }
                }
            }
            Err(e) => {
                // 「索引已取消」= 主动取消, 静默; 失败语义按 kind 分流 (保旧行为):
                // Fresh 失败 notice + 留空态/旧视图; Rebuild/Append 静默,
                // 文件仍过期, 下轮 250ms poll 自然驱动重试。
                if !e.to_string().contains(INDEX_CANCELLED) {
                    match job.kind() {
                        OpenKind::Fresh => {
                            log::warn!("打开失败：{e:#}");
                            self.set_notice(
                                format!(
                                    "无法打开：{}",
                                    job.path()
                                        .file_name()
                                        .map(|n| n.to_string_lossy())
                                        .unwrap_or_default()
                                ),
                                NoticeKind::Warn,
                            );
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

    /// 实时过滤：增量行追加命中表 (只跑 `[from, …)`，不全量重跑)。
    fn append_filter_hits(&mut self, from: u64) {
        if self.filter_applied.is_empty() {
            return;
        }
        if self.filtered.is_none() {
            return;
        }
        let clauses = self.parse_filter(&self.filter_applied);
        let new_hits = jsonl::run_filter_from(&self.file, &clauses, from);
        self.merge_filter_hits(new_hits);
    }

    /// 摘掉过滤表中 `>= from` 的旧命中 —— 它们落在本次重算区间内, 会被重新跑出来。
    ///
    /// 必须与 [`Self::append_filter_hits`] 的起点**同一个 `from`**: 重叠行若只摘不补
    /// 就漏, 只补不摘就重, 两种都让底栏行数与侧栏柱条一起偏 (且一起偏就意味着
    /// D2 的对照检查看不出来)。
    fn drop_filter_hits_from(&mut self, from: u64) {
        let Some(existing) = &self.filtered else {
            return;
        };
        // 表按行号升序 (过滤产出即有序), 故二分找到第一个 >= from 的位置
        let keep = existing.partition_point(|&l| l < from);
        if keep == existing.len() {
            return; // 无命中落在重算区间, 无需摘
        }
        let mut kept = existing.as_ref()[..keep].to_vec();
        kept.shrink_to_fit();
        self.filtered = Some(Arc::new(kept));
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
        // 索引之后的阶段 (列发现 / 级别计数) 没有细粒度字节进度 —— 继续显示百分比
        // 就会卡在 99% 不动, 用户看到的等待于是和状态栏那个「索引 N ms」对不上
        // (2026-09-12 用户反馈)。如实报阶段名, 把这段等待显性化。
        if let Some(phase) = job.phase_name() {
            return Some((phase, name, "…".to_string()));
        }
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

    /// 置一条底栏提示 —— **notice 的唯一入口** (T18)。
    ///
    /// 自带消退期限 (Q3 裁的「自动消退」): 提示是**对刚才那个动作**的回答,
    /// 一直赖在底栏会变成噪声, 还会让「底栏读数」这件事失去可信度。
    ///
    /// **`NOTICE_TTL` 是待实机核对的估值**: spec 说「具体时长 build 时**实测定**,
    /// 不估算」, 而本机跑不了真机走查 —— 故先取一个, 并挂进矩阵 §6 的核对单
    /// (实机那轮把「太短没看见 / 太长碍事」两个方向都试一次)。
    fn set_notice(&mut self, text: String, kind: NoticeKind) {
        self.notice = Some((text, kind));
        self.notice_until = Some(Instant::now() + NOTICE_TTL);
        self.refresh_status();
    }

    /// notice 到点即消退 (T18/Q3)。抽成独立方法是为了**可测**: `tick` 要
    /// `AnimationCtx`, 而本方法不必。
    fn expire_notice(&mut self) {
        if self.notice_until.is_some_and(|t| Instant::now() >= t) {
            self.notice = None;
            self.notice_until = None;
            self.refresh_status();
        }
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
        // notice **不进这个串** —— 它是第二条通道, 由 `LogView::paint` 单独取色单独
        // 落笔 (T11)。此前把它拼进来, 结果是同一句话被画两遍 (串尾一遍、notice 段
        // 又一遍), 而且「警示色」和「常态色」压在同一个字符串上根本没处分。
        self.status = s;
    }

    /// 解析过滤查询 —— **全应用唯一的过滤解析入口** (parse + 键名规范化)。
    ///
    /// 三处调用点 (应用过滤 / 巨量追平作业 / live-tail 重滤) 必须都走这里:
    /// 键名规范化 (`LEVEL=ERROR` → `level=ERROR`) 只做在一处, 同一个查询串在不同
    /// 路径上就会得到不同命中集 —— 而增量与全量不一致只在「开着过滤又赶上追加」
    /// 时才现形, 是最难查的一类差异 (D2 红线同款理由)。
    ///
    /// `.log` 文件 schema 为 None → 跳过规范化, 行为与从前一字不差。
    fn parse_filter(&self, query: &str) -> Vec<jsonl::Clause> {
        let mut clauses = jsonl::parse_query(query);
        if let Some(schema) = self.schema.as_deref() {
            jsonl::normalize_clause_keys(&mut clauses, schema);
        }
        clauses
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
        let clauses = self.parse_filter(&query);
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
            self.focus_target = Some("log-bar");
        }
        self.refresh_status();
    }

    /// 聚焦搜索栏 (`/` 原始模式 / Ctrl+F 任意模式)。
    ///
    /// **T21 (P39): 不再「聚焦即干净开始」**。Ctrl+F 是「回到搜索框」的**反射键**,
    /// 而原实现每次都把已输入未应用的草稿清掉 —— 反射键不该销毁工作。现在:
    /// 没持焦 → 聚焦 (草稿原样留着); 已持焦 → 全选 (直接覆写)。
    /// 清空仍归 Esc, 那条路径没动。
    fn open_search(&mut self) {
        self.focus_target = Some("log-bar");
        self.search_refocus_rev += 1;
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
    /// 状态栏补动作反馈 —— 此前只有行号变金一个信号，用户按完不知道成没成;
    /// 计数由 `refresh_status` 的常驻「书签 N」段承担，这里只缀动作。
    fn toggle_bookmark(&mut self) {
        let line = self.file_line_of(self.selected);
        let added = self.bookmarks.insert(line);
        if !added {
            self.bookmarks.remove(&line);
        }
        self.refresh_status();
        self.status.push_str(if added {
            " · 已添加书签"
        } else {
            " · 已去掉书签"
        });
    }

    /// `'` / Ctrl+G: 跳下一书签 (严格大于当前行，环绕)。状态栏报位次 `书签 i/N`。
    fn goto_next_bookmark(&mut self) {
        if let Some(line) = next_bookmark(&self.bookmarks, self.file_line_of(self.selected)) {
            self.jump_to_file_line(line);
            self.refresh_status();
            // line 必在集合内: 位次 = 比它小的书签数 + 1
            let i = self.bookmarks.range(..line).count() + 1;
            let n = self.bookmarks.len();
            self.status.push_str(&format!(" · 书签 {i}/{n}"));
        } else {
            // M3 (P25): 无书签时 Ctrl+G 按了没反应 —— 说清为什么。
            self.set_notice("尚无书签 (b 添加)".into(), NoticeKind::Info);
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
    // 两分支一律 `(?i)` 前缀 (2026-09-15, spec D2/D3): 默认大小写不敏感。
    // UTF-8 用 `(?i)` 而非 `(?i-u)` —— 查询是用户的裸正则, `(?-u)` 会顺带把
    // `\w`/`\d`/`\b` 降级成 ASCII 语义, 与大小写无关的行为不该被本模块改掉。
    // 逃逸舱零代码: 用户写 `(?-i)` 即局部恢复敏感 (组内 flag 覆盖, 有测试锁)。
    // 非 UTF-8 分支同理套在字节字面量外 —— `(?i)` 对 `(?-u)\xNN` 折叠成立, 已用
    // 真 GBK 文件实测 (7884 命中行, 与手工 [eE] 展开逐字节一致)。
    if enc == Encoding::Utf8 {
        format!("(?i){query}")
    } else {
        format!(
            "(?i){}",
            bytes_as_literal_regex(&encoding::encode_query(enc, query))
        )
    }
}

/// top_row 钳制到 [0, max_top]。独立成函数供单测。
fn clamp_top(top: f64, line_count: u64) -> f64 {
    let max = line_count.saturating_sub(1) as f64;
    top.clamp(0.0, max)
}

/// 滚轮 delta → 要滚的显示行数 (**单一换算点**, T17)。
///
/// 三处滚轮 (内容区 LogView / 未认领的滚轮 / 将来任何新入口) 必须走同一支,
/// 否则「在侧栏滚」与「在列表上滚」手感会不一样 —— 而 P30 补的正是这两处的
/// **一致性**, 各写一份等于把刚修好的东西再拆开。
///
/// **框架不归一 delta** (普查 G10): `window/event.rs:150-154` 把 `LineDelta`
/// (行数, 通常 ±1..3) 与 `PixelDelta` (像素, 精确触控板可达 ±100) 抹平成同一个
/// `f32`, 下游**无从分辨**。按行数档取 3 倍再夹一个**每次事件**的上界: 不夹的话
/// 触控板一次能跳几百行。夹的是单次事件, 不是总量, 连续滚不受影响。
fn wheel_rows(delta_y: f32) -> f64 {
    (-f64::from(delta_y) * 3.0).clamp(-WHEEL_MAX_ROWS, WHEEL_MAX_ROWS)
}

/// 单次滚轮事件最多滚多少显示行 (见 [`wheel_rows`])。
const WHEEL_MAX_ROWS: f64 = 12.0;

/// 底栏提示的停留时长 (见 `LogApp::set_notice`)。**待实机核对**。
const NOTICE_TTL: std::time::Duration = std::time::Duration::from_secs(4);

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
            Msg::ScrollTo { top, at_bottom } => {
                // 往回(上)拖 = 想回头看 → 停止跟随 (与滚轮同规, 别把用户拽回底部)。
                // **拖到条底不算往回** —— 见枚举上的注释: 跟随态的 `top_row` 比条能
                // 表达的底还大, 不排除这一格就会「在底部碰一下条 → FOLLOW 没了」。
                if !at_bottom && top < self.top_row && self.follow {
                    self.follow = false;
                    self.refresh_status();
                }
                // **不动 `selected`**: 抓滚动条是「看」不是「选」(T17 不变量 ③)。
                self.top_row = clamp_top(top, self.display_count());
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
            Msg::SelectSettingsTab(i) => {
                self.settings_tab = i;
            }
            Msg::OpenUrl(url) => {
                if let Err(err) = open::that(&url) {
                    log::warn!("打开链接失败：{err}");
                }
            }
            Msg::OpenFile(path) => {
                self.reload_file(path);
            }
            Msg::Notice(text, kind) => self.set_notice(text, kind),
            Msg::SelectTheme(idx) => {
                self.theme = config::AppTheme::from_index(idx);
                // 通知窗口换底色。**这一步此前从缺** —— 于是切主题后标题栏那条
                // (透出的清屏色) 纹丝不动, 只有内容区变了色。
                if let Some(sender) = &self.window_sender {
                    sender.set_clear_color(window_clear_color(self.theme));
                }
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
                        .child(title_bar(self.theme, self.make_title()))
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
        // P30 (T17): **未认领的滚轮**转给列表滚动。
        //
        // 框架按点子命中分发滚轮、不向父级回落, 所以指针停在侧栏/过滤栏上时,
        // 日志区根本收不到 —— 用户必须把指针挪回内容区才滚得动。这里补的是
        // 另一半: 凡是**没有组件认领**的滚轮 (侧栏、过滤栏、标题栏、行外空白)
        // 一律滚列表。**不需要位置数学**: 指针在日志区上时 LogView 已经
        // `Consumed` 了, 能走到这里的本来就不是它。
        if let Event::MouseWheel { delta, .. } = event {
            // 模态不穿透 (与 T16 同一条纪律): 卡开着时滚轮只属于卡, 不许滚卡后的日志
            if !self.settings_open && self.has_file {
                let rows = wheel_rows(delta.1);
                if rows != 0.0 {
                    self.update(Msg::ScrollRows(rows));
                }
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
        // 设置卡打开 = 模态: Esc 关卡 (S3), 其余键一律吞掉 —— 卡底下的日志区
        // 不该响应键盘 (2026-09-14 用户实机: 卡内主题下拉未持焦时 ↑↓ 滚动了
        // 底层日志)。卡内控件经焦点路由自行消费、到不了这里; 能到这里的都是
        // 无人认领的键。(Ctrl+O/Ctrl+L 走 app_key_filter 前置, 不在此门禁内。)
        if self.settings_open {
            if let Some(msg) = settings::handle_settings_key(key) {
                self.update(msg);
            }
            return;
        }
        // 空态门禁: 仅 Ctrl+O (app_key_filter 前置, 不经此处) 与设置可用, 其余键无文件无意义
        if !self.has_file {
            // M3 (2026-09-14 实机 M0 P26): 空态按键被吞时**说清为什么** ——
            // 原先 `return` 静默, 用户按 Ctrl+F/方向键毫无反应。
            self.set_notice("尚未打开文件 (Ctrl+O 打开)".into(), NoticeKind::Info);
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
                if s.eq_ignore_ascii_case("t") {
                    if self.schema.is_some() {
                        self.update(Msg::ToggleMode);
                    } else {
                        // M3 (P23): Ctrl+T 无 JSONL 时**说清为什么** —— 原先静默。
                        self.set_notice("本文件非 JSONL, 无表格模式".into(), NoticeKind::Info);
                    }
                    return;
                }
                if s.eq_ignore_ascii_case("a") {
                    // M3 (P34): Ctrl+A 被吞 —— 说清为什么 (行多选已裁挂 v1.x,
                    // 见 docs/ROADMAP-v1x.md §四)。
                    self.set_notice("行多选未实现 (v1.x 待裁)".into(), NoticeKind::Info);
                    return;
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
                } else {
                    // M3 (P24): → 在已展开行上按了没反应 —— 说清为什么。
                    self.set_notice("本行已展开".into(), NoticeKind::Info);
                }
            }
            Key::Named(NamedKey::ArrowLeft) if self.mode == ViewMode::Table => {
                let file_line = self.file_line_of(self.selected);
                if self.expanded.is_expanded(file_line) {
                    self.toggle_expand(file_line);
                } else {
                    // M3 (P24): ← 在未展开行上按了没反应 —— 说清为什么。
                    self.set_notice("本行未展开".into(), NoticeKind::Info);
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
        // 模态守卫 (T16/P32): 设置卡开着时, 全局键**不得穿透到卡后**。
        // 原先三个后果: Ctrl+O 在卡片**之上**弹系统文件对话框; Ctrl+F 把焦点按
        // id 送到卡后**看不见的**输入框 (此后打的字全进它); Ctrl+L 把卡后的侧栏
        // 显隐掉。框架的 `app_key_filter` 是应用回调、在模态判定之前无条件跑
        // (`handler.rs:434-441`), 所以这个守卫只能加在产品侧。
        //
        // **位置很要紧: 必须在「ctrl + 字符」筛选之后**。框架在这一函数返回
        // `Some` 时**直接 return, 不再走焦点分发** (`handler.rs:436-441`), 而卡内
        // 控件 (主题下拉 / 侧栏开关 / 关闭钮) 全靠焦点分发收键 —— 守卫若放在函数
        // 入口, 卡内键盘会**全死**: 下拉导航不动、开关切不了、Enter 关不掉卡。
        // 本批第一版正是那么写的 (见测试里的反向对照), 被 review 抓出来。
        if self.settings_open {
            return Some(Msg::Noop);
        }
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
        self.expire_notice();
        self.pickup_open_job();
        self.pickup_levels_job();
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
        self.focus_target
    }

    /// 消费焦点请求 (逐帧调用，一次性：置位后立即清除，避免 Esc 后误拉回)。
    fn focus_restored(&mut self) {
        self.focus_target = None;
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
    // 报**打开总墙钟**在先, 行索引在后。
    //
    // 原来只报「索引 N ms」, 而它不含 UTF-16 的读整文件 + 转码 —— 实测 100 MiB
    // UTF-16LE: 报「索引 7 ms」而实际 open 200 ms (**13 倍**), GB 级按比例是秒级。
    // 「数字和视觉不符」的这类反馈, 根子就是报了个不等于等待时间的数字。
    let mut load = format!("打开 {} ms", s.open.as_millis());
    if s.preprocess > Duration::ZERO {
        load.push_str(&format!(" (转码 {} ms)", s.preprocess.as_millis()));
    }
    format!(
        "{name} · {} · {:.1} MiB · {} 行 · {load} · mmap {} us · 索引 {} ms ({:.0} MiB/s)",
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
        // 清屏色随配置里的主题 —— 此前写死浅色, 存暗色配置启动也开在白底上
        // (app 在上一行已从配置读出主题, 只是当时没人问它)。
        clear_color: window_clear_color(app.theme),
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
    use std::io::Write;

    /// expand_rev (M3/T6): 实际展开/折叠才 +1; parse 失败/无嵌套不涨 ——
    /// 守卫的触发源必须精确, 虚涨会误杀活着的选区 (LogView 侧见
    /// `sync_clears_selection_when_expand_rev_changes`)。
    #[test]
    fn toggle_expand_bumps_expand_rev_only_on_real_change() {
        let mut app = LogApp::new_empty();
        let p = temp_log("{\"a\":{\"b\":1}}\nplain\n".as_bytes());
        app.file = Arc::new(LogFile::open(&p).unwrap());
        let rev0 = app.expand_rev;
        app.toggle_expand(0); // 展开含嵌套行
        assert_eq!(app.expand_rev, rev0 + 1);
        app.toggle_expand(1); // 明文行 parse 失败 → 不涨
        assert_eq!(app.expand_rev, rev0 + 1);
        app.toggle_expand(0); // 折叠
        assert_eq!(app.expand_rev, rev0 + 2);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn clamp_top_bounds() {
        assert_eq!(clamp_top(-5.0, 100), 0.0, "负数钳到 0");
        assert_eq!(clamp_top(500.0, 100), 99.0, "越界钳到末行");
        assert_eq!(clamp_top(42.5, 100), 42.5, "区间内不变 (保小数偏移)");
        assert_eq!(clamp_top(3.0, 0), 0.0, "空文件归零");
    }

    /// `title_theme` 的六项必须**取自 `LogTheme`**, 不再手抄。
    ///
    /// 手抄的后果是同一个界面里出现**两套强调色**: 浅色分支的 accent 曾经是蓝
    /// `0.18,0.35,0.60`, 而框架玉色是 `#0F766E`。六个值全部改成从 `LogTheme` 取,
    /// 只剩 `backdrop_light/dark` 手写 (框架没有对应 token, 它们只服务标题栏
    /// 这一层场景)。
    #[test]
    fn title_theme_derives_tokens_from_log_theme() {
        for app in [config::AppTheme::Light, config::AppTheme::Dark] {
            let scene = title_theme(app);
            let t = app.theme();
            assert_eq!(scene.background(), t.background(), "{app:?} base");
            assert_eq!(scene.accent(), t.accent(), "{app:?} accent");
            assert_eq!(
                scene.text_primary(),
                t.text_primary(),
                "{app:?} text_primary"
            );
            assert_eq!(
                scene.text_secondary(),
                t.text_secondary(),
                "{app:?} text_secondary"
            );
            assert_eq!(scene.surface(), t.surface(), "{app:?} surface");
            assert_eq!(
                scene.surface_input(),
                t.surface_input(),
                "{app:?} surface_input"
            );
        }
    }

    /// 切主题后**标题栏文字色必须跟着变** —— 它靠 `bind_theme` 每帧重取,
    /// 不能靠构造。
    ///
    /// 复现的是这个缺陷: `view()` 只在启动时求值一次
    /// (`danqing/src/window/mod.rs:207` 的 `let tree = app.view();`),
    /// 所以构造时烘进 `TitleBar` 的主题色**不随切换而变**, 而底色 (清屏色) 变了
    /// → 切到浅色是浅字压浅底、切到暗色是暗字压暗底。用户实机报的两个方向都成立。
    ///
    /// **关键在「构造用一个主题、sync 用另一个」**: 两边都用同一主题的话,
    /// 就算 `bind_theme` 掉了也照样绿 —— 那样测的是构造, 不是绑定。
    #[test]
    fn title_bar_colors_follow_theme_switch() {
        use danqing::{Constraints, Point, Rect, RectBatch, TextBatch};

        // 构造用暗色 —— 字号/间距两主题相同, 变的只有颜色。
        let mut dark_app = LogApp::new_empty();
        dark_app.theme = config::AppTheme::Dark;
        let mut bar = title_bar(dark_app.theme, dark_app.make_title());

        // 每帧状态换成浅色 (模拟运行中切主题)。
        let mut light_app = LogApp::new_empty();
        light_app.theme = config::AppTheme::Light;
        bar.sync(&light_app);

        let mut texts = TextBatch::default();
        let mut rects = RectBatch::new();
        let size = bar.layout(Constraints::tight(Size::new(1100.0, 40.0)), &mut texts);
        bar.paint(Rect::new(Point::ZERO, size), &mut rects, &mut texts);

        // 期望值**从主题取**, 不写字面量: 原先这里钉的是手抄时代的 `0.12`,
        // R5 把 `title_theme` 改成取自 `LogTheme` 后它就过期了 (token 是 #0F172A),
        // 断言随即变红 —— 那条红是真的, 说明它确实盯着颜色。
        let t = danqing::theme::LightTheme.text_primary();
        let want = danqing::srgb_to_linear(t.r);
        let hit = texts
            .instance_colors()
            .iter()
            .any(|c| (c.r - want).abs() < 1e-3);
        assert!(
            hit,
            "切到浅色后标题文字应变成浅色主题的正文色 {t:?} —— 没命中即 `bind_theme` 缺失"
        );
    }

    #[test]
    fn window_clear_color_follows_theme() {
        // 清屏色的**单点定义** (AD1): 启动与切主题都取它, 不许两处各算一份。
        // 回归的是这个缺陷: 清屏色原本写死浅色, 且全仓**零处** set_clear_color 调用 ——
        // 于是存暗色配置启动、或运行中切到暗色, 窗口底色纹丝不动。
        // 标题栏那条亮带**就是**清屏色 (框架 TitleBar 背景是有意的 TRANSPARENT)。
        use danqing::theme::{DarkTheme, LightTheme, Theme};
        assert_eq!(
            window_clear_color(config::AppTheme::Light),
            LightTheme.background(),
            "浅色: 清屏色 = 主题背景"
        );
        assert_eq!(
            window_clear_color(config::AppTheme::Dark),
            DarkTheme.background(),
            "暗色: 清屏色 = 主题背景"
        );
        assert_ne!(
            window_clear_color(config::AppTheme::Light),
            window_clear_color(config::AppTheme::Dark),
            "两主题的清屏色必须不同 —— 相同即等于没跟随 (本缺陷的原始形态)"
        );
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
        // 两分支的 `(?i)` 前缀是 2026-09-15 的默认不敏感 (spec D2/D3)。
        assert_eq!(
            build_search_pattern(Encoding::Utf8, "ERROR|FATAL"),
            "(?i)ERROR|FATAL"
        );
        // GBK 存储：转字节 → \xNN 字面量 (退化为字面量语义), 外面套同一个 (?i)
        assert_eq!(
            build_search_pattern(Encoding::Gbk, "中文"),
            "(?i)(?-u)\\xD6\\xD0\\xCE\\xC4"
        );
    }

    #[test]
    fn search_pattern_is_case_insensitive_by_default_with_opt_out() {
        // 默认不敏感: 小写查询命中大写内容
        let re = regex::bytes::Regex::new(&build_search_pattern(Encoding::Utf8, "error")).unwrap();
        assert!(re.is_match(b"2026-09-15 ERROR boom"));
        // 逃逸舱: `(?-i)` 组内覆盖, 用户想精确时零代码可用
        let re =
            regex::bytes::Regex::new(&build_search_pattern(Encoding::Utf8, "(?-i)error")).unwrap();
        assert!(!re.is_match(b"2026-09-15 ERROR boom"), "逃逸舱须恢复敏感");
        assert!(re.is_match(b"2026-09-15 error boom"));
    }

    #[test]
    fn case_prefix_does_not_change_regex_class_semantics() {
        // `(?i)` 而非 `(?i-u)`: `\w` 仍是 Unicode 语义 (含汉字), `\d` 仍是 Unicode 数字。
        // 若误用 `(?-u)`, `\w` 会退化成 ASCII 类 —— 这两条断言就是那把锁。
        let re = regex::bytes::Regex::new(&build_search_pattern(Encoding::Utf8, r"\w+")).unwrap();
        assert!(re.is_match("汉字".as_bytes()), "\\w 必须仍是 Unicode 语义");
        let re = regex::bytes::Regex::new(&build_search_pattern(Encoding::Utf8, r"^\d+$")).unwrap();
        assert!(
            re.is_match("１２３".as_bytes()),
            "\\d 必须仍是 Unicode 数字类"
        );
    }

    /// `parse_filter` 是过滤解析的**唯一入口**: 键名规范化只在有 schema 时发生,
    /// 且用户输入的原文不改 (状态栏显示的是他敲的那串)。
    #[test]
    fn parse_filter_normalizes_keys_only_when_schema_present() {
        let mut app = LogApp::new_empty();
        let field = |p: &str| {
            vec![jsonl::Clause::Field {
                path: vec![p.to_string()],
                op: jsonl::Op::Eq,
                value: "ERROR".to_string(),
            }]
        };

        // .log (无 schema): 键名原样 —— 与改造前一字不差
        assert!(app.schema.is_none(), "new_empty 应无 schema");
        assert_eq!(app.parse_filter("LEVEL=ERROR"), field("LEVEL"));

        // JSONL (有 schema): 改写为列里的真实写法
        app.schema = Some(Arc::new(Schema {
            columns: vec![jsonl::Column {
                name: "level".into(),
                width_chars: 5,
            }],
        }));
        assert_eq!(app.parse_filter("LEVEL=ERROR"), field("level"));
        // 大小写已一致的照常通过
        assert_eq!(app.parse_filter("level=ERROR"), field("level"));
        // 裸词与多段路径不经规范化
        assert_eq!(
            app.parse_filter("boom"),
            vec![jsonl::Clause::Bare("boom".into())]
        );
        assert_eq!(
            app.parse_filter("a.b=1"),
            vec![jsonl::Clause::Field {
                path: vec!["a".into(), "b".into()],
                op: jsonl::Op::Eq,
                value: "1".into(),
            }]
        );
    }

    /// 过滤栏存的是**用户敲的原文**, 规范化只发生在解析层 —— 状态栏回显与
    /// 清空重放 (`filter_applied`) 都拿它, 改掉会让用户看到自己没敲过的字。
    #[test]
    fn apply_filter_keeps_raw_query_for_display() {
        let mut app = LogApp::new_empty();
        app.schema = Some(Arc::new(Schema {
            columns: vec![jsonl::Column {
                name: "level".into(),
                width_chars: 5,
            }],
        }));
        app.apply_filter("LEVEL=ERROR".to_string());
        assert_eq!(
            app.filter_applied, "LEVEL=ERROR",
            "存的是原文不是规范化结果"
        );
    }

    /// 模态键盘门禁 (2026-09-14 用户实机): 设置卡开着、卡内下拉未持焦时
    /// 按 ↑↓, 卡底下的日志区滚动了。卡内控件经焦点路由消费、不经 `app.event`;
    /// 能到这里的都是无人认领的键, 除 Esc (关卡) 外一律吞掉。
    #[test]
    fn settings_modal_swallows_unhandled_keys() {
        let mut app = LogApp::new_empty();
        let lines: String = (0..100)
            .map(|i| format!("2026-09-14 12:00:{i:02} INFO line {i}\n"))
            .collect();
        let p = temp_log(lines.as_bytes());
        app.file = Arc::new(LogFile::open(&p).unwrap());
        app.has_file = true;
        app.settings_open = true;

        let key = |key: NamedKey| Event::Key {
            key: Key::Named(key),
            pressed: true,
            shift: false,
            ctrl: false,
            alt: false,
        };
        app.event(&key(NamedKey::ArrowDown));
        assert_eq!(app.top_row, 0.0, "设置卡开着: ↓ 不得滚动底层日志");
        app.event(&key(NamedKey::PageDown));
        assert_eq!(app.top_row, 0.0, "设置卡开着: PageDown 不得滚动底层日志");

        // Esc 不在吞键范围: 必须仍能关卡。
        app.event(&key(NamedKey::Escape));
        assert!(!app.settings_open, "Esc 必须仍能关闭设置卡");
        std::fs::remove_file(&p).ok();
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

    /// 追加换入: 计数由落点按「重叠一行」增量更新, 终值必须等于对新文件的全量重算。
    #[test]
    fn apply_appended_updates_counts_incrementally() {
        let mut app = LogApp::new_empty();
        let p1 =
            temp_log("2026-09-05 12:00:01 ERROR one\n2026-09-05 12:00:02 INFO two\n".as_bytes());
        let f1 = LogFile::open(&p1).unwrap();
        app.level_counts = Arc::new(levels::count_levels(&f1));
        app.file = Arc::new(f1);
        app.levels_pending = false;
        assert_eq!(app.level_counts.get(Level::Error), 1, "起点 1 条 ERROR");

        let p2 = temp_log(
            "2026-09-05 12:00:01 ERROR one\n2026-09-05 12:00:02 INFO two\n2026-09-05 12:00:03 ERROR three\n"
                .as_bytes(),
        );
        let f2 = LogFile::open(&p2).unwrap();
        app.apply_appended(f2, None);

        assert_eq!(
            *app.level_counts.as_ref(),
            levels::count_levels(&app.file),
            "增量更新 == 对新文件全量重算"
        );
        assert_eq!(app.level_counts.get(Level::Error), 2, "旧 1 + 新 1");
        assert_eq!(app.level_counts.total(), 3);
        std::fs::remove_file(&p1).ok();
        std::fs::remove_file(&p2).ok();
    }

    /// **计数未就绪时追加**: 不得重起作业 —— 一个持续增长的 tail 会把计数
    /// 一遍遍从头来过 (永远算不完, 侧栏永远挂在「…」); 也不得把旧快照的增量
    /// 贴到新文件上。正确做法是等作业交付时与快照对账 (`pickup_levels_job`)。
    #[test]
    fn apply_appended_while_pending_keeps_counts_pending() {
        let mut app = LogApp::new_empty();
        let p = temp_log(
            "2026-09-05 12:00:01 ERROR one
"
            .as_bytes(),
        );
        app.file = Arc::new(LogFile::open(&p).unwrap());
        app.levels_pending = true; // 模拟后台作业仍在算旧快照
        app.level_counts = Arc::new(LevelCounts::default());

        let f2 = LogFile::open(&p).unwrap();
        app.apply_appended(f2, None);
        assert!(app.levels_pending, "仍等原作业交付, 不得重起");
        assert_eq!(
            app.level_counts.total(),
            0,
            "不得把旧快照的增量贴到新文件上"
        );
        std::fs::remove_file(&p).ok();
    }

    /// 作业快照之后文件又长过 → 交付时按「重叠一行」与快照对账, 得到与**当前**
    /// 文件一致的计数 (既不重起作业, 也不交付一份过期的数)。
    #[test]
    fn pickup_levels_job_reconciles_lines_added_after_snapshot() {
        let mut app = LogApp::new_empty();
        let p = temp_log(
            "2026-09-05 12:00:01 ERROR one
"
            .as_bytes(),
        );
        app.file = Arc::new(LogFile::open(&p).unwrap());
        app.level_column = None;
        app.launch_levels_job(); // 快照 = 此刻的 1 行

        // 作业在算的同时文件又长了两行
        {
            let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
            f.write_all(
                b"2026-09-05 12:00:02 INFO two
2026-09-05 12:00:03 ERROR three
",
            )
            .unwrap();
        }
        app.file = Arc::new(LogFile::open(&p).unwrap());
        assert_eq!(app.file.line_count(), 3, "快照之后又长了两行");

        let deadline = Instant::now() + Duration::from_secs(5);
        while app.levels_pending {
            app.pickup_levels_job();
            assert!(Instant::now() < deadline, "计数作业 5s 未交卷");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            *app.level_counts.as_ref(),
            levels::counts_for(Arc::clone(&app.file), None).counts,
            "对账后 == 当前文件全量重算 (不是过期的那份)"
        );
        assert_eq!(app.level_counts.get(Level::Error), 2, "两条 ERROR 都在");
        assert_eq!(app.level_counts.total(), 3);
        std::fs::remove_file(&p).ok();
    }

    /// **R1 回归 (应用层)**: 旧快照末行无换行、被追加补全改判时, 计数与过滤必须
    /// **同时**覆盖那一行 —— 只改一侧就会让柱条数字与筛选结果分岔 (D2 红线)。
    #[test]
    fn apply_appended_covers_completed_half_line_on_both_sides() {
        let mut app = LogApp::new_empty();
        let p = temp_log("X\n2026-09-05 12:00:01 ".as_bytes());
        let f1 = LogFile::open(&p).unwrap();
        app.level_counts = Arc::new(levels::count_levels(&f1));
        app.file = Arc::new(f1);
        app.levels_pending = false;
        assert_eq!(app.level_counts.get(Level::Error), 0, "半行未成词");

        // 已应用的过滤 + 已建立(空)的过滤表, 模拟 tail 中途
        app.filter_applied = "ERROR".into();
        app.filtered = Some(Arc::new(Vec::new()));

        {
            let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
            f.write_all(b"ERROR disk\n").unwrap();
        }
        let f2 = LogFile::open(&p).unwrap();
        assert_eq!(f2.line_count(), 2, "补全半行不增行数");
        app.apply_appended(f2, None);

        assert_eq!(
            app.level_counts.get(Level::Error),
            1,
            "被补全的半行必须改判"
        );
        assert_eq!(app.level_counts.get(Level::Other), 1, "只剩开头那行 X");
        let shown = app.filtered.as_ref().expect("过滤表仍在");
        assert_eq!(
            shown.len() as u64,
            app.level_counts.get(Level::Error),
            "D2 红线: 筛出行数必须 == 柱条数字"
        );
        assert_eq!(
            shown.as_slice(),
            [1],
            "第 1 行 (0-based) 是那条补全的 ERROR"
        );
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

    /// **本次事故的回归 (2026-09-12)**: 打开**不得**等计数。
    ///
    /// 事由: 计数原先与文件同批交付, 而字段口径的 `extract_field` 要在整行里找
    /// `"level":`, 成本随行内容走 —— 用户实机打开 1GB JSONL 时, 状态栏写
    /// 「索引 92ms」却等了十几秒 (那十几秒全在 worker 里数级别)。
    /// 修法: 打开只交出口径列名, 计数交独立后台作业, 侧栏随后补入。
    ///
    /// 这条钉住三件事: ① 落地后侧栏处于「未就绪」而非拿 0 冒充; ② 此时只读
    /// (不得拿空子句表去点); ③ 作业交付后计数等于全量重算。
    #[test]
    fn apply_fresh_does_not_block_on_level_counting() {
        let mut app = LogApp::new_empty();
        let p = temp_log(
            "{\"level\":\"ERROR\",\"m\":\"a\"}\n{\"level\":\"INFO\",\"m\":\"b\"}\n".as_bytes(),
        );
        let f = LogFile::open(&p).unwrap();
        let out = OpenOutcome {
            file: f,
            schema: jsonl::discover_schema(&LogFile::open(&p).unwrap()),
            incremental_hits: None,
            rebuilt: false,
            level_column: Some("level".into()),
        };
        app.apply_fresh(p.clone(), out);

        // ① 落地即返回: 计数未就绪
        assert!(app.levels_pending, "打开不得等计数 —— 落地时计数必未就绪");
        assert_eq!(app.level_counts.total(), 0, "不得拿 0 冒充真实计数");
        // ② 未就绪期间侧栏只读 (空子句表), 点不到任何一行
        assert!(
            app.level_queries.iter().all(Option::is_none),
            "计数未就绪 → 侧栏只读"
        );

        // ③ 等后台作业交付
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.levels_pending {
            app.pickup_levels_job();
            assert!(Instant::now() < deadline, "计数作业 5s 未交卷 (悬挂?)");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            *app.level_counts.as_ref(),
            levels::counts_for(Arc::clone(&app.file), Some("level")).counts,
            "交付的计数 == 全量重算"
        );
        assert_eq!(app.level_counts.get(Level::Error), 1);
        assert_eq!(app.level_counts.get(Level::Info), 1);
        assert_eq!(
            app.level_queries[Level::Error as usize].as_deref(),
            Some("level=ERROR*"),
            "就绪后子句表随口径一起到位"
        );
        std::fs::remove_file(&p).ok();
    }

    /// **打开文件后必须有人持焦** (review 轮补的第四条): T14 把「选中行高亮」改成
    /// 只在 `LogView` 持焦时才画 (`visible_selection` 的焦点门禁), 而 `Ctrl+O`
    /// 打开文件后**没有任何控件**持焦 —— 于是打开 1GB 日志看到的是「一行都没选中」,
    /// 按 ↑↓ 只动底栏行号、屏上什么都不动: 正是本模块判据① (按了有没有立刻的
    /// 变化) 要消灭的那一类。修法是 `apply_fresh` 尾部把焦点送进列表。
    ///
    /// **两半都要钉**: Fresh 送、append 不送。后者若也送, 后台追长 / 轮转会把正在
    /// 过滤栏里打字的用户当场拽走 (焦点一挪, 接下来敲的字就不进栏了)。
    #[test]
    fn fresh_open_hands_focus_to_the_list_but_append_does_not() {
        let mut app = LogApp::new_empty();
        let p = temp_log("2026-09-05 12:00:01 ERROR one\n".as_bytes());
        let out = OpenOutcome {
            file: LogFile::open(&p).unwrap(),
            schema: None,
            incremental_hits: None,
            rebuilt: false,
            level_column: None,
        };

        // 打开文件 (Fresh): 焦点必须送进列表
        app.focus_target = Some("log-bar"); // 假装用户正停在过滤栏
        app.apply_fresh(p.clone(), out);
        assert_eq!(
            app.focus_target,
            Some("log-view"),
            "打开文件后须把焦点送进列表 —— T14 起高亮只在持焦时画, 不送就是「一行没选中」"
        );

        // 对照: **追加**不得动焦点 (用户可能正在栏里打字)
        app.focus_target = Some("log-bar");
        app.level_counts = Arc::new(levels::count_levels(&app.file));
        app.levels_pending = false;
        let p2 =
            temp_log("2026-09-05 12:00:01 ERROR one\n2026-09-05 12:00:02 INFO two\n".as_bytes());
        app.apply_appended(LogFile::open(&p2).unwrap(), None);
        assert_eq!(
            app.focus_target,
            Some("log-bar"),
            "追长/轮转不得抢焦点 —— 否则正在过滤栏里打的字当场丢失"
        );

        std::fs::remove_file(&p).ok();
        std::fs::remove_file(&p2).ok();
    }

    /// 轮转/重建: 计数重新后台算, 且列名与子句表跟着换 —— JSONL 变明文后必须
    /// 降级只读, 不能留着旧列名的子句去点 (会筛出 0 行)。
    #[test]
    fn apply_rebuild_switches_column_and_recounts() {
        let mut app = LogApp::new_empty();
        let p1 = temp_log(
            "{\"level\":\"ERROR\",\"m\":\"a\"}\n{\"level\":\"INFO\",\"m\":\"b\"}\n".as_bytes(),
        );
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

        // 轮转后内容变明文 → 口径列随之作废
        let p2 = temp_log("2026-09-05 ERROR plain one\n2026-09-05 WARN plain two\n".as_bytes());
        let out = OpenOutcome {
            file: LogFile::open(&p2).unwrap(),
            schema: None,
            incremental_hits: None,
            rebuilt: true,
            level_column: None,
        };
        app.apply_rebuild(&p2, out);

        assert!(app.levels_pending, "重建后计数重新后台算");
        assert!(app.level_column.is_none(), "列名换掉, 不沿用旧的");
        assert!(
            app.level_queries.iter().all(Option::is_none),
            "明文 → 子句表清空 (降级只读)"
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        while app.levels_pending {
            app.pickup_levels_job();
            assert!(Instant::now() < deadline, "计数作业 5s 未交卷");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            *app.level_counts.as_ref(),
            levels::counts_for(Arc::clone(&app.file), None).counts,
            "重建后计数 == 新文件全量重算"
        );
        assert_eq!(app.level_counts.get(Level::Error), 1);
        assert_eq!(app.level_counts.get(Level::Warn), 1);
        assert_eq!(app.level_counts.total(), 2);
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
        let out = OpenOutcome {
            file: f,
            schema: None,
            incremental_hits: None,
            rebuilt: false,
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

    // ---- M3 (T12): 九条沉默接入的回归锁 ----
    //
    // 公共判据是「触发后**出声**且**说得对**」: 只测「有提示」会放过提示写错原因
    // 的形态, 只测「返回 Consumed/Ignored」则完全测不出这一批改动 (M3 一律不动
    // 归因, 只加原因)。

    /// 敲一个无修饰键。
    fn press(app: &mut LogApp, key: Key) {
        app.event(&Event::Key {
            key,
            pressed: true,
            shift: false,
            ctrl: false,
            alt: false,
        });
    }

    /// 敲一个 Ctrl+字母。
    fn press_ctrl(app: &mut LogApp, ch: &str) {
        app.event(&Event::Key {
            key: Key::Character(ch.to_string()),
            pressed: true,
            shift: false,
            ctrl: true,
            alt: false,
        });
    }

    /// 造一个开了真文件 (内容自定) 的 app, 交给 `f` 跑, 收尾删文件。
    /// 走 `has_file` 门禁**之后**的键处理路径 —— 空态会先被 P26 那条拦下。
    fn with_file<T>(content: &[u8], f: impl FnOnce(&mut LogApp) -> T) -> T {
        let p = temp_log(content);
        let mut app = LogApp::new_empty();
        app.file = Arc::new(LogFile::open(&p).unwrap());
        app.has_file = true;
        let out = f(&mut app);
        std::fs::remove_file(&p).ok();
        out
    }

    /// 取当前 notice 文本; 没有则连底栏一起报出来 (方便定位是「没出声」还是
    /// 「声出到了别处」)。
    fn notice_of(app: &LogApp) -> String {
        app.notice
            .as_ref()
            .map(|(t, _)| t.clone())
            .unwrap_or_else(|| panic!("须有一条 notice; 当前底栏: {}", app.status))
    }

    /// 敲一个 Ctrl+字符键事件 (不经过 app, 供 `app_key_filter` 直调)。
    fn ctrl_key(ch: &str) -> Event {
        Event::Key {
            key: Key::Character(ch.to_string()),
            pressed: true,
            shift: false,
            ctrl: true,
            alt: false,
        }
    }

    /// T17 验收 ③: 拖滚动条**不改选中行** —— 抓条是「看」不是「选」。
    ///
    /// 对照在同一支测试里: 方向键那条路**会**动选中 (既有行为, 本项不动它)。
    /// 有对照才说明这条不变量是**有意**分开的, 不是碰巧没动。
    #[test]
    fn scroll_to_does_not_move_the_selection() {
        let body: String = (0..300).map(|i| format!("line {i}\n")).collect();
        let (mut app, p) = {
            let p = temp_log(body.as_bytes());
            let mut app = LogApp::new_empty();
            app.file = Arc::new(LogFile::open(&p).unwrap());
            app.has_file = true;
            (app, p)
        };
        app.selected = 7;

        app.update(Msg::ScrollTo {
            top: 120.0,
            at_bottom: false,
        });
        assert_eq!(app.top_row, 120.0, "绝对定位须原样落到 top_row");
        assert_eq!(app.selected, 7, "抓滚动条不得动选中行");

        app.update(Msg::ScrollRows(1.0));
        assert_eq!(app.selected, 121, "对照: 滚轮/方向键那条路仍让选中跟随首行");
        std::fs::remove_file(&p).ok();
    }

    /// T17 回归锁: 在底部**碰一下滚动条**不得静默脱掉 FOLLOW。
    ///
    /// 跟随态下 app 的 `top_row` 是 `count-1`, 而滚动条能表达的最大值是
    /// `count-可见行数` —— 两个口径差着 `可见行数-1` 行。原先直接比
    /// `top < top_row`, 于是「在底部往下拖」也被判成「向上看」, ` · FOLLOW`
    /// 悄悄从底栏消失。修法: 落到条底 (`at_bottom`) 时不参与这个判断、
    /// 往回拖仍照旧。**A/B 实证**: 去掉 `!at_bottom` 那一项, 本测试第一段必红。
    #[test]
    fn touching_the_scroll_bar_at_the_bottom_keeps_follow() {
        let body: String = (0..300).map(|i| format!("line {i}\n")).collect();
        let p = temp_log(body.as_bytes());
        let mut app = LogApp::new_empty();
        app.file = Arc::new(LogFile::open(&p).unwrap());
        app.has_file = true;

        // 条能表达的最底位比 app 的 `max_top()` 小 —— 正是当年误判的那一格
        let bar_bottom = app.max_top() - 10.0;
        app.follow = true;
        app.top_row = app.max_top();
        app.update(Msg::ScrollTo {
            top: bar_bottom,
            at_bottom: true,
        });
        assert!(app.follow, "拖到条底不该脱跟随 (用户没在往回看)");

        // 对照: 真往回拖 (没到条底) 仍然脱跟随。**必须把 top_row 放回跟随位**
        // —— 上一条已经把 top_row 拉到了 bar_bottom, 不放回去这条就不是「往回拖」。
        app.follow = true;
        app.top_row = app.max_top();
        app.update(Msg::ScrollTo {
            top: bar_bottom,
            at_bottom: false,
        });
        assert!(!app.follow, "往回拖须停止跟随");
        std::fs::remove_file(&p).ok();
    }

    /// P30 (T17): **未认领的滚轮**转给列表滚动 —— 指针停在侧栏/过滤栏上时,
    /// 日志区收不到滚轮 (框架按点子命中分发、不向父级回落), 原先必须把指针挪回
    /// 内容区才滚得动。
    ///
    /// 判据取**效果** (top_row 动了) 而不是「有没有走那个分支」。
    #[test]
    fn unclaimed_wheel_scrolls_the_list() {
        let body: String = (0..300).map(|i| format!("line {i}\n")).collect();
        let p = temp_log(body.as_bytes());
        let mut app = LogApp::new_empty();
        app.file = Arc::new(LogFile::open(&p).unwrap());
        app.has_file = true;

        let wheel = |dy: f32| Event::MouseWheel {
            delta: (0.0, dy),
            position: danqing::Point::new(10.0, 400.0), // 侧栏上
            shift: false,
            ctrl: false,
            alt: false,
        };
        app.event(&wheel(-1.0)); // 向下滚 (与 view 同向: delta.1 < 0 = 向下)
        assert!(app.top_row > 0.0, "侧栏上的滚轮须滚得动列表");

        // 模态不穿透: 卡开着时滚轮只属于卡
        let before = app.top_row;
        app.settings_open = true;
        app.event(&wheel(-1.0));
        assert_eq!(app.top_row, before, "设置卡开着时滚轮不得滚卡后的日志");
        std::fs::remove_file(&p).ok();
    }

    /// T17: 滚轮换算**单点** —— 内容区与「未认领」那一路必须是同一个手感。
    ///
    /// 顺带钉住上界: 框架把 `LineDelta`(行) 与 `PixelDelta`(像素) 抹平成同一个
    /// `f32` (G10), 触控板一次给 ±100 时若不夹, 一滚就跳几百行。
    #[test]
    fn wheel_rows_is_clamped_and_sign_flipped() {
        assert!(wheel_rows(-1.0) > 0.0, "与 danqing Scrollable 同向");
        assert_eq!(wheel_rows(0.0), 0.0);
        // 钉**值**而不是钉「有夹子」: 写 `<= WHEEL_MAX_ROWS` 的话, 把常量从 12
        // 改成 50 它照样绿 —— 那就不叫守卫了。
        assert_eq!(wheel_rows(-100.0), WHEEL_MAX_ROWS, "像素档夹到上界");
        assert_eq!(wheel_rows(100.0), -WHEEL_MAX_ROWS);
    }

    /// T18 (P17): notice 自带消退期限 —— 到点清掉, 不留常驻噪声。
    ///
    /// 不真等 4 秒: 把期限拨到过去, 语义等价。
    #[test]
    fn notice_expires_when_its_deadline_passes() {
        let mut app = LogApp::new_empty();
        app.set_notice("已复制该行".into(), NoticeKind::Info);
        assert!(app.notice.is_some());
        app.expire_notice();
        assert!(app.notice.is_some(), "未到点不得消退");

        app.notice_until = Some(Instant::now() - Duration::from_millis(1));
        app.expire_notice();
        assert!(app.notice.is_none(), "到点须消退");
        assert!(app.notice_until.is_none(), "期限也要一并清掉");
    }

    /// T20 (P37): 侧栏开关与 Ctrl+L 是**同一份状态** —— 两条入口同发一条消息。
    ///
    /// 「双向同步」不是两处赋值互相对, 而是只有一份真相: 开关读
    /// `app.histogram_visible`、Ctrl+L 与开关都发 `Msg::ToggleHistogram`。
    ///
    /// **配置路径必须显式给临时文件**: 这条消息会 `save_config()`, 而默认路径是
    /// 用户真实的 `config.toml` (整文件覆盖写)。本测试第一版就是这么写的, 被
    /// review 抓出来 —— 现在 `save_config` 在测试里拿不到路径会直接 panic。
    #[test]
    fn histogram_toggle_is_one_state_for_both_entries() {
        let p = temp_log(b"");
        let mut app = LogApp::new_empty_at(Some(p.clone()));
        let before = app.histogram_visible;
        app.update(Msg::ToggleHistogram);
        assert_eq!(app.histogram_visible, !before, "两条入口共用的那一支须翻转");
        app.update(Msg::ToggleHistogram);
        assert_eq!(app.histogram_visible, before, "再切一次回到原状");
        // 落盘也走的是**同一个**路径 (整文件同源, 不碰用户真配置)
        assert_eq!(
            config::Config::load_from(&p).histogram,
            before,
            "切换须落盘到注入的路径"
        );
        std::fs::remove_file(&p).ok();
    }

    /// T21 (P39): Ctrl+F **不清草稿** —— 它是「回到搜索框」的反射键,
    /// 而原实现每次都销毁用户已输入未应用的内容。
    ///
    /// 这里锁的是**应用侧那一半** (不再发清空信号、改发重聚焦信号);
    /// 「全选」那一半在 `view.rs::refocus_selects_the_existing_draft`。
    #[test]
    fn reopen_search_keeps_the_draft() {
        let mut app = LogApp::new_empty();
        let clear0 = app.search_clear_rev;
        let refocus0 = app.search_refocus_rev;

        app.open_search();
        assert_eq!(app.search_clear_rev, clear0, "Ctrl+F 不得再触发清空");
        assert_eq!(app.search_refocus_rev, refocus0 + 1, "改为请栏处理重聚焦");
        assert_eq!(app.focus_target, Some("log-bar"), "仍要把焦点送进栏");

        // 对照: Esc 那条路**仍然**清空 (本项只动「回到搜索框」这一条)
        app.clear_search();
        assert_eq!(app.search_clear_rev, clear0 + 1, "Esc 仍须清空");
    }

    /// T16 (P32) 回归锁: 设置卡开着时, 全局键**不得穿透到卡后**。
    ///
    /// 逐条断言**卡后副作用没发生**, 而不是断言返回值是不是某个 `Msg` ——
    /// 「不穿透」的实现可以是吞掉、也可以是别的, 判据应该绑在**后果**上。
    ///
    /// **Ctrl+O 不在表内 (有意)**: 它的穿透后果是弹一个**阻塞的原生文件对话框**,
    /// 一旦回归, 这条测试不是红而是**挂住** —— 一个会挂死的守卫比没有守卫更坏。
    /// 它的门禁与表内三条是同一个 `if`, 位置对了三条就都对了; 实机那一半由
    /// 矩阵 §6 的「卡开着按 Ctrl+O」(P32) 覆盖。
    #[test]
    fn settings_modal_does_not_leak_global_keys_behind_the_card() {
        let mut app = LogApp::new_empty();
        app.settings_open = true;
        for (name, ch) in [("Ctrl+F", "f"), ("Ctrl+T", "t"), ("Ctrl+L", "l")] {
            let got = app.app_key_filter(&ctrl_key(ch));
            let leaked = match got {
                Some(Msg::FocusSearch) => "把焦点送到了卡后",
                Some(Msg::ToggleMode) => "切了卡后的模式",
                Some(Msg::ToggleHistogram) => "动了卡后的侧栏",
                Some(Msg::OpenFile(_)) => "弹了文件对话框",
                _ => "",
            };
            assert!(leaked.is_empty(), "{name} 穿透了设置卡: {leaked}");
        }
        // **反向对照 —— 本测试第一版缺的就是这半边, 于是漏掉了一个 Critical**:
        // 卡内控件 (主题下拉 / 侧栏开关 / 关闭钮) 全靠**焦点分发**收键, 而框架在
        // `app_key_filter` 返回 `Some` 时直接 return、不再分发 (`handler.rs:436-441`)。
        // 所以非全局键**必须**放行 (`None`), 否则卡内键盘全死 —— 而上一段
        // 「不该发生的副作用没发生」是**看不出**这一点的 (吞得越多它越绿)。
        let plain = |k: Key| Event::Key {
            key: k,
            pressed: true,
            shift: false,
            ctrl: false,
            alt: false,
        };
        for (name, ev) in [
            ("↓ (下拉导航)", plain(Key::Named(NamedKey::ArrowDown))),
            (
                "Enter (下拉选中 / 关闭钮)",
                plain(Key::Named(NamedKey::Enter)),
            ),
            ("Space (侧栏开关)", plain(Key::Named(NamedKey::Space))),
            ("普通字符 (打字)", plain(Key::Character("x".into()))),
        ] {
            assert!(
                app.app_key_filter(&ev).is_none(),
                "{name} 必须放行给卡内控件 —— 守卫吞了它, 卡内键盘就死了"
            );
        }
        // 这三条由 `LogApp::event` 那条路认领, 那里有同款门禁 —— 一并锁住
        app.event(&ctrl_key("b"));
        app.event(&ctrl_key("g"));
        assert!(app.bookmarks.is_empty(), "Ctrl+B 不得在卡后加书签");
        // Esc 仍须能关卡 (既有行为保持, 不被守卫吃掉)
        assert!(
            matches!(
                app.app_key_filter(&Event::Key {
                    key: Key::Named(NamedKey::Escape),
                    pressed: true,
                    shift: false,
                    ctrl: false,
                    alt: false,
                }),
                Some(Msg::CloseSettings)
            ),
            "Esc 必须仍能关闭设置卡"
        );
    }

    /// T11 回归锁: notice 是**第二条通道**, 不得拼进 `status` 串。
    ///
    /// 曾把它拼进去, 而 `LogView::paint` 又单独画一遍 `notice` —— 同一句话在底栏
    /// 出现**两次**。而两遍还是同色的, 第一眼只像「重复」不像「出错」, 所以这条
    /// 必须由守卫而不是靠眼睛。
    #[test]
    fn notice_is_not_folded_into_the_status_line() {
        let mut app = LogApp::new_empty();
        app.refresh_status();
        app.notice = Some(("此处无行".into(), NoticeKind::Info));
        app.refresh_status();
        assert!(app.notice.is_some(), "前提: notice 还在");
        assert!(
            !app.status.contains("此处无行"),
            "notice 不得拼进 status (会被画两遍): {}",
            app.status
        );
        // 反向对照: 常态信息该在的仍在 (别把整条底栏一起删了)
        assert!(!app.status.is_empty(), "常态信息不得一起被删掉");
    }

    /// T13 —— 「被吞掉的输入必有原因」: **本表就是规则的挂载点**。
    ///
    /// M3 之前本仓只有一处出声 (正则无效进底栏), 其余静默。这条把那个孤例**升格
    /// 为规则**: 再遇到「按了没反应」, 要做的不是「记得去加提示」, 而是**往这张表
    /// 加一行** —— 行在, 判据在, 原因就漏不掉。
    ///
    /// 每行给的是**期望的原因片段**, 不是「有提示就算」: 后者会放过原因写错的形态。
    /// 本表只收**键盘路径**上的吞键; 鼠标路径各有同构的守卫
    /// (`view.rs::click_below_the_last_row_says_there_is_no_row` /
    /// `expand_glyph_on_a_leaf_row_says_why_instead_of_toggling` /
    /// `histogram.rs::readonly_sidebar_swallows_clicks_but_says_why`)。
    #[test]
    fn every_swallowed_key_says_why() {
        type Run = Box<dyn Fn() -> String>;
        let rows: Vec<(&str, &str, Run)> = vec![
            (
                "P26 空态按键",
                "尚未打开文件",
                Box::new(|| {
                    let mut app = LogApp::new_empty();
                    assert!(!app.has_file);
                    press(&mut app, Key::Named(NamedKey::ArrowDown));
                    notice_of(&app)
                }),
            ),
            (
                "P23 Ctrl+T 无 JSONL",
                "非 JSONL",
                Box::new(|| {
                    with_file(b"2026-09-14 12:00:00 INFO ready\n", |app| {
                        assert!(app.schema.is_none(), "前提: 无 JSONL schema");
                        press_ctrl(app, "t");
                        notice_of(app)
                    })
                }),
            ),
            (
                "P24 ← 落在未展开行",
                "未展开",
                Box::new(|| {
                    with_file(b"{\"a\":{\"b\":1}}\nplain\n", |app| {
                        app.mode = ViewMode::Table;
                        press(app, Key::Named(NamedKey::ArrowLeft));
                        notice_of(app)
                    })
                }),
            ),
            (
                "P24 → 落在已展开行",
                "已展开",
                Box::new(|| {
                    with_file(b"{\"a\":{\"b\":1}}\nplain\n", |app| {
                        app.mode = ViewMode::Table;
                        app.toggle_expand(0); // 真展开 (行 0 含嵌套)
                        assert!(app.expanded.is_expanded(0), "前提: 行 0 已展开");
                        press(app, Key::Named(NamedKey::ArrowRight));
                        notice_of(app)
                    })
                }),
            ),
            (
                "P25 Ctrl+G 无书签",
                "尚无书签",
                Box::new(|| {
                    with_file(b"2026-09-14 12:00:00 INFO ready\n", |app| {
                        assert!(app.bookmarks.is_empty(), "前提: 无书签");
                        press_ctrl(app, "g");
                        notice_of(app)
                    })
                }),
            ),
            (
                // 口径须与 `ROADMAP-v1x.md` §四 同源 —— 写成「暂不支持」之类
                // 就又是一处各说各话, 故断言片段锁在 "v1.x" 上。
                "P34 Ctrl+A 被吞",
                "v1.x",
                Box::new(|| {
                    with_file(b"2026-09-14 12:00:00 INFO ready\n", |app| {
                        press_ctrl(app, "a");
                        notice_of(app)
                    })
                }),
            ),
        ];
        for (name, expect, run) in &rows {
            let text = run();
            assert!(
                text.contains(expect),
                "{name}: 被吞掉的输入须说出「{expect}」, 实得「{text}」"
            );
        }
    }
}
