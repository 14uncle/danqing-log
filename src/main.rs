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

mod analysis_panel;
mod app_export;
mod app_filter;
mod app_license;
mod app_merge;
mod app_open;
mod app_persist;
mod app_status;
mod app_update;

pub(crate) use app_filter::{build_clause, build_search_pattern, clause_reject_notice};

/// 测试专用 re-export (仅 tests.rs 经根路径用; 生产侧无调用点, cfg 隔离免 unused 警告)
#[cfg(test)]
pub(crate) use app_filter::next_bookmark;
pub(crate) use app_status::status_text;
mod config;
mod histogram;
mod pick_list;
mod settings;
mod sidebar;
mod store_license;
mod toast;
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
use danqing_log::export::{self, ExportEnd, ExportFormat, ExportJob, ExportPick, ExportSet};
use danqing_log::jsonl::{self, Schema, SubRow};
use danqing_log::levels::{self, Level, LevelCounts, LevelQueries};
use danqing_log::license::{self, Entitlement, Feature, PaidSource};
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

/// **合并态**增量合流的同步闸 (T9 实测定，比字节闸更紧): 单文件侧的 32 MiB
/// 口径只按**文件行索引**的成本定 («2.4GB/s»), 不含合并侧的增量合流 —— 而
/// 后者的代价 ∝ 批行数 + 回找深度 (索引 ×16B 的内存位移)。合并态超此批行数
/// 即交 worker 全量重归并 (旧 bundle 保持可见), 不赌 UI 线程。
/// 常态 live-tail (每轮几十~几千行) 远在闸下，走同步快路。
const MERGE_SYNC_MAX_ROWS: u64 = 20_000;

/// 空态底栏提示 (无参启动，未打开文件时)。
const EMPTY_STATUS: &str = "未打开文件 · 按 Ctrl+O 打开日志文件";

/// 视图模式 (仅 JSONL 检出后可进表格; Ctrl+T 互切)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewMode {
    /// 原始文本 (级别着色)。
    Raw,
    /// JSONL 列化表格 (前提②)。
    Table,
}

/// 工作区模式 (merge-timeline 腿一 T3, SPEC-v1x-merge-timeline D4):
/// Single = 单文件 (v1.0 起既有全部行为); Merge = 多源合并时间线。
/// 与 ViewMode (Raw|Table 显示模式) 正交 —— 合并视图有自己的三栏形态，
/// 合并期间 Single 的 mode/schema/filtered 等字段**保持原值不触碰**,
/// 退出合并即原样回来 (互不丢状态)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Workspace {
    /// 单文件。
    Single,
    /// 合并时间线。
    Merge,
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
/// `#0F766E`; `base` 也手抄成 `0.96` 灰，与主题真实的 `#F0F8F6` 差一截。
///
/// `backdrop_light` / `backdrop_dark` **保留手写**: 框架没有对应 token ——
/// 它们是场景层的前后景渐变端点，只服务标题栏这一层场景。
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

/// 窗口清屏色 —— **单点定义，启动与切主题都取它**。
///
/// 为什么必须是单点：清屏色有两条来路 (启动的 `WindowConfig`、运行时的
/// `set_clear_color`), 各算一份就会漂 —— 本仓已有先例 (设置卡页签序号曾在两个文件
/// 各抄一份、双双漂掉)。
///
/// 为什么它值得存在：标题栏那条亮带**就是**清屏色。框架 `TitleBar` 的背景是有意的
/// `TRANSPARENT` (`danqing/src/widget/title_bar.rs`, 且有测试锁死), 让窗口底色透出;
/// 内容区反而看不见它 (被不透明的 `th.background()` 盖住)。所以清屏色不跟随主题时，
/// 症状恰好是「暗色下标题栏一条白板」。
fn window_clear_color(theme: config::AppTheme) -> Color {
    theme.theme().background()
}

/// 标题栏 (含嵌入的过滤栏)。
///
/// 抽成函数不只为了整洁 —— **测试必须复用同一份构建代码**才守得住下面这个坑：
/// `view()` 只在启动时求值一次 (`danqing/src/window/mod.rs:207` 的
/// `let tree = app.view();`, 之后整棵交给 Handler, 不再重建), 所以
/// `TitleBar::themed(&title_theme(..))` 烘进去的是**启动那一刻**的主题色。
/// 卡面上其它控件走 `bind_color` 闭包、每帧重读，于是切主题时**只有标题栏停在旧色**
/// —— 底色换了、文字没换：切到浅色是浅字压浅底，切到暗色是暗字压暗底
/// (用户实机报的「标题看不清」, 两个方向都成立)。
///
/// `bind_theme` 是框架**专为这件事**准备的 API (见 `TitleBar` 文档，
/// 「每帧从应用状态重取主题」)。**不许删**。
/// 参数取**值**而非 `&LogApp`: edition 2024 里 `impl Trait` 会捕获全部输入生命周期，
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
    /// 是否已打开真实文件 (false = 无参启动空态占位：轮询/键盘导航全门禁，
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
    /// 这一行 `status` 是不是**错误** (P27)。只由 [`Self::set_status_error`] 置位，
    /// 由 [`Self::set_status`] / [`Self::refresh_status`] 清除 —— **单一写入点**,
    /// 别处直接 `self.status = …` 会让标志与实际内容脱钩。
    status_error: bool,
    mode: ViewMode,
    /// JSONL 列定义 (检出才有; Ctrl+T 切换的前置条件)。
    schema: Option<Arc<Schema>>,
    /// 列配置真身 (SPEC-v1x-table-column-config D1): 用户列宽/显隐/列序,
    /// 换文件 (`apply_fresh`) 载入该路径记忆，变更即落 `state.json` (D4)。
    columns: danqing_log::columns::ColumnConfig,
    /// 全文件级别计数 (level-histogram 侧栏)。随文件同批换入 —— worker 算好
    /// 与 file 一起交卷，故不存在「行数已更新、计数还是旧的」窗口。
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
    /// 打开路径上最重的一段，挡在内容显示之前就是「索引 92ms 却等十几秒」。
    /// 现在打开只交出口径列名，计数由这里的作业后台完成，侧栏随后补入。
    levels_job: AsyncJob<levels::LevelsOutcome>,
    /// 计数是否仍在算 —— 侧栏据此显示「计算中」而非把 0 当数读。
    levels_pending: bool,
    /// 过滤命中的文件行号 (升序); None = 全量。
    filtered: Option<Arc<Vec<u64>>>,
    /// 已应用的过滤查询。
    filter_applied: String,
    /// **落账**过滤查询 —— 产出当前 `filtered` 行集的那一串。
    ///
    /// 与 `filter_applied` 的差异只在「作业在途」窗口：`apply_filter` 发起时
    /// 即写 `filter_applied`, 而行集要等 job 拾取才换 (评审 R5)。窗口内两者
    /// 脱钩，凡消费「行集 ↔ 过滤串」对应关系的 (分析快照) 必须读本字段;
    /// 增量过滤合并在本窗口内直接禁行 (全程重跑会覆盖，合并是白干 + 错账)。
    filter_landed: String,
    /// 过滤作业在途 (发起未拾取)。见上。
    filter_pending: bool,
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
    /// 高亮只在持焦时画，否则打开文件看到的是「一行都没选中」, 而按 ↑↓ 只动底栏
    /// 行号、屏上什么都不动：本模块自己判据里的「按了没反应」)。
    focus_target: Option<&'static str>,
    /// 已应用搜索的导航态 (命中表 + 当前位置)。
    search: Option<SearchNav>,
    search_query: String,
    /// 已应用的正则模式 (view 编译高亮用; 与 search 同生同灭)。
    search_pattern: Option<String>,
    search_elapsed: Option<Duration>,
    search_job: AsyncJob<SearchOutcome>,
    /// 书签：文件行号集合 (per-路径持久化 + 越界剔除，SPEC-v1x-bookmark-persist D1/D2)。
    bookmarks: std::collections::BTreeSet<u64>,
    /// 展开态 (jsonl-table T4): 文件行号 → 子行数。
    expanded: ExpandMap,
    /// 展开行的拍平子行 (渲染用; 与 expanded 同生同灭，惰性 parse)。
    sub_rows: std::collections::BTreeMap<u64, Vec<SubRow>>,
    /// 展开态修订号 (M3): `toggle_expand` 每次实际改动 +1; LogView 据它
    /// 作废旧选区/单元格选中 —— 展开/折叠改变显示行映射, 旧 (显示行，偏移)
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
    /// **改它一律走 [`Self::set_notice`]**, 不直接赋值 —— 直接赋值会漏掉消退期限，
    /// 那条提示就永远赖在底栏上 (T18/Q3 之前是 8 处各写各的)。
    notice: Option<(String, NoticeKind)>,
    /// notice 的消退时刻; `None` = 当前无提示。见 [`Self::set_notice`]。
    notice_until: Option<Instant>,
    /// 窗口是否已最大化 (TitleBar::bind_maximized 读; 框架 Handler 经 maximized_changed 写)。
    maximized: bool,
    /// 在途打开作业 (async-open): Some = 打开/重建/追平进行中, UI 全程可响应;
    /// 取消 = 置 None (worker 持 cancel Arc 早退，见 open.rs drop 语义)。
    open_job: Option<OpenJob>,
    /// Loading 占位文案 (仅 无旧文件 + job 在途 时 Some; view 空态分支呈现)。
    loading_label: Option<(String, String)>,
    /// 设置卡是否打开 (S2)。
    settings_open: bool,
    /// 主题模式 (浅色/深色)。
    theme: config::AppTheme,
    /// 级别计数侧栏是否显示 (`Ctrl+L` 切换，落 config.toml)。
    histogram_visible: bool,
    /// 配置读写路径。`None` = 用户真实配置 (`%APPDATA%\danqing-log\config.toml`)。
    ///
    /// **测试必须给临时路径** —— 见 [`Self::save_config`] 里那条 `#[cfg(test)]` 的
    /// 硬拦。这不是洁癖：`save_to` 是**整文件覆盖写**, 而 `load_from` 对认不出的
    /// `mode` 会取值域默认 (light) —— 一次 `cargo test` 就能把用户的主题**改掉**,
    /// 并抹掉手写注释。
    cfg_path: Option<std::path::PathBuf>,
    /// 设置卡当前页签**下标** —— 序号含义见 `settings.rs` 里 `.tab()` 处 (**唯一真身**,
    /// 别在这里另列一份，加页签时会漂)。越界值无需在此防御：框架 `Tabs` 自行钳制
    /// (`clamp_active`), 且 `on_change` 只会回传合法下标。
    /// 留在应用状态里：重开卡片停在上次那页。
    settings_tab: usize,
    /// 更新角标谓词的测试注入位 (`Some` = 覆写; 生产恒 `None`, 闭包现查
    /// `app_update::hint()`)。测试两态注入不碰全局 publish —— 构造注入惯例
    /// 在「每帧查全局」场景下的形态 (SPEC-update-hint-ui 腿 A)。
    update_hint_override: std::cell::Cell<Option<bool>>,
    /// 授权状态 (SPEC-v1x-licensing)。启动时判定一次 (D4: 失效不踢会话内的人),
    /// 激活动作即时翻转。免费层下全功能行为与 v1.0 逐点一致 (暗发)。
    entitlement: Entitlement,
    /// 验签公钥。生产 = `license::PRODUCT_PUBKEY` 常量; 测试可注入 (公钥占位
    /// 全零时任何 key 都验不过，没有注入就没法测激活路径)。
    license_pubkey: [u8; 32],
    /// 商店版启动授权查询 (T5; AsyncJob 模式与 search/levels 同源)。
    store_license_job: AsyncJob<Entitlement>,
    /// 商店版购买流程 (T5)。
    purchase_job: AsyncJob<store_license::PurchaseOutcome>,
    /// 「许可」页 key 输入框的内容镜像 (widget 自持编辑器，这里随 on_change 同步)。
    license_key_input: String,
    /// 激活结果反馈 (显示在「许可」页内 —— 底栏 notice 会被模态卡遮住，看不见)。
    license_feedback: Option<(String, NoticeKind)>,
    /// 统一升级提示 (T7): 免费用户触发付费功能时弹出，值为被拦的功能。
    upgrade_prompt: Option<Feature>,
    /// 商店购买是否在途 (评审 Required: pomodoro 成稿的防重入移植 ——
    /// 在途时忽略再次发起; AsyncJob 代次语义下重复 launch 会让晚到的旧轮
    /// 覆盖新轮结果被丢弃 = 用户付了钱会话内无感知)。
    purchase_in_flight: bool,
    /// 导出作业 (SPEC-v1x-export D2): 在途/进度/取消删半成品。单作业,
    /// 进行中入口变取消。
    export_job: ExportJob,
    /// 导出格式小菜单 (D7): 格式行按模式收口 (JSONL 三项 / 明文仅原始行)。
    export_menu_open: bool,
    /// 列管理弹层开合 (SPEC-v1x-table-column-config D3; 与 export_menu 互斥)。
    col_menu_open: bool,
    /// 字段查询弹层开合 (SPEC-v1x-field-picker-ui D1; 与 col_menu/export_menu 互斥)。
    picker_open: bool,
    /// 表单已点选的字段名 (None = 还没点字段; 提交/开弹层时复位)。
    picker_field: Option<String>,
    /// 表单已点选的算符 (默认 `=`; 开弹层时复位)。
    picker_op: jsonl::Op,
    /// 值输入框清空代次 (框架 `TextInput::bind_clear` 消费): 提交/关弹层时 +1。
    picker_clear_rev: u64,
    /// 当前路径的命名会话 (SPEC-v1x-workspace-sessions T2): 随
    /// `load_state_for_current_file` 换路径整片替换，变更即落 `state.json`。
    sessions: Vec<danqing_log::columns::SessionEntry>,
    /// 会话弹层最近点选名 (「删除」指针，T3; 应用即记选中)。换文件清。
    session_selected: Option<String>,
    /// 合并源管理弹层开合 (T4; 与弹层族互斥并入 `close_popovers`/`popover_open`)。
    merge_menu_open: bool,
    /// 源管理弹层选中源 (路径串; 「移除」指针 —— DeleteSelectedSession 同款)。
    merge_source_selected: Option<String>,
    /// 弹层输入框清稿计数 (关弹层 bump, SubmitInput bind_clear 同 session 规)。
    merge_clear_rev: u64,
    /// 命名会话弹层开合 (D5; 与弹层族互斥并入 `close_popovers`/`popover_open`)。
    session_menu_open: bool,
    /// 命名输入清空代次 (`bind_clear` 消费): 开/关/保存/应用时 +1。
    session_clear_rev: u64,
    /// key 输入框清空代次 (框架 `TextInput::bind_clear` 消费): 激活成功时 +1,
    ///  widget 侧把明文 key 清掉 (安全评审：激活后 key 不该继续裸奔在卡里)。
    license_clear_rev: u64,
    /// 字段分析后台作业 (腿二; AsyncJob 代次语义在此正是想要的：重跑作废旧轮
    /// —— 与购买防重入那次的「不可丢」相反，见 plan 风险表的对照注)。
    analysis_job: AsyncJob<danqing_log::analysis::Analysis>,
    /// 最近一次分析结果 (None = 选择器态)。
    analysis_result: Option<danqing_log::analysis::Analysis>,
    /// 结果所基于的过滤串快照 (D8: 过滤变了不自动重跑，标「基于旧过滤」)。
    analysis_filter_src: String,
    /// 分析在途 (面板标题旁显示「分析中…」)。
    analysis_running: bool,
    /// 发起计数 —— 测试断言门控拦截时**没有**发起扫描用的观测点。
    analysis_launches: u32,
    // ---- merge-timeline 腿一 (T3) ----
    /// 合并工作区状态 bundle (None = 从未进过合并)。SPEC-v1x-merge-timeline D4:
    /// 与 Single 字段并列互不动 —— 退出合并回 Single 时单文件现场原样还在。
    merge: Option<danqing_log::merge_view::MergeState>,
    /// 工作区模式 (D4; 与 ViewMode 正交，见枚举注释)。
    workspace: Workspace,
    /// 归并后台作业 (AsyncJob 同款先例：launch 起线程，tick 拾取)。
    merge_job: AsyncJob<danqing_log::merge_view::MergeOutcome>,
    /// 归并作业在途标记 (T7): AsyncJob 无 in-flight 查询，poll_growth_merge
    /// 的「重归并在途不叠加」门禁靠它 (launch 置位 / pickup 清除)。
    merge_job_live: bool,
    /// 合并会话恢复载荷 (T8): apply_session (合并组) 挂上，交卷
    /// (apply_merge_outcome) 套回保存的源时间参数/显隐; start_merge /
    /// rebuild_merge 清理 (用户另起/改源归并 = 旧载荷作废，防错嫁新归并)。
    pending_merge_apply: Option<danqing_log::merge_view::MergeGroup>,
    /// 追踪过滤后台作业 (腿 E/T6): per-source run_filter 全源并行在线程内串行，
    /// tick 拾取落地; 重建/退出合并即 invalidate (R5 族：在途不得晚到复活)。
    trace_job: AsyncJob<danqing_log::merge_view::TraceOutcome>,
}

/// 提示级别 = **两条呈现通道的分派键** (SPEC-notice-visibility D4, 2026-09-28):
/// Warn 的呈现位是 toast 浮层，Info 在底栏 —— 两者不「同屏」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoticeKind {
    /// 警示 (打开失败/轮转/选区超限) —— toast 浮层画 (`danger()` 左色条)。
    Warn,
    /// 提示 (复制回执等) —— 底栏画：`text_primary()` 字 + `surface_variant` 衬底。
    Info,
}

/// 应用消息。
pub(crate) enum Msg {
    /// 滚轮/键盘滚动 N 显示行 (负 = 向上)。
    ScrollRows(f64),
    /// **绝对**滚到某显示行 (T17 滚动条拖拽)。与 `ScrollRows` 的两点不同都是
    /// 有意的：① 它是绝对定位，拖拽是「拇指在哪内容就在哪」而不是增量;
    /// ② 它**不动 `selected`** —— 抓滚动条是「看」不是「选」, 见 todo T17 不变量 ③。
    ///
    /// `at_bottom` = 目标就在**条能被拖到的最底**。它存在只为一个理由：跟随态下
    /// app 的 `top_row` 用 `count-1` 口径，而条能表达的最大值是 `count-可见行数`
    /// —— 两者不等，直接比 `top < top_row` 会把「在底部碰一下条」误判成「向上看」
    /// 而**静默脱掉 FOLLOW** (T17 review 抓出来的)。带上这一位，判断才有据可依。
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
    /// 点侧栏柱条：套用该桶的过滤子句; 已是当前生效项则清除 (切换语义)。
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
    /// 更新动作 (版本行按钮): 按轨道分派 —— GitHub 开发布页 / 商店拉起系统更新
    /// (语义收敛在 `app_update::perform_action`)。
    PerformUpdateAction,
    /// 「许可」页 key 输入框内容变化 (镜像进应用状态，激活按钮读它)。
    LicenseKeyInput(String),
    /// 「许可」页点「激活」按钮 (读输入镜像走激活)。
    ActivateLicenseClicked,
    /// 「获取付费层」(T5): 商店版拉购买对话框，便携版开购买页。
    PurchasePaidLayer,
    /// 免费用户触发付费功能 → 统一升级提示 (T7; 付费态不发，两道闸)。
    ShowUpgradePrompt(Feature),
    /// 关闭升级提示。
    CloseUpgradePrompt,
    /// 升级提示的「已有 key？去激活」: 关提示 → 开设置卡停「许可」页。
    UpgradeGotoActivate,
    /// 升级提示的「获取付费层」: 关提示 → 走购买 (与许可页按钮同源)。
    UpgradePurchase,
    /// 点分析面板的字段行：分析该字段 (门控点位，D5)。
    AnalyzeField(usize),
    /// 结果视图「← 换个字段」: 清结果回选择器。
    AnalysisBack,
    /// 底栏「导出…」/ `Ctrl+E` (SPEC-v1x-export D7): 作业态点 = 取消;
    /// 否则门控 (免费态弹升级提示，**保存对话框之前**) → 开格式菜单。
    ExportEntryClicked,
    /// 格式菜单选中。
    ExportFormatChosen(ExportPick),
    /// 关格式菜单 (scrim 点击 / Esc)。
    CloseExportMenu,
    // ---- 列配置三件套 (SPEC-v1x-table-column-config T4) ----
    /// 列宽拖拽提交 (抬起落账): 手动宽覆盖，落 `state.json` (D1/D4)。
    ColumnWidthSet(String, f32),
    /// 双击手柄恢复采样宽：删手动宽覆盖。
    ColumnWidthClear(String),
    /// 表头拖拽换位落点：`name` 移到 `before` 之前 (`None` = 排尾;
    /// `before == name` = 落点即原位无操作)。
    ColumnMoveBefore(String, Option<String>),
    /// 表头「列…」按钮 / 表头右键：开列管理弹层 (与导出菜单互斥，D3)。
    OpenColMenu,
    /// 关列管理弹层 (scrim 点击 / Esc)。
    CloseColMenu,
    /// 列管理开关行：切换该列显隐 (≥1 可见守卫在应用层，D6)。
    ToggleColumn(String),
    /// 「恢复默认」: 列摆法回首见序 + 全显 + 采样宽 (D3)。
    ResetColumns,
    /// 「字段…」按钮：开字段查询弹层 (与 col_menu/export_menu 互斥，D1)。
    OpenPicker,
    /// 关字段查询弹层 (scrim 点击 / Esc; 不提交)。
    ClosePicker,
    /// 点字段行：记选中字段 (RowList 载荷 = 列名)。
    PickPickerField(String),
    /// 点算符行：记选中算符 (六钮常显，Open Q①)。
    PickPickerOp(jsonl::Op),
    /// 保存/覆盖命名会话 (命名输入 Enter /「保存当前」同路，T2)。
    SaveSession(String),
    /// 应用命名会话 (点行 = 应用并记选中，T2/T3)。
    ApplySession(String),
    /// 删除**选中**会话 (「删除」钮; 无确认，Open Q4; 无选中 = 提示)。
    DeleteSelectedSession,
    /// 开命名会话弹层 (状态栏「会话」入口; D4 门控点位 = 入口)。
    OpenSessionMenu,
    /// 关命名会话弹层 (Esc/scrim)。
    CloseSessionMenu,
    /// 表单提交 (值输入 Enter / 「过滤」钮，`PickerInput` 持有者内同路):
    /// 值随信 (评审 R3: 不设镜像，镜像有 set_text/clear 不回 on_change 的脱钩窗),
    /// 拼子句 → 空格追加 → `apply_filter` (D1/D3)。
    PickerSubmit(String),
    /// Ctrl+O / 拖拽文件：打开新文件。
    OpenFile(PathBuf),
    /// 底栏「合并…」/ `Ctrl+M` (SPEC-v1x-merge-timeline D6): **门控点位 = 入口**
    /// (免费态弹升级对话框) → 开合并源管理弹层 (弹层族第七员)。
    OpenMergeMenu,
    /// 关合并源管理弹层 (Esc/scrim)。
    CloseMergeMenu,
    /// 「加源…」: 开系统文件对话框 (UI 层，测试不发 —— 家法：测试不触真实桌面),
    /// 选出后发 [`Msg::MergeSourcePicked`]。
    PickMergeSource,
    /// 对话框选定追加源 (payload = 路径; 也供测试直注): 未合并 → 以此起并;
    /// 已合并 → 追加重归并。**动作兜底闸** (两道闸第二道) 在臂内。
    MergeSourcePicked(PathBuf),
    /// 「移除」: 移除**选中**源 (指针语义，DeleteSelectedSession 同款) → 重归并;
    /// 只剩两源时不出手、指路「退出合并」(D 闸，G 组验收缺陷②; 原「不足两源退出」
    /// 分支随之删除)。
    RemoveSelectedMergeSource,
    /// 点源行：切显隐 + 记选中 (payload = 路径; 「点行 = 动作并记选中」对齐
    /// ApplySession 先例)。显隐 = 掩码重建 (源序号不漂移), 不重跑提取。
    ToggleMergeSource(String),
    /// 弹层「退出合并 / 返回合并」一钮双态 (D4 互不丢：退出 bundle 保留，
    /// 返回不重建; 无合并 = 提示)。
    ToggleMergeWorkspace,
    /// T5 选中源偏移微调 (快捷档 ±1s/±1min/±1h; 值 = **增量** ms)。
    NudgeMergeOffset(i64),
    /// T5 选中源偏移手输 (绝对值 ms; ±ms 粒度)。
    SetMergeOffset(String),
    /// T5 选中源时区手输 (±hh:mm 或小时数; 无 tz 时间戳按它解释，腿 D)。
    SetMergeTz(String),
    /// 启动合并 (merge-timeline 腿一 T3): 源列表 = 当前文件 (主源) + 追加源。
    /// 构造点 = 弹层加源首并 (add_merge_source); D6 两道闸在入口层，不在作业层。
    StartMerge(Vec<PathBuf>),
    /// 退出合并回单文件工作区 (MergeState 保留，再进不重建，D4 互不丢状态)。
    /// 构造点 = 弹层「退出合并」(ToggleMergeWorkspace)。
    ExitMerge,
    /// T6 (腿 E): 追踪选中值 —— view 出选区键 (源，文件行，字节区间),
    /// 值提取 (JSONL 放大字段值 / .log 原文) + per-source 过滤在应用层。
    TraceValue {
        src: u32,
        line: u32,
        lo: usize,
        hi: usize,
    },
    /// T6: 清除追踪过滤 (Esc 升级序列末级：选区 → 追踪 → 清焦)。
    ClearTrace,
    /// 底栏一次性提示 (选区超限未复制等，组件层 → 应用层 notice 通道)。
    ///
    /// **级别语义与呈现分派** (SPEC-notice-visibility D4, 2026-09-28):
    /// `NoticeKind::Warn` = 警示 (打开失败/轮转/选区超限) → **toast 浮层**
    /// (视野内，`danger()` 色条); `NoticeKind::Info` = 提示 (复制回执等) →
    /// **底栏** (`text_primary()` + `surface_variant` 衬底)。分派在 view 层，
    /// 本消息只进通道不挑呈现位。
    Notice(String, NoticeKind),
    /// 点掉 toast 浮层 (SPEC-notice-visibility 腿 A): 与到点消退同途清 notice。
    DismissNotice,
    // ---- 主题下拉 ----
    /// 通过下拉选择器选择主题 (索引)。
    /// 展开/收起/键盘导航/点外关闭均由 `Dropdown` 自管，不再经应用消息。
    SelectTheme(usize),
    /// 退出应用 (托盘菜单)。
    Quit,
    /// 无操作 (事件吞噬用，不触发任何状态变更)。
    Noop,
}

impl LogApp {
    /// 空态骨架 (run() 启动与测试夹具共享，字段只许有一份真身)。
    fn new_empty() -> Self {
        Self::new_empty_at(None)
    }

    /// 同上，但可指定配置路径 (仅测试用; 见 `cfg_path` 字段)。
    fn new_empty_at(cfg_path: Option<std::path::PathBuf>) -> Self {
        let cfg = match &cfg_path {
            Some(p) => config::Config::load_from(p),
            None => config::Config::load(),
        };
        // 测试构建一律 Free 起手且不读真实 license 文件 (hermetic);
        // 真实加载只在非 test 构建的生产路径 (见 initial_entitlement)。
        // T5 占位：商店版 (is_packaged) 授权查询在 T5 接，当前打包态也走这里。
        let entitlement = Self::initial_entitlement(&cfg_path);
        Self {
            cfg_path,
            window_sender: None,
            file: Arc::new(LogFile::empty()),
            has_file: false,
            top_row: 0.0,
            selected: 0,
            base_status: EMPTY_STATUS.to_string(),
            status: String::new(),
            status_error: false,
            mode: ViewMode::Raw,
            schema: None,
            columns: danqing_log::columns::ColumnConfig::default(),
            level_counts: Arc::new(LevelCounts::default()),
            level_column: None,
            level_queries: levels::no_level_queries(),
            levels_job: AsyncJob::new(),
            levels_pending: false,
            filtered: None,
            filter_applied: String::new(),
            filter_landed: String::new(),
            filter_pending: false,
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
            update_hint_override: std::cell::Cell::new(None),
            entitlement,
            license_pubkey: license::PRODUCT_PUBKEY,
            store_license_job: AsyncJob::new(),
            purchase_job: AsyncJob::new(),
            license_key_input: String::new(),
            license_feedback: None,
            upgrade_prompt: None,
            purchase_in_flight: false,
            export_job: ExportJob::new(),
            merge_menu_open: false,
            merge_source_selected: None,
            merge_clear_rev: 0,
            export_menu_open: false,
            col_menu_open: false,
            picker_open: false,
            picker_field: None,
            picker_op: jsonl::Op::Eq,
            picker_clear_rev: 0,
            sessions: Vec::new(),
            session_selected: None,
            session_menu_open: false,
            session_clear_rev: 0,
            license_clear_rev: 0,
            analysis_job: AsyncJob::new(),
            analysis_result: None,
            analysis_filter_src: String::new(),
            analysis_running: false,
            analysis_launches: 0,
            merge: None,
            workspace: Workspace::Single,
            merge_job: AsyncJob::new(),
            merge_job_live: false,
            pending_merge_apply: None,
            trace_job: AsyncJob::new(),
        }
    }

    /// 启动授权判定 (D4: 失效只以启动时判定)。测试构建恒 Free —— 读真实
    /// license 文件 = 测试依赖用户机器状态，与 save_config 的硬拦同一个理由。
    fn initial_entitlement(cfg_path: &Option<std::path::PathBuf>) -> Entitlement {
        #[cfg(test)]
        {
            let _ = cfg_path;
            Entitlement::Free
        }
        #[cfg(not(test))]
        match cfg_path {
            Some(_) => Entitlement::Free,
            None => license::load(),
        }
    }

    /// 采纳一份产物带来的计数口径：列名与据此生成的点选子句表。
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

    /// 计数作业交付：与快照对账后换入计数与子句表。
    ///
    /// 作业在算的时候文件可能又增长了 —— 此时**不能重起作业** (持续增长的 tail
    /// 会永远算不完), 而是用 `update_for_append` 把快照之后的那几行按「重叠一行」
    /// 补上。只数增量，很便宜。
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

    /// 字段查询弹层草稿三态复位 (评审 Optional: 开/关/提交成功共用一处收口,
    /// 免得第三条路径漏复位)。
    fn reset_picker_draft(&mut self) {
        self.picker_field = None;
        self.picker_op = jsonl::Op::Eq;
        self.picker_clear_rev += 1;
    }

    /// 关尽弹层族 (列管理 / 导出格式 / 字段查询 / 命名会话) —— 互斥「开一关二」、
    /// 换文件/重建、开设置/升级去激活同纪律的**单一收口** (各处各写一份漏过项：
    /// 评审 R5 双开劫 Enter / R6 门禁漏 picker)。
    fn close_popovers(&mut self) {
        self.col_menu_open = false;
        self.export_menu_open = false;
        self.picker_open = false;
        if self.merge_menu_open {
            self.merge_menu_open = false;
            self.merge_source_selected = None; // 关清草稿 (CloseSessionMenu 同纪律)
            self.merge_clear_rev += 1;
        }
        if self.session_menu_open {
            self.session_menu_open = false;
            // 关清草稿随关走 (评审 M12): 换文件/重建/互斥全在这收口, 不靠下次
            // 开弹层兜 —— 「关弹层清草稿」纪律对齐 CloseSessionMenu 臂。
            self.session_clear_rev += 1;
        }
    }

    /// 弹层族任一开着 —— 模态清单的共同判据 (滚轮/键盘门禁与 Ctrl 守卫**同源**,
    /// 评审 R6: 各列一份就漏一项)。
    fn popover_open(&self) -> bool {
        self.col_menu_open
            || self.export_menu_open
            || self.picker_open
            || self.session_menu_open
            || self.merge_menu_open
    }

    /// 列配置门 (todo-gate-trio G2, 两道闸): 免费态弹统一升级提示并拦下动作。
    /// 免费态默认列摆法照用 (v1.0 行为), 不因此毁数据 —— 通路段另有「不读不写」。
    fn column_gate(&mut self) -> bool {
        if self.entitlement.allows(Feature::ColumnConfig) {
            return true;
        }
        self.update(Msg::ShowUpgradePrompt(Feature::ColumnConfig));
        false
    }

    /// 字段点选门 (todo-gate-trio G4): 免费态「字段…」按钮拦下; 手输迷你语法
    /// 照用 (过滤栏输入不在本闸后)。
    fn picker_gate(&mut self) -> bool {
        if self.entitlement.allows(Feature::FieldPicker) {
            return true;
        }
        self.update(Msg::ShowUpgradePrompt(Feature::FieldPicker));
        false
    }

    /// 合并动作门 (两道闸，D6): 免费态弹统一升级提示并拦下动作。
    /// 入口闸在 `OpenMergeMenu` 臂; 本闸兜住加源/减源/显隐三个动作
    /// (会话 session_gate 同构 —— 门控点位在动作层，不在作业层)。
    fn merge_gate(&mut self) -> bool {
        if self.entitlement.allows(Feature::MergeTimeline) {
            return true;
        }
        self.update(Msg::ShowUpgradePrompt(Feature::MergeTimeline));
        false
    }

    /// 会话动作门 (两道闸第二道，D4): 免费态弹统一升级提示并拦下动作。
    /// **数据永在**: 账本读写不走这道门 (降级锁动作不毁数据)。
    fn session_gate(&mut self) -> bool {
        if self.entitlement.allows(Feature::WorkspaceSessions) {
            return true;
        }
        self.update(Msg::ShowUpgradePrompt(Feature::WorkspaceSessions));
        false
    }

    /// 字段分析入口 (腿二，D5 门控点位): 免费态弹升级提示且**不发起扫描**;
    /// 付费态带 (文件，字段名，过滤行集快照) 进 worker。
    fn analyze_field(&mut self, idx: usize) {
        if !self.entitlement.allows(Feature::FieldAnalytics) {
            self.update(Msg::ShowUpgradePrompt(Feature::FieldAnalytics));
            return;
        }
        let Some(schema) = &self.schema else { return };
        let Some(col) = schema.columns.get(idx) else {
            return;
        };
        let field = col.name.clone();
        let file = Arc::clone(&self.file);
        let rows = self.filtered.clone();
        // 快照串必须与行集同源 (评审 R5): `filter_applied` 在作业在途窗口里
        // 已是新串而行集还是旧的 —— 读落账串，保证「作用域标注说的过滤」
        // 就是「实际跑了的行集」的产出者; 新过滤落账后 stale 标注自然出现。
        self.analysis_filter_src = self.filter_landed.clone();
        self.analysis_running = true;
        self.analysis_launches += 1;
        self.analysis_job.launch(move || {
            danqing_log::analysis::analyze_field(&file, &field, rows.as_ref().map(|v| v.as_slice()))
        });
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
    /// Merge 工作区 = 合并时间线行数 (T3: 合并内暂无展开子行 —— 嵌套展开待
    /// 并集列 (T4) 波再裁，spec 实现记收录)。
    fn display_count(&self) -> u64 {
        if let Some(m) = self.merge_active() {
            return m.row_count();
        }
        expand::display_count(self.lines(), &self.expanded)
    }

    // ---- merge-timeline T3: 工作区访问器 ----

    /// Merge 激活时借状态 bundle (workspace=Merge 且 bundle 在); 否则 None。
    fn merge_active(&self) -> Option<&danqing_log::merge_view::MergeState> {
        if self.workspace == Workspace::Merge {
            self.merge.as_ref()
        } else {
            None
        }
    }

    /// 同上，可变。
    fn merge_active_mut(&mut self) -> Option<&mut danqing_log::merge_view::MergeState> {
        if self.workspace == Workspace::Merge {
            self.merge.as_mut()
        } else {
            None
        }
    }

    /// 当前工作区的首可见显示行 (Merge 读 bundle, Single 读自身字段 —— D4)。
    fn cur_top(&self) -> f64 {
        match self.merge_active() {
            Some(m) => m.top_row,
            None => self.top_row,
        }
    }

    /// 写首可见行 (路由到当前工作区)。
    fn set_top(&mut self, v: f64) {
        match self.merge_active_mut() {
            Some(m) => m.top_row = v,
            None => self.top_row = v,
        }
    }

    /// 当前工作区的选中显示行。
    fn cur_selected(&self) -> u64 {
        match self.merge_active() {
            Some(m) => m.selected,
            None => self.selected,
        }
    }

    /// 写选中行 (路由到当前工作区)。
    fn set_selected(&mut self, v: u64) {
        match self.merge_active_mut() {
            Some(m) => m.selected = v,
            None => self.selected = v,
        }
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
        if self.workspace == Workspace::Merge {
            // T7 (腿 F): 合并跟随接通 —— 钉时间线尾 (最新事件处); 新行合流时
            // append_source 内钉 (pin_tail), 与单文件「跟随新行」同语义。
            if let Some(m) = self.merge_active_mut() {
                m.follow = !m.follow;
                if m.follow {
                    m.pin_tail();
                }
            }
            self.refresh_status();
            return;
        }
        self.follow = !self.follow;
        if self.follow {
            self.set_top(self.max_top());
            self.set_selected(self.display_count().saturating_sub(1));
        }
        self.refresh_status();
    }

    // ---- merge-timeline 腿一 (T3): 合并生命周期 ----
}

/// top_row 钳制到 [0, max_top]。独立成函数供单测。
fn clamp_top(top: f64, line_count: u64) -> f64 {
    let max = line_count.saturating_sub(1) as f64;
    top.clamp(0.0, max)
}

/// 滚轮 delta → 要滚的显示行数 (**单一换算点**, T17)。
///
/// 三处滚轮 (内容区 LogView / 未认领的滚轮 / 将来任何新入口) 必须走同一支，
/// 否则「在侧栏滚」与「在列表上滚」手感会不一样 —— 而 P30 补的正是这两处的
/// **一致性**, 各写一份等于把刚修好的东西再拆开。
///
/// **框架不归一 delta** (普查 G10): `window/event.rs:150-154` 把 `LineDelta`
/// (行数，通常 ±1..3) 与 `PixelDelta` (像素，精确触控板可达 ±100) 抹平成同一个
/// `f32`, 下游**无从分辨**。按行数档取 3 倍再夹一个**每次事件**的上界：不夹的话
/// 触控板一次能跳几百行。夹的是单次事件，不是总量，连续滚不受影响。
fn wheel_rows(delta_y: f32) -> f64 {
    (-f64::from(delta_y) * 3.0).clamp(-WHEEL_MAX_ROWS, WHEEL_MAX_ROWS)
}

/// 单次滚轮事件最多滚多少显示行 (见 [`wheel_rows`])。
const WHEEL_MAX_ROWS: f64 = 12.0;

/// 底栏提示的停留时长 (见 `LogApp::set_notice`)。**待实机核对**。
const NOTICE_TTL: std::time::Duration = std::time::Duration::from_secs(4);

/// 当前 epoch 秒 (记忆状态落账时间戳; 系统时钟取不到按 0 —— 落账不因钟坏拒写)。
/// `save_state`/`save_session` 同用。
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl App for LogApp {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            // ---- 列配置三件套 (SPEC-v1x-table-column-config T4): 变更即落盘 ----
            Msg::ColumnWidthSet(name, w) => {
                if self.column_gate() {
                    self.merge_columns();
                    self.columns.set_width(&name, w);
                    self.save_state();
                }
            }
            Msg::ColumnWidthClear(name) => {
                if self.column_gate() {
                    self.merge_columns();
                    self.columns.widths.remove(&name);
                    self.save_state();
                }
            }
            Msg::ColumnMoveBefore(name, before) => {
                if self.column_gate() {
                    self.merge_columns();
                    self.columns.move_name_before(&name, before.as_deref());
                    self.save_state();
                }
            }
            Msg::OpenColMenu => {
                // G2 门控点位 = 入口 (免费态弹升级对话框，手势起点另有拦截)
                if !self.column_gate() {
                    return;
                }
                self.close_popovers(); // 互斥 (D3): 双 scrim 不叠，开一关二
                self.col_menu_open = true;
            }
            Msg::CloseColMenu => self.col_menu_open = false,
            Msg::OpenPicker => {
                // G4 门控点位 = 「字段…」按钮 (免费态弹升级对话框; 手输迷你语法
                // 不经此臂，照用)
                if !self.picker_gate() {
                    return;
                }
                // 互斥 (D1): 「开一关二」+ 关 settings (评审 R5: 托盘双开会让
                // Enter 被劫到提交)。
                self.close_popovers();
                self.settings_open = false;
                self.picker_open = true;
                self.reset_picker_draft();
                // 开弹层直接打字进值框 (评审 R8: 点按钮会清焦，送回值框)
                self.focus_target = Some("picker-value");
            }
            Msg::ClosePicker => {
                self.picker_open = false;
                self.reset_picker_draft();
            }
            Msg::PickPickerField(name) => self.picker_field = Some(name),
            Msg::PickPickerOp(op) => self.picker_op = op,
            Msg::PickerSubmit(value) => {
                let Some(field) = self.picker_field.clone() else {
                    self.set_notice("先点选字段".into(), NoticeKind::Warn);
                    return;
                };
                let Some(clause) = build_clause(&field, self.picker_op, &value) else {
                    // 拒收说清 (评审 Critical): 文案与判据同一函数 (单一事实源)
                    let why = clause_reject_notice(&field, self.picker_op, &value)
                        .expect("build_clause 拒收 ⇒ clause_reject_notice 必有文案");
                    self.set_notice(why.into(), NoticeKind::Warn);
                    return;
                };
                // 追加 AND (D3): 空查询 = 就是它; 有查询 = 空格连接
                // (parse_query = split_whitespace, 多子句 AND)。提交即关弹层清草稿。
                let q = if self.filter_applied.is_empty() {
                    clause
                } else {
                    format!("{} {}", self.filter_applied, clause)
                };
                self.picker_open = false;
                self.reset_picker_draft();
                self.apply_filter(q);
            }
            // ---- 命名工作台会话 (SPEC-v1x-workspace-sessions) ----
            Msg::OpenSessionMenu => {
                // 门控 (D4, 两道闸第一道): 免费态入口 → 升级提示，弹层不开
                if !self.session_gate() {
                    return;
                }
                self.close_popovers();
                self.settings_open = false;
                self.session_menu_open = true;
                self.session_clear_rev += 1;
                // 开弹层直接打字命名 (评审 R8 同纪律：送焦值框)
                self.focus_target = Some("session-name");
            }
            Msg::CloseSessionMenu => {
                self.session_menu_open = false;
                self.session_clear_rev += 1;
            }
            Msg::SaveSession(name) => {
                if self.session_gate() {
                    self.save_session(&name);
                    self.session_clear_rev += 1;
                }
            }
            Msg::ApplySession(name) => {
                // 应用成功即关弹层清草稿 (picker 提交先例); 未知名留着重选
                if self.session_gate() && self.apply_session(&name) {
                    self.session_menu_open = false;
                    self.session_clear_rev += 1;
                }
            }
            Msg::DeleteSelectedSession => {
                if self.session_gate() {
                    match self.session_selected.clone() {
                        Some(n) => self.delete_session(&n),
                        None => self.set_notice("先点选会话再删".into(), NoticeKind::Warn),
                    }
                }
            }
            // ---- 合并源管理 (SPEC-v1x-merge-timeline T4) ----
            Msg::OpenMergeMenu => {
                // 门控 (D6, 两道闸第一道 = 入口): 免费态点「合并…」→ 升级提示
                if !self.merge_gate() {
                    return;
                }
                if !self.has_file {
                    // 合并以当前文件为主源 (D4): 空态没主源，说清再拦
                    self.set_notice("尚未打开文件 (Ctrl+O 打开)".into(), NoticeKind::Warn);
                    return;
                }
                self.close_popovers();
                self.settings_open = false;
                self.merge_menu_open = true;
            }
            Msg::CloseMergeMenu => {
                self.merge_menu_open = false;
                self.merge_source_selected = None;
                self.merge_clear_rev += 1; // 关清草稿 (session 同纪律)
            }
            Msg::PickMergeSource => {
                // 「加源…」: 系统对话框 (UI 层 —— 测试走 MergeSourcePicked 直注，
                // 家法：测试不触真实桌面)。**门在对话框之前**: 免费态连框都不该开。
                if !self.merge_gate() {
                    return;
                }
                if let Some(p) = rfd::FileDialog::new()
                    .set_title("选择要合并的源")
                    .pick_file()
                {
                    self.update(Msg::MergeSourcePicked(p));
                }
            }
            Msg::MergeSourcePicked(path) => {
                if self.merge_gate() {
                    self.add_merge_source(path);
                }
            }
            Msg::RemoveSelectedMergeSource => {
                if self.merge_gate() {
                    self.remove_selected_merge_source();
                }
            }
            Msg::ToggleMergeSource(path) => {
                if !self.merge_gate() {
                    return;
                }
                // 「点行 = 动作并记选中」(ApplySession 先例): 记指针给「移除」;
                // 合并中才谈显隐 (未合并无掩码对象，只选中)。
                self.merge_source_selected = Some(path.clone());
                if self.workspace == Workspace::Merge {
                    if let Some(m) = self.merge.as_mut() {
                        if let Some(s) = m
                            .sources
                            .iter_mut()
                            .find(|s| s.path.as_path() == std::path::Path::new(&path))
                        {
                            s.hidden = !s.hidden;
                        }
                        m.rebuild_masked();
                    }
                    self.discard_trace_job(); // 掩码重建 = 过滤行集作废
                    self.refresh_status();
                }
            }
            Msg::NudgeMergeOffset(delta) => {
                if self.merge_gate() {
                    self.edit_source_time(|o, t| (Some(o + delta), Some(t)));
                }
            }
            Msg::SetMergeOffset(raw) => {
                if self.merge_gate() {
                    match raw.trim().parse::<i64>() {
                        Ok(v) => self.edit_source_time(|_, t| (Some(v), Some(t))),
                        Err(_) => {
                            self.set_notice("偏移须是毫秒整数 (如 -3000)".into(), NoticeKind::Warn)
                        }
                    }
                }
            }
            Msg::SetMergeTz(raw) => {
                if self.merge_gate() {
                    match danqing_log::merge_view::parse_tz_ms(raw.trim()) {
                        Some(v) => self.edit_source_time(|o, _| (Some(o), Some(v))),
                        None => self.set_notice(
                            "时区格式：±hh:mm 或 小时数 (如 +08:00 / 8)".into(),
                            NoticeKind::Warn,
                        ),
                    }
                }
            }
            Msg::ToggleMergeWorkspace => {
                // 「退出合并」/「返回合并」一个钮 (label 随态，见 settings 卡):
                // 退出 = 回单文件 (bundle 保留，D4 不丢); 返回 = 再进**不重建**。
                if self.merge.is_none() {
                    self.set_notice("当前没有合并".into(), NoticeKind::Warn);
                } else if self.workspace == Workspace::Merge {
                    self.update(Msg::ExitMerge);
                } else {
                    self.workspace = Workspace::Merge;
                    self.focus_target = Some("log-view");
                    self.refresh_status();
                }
            }
            Msg::ToggleColumn(name) => {
                self.merge_columns();
                if !self.columns.order.contains(&name) {
                    return; // 未知列 (陈旧弹层快照): 零动作零提示，不误报守卫文案
                }
                if !self.columns.toggle_hidden(&name) {
                    // D6: ≥1 可见列守卫 —— 关最后一可见列拒绝并提示 (零变更不写)
                    self.set_notice("至少保留一列可见".into(), NoticeKind::Warn);
                } else {
                    self.save_state();
                }
            }
            Msg::ResetColumns => {
                self.merge_columns();
                if let Some(s) = &self.schema {
                    let names: Vec<String> = s.columns.iter().map(|c| c.name.clone()).collect();
                    self.columns.reset(&names);
                    self.save_state();
                }
            }
            Msg::ScrollRows(d) => {
                // 用户向上滚动 → 脱离跟随 (不打扰阅读)
                if d < 0.0 && self.follow {
                    self.follow = false;
                    self.refresh_status();
                }
                self.set_top(clamp_top(self.cur_top() + d, self.display_count()));
                // 方向键滚动时选中跟随首行，底栏读数即当前位置
                self.set_selected(self.cur_top() as u64);
            }
            Msg::ScrollTo { top, at_bottom } => {
                // 往回 (上) 拖 = 想回头看 → 停止跟随 (与滚轮同规，别把用户拽回底部)。
                // **拖到条底不算往回** —— 见枚举上的注释：跟随态的 `top_row` 比条能
                // 表达的底还大，不排除这一格就会「在底部碰一下条 → FOLLOW 没了」。
                if !at_bottom && top < self.cur_top() && self.follow {
                    self.follow = false;
                    self.refresh_status();
                }
                // **不动 `selected`**: 抓滚动条是「看」不是「选」(T17 不变量 ③)。
                self.set_top(clamp_top(top, self.display_count()));
            }
            Msg::Select(row) => {
                if row < self.display_count() {
                    self.set_selected(row);
                }
            }
            Msg::ToggleExpand(row) => {
                let file_line = self.file_line_of(row);
                self.toggle_expand(file_line);
            }
            Msg::GotoStart => {
                self.set_top(0.0);
                self.set_selected(0);
            }
            Msg::GotoEnd => {
                self.set_top(self.max_top());
                self.set_selected(self.display_count().saturating_sub(1));
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
                // 互斥 (评审 R5): 托盘等入口无模态屏障，开设置先关弹层族 ——
                // 双开时 Enter 会被劫到 PickerSubmit (原「互斥保证」前提不成立)
                self.close_popovers();
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
            Msg::PerformUpdateAction => {
                crate::app_update::perform_action();
            }
            Msg::LicenseKeyInput(s) => {
                self.license_key_input = s;
                // 继续输入 = 在改上一份答案，旧反馈作废
                self.license_feedback = None;
            }
            Msg::ActivateLicenseClicked => {
                let key = self.license_key_input.trim().to_owned();
                self.activate_license(key);
            }
            Msg::PurchasePaidLayer => self.purchase_paid_layer(),
            Msg::ShowUpgradePrompt(f) => {
                // 付费态不出现 (spec 成功判据): 门控点位先查 `allows` 再发，
                // 这里再兜一道 —— 两道都守着「付费用户永远看不到升级提示」。
                if !self.entitlement.allows(f) {
                    self.upgrade_prompt = Some(f);
                }
            }
            Msg::CloseUpgradePrompt => self.upgrade_prompt = None,
            Msg::UpgradeGotoActivate => {
                self.upgrade_prompt = None;
                self.settings_tab = settings::LICENSE_TAB_INDEX;
                self.close_popovers(); // 互斥 (评审 R5, OpenSettings 同规)
                self.settings_open = true;
            }
            Msg::UpgradePurchase => {
                self.upgrade_prompt = None;
                self.purchase_paid_layer();
            }
            Msg::AnalyzeField(idx) => self.analyze_field(idx),
            Msg::AnalysisBack => {
                self.analysis_result = None;
            }
            Msg::ExportEntryClicked => self.export_entry_clicked(),
            Msg::ExportFormatChosen(pick) => self.begin_export(pick),
            Msg::CloseExportMenu => self.export_menu_open = false,
            Msg::OpenFile(path) => {
                self.reload_file(path);
            }
            Msg::StartMerge(paths) => self.start_merge(paths),
            Msg::ExitMerge => {
                if self.workspace == Workspace::Merge {
                    self.discard_trace_job(); // 工作区离场
                    self.workspace = Workspace::Single;
                    self.refresh_status();
                }
            }
            Msg::TraceValue { src, line, lo, hi } => self.start_trace(src, line, lo, hi),
            Msg::ClearTrace => {
                if let Some(m) = self.merge_active_mut() {
                    m.clear_trace();
                    self.refresh_status();
                }
            }
            Msg::Notice(text, kind) => self.set_notice(text, kind),
            Msg::DismissNotice => self.dismiss_notice(),
            Msg::SelectTheme(idx) => {
                self.theme = config::AppTheme::from_index(idx);
                // 通知窗口换底色。**这一步此前从缺** —— 于是切主题后标题栏那条
                // (透出的清屏色) 纹丝不动，只有内容区变了色。
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
        // 一整套坐标数学，sibling 方案下 LogView 只是拿到一个更窄的 area。
        node(
            Stack::new()
                .child(
                    Column::new()
                        .child(title_bar(self.theme, self.make_title()))
                        .fill(
                            Row::new()
                                .fill(
                                    // 侧栏 = 直方图 (吃剩余高度) + 字段分析区
                                    // (自然高，无 schema 时归零坍缩)。宽度折叠
                                    // 判定 (Ctrl+L / 窄窗) 归容器 —— 只有 Row 的
                                    // 直接子项拿得到整个 Row 的可用宽 (sidebar.rs)。
                                    sidebar::Sidebar::new(
                                        histogram::LevelHistogram::new(),
                                        analysis_panel::AnalysisPanel::new(),
                                    ),
                                    0,
                                )
                                .fill(view::LogView::new(), 1),
                            1,
                        ),
                )
                .child(settings::settings_overlay(self.theme))
                .child(settings::upgrade_overlay(self.theme))
                .child(settings::export_menu_overlay(self.theme))
                .child(settings::export_menu_overlay_jsonl(self.theme))
                .child(settings::col_menu_overlay(self.theme))
                .child(settings::picker_overlay(self.theme))
                .child(settings::session_menu_overlay(self.theme))
                .child(settings::merge_menu_overlay(self.theme))
                // toast 挂 Stack 末位 (SPEC-notice-visibility 腿 A): 框架反序分发事件
                // (`stack.rs` rev) = 最先收点击，后画 = 最上层 —— 模态弹层开着时
                // Warn 浮层仍可见可点。非模态：不进 popover_open/close_popovers/Esc 表。
                .child(toast::Toast::new()),
        )
    }

    fn event(&mut self, event: &Event) {
        // P30 (T17): **未认领的滚轮**转给列表滚动。
        //
        // 框架按点子命中分发滚轮、不向父级回落，所以指针停在侧栏/过滤栏上时，
        // 日志区根本收不到 —— 用户必须把指针挪回内容区才滚得动。这里补的是
        // 另一半：凡是**没有组件认领**的滚轮 (侧栏、过滤栏、标题栏、行外空白)
        // 一律滚列表。**不需要位置数学**: 指针在日志区上时 LogView 已经
        // `Consumed` 了，能走到这里的本来就不是它。
        if let Event::MouseWheel { delta, .. } = event {
            // 模态不穿透 (与 T16 同一条纪律): 卡开着时滚轮只属于卡，不许滚卡后的日志
            // (判据与键盘门禁同源：settings + 弹层族，[`Self::popover_open`])。
            if !self.settings_open && !self.popover_open() && self.has_file {
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
        // 设置卡打开 = 模态：Esc 关卡 (S3), 其余键一律吞掉 —— 卡底下的日志区
        // 不该响应键盘 (2026-09-14 用户实机：卡内主题下拉未持焦时 ↑↓ 滚动了
        // 底层日志)。卡内控件经焦点路由自行消费、到不了这里; 能到这里的都是
        // 无人认领的键。(Ctrl+O/Ctrl+L 走 app_key_filter 前置，不在此门禁内。)
        if self.settings_open {
            if let Some(msg) = settings::handle_settings_key(key) {
                self.update(msg);
            }
            return;
        }
        // 弹层模态 (评审 R6, 与滚轮守卫同源): 弹层族开着时，无人认领的导航键
        // (↑↓/Space/Home/End/Page*) 不许穿到弹层后滚日志 —— 2026-09-14 设置卡
        // 同款漏洞的守卫扩展面。卡内控件经焦点路由自行消费。
        if self.popover_open() {
            return;
        }
        // 空态门禁：仅 Ctrl+O (app_key_filter 前置，不经此处) 与设置可用，其余键无文件无意义
        if !self.has_file {
            // M3 (2026-09-14 实机 M0 P26): 空态按键被吞时**说清为什么** ——
            // 原先 `return` 静默，用户按 Ctrl+F/方向键毫无反应。
            self.set_notice("尚未打开文件 (Ctrl+O 打开)".into(), NoticeKind::Warn);
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
                        self.set_notice("本文件非 JSONL, 无表格模式".into(), NoticeKind::Warn);
                    }
                    return;
                }
                if s.eq_ignore_ascii_case("a") {
                    // M3 (P34): Ctrl+A 被吞 —— 说清为什么 (行多选已裁挂 v1.x,
                    // 见 docs/ROADMAP-v1x.md §四)。
                    self.set_notice("行多选未实现 (v1.x 待裁)".into(), NoticeKind::Warn);
                    return;
                }
                if s.eq_ignore_ascii_case("r") {
                    // 腿 E/T6: 合并视图持焦时 LogView 已消费 Ctrl+R (选区键在
                    // 组件侧); 到这里的 = 单文件态或未持焦 —— 说清为什么 (P24)。
                    match self.workspace {
                        Workspace::Merge => self.set_notice(
                            "先双击/框选消息里的追踪值，再按 Ctrl+R".into(),
                            NoticeKind::Warn,
                        ),
                        Workspace::Single => self.set_notice(
                            "追踪跨源值在合并视图可用 (底栏「合并…」/Ctrl+M)".into(),
                            NoticeKind::Warn,
                        ),
                    }
                    return;
                }
            }
            return;
        }
        // 原始模式：`/` 开搜索栏; `b`/`'` 书签; `f` 跟随 (栏聚焦时键进 TextInput, 不达此处)
        // T7: 合并工作区放行 b/'/f (书签/跟随都路由 merge bundle); `/` 不放行 —
        // 合并搜索栏属后续波次 (T3 边界清单), 开了搜的是单文件快照 = 错账。
        let raw_or_merge = self.mode == ViewMode::Raw || self.workspace == Workspace::Merge;
        if raw_or_merge {
            if let Key::Character(s) = key {
                match s.as_str() {
                    "/" if self.workspace == Workspace::Single => {
                        self.open_search();
                        return;
                    }
                    "/" => {
                        // 合并态：搜索栏属后续波次 (T3 边界清单) —— 说清，不静默
                        // (P24); 顺带指路已接通的跨源追踪。
                        self.set_notice(
                            "合并视图暂无搜索栏; 跨源追踪：框选消息后按 Ctrl+R".into(),
                            NoticeKind::Warn,
                        );
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
            Key::Named(NamedKey::ArrowRight)
                if self.workspace == Workspace::Single && self.mode == ViewMode::Table =>
            {
                let file_line = self.file_line_of(self.cur_selected());
                if !self.expanded.is_expanded(file_line) {
                    self.toggle_expand(file_line);
                } else {
                    // M3 (P24): → 在已展开行上按了没反应 —— 说清为什么。
                    self.set_notice("本行已展开".into(), NoticeKind::Warn);
                }
            }
            Key::Named(NamedKey::ArrowLeft)
                if self.workspace == Workspace::Single && self.mode == ViewMode::Table =>
            {
                let file_line = self.file_line_of(self.cur_selected());
                if self.expanded.is_expanded(file_line) {
                    self.toggle_expand(file_line);
                } else {
                    // M3 (P24): ← 在未展开行上按了没反应 —— 说清为什么。
                    self.set_notice("本行未展开".into(), NoticeKind::Warn);
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
        // Esc 前置：升级提示 > 设置卡 > (后续留给搜索/过滤栏)
        if let Event::Key {
            key: Key::Named(NamedKey::Escape),
            pressed: true,
            ..
        } = event
        {
            if self.upgrade_prompt.is_some() {
                return Some(Msg::CloseUpgradePrompt);
            }
            if self.settings_open {
                // 本函数在焦点分发前运行 (无论有无焦点)，所以卡内主题下拉展开
                // 时按 Esc 也走这条路径：整卡通关，而非先收下拉。与「设置卡
                // 优先」的次序一致; 组件自身的 Esc 折叠只在该路径之外可达。
                return Some(Msg::CloseSettings);
            }
            // Esc 次序：升级提示 > 设置卡 > **合并源管理** > 命名会话 > 字段查询 >
            // 列管理 > 导出格式菜单 > 栏 (merge-timeline T4 插层，会话插层同规)
            if self.merge_menu_open {
                return Some(Msg::CloseMergeMenu);
            }
            // Esc 次序：升级提示 > 设置卡 > **命名会话** > 字段查询 > 列管理 >
            // 导出格式菜单 > 栏 (SPEC-v1x-workspace-sessions 插层)
            if self.session_menu_open {
                return Some(Msg::CloseSessionMenu);
            }
            // Esc 次序：升级提示 > 设置卡 > **字段查询** > 列管理 > 导出格式菜单 > 栏
            // (SPEC-v1x-field-picker-ui D1 插层)
            if self.picker_open {
                return Some(Msg::ClosePicker);
            }
            // Esc 次序：升级提示 > 设置卡 > **列管理** > 导出格式菜单 > 栏
            // (SPEC-v1x-table-column-config D3 插层)
            if self.col_menu_open {
                return Some(Msg::CloseColMenu);
            }
            // Esc 次序：升级提示 > 设置卡 > **导出格式菜单** > 栏 (SPEC-v1x-export)
            if self.export_menu_open {
                return Some(Msg::CloseExportMenu);
            }
        }
        // (评审 R7: 全局 Enter 拦截已撤 —— 会把算符钮的 Enter 激活劫成提交。
        // 提交归 `PickerInput` 持有者内收口：值框持焦时 Enter / 「过滤」钮同路。)
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
        // 模态守卫 (T16/P32): 设置卡或升级提示开着时，全局键**不得穿透到卡后**。
        // 原先三个后果：Ctrl+O 在卡片**之上**弹系统文件对话框; Ctrl+F 把焦点按
        // id 送到卡后**看不见的**输入框 (此后打的字全进它); Ctrl+L 把卡后的侧栏
        // 显隐掉。框架的 `app_key_filter` 是应用回调、在模态判定之前无条件跑
        // (`handler.rs:434-441`), 所以这个守卫只能加在产品侧。
        //
        // **位置很要紧：必须在「ctrl + 字符」筛选之后**。框架在这一函数返回
        // `Some` 时**直接 return, 不再走焦点分发** (`handler.rs:436-441`), 而卡内
        // 控件 (主题下拉 / 侧栏开关 / 关闭钮) 全靠焦点分发收键 —— 守卫若放在函数
        // 入口，卡内键盘会**全死**: 下拉导航不动、开关切不了、Enter 关不掉卡。
        // 本批第一版正是那么写的 (见测试里的反向对照), 被 review 抓出来。
        // (T7 扩展：升级提示同享此守卫 —— 它是第二个模态层。)
        if self.settings_open || self.upgrade_prompt.is_some() || self.popover_open() {
            // **剪辑组合键必须放行** (评审 Critical, 2026-09-19): 框架的剪贴板
            // 路由 (handler.rs:471 → Event::Paste) 活在焦点分发里，这里吞掉 =
            // 许可页输入框没法 Ctrl+V 粘贴 key —— 而粘贴是 200+ 字符 key 的
            // 唯一现实输入方式。放行后若焦点在卡后，剪辑键落卡后 —— 与普通字符
            // 今天的既有暴露面相同，不因此更坏。
            if matches!(
                s.to_ascii_lowercase().as_str(),
                "c" | "x" | "v" | "a" | "z" | "y"
            ) {
                return None;
            }
            return Some(Msg::Noop);
        }
        if s.eq_ignore_ascii_case("f") {
            return Some(Msg::FocusSearch);
        }
        if s.eq_ignore_ascii_case("t") && self.schema.is_some() {
            return Some(Msg::ToggleMode);
        }
        // Ctrl+L 侧栏显隐：走前置过滤而非 event(), 故栏聚焦时也生效 (与 Ctrl+T 同级)
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
        // Ctrl+E 导出 (SPEC-v1x-export D7): 与底栏按钮同消息，门控/取消在应用层
        if s.eq_ignore_ascii_case("e") {
            return Some(Msg::ExportEntryClicked);
        }
        // Ctrl+M 合并 (SPEC-v1x-merge-timeline D6): 与底栏「合并…」同消息，门控在应用层
        if s.eq_ignore_ascii_case("m") {
            return Some(Msg::OpenMergeMenu);
        }
        None
    }

    /// LogView 持焦后，焦点组件未消费的键回退应用层 (danqing opt-in):
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
        self.pickup_merge_job();
        if let Some(out) = self.trace_job.poll() {
            self.apply_trace_outcome(out);
        }
        // T5: 商店授权查询 / 购买结果
        if let Some(e) = self.store_license_job.poll() {
            self.adopt_store_entitlement(e);
        }
        if let Some(outcome) = self.purchase_job.poll() {
            self.adopt_purchase_outcome(outcome);
        }
        if let Some(a) = self.analysis_job.poll() {
            self.analysis_running = false;
            self.analysis_result = Some(a);
        }
        // 导出作业收尾 (SPEC-v1x-export D2): 完成/取消/失败一律给反馈
        if let Some(r) = self.export_job.poll() {
            self.handle_export_result(r);
        }
        if let Some(out) = self.filter_job.poll() {
            self.filter_pending = false;
            self.filter_landed = self.filter_applied.clone();
            self.filtered = Some(Arc::new(out.lines));
            self.filter_elapsed = Some(out.elapsed);
            self.set_top(0.0);
            self.set_selected(0);
            self.refresh_status();
        }
        if let Some(out) = self.search_job.poll() {
            let mut nav = SearchNav::new(Arc::new(out.hits), out.total);
            self.search_pattern = Some(out.pattern);
            self.search_query = out.query;
            self.search_elapsed = Some(out.elapsed);
            // 首跳：当前选中行之后的第一条命中 (无则环绕回首条)
            let from = self.file_line_of(self.cur_selected());
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

fn main() {
    danqing::log::init_log();
    // 路径参数可选：无参进空态 (Ctrl+O 打开), 带参直接打开。
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
    // 窗口按 GPU 速度出现，索引在 worker 后台跑 (spec 判据：≤ 无文件启动 +200ms)。
    let mut app = LogApp::new_empty();
    // 商店版 (MSIX 打包): 后台查授权 (broker 进程外调用可能耗时，不堵窗口出现)。
    // 便携版授权在构造时已从 license.key 加载 (initial_entitlement), 不走这里。
    if danqing::platform::is_packaged() {
        app.store_license_job
            .launch(|| license::map_store_snapshot(&store_license::query_snapshot()));
    }
    if let Some(p) = path {
        app.open_job = Some(OpenJob::launch(OpenKind::Fresh, p));
    }
    app.refresh_status();
    let config = WindowConfig {
        title: "丹青日志 LogLens".to_string(),
        size: Size::new(1100.0, 760.0),
        // 清屏色随配置里的主题 —— 此前写死浅色，存暗色配置启动也开在白底上
        // (app 在上一行已从配置读出主题，只是当时没人问它)。
        clear_color: window_clear_color(app.theme),
        logo_name: "log".into(),
        maximized: true, // 日志查看器主战场是全屏阅读：初始最大化
        hotkeys: vec![], // 显式置空：不继承番茄钟默认热键 (danqing WindowConfig 注释)
        ..Default::default()
    };
    run_app(config, &mut app).context("事件循环异常退出")
}

#[cfg(test)]
mod tests;
