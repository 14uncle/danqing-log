//! @author 十四叔
//! @date 2026/09/05
//!
//! 行锚定虚拟视口组件: 只布局/绘制可见行, 任何文件大小下渲染成本恒定。
//!
//! 为什么不复用 danqing Scrollable: 其 scroll_offset 为 f32 像素坐标,
//! 10M 行 × 20px = 2 亿逻辑像素远超 f32 精确整数域 (2^24≈16.7M),
//! 底部滚动会出现亚行漂移 —— 恰好在 POC 要证明的 1GB+ 场景失守。
//! 行锚定 (u64 行号 + 行内小数偏移) 在任意文件大小下精度无损。
//!
//! 布局 (v1 后过滤/搜索栏是独立 sibling, 不在此组件内):
//! `Column[ Bar(真 TextInput, 内容高 32) · LogView(本组件, fill) ]`
//!  LogView 负责: [表头 24px(仅表格)] [虚拟化行] [状态栏 26px]。
//! - 原始模式: 整行文本 + 级别着色, 与 POC v0 一致;
//! - 表格模式 (前提②): 表头 + 虚拟化行, 列定义来自 JSONL 采样 (jsonl::Schema),
//!   单元格经 memmem 字段提取 (零 parse), 显示行→文件行号经 filtered 命中表映射
//!   (无过滤时恒等), 行号槽恒显真实行号。
//!
//! POC 边界: 无水平滚动 (超宽单元格截断省略, 溢出右缘的列整列不画),
//! 无鼠标拖滚动条 (滚轮/键盘/点击), 无搜索/过滤命中高亮。

use std::any::Any;
use std::sync::Arc;
use std::time::Instant;

use danqing::widget::{EventResult, MsgQueue, TextInput, Widget};
use danqing::{
    Color, Constraints, Edges, Event, Key, LightTheme, MouseButton, NamedKey, Point, Rect,
    RectBatch, Size, TextBatch, Theme,
};

use danqing::selection::{self, TextSelection};
use danqing_log::expand::{self, ExpandMap};
use danqing_log::jsonl::{self, Column, Schema, SubRow};
use danqing_log::logfile::LogFile;

use crate::{LogApp, Msg, ViewMode};

/// 复制行数上限 (R3 业务护栏): 滚轮甩底可造出全文件选区, 无上限复制 = 逐行
/// 解码 + 逐行分配, UI 冻结分钟级。10 万行 ≈ 16MB 文本, 剪贴板与内存都安全。
/// (text::selection 下沉时留产品侧 —— 框架 copy_text 本身无行数限制。)
const COPY_MAX_LINES: u64 = 100_000;

/// 行高 (逻辑像素)。24 = 可读性底线: 14px 正文上下仍留呼吸, 终端感消失。
pub(crate) const ROW_HEIGHT: f32 = 24.0;
/// 正文字号 (实机验收定档 14: 像素吸附落地后用户拍板; 行高 24 容得下)。
const FONT_SIZE: u16 = 14;
/// 行号/状态栏字号 (12: 竞品基准的可读底线, 11 在白底上偏吃力)。
const AUX_FONT_SIZE: u16 = 12;
/// 行号槽最小宽度。
const GUTTER_MIN: f32 = 56.0;
/// 文本与行号槽间距。
const GUTTER_GAP: f32 = 12.0;
/// 展开标识区宽度 (行首 ▶/▼, 独立于行号槽, 不与行号重叠)。
const EXPAND_W: f32 = 20.0;
/// 展开标识字号 (比正文大一号, 12px 太小看不清)。
const EXPAND_FONT_SIZE: u16 = 16;
/// 底栏状态行高度。
const STATUS_HEIGHT: f32 = 26.0;
/// 过滤栏高度 (表格模式)。
const FILTER_BAR_H: f32 = 32.0;
/// 表头高度 (表格模式)。
const HEADER_H: f32 = 28.0;
/// 列内边距 (含在列宽里, 单元格文本右留 8)。
const COL_PAD: f32 = 16.0;
/// 右侧滚动条宽度。
const SCROLLBAR_W: f32 = 6.0;
/// 滚动条拇指最小高度。
const THUMB_MIN_H: f32 = 24.0;
/// 双击判定窗口 (沿用 danqing title_bar.rs 先例)。
const DOUBLE_CLICK_MS: u128 = 300;
/// 双击位移容差; 同值兼任「按下→框选」升级阈值 (抖动不产选区)。
const CLICK_DIST: f32 = 4.0;

/// header_line 使用框架 LightTheme divider。
fn header_line() -> Color {
    LightTheme.divider()
}
/// INFO / 3xx 蓝。
fn info_fg() -> Color {
    Color::rgb(0.22, 0.46, 0.74)
}
/// 2xx 绿。
fn ok_fg() -> Color {
    Color::rgb(0.16, 0.56, 0.32)
}
/// WARN / 4xx 琥珀。
fn warn_fg() -> Color {
    Color::rgb(0.72, 0.50, 0.02)
}
/// ERROR / 5xx 红。
fn err_fg() -> Color {
    Color::rgb(0.76, 0.21, 0.21)
}
/// DEBUG / TRACE 灰。
fn trace_fg() -> Color {
    Color::rgb(0.56, 0.56, 0.60)
}

/// 日志级别着色: 行前 200 字节内找级别关键字 (日志行级别几乎都在行首)。
fn level_color(line: &[u8]) -> Color {
    let head = &line[..line.len().min(200)];
    // 长词优先: FATAL 含 "AT" 之类子串碰撞无所谓 (都是错误级), 但 WARN 要先于 INFO 判
    let has = |pat: &[u8]| memchr::memmem::find(head, pat).is_some();
    if has(b"FATAL") || has(b"ERROR") {
        Color::rgb(0.75, 0.18, 0.18)
    } else if has(b"WARN") {
        Color::rgb(0.70, 0.50, 0.08)
    } else if has(b"DEBUG") || has(b"TRACE") {
        Color::rgb(0.55, 0.55, 0.58)
    } else {
        Color::rgb(0.12, 0.12, 0.12) // text_default
    }
}

/// level 列单元格着色 (表格模式): INFO 也给蓝 —— 窄列色带是语义扫描线;
/// 原始模式整行着色的降噪策略 (INFO 走默认色) 不同, 两函数有意不共用。
fn level_cell_color(v: &str) -> Color {
    let b = v.as_bytes();
    let has = |pat: &[u8]| memchr::memmem::find(b, pat).is_some();
    if has(b"FATAL") || has(b"ERROR") {
        err_fg()
    } else if has(b"WARN") {
        warn_fg()
    } else if has(b"INFO") {
        info_fg()
    } else if has(b"DEBUG") || has(b"TRACE") {
        trace_fg()
    } else {
        Color::rgb(0.12, 0.12, 0.12) // text_default
    }
}

/// status 列按首数字分段: 2xx 绿 / 3xx 蓝 / 4xx 琥珀 / 5xx 红。
fn status_color(v: &str) -> Color {
    match v.as_bytes().first() {
        Some(b'2') => ok_fg(),
        Some(b'3') => info_fg(),
        Some(b'4') => warn_fg(),
        Some(b'5') => err_fg(),
        _ => Color::rgb(0.12, 0.12, 0.12), // text_default
    }
}

/// 纯数字值判定 (含小数点/负号/千分位): 有资格右对齐。
fn is_numeric(v: &str) -> bool {
    let b = v.as_bytes();
    !b.is_empty()
        && b.iter().any(|c| c.is_ascii_digit())
        && b.iter()
            .all(|c| matches!(c, b'0'..=b'9' | b'.' | b'-' | b','))
}

/// 表格单元格语义配色: level/status 按值分段, 其余一律正文色。
/// (曾有时间戳列/hex 标识符降灰设计, 用户验收判死: 白底小字看不清;
/// klogg/LogViewPlus/Daucloud 三家竞品对 ts/req_id 均一视同仁用正文色。)
fn cell_color(name: &str, v: &str) -> Color {
    if name == "level" || name == "severity" {
        return level_cell_color(v);
    }
    if (name.contains("status") || name == "code")
        && v.as_bytes().first().is_some_and(u8::is_ascii_digit)
    {
        return status_color(v);
    }
    Color::rgb(0.12, 0.12, 0.12) // text_default
}

/// 一行可见窗口的命中几何 (选区 T3): `base_byte` = 左截断起点的解码字节偏移,
/// `offs` = 窗口内逐字符 (内容域 x_end, 字节_end)。只缓存可见窗口字符 ——
/// 用户点不到的不测, 超长行 (minified JSON) 单帧成本有界。
struct RowGeom {
    base_byte: usize,
    offs: Vec<(f32, usize)>,
}

/// 构建一行可见窗口的命中几何: 从 `base` (左截断字节) 起逐字符累计宽度,
/// 越过 `right_bound` (内容域 x) 即停 —— 右缘外字符用户点不到, 不测。
/// `start_x` = base 处的内容域 x (调用方由 scroll_trim 的 sub 换算, 免重复测量)。
fn measure_row_geom(
    texts: &mut TextBatch,
    raw: &str,
    base: usize,
    start_x: f32,
    right_bound: f32,
) -> RowGeom {
    let mut acc = start_x;
    let mut offs = Vec::new();
    for (rel, ch) in raw[base..].char_indices() {
        let end = base + rel + ch.len_utf8();
        acc += texts.measure(&raw[base + rel..end], FONT_SIZE);
        if acc > right_bound {
            break;
        }
        offs.push((acc, end));
    }
    RowGeom {
        base_byte: base,
        offs,
    }
}

/// 行锚定虚拟列表 (整窗唯一组件, 含搜索栏/过滤栏/表头/底栏状态行与滚动条)。
pub(crate) struct LogView {
    file: Option<Arc<LogFile>>,
    /// 是否已打开真实文件 (false = 无参启动空态, 画欢迎提示)。
    has_file: bool,
    /// Loading 占位文案 (async-open: 无旧文件 + job 在途; 空态分支改画打开进度)。
    loading_label: Option<(String, String)>,
    top_row: f64,
    /// 选中的显示行。
    selected: u64,
    status: String,
    mode: ViewMode,
    schema: Option<Arc<Schema>>,
    /// 过滤命中的文件行号 (升序); None = 全量。
    filtered: Option<Arc<Vec<u64>>>,
    // ---- 搜索态 (T5) ----
    /// 已应用模式原文 (变化检测) 与编译缓存 (sync 时按需重编译, paint 零成本)。
    search_pattern_src: Option<String>,
    search_re: Option<regex::bytes::Regex>,
    /// 命中文件行号表 (升序), 高亮成员判定走二分。
    search_hits: Option<Arc<Vec<u64>>>,
    /// 文件数据编码 (高亮前缀宽度测量须与行显示同一解码路径)。
    encoding: danqing::encoding::Encoding,
    /// 书签文件行号 (用户量级, 逐帧克隆无感)。
    bookmarks: std::collections::BTreeSet<u64>,
    // ---- 展开态 (jsonl-table T4) ----
    expanded: ExpandMap,
    sub_rows: std::collections::BTreeMap<u64, Vec<SubRow>>,
    // ---- 水平滚动 (T7, 纯视图态: 应用层不参与) ----
    /// 内容左缘偏移像素 (Cell: paint 只读, 事件写入, paint 防御性回钳)。
    x_offset: std::cell::Cell<f32>,
    /// 迄今见过的最大内容宽 (渲染时边测边长; 滚动范围的下界估计, 诚实边界:
    /// 未探索区域的宽度未知, 与编辑器「边走边长」同构)。
    max_seen: std::cell::Cell<f32>,
    // ---- 设置入口 (S2) ----
    /// 设置按钮 hover 态 (event 写, paint 读)。
    settings_hover: std::cell::Cell<bool>,
    /// 设置按钮矩形 (paint 计算, event 用; Cell 跨 paint/event 共享)。
    settings_btn_rect: std::cell::Cell<Rect>,
    /// 鼠标悬停显示行 (u64::MAX = 无; event 写, paint 读)。
    hover_row: std::cell::Cell<u64>,
    // ---- 文本选区 (T3, 仅原始模式) ----
    /// 文本选区 (锚点/光标点 = 显示行 + 解码字节偏移); None 或空 = 无选区。
    selection: Option<TextSelection>,
    /// 按下待升级 (行, 字节, 屏幕位置): 拖超 CLICK_DIST 升级框选,
    /// 未超即抬起 = 单击 (不产空选区)。
    press: Option<(u64, usize, Point)>,
    /// 框选进行中 (press 已升级; release 落定选区)。
    dragging: bool,
    /// 上次按下 (时刻, 位置) —— 双击判定用。
    last_click: Option<(Instant, Point)>,
    /// 选区命中几何 (paint 写, event 读): 显示行 → 可见窗口逐字符偏移。
    /// event 无 TextBatch, 命中测试与渲染同源靠它 (TextInput char_offsets 同法)。
    row_geom: std::cell::RefCell<std::collections::BTreeMap<u64, RowGeom>>,
    /// 行号槽宽缓存 (paint 算, event 用; measure 只在 paint 可得)。
    gutter_w: std::cell::Cell<f32>,
    /// 组件矩形缓存 (paint 写, 窗口坐标): hit_area 供 set_by_click 聚焦用。
    /// 无它 focusable 形同虚设 —— hit_focusable 只认 hit_area (评审自查发现:
    /// 只加 focusable 不加 hit_area, 点击永不聚焦, Ctrl+C 链路断路)。
    area: std::cell::Cell<Rect>,
    /// 主题模式 (从 LogApp 同步)。
    theme: crate::config::AppTheme,
}

impl LogView {
    pub(crate) fn new() -> Self {
        Self {
            file: None,
            has_file: false,
            loading_label: None,
            top_row: 0.0,
            selected: 0,
            status: String::new(),
            mode: ViewMode::Raw,
            schema: None,
            filtered: None,
            search_pattern_src: None,
            search_re: None,
            search_hits: None,
            encoding: danqing::encoding::Encoding::Utf8,
            bookmarks: std::collections::BTreeSet::new(),
            expanded: ExpandMap::new(),
            sub_rows: std::collections::BTreeMap::new(),
            x_offset: std::cell::Cell::new(0.0),
            max_seen: std::cell::Cell::new(0.0),
            settings_hover: std::cell::Cell::new(false),
            settings_btn_rect: std::cell::Cell::new(Rect::default()),
            hover_row: std::cell::Cell::new(u64::MAX),
            selection: None,
            press: None,
            dragging: false,
            last_click: None,
            row_geom: std::cell::RefCell::new(std::collections::BTreeMap::new()),
            gutter_w: std::cell::Cell::new(GUTTER_MIN),
            area: std::cell::Cell::new(Rect::default()),
            theme: crate::config::AppTheme::Light,
        }
    }

    /// 表格模式 = 模式开关开 + 列定义在手 (构造保证同生同灭, 这里仍取交集防御)。
    fn table_mode(&self) -> bool {
        self.mode == ViewMode::Table && self.schema.is_some()
    }

    /// 顶部 chrome 高度: 过滤/搜索栏是独立 sibling (Column 上面), LogView 不含它;
    /// 表格模式只留表头, 原始模式无 chrome。
    fn chrome_top(&self) -> f32 {
        if self.table_mode() { HEADER_H } else { 0.0 }
    }

    /// 显示行来源 (全量恒等或过滤命中)。
    fn lines(&self) -> expand::Lines<'_> {
        match &self.filtered {
            Some(hits) => expand::Lines::Filtered(hits),
            None => expand::Lines::All {
                total: self.file.as_ref().map(|f| f.line_count()).unwrap_or(0),
            },
        }
    }

    /// 显示行数: 文件行数 + 展开子行数。
    fn display_count(&self) -> u64 {
        expand::display_count(self.lines(), &self.expanded)
    }

    /// 显示行 → (文件行, 子行偏移)。偏移 0 = 文件行本身。
    fn line_at(&self, row: u64) -> (u64, usize) {
        expand::file_line_at(row, self.lines(), &self.expanded).unwrap_or((0, 0))
    }

    /// 选区是否超复制上限 (R3, 2026-09-08 评审): 滚轮甩底可造出全文件选区,
    /// 无上限复制 = 逐行解码 + 逐行分配, UI 冻结分钟级 —— 对「1GB 不卡」
    /// 立身之本的产品是口碑级事故。超限 Ctrl+C 不动作 + 底栏提示。
    fn selection_over_limit(&self) -> bool {
        self.selection.as_ref().is_some_and(|s| {
            let ((r0, _), (r1, _)) = s.ordered();
            r1.saturating_sub(r0) + 1 > COPY_MAX_LINES
        })
    }

    /// 行文本区左键按下的选区处理 (T3)。双击 (300ms/4px, title_bar 先例) =
    /// 空白分隔 token 整选; 单击 = 潜伏锚点 (拖超阈值才升级框选) 且清除旧选区。
    /// 调用方已判定: 原始模式 + 左键 + 已打开文件 + px 在文本区。
    fn handle_text_press(&mut self, area: Rect, position: Point) {
        let now = Instant::now();
        let dbl = self.last_click.is_some_and(|(t, p)| {
            now.duration_since(t).as_millis() <= DOUBLE_CLICK_MS
                && (position.x - p.x).abs() < CLICK_DIST
                && (position.y - p.y).abs() < CLICK_DIST
        });
        if dbl {
            if let Some((r, b)) = self.hit_text(area, position) {
                let (line_no, _) = self.line_at(r);
                let text = self
                    .file
                    .as_ref()
                    .map(|f| f.line_lossy(line_no))
                    .unwrap_or_default();
                let (s, e) = selection::token_at(&text, b);
                self.selection = Some(TextSelection::new((r, s), (r, e)));
            }
            self.press = None;
            self.dragging = false;
            self.last_click = None; // 三连击不链式放大, 重新开始计数
        } else {
            self.press = self.hit_text(area, position).map(|(r, b)| (r, b, position));
            self.dragging = false;
            self.selection = None;
            self.last_click = Some((now, position));
        }
    }

    /// 选区命中测试 (T3): 屏幕坐标 → (显示行, 解码字节偏移)。
    ///
    /// event 无 TextBatch, 字符级命中走 paint 缓存的逐字符几何 ([`Self::row_geom`],
    /// TextInput char_offsets 同法); 与渲染同一解码路径, 宽度天然一致。
    /// 返回 None = 该坐标不可选 (列表区外 / 表格模式 / 展开子行 / 未缓存行)。
    /// 按下路径由调用方挡 gutter 区 (px < text_x 不进选区); 拖动路径的
    /// gutter 内坐标 (content_x 为负) 归 base_byte = 可见窗口首字符 ——
    /// 水平滚动后 != 行首, 语义为「选到最左可见处」。缓存未覆盖的字符区间
    /// (视口右缘外) 归最后可见字符 —— 用户只能选中看得到的文本。
    fn hit_text(&self, area: Rect, pos: Point) -> Option<(u64, usize)> {
        let chrome_top = self.chrome_top();
        let list_h = (area.size.height - chrome_top - STATUS_HEIGHT).max(0.0);
        let rel_y = pos.y - area.origin.y - chrome_top;
        if !(0.0..list_h).contains(&rel_y) {
            return None;
        }
        let row = (self.top_row + f64::from(rel_y / ROW_HEIGHT)) as u64;
        let text_x = area.origin.x + EXPAND_W + self.gutter_w.get() + GUTTER_GAP;
        let content_x = pos.x - text_x + self.x_offset.get();
        let geom = self.row_geom.borrow();
        let g = geom.get(&row)?;
        // 首个 x_end > content_x 的字符即 caret 归属 (TextInput hit_to_index 同语义);
        // 全在左侧 → 末字符后; 全在右侧 (左缘点击) → base_byte。
        let n = g.offs.partition_point(|(x_end, _)| *x_end <= content_x);
        let byte = if n == 0 { g.base_byte } else { g.offs[n - 1].1 };
        Some((row, byte))
    }
}

/// 画一段可能超宽的文本 (截断补省略号), 返回是否截断。
/// 截断逻辑走 [`danqing::fit::fit_line`] (measure 闭包适配 TextBatch)。
fn fit_push(texts: &mut TextBatch, s: &str, max_w: f32, x: f32, baseline: f32, px: u16, c: Color) {
    let (shown, truncated) = danqing::fit::fit_line(s, max_w, |t| texts.measure(t, px));
    texts.push_text(shown, x, baseline, px, c);
    if truncated {
        let w = texts.measure(shown, px);
        texts.push_text("…", x + w, baseline, px, c);
    }
}

/// 水平偏移钳制 (T7): [0, 内容宽 - 视口宽]。独立成函数供单测。
fn clamp_x(x: f32, content_w: f32, viewport_w: f32) -> f32 {
    x.clamp(0.0, (content_w - viewport_w).max(0.0))
}

impl Widget for LogView {
    fn sync(&mut self, state: &dyn Any) {
        let app = state
            .downcast_ref::<LogApp>()
            .expect("LogView 绑定状态类型不匹配");
        // 选区失效守卫: 显示行→内容映射变化 (换文件/过滤变化/切模式) 时
        // 旧选区的 (显示行, 偏移) 不再对应原文, 必须作废 (含潜伏按下);
        // 追加 tail 换入新 Arc 同样触发 —— 保守清除胜过复制出错行。
        let file_changed = self
            .file
            .as_ref()
            .is_none_or(|f| !Arc::ptr_eq(f, &app.file));
        let filtered_changed = match (&self.filtered, &app.filtered) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if file_changed || filtered_changed || self.mode != app.mode {
            self.selection = None;
            self.press = None;
            self.dragging = false;
        }
        self.file = Some(Arc::clone(&app.file));
        self.has_file = app.has_file;
        self.loading_label = app.loading_label.clone();
        self.top_row = app.top_row;
        self.selected = app.selected;
        self.status = app.status.clone();
        self.mode = app.mode;
        self.schema = app.schema.clone();
        self.filtered = app.filtered.clone();
        self.search_hits = app.search.as_ref().map(|n| Arc::clone(n.hits()));
        self.encoding = app.file.encoding();
        self.bookmarks = app.bookmarks.clone();
        self.expanded = app.expanded.clone();
        self.sub_rows = app.sub_rows.clone();
        self.theme = app.theme;
        // 模式变化才重编译 (正则编译 ms 级, 不能进 paint)
        if self.search_pattern_src != app.search_pattern {
            self.search_pattern_src = app.search_pattern.clone();
            self.search_re = app
                .search_pattern
                .as_deref()
                .and_then(|p| regex::bytes::Regex::new(p).ok());
        }
    }

    fn layout(&mut self, constraints: Constraints, _texts: &mut TextBatch) -> Size {
        constraints.max()
    }

    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        self.area.set(area); // hit_area 供 set_by_click 聚焦 (窗口坐标)
        let Some(file) = &self.file else { return };
        let count = self.display_count();
        let table = self.table_mode();
        let th = self.theme.theme();

        // 背景 + 区域划分
        rects.push_rect(area, th.background(), 0.0);
        let chrome_top = self.chrome_top();
        let list_h = (area.size.height - chrome_top - STATUS_HEIGHT).max(0.0);
        let status_y = area.origin.y + chrome_top + list_h;

        // 行号槽宽: 按文件行数位数实测一次 (行号恒为文件真实行号, 与过滤无关)
        let digits = format!("{}", file.line_count()).len();
        let sample = "8".repeat(digits.max(4));
        let gutter_w = (texts.measure(&sample, AUX_FONT_SIZE) + 20.0).max(GUTTER_MIN);
        self.gutter_w.set(gutter_w); // 选区命中 (event 无 TextBatch) 同源
        // 展开标识区 + 行号槽 + 间距
        let text_x = area.origin.x + EXPAND_W + gutter_w + GUTTER_GAP;
        let text_right = area.origin.x + area.size.width - SCROLLBAR_W - 6.0;
        let text_w = (text_right - text_x).max(1.0);
        // 水平偏移 (T7): paint 防御性回钳 (窗口变宽/内容变窄后 offset 可能越界)
        let x_off = clamp_x(self.x_offset.get(), self.max_seen.get(), text_w);
        self.x_offset.set(x_off);

        let line_h = texts.line_height(f32::from(FONT_SIZE));
        let baseline_off = (ROW_HEIGHT - line_h) / 2.0 + texts.ascent(f32::from(FONT_SIZE));
        let aux_line_h = texts.line_height(f32::from(AUX_FONT_SIZE));
        let aux_baseline_off =
            (ROW_HEIGHT - aux_line_h) / 2.0 + texts.ascent(f32::from(AUX_FONT_SIZE));
        let expand_line_h = texts.line_height(f32::from(EXPAND_FONT_SIZE));
        let row_baseline_off =
            (ROW_HEIGHT - expand_line_h) / 2.0 + texts.ascent(f32::from(EXPAND_FONT_SIZE));

        // 表格模式: 列布局 (列宽 = 采样字符宽实测 + 内边距, ≤16 列常量成本;
        // 水平滚动: 列区整体左移, 滚出左右缘的列整列不画)
        let mut cols: Vec<(f32, f32, &Column)> = Vec::new();
        if table {
            let schema = self.schema.as_deref().expect("表格模式必有 schema");
            let mut x = text_x - x_off;
            let mut total_w = 0.0f32;
            for col in &schema.columns {
                let w = texts.measure(&"8".repeat(col.width_chars), FONT_SIZE) + COL_PAD;
                total_w += w;
                if x + w <= text_x {
                    x += w;
                    continue; // 整列滚出左缘
                }
                if x >= text_right {
                    x += w;
                    continue; // 整列滚出右缘 (继续累计 total_w)
                }
                cols.push((x, w, col));
                x += w;
            }
            if total_w > self.max_seen.get() {
                self.max_seen.set(total_w);
            }
            // 表头: 淡灰底与数据区分层 + 底部 1px 线 (表头随列水平滚动, 左缘切断同单元格)
            let hy = area.origin.y;
            rects.push_rect(
                Rect::from_xywh(area.origin.x, hy, area.size.width, HEADER_H - 1.0),
                th.surface_variant(),
                0.0,
            );
            for (cx, cw, col) in &cols {
                let cell_x = cx + 8.0;
                let left_cut = (text_x - cell_x).max(0.0);
                let (shown, sub) =
                    danqing::fit::scroll_trim(&col.name, left_cut, |t| texts.measure(t, FONT_SIZE));
                let draw_x = cell_x + left_cut - sub;
                let max_w = (cx + cw - 8.0).min(text_right) - draw_x;
                if max_w > 0.0 {
                    fit_push(
                        texts,
                        shown,
                        max_w,
                        draw_x,
                        hy + baseline_off,
                        FONT_SIZE,
                        th.text_secondary(),
                    );
                }
            }
            rects.push_rect(
                Rect::from_xywh(area.origin.x, hy + HEADER_H - 1.0, area.size.width, 1.0),
                header_line(),
                0.0,
            );
        }

        // 可见行窗口: 唯一有渲染成本的部分, 与文件大小无关
        let rows_top = area.origin.y + chrome_top;
        let rows_bottom = rows_top + list_h;
        // 选区 (T3): 命中几何随可见窗口逐帧重建; 非空选区存在时行选中视觉让位
        self.row_geom.borrow_mut().clear();
        let has_text_sel = self.selection.as_ref().is_some_and(|s| !s.is_empty());
        // 空态欢迎 (无参启动): 列表区居中两行提示; 行循环 count=0 本就不画
        if !self.has_file {
            let mid_y = rows_top + list_h / 2.0;
            // Loading (async-open): 文件名 + 进度行; 否则欢迎语
            let (title, hint) = match &self.loading_label {
                Some((name, progress)) => (name.as_str(), progress.as_str()),
                None => ("丹青日志 LogLens", "按 Ctrl+O 打开日志文件"),
            };
            let title_w = texts.measure(title, 16);
            texts.push_text(
                title,
                area.origin.x + (area.size.width - title_w) / 2.0,
                mid_y - 12.0,
                16,
                th.text_primary(),
            );
            let hint_w = texts.measure(hint, FONT_SIZE);
            texts.push_text(
                hint,
                area.origin.x + (area.size.width - hint_w) / 2.0,
                mid_y + 12.0,
                FONT_SIZE,
                th.text_secondary(),
            );
        }
        // 裁剪: 行内容不溢出到表头/底栏
        let clip = Rect::from_xywh(area.origin.x, rows_top, area.size.width, list_h);
        rects.push_clip(clip);
        texts.push_clip(clip);
        let first = self.top_row.floor() as u64;
        let frac = (self.top_row - first as f64) as f32;
        let mut i = first;
        loop {
            let y = rows_top + (i - first) as f32 * ROW_HEIGHT - frac * ROW_HEIGHT;
            // 行顶部超出可见区底部 → 停止
            if y >= rows_bottom || i >= count {
                break;
            }
            // 行完全在可见区上方 → 跳过 (滚动时首行可能部分溢出)
            if y + ROW_HEIGHT <= rows_top {
                i += 1;
                continue;
            }
            // 行底色层叠: 斑马纹 (仅表格模式 —— 宽表横向跟踪不串行;
            // 原始模式整行是连续文本, 斑马打断阅读, klogg 基准无斑马;
            // 奇数显示行, 绝对行号奇偶, 滚动时不游动)
            // → 选中 (底色 + 左侧 3px 强调条) / hover (选中行不再叠 hover)
            let row_rect =
                Rect::from_xywh(area.origin.x, y, area.size.width - SCROLLBAR_W, ROW_HEIGHT);
            if table && i % 2 == 1 {
                rects.push_rect(row_rect, th.surface_variant(), 0.0);
            }
            if i == self.selected && !has_text_sel {
                rects.push_rect(row_rect, th.selection(), 0.0);
                rects.push_rect(
                    Rect::from_xywh(area.origin.x, y, 3.0, ROW_HEIGHT),
                    th.accent(),
                    0.0,
                );
            } else if i == self.hover_row.get() {
                rects.push_rect(row_rect, th.surface_variant(), 0.0);
            }
            let (line_no, sub_off) = self.line_at(i);
            // 展开子行: 缩进路径段 = 值, 无行号/列/搜索高亮
            if sub_off > 0 {
                if let Some(row) = self.sub_rows.get(&line_no).and_then(|v| v.get(sub_off - 1)) {
                    let indent = (row.depth as f32 - 1.0) * 16.0;
                    let s = format!("{} = {}", row.label, row.value);
                    let draw_x = text_x + indent;
                    if draw_x < text_right {
                        fit_push(
                            texts,
                            &s,
                            text_right - draw_x,
                            draw_x,
                            y + baseline_off,
                            FONT_SIZE,
                            th.text_secondary(),
                        );
                    }
                }
                i += 1;
                continue;
            }
            // 搜索命中行: 表格模式淡琥珀行底 (原始模式在行内画区间高亮, 见下)
            if table {
                if let Some(hits) = &self.search_hits {
                    if hits.binary_search(&line_no).is_ok() {
                        rects.push_rect(
                            Rect::from_xywh(
                                area.origin.x,
                                y,
                                area.size.width - SCROLLBAR_W,
                                ROW_HEIGHT,
                            ),
                            th.selection(),
                            0.0,
                        );
                    }
                }
            }
            // 行号 (文件真实行号; 书签行金色)
            let no = format!("{}", line_no + 1);
            let no_w = texts.measure(&no, AUX_FONT_SIZE);
            let no_color = if self.bookmarks.contains(&line_no) {
                Color::rgb(0.75, 0.60, 0.10)
            } else {
                th.text_secondary()
            };
            texts.push_text(
                &no,
                area.origin.x + EXPAND_W + gutter_w - 10.0 - no_w,
                y + aux_baseline_off,
                AUX_FONT_SIZE,
                no_color,
            );
            let raw = file.line(line_no);
            // 原始模式: 命中行内区间高亮 (仅命中行跑 regex, 逐可见行恒定成本;
            // 前缀宽度测量与行显示同一解码路径, 宽度一致)
            if !table {
                if let (Some(re), Some(hits)) = (&self.search_re, &self.search_hits) {
                    if hits.binary_search(&line_no).is_ok() {
                        for m in re.find_iter(raw) {
                            let px0 = texts.measure(
                                &danqing::encoding::decode_line(self.encoding, &raw[..m.start()]),
                                FONT_SIZE,
                            );
                            let px1 = px0
                                + texts.measure(
                                    &danqing::encoding::decode_line(
                                        self.encoding,
                                        &raw[m.start()..m.end()],
                                    ),
                                    FONT_SIZE,
                                );
                            let x0 = (text_x + px0 - x_off).max(text_x);
                            let x1 = (text_x + px1 - x_off).min(text_right);
                            if x1 > x0 {
                                rects.push_rect(
                                    Rect::from_xywh(x0, y + 2.0, x1 - x0, ROW_HEIGHT - 4.0),
                                    th.selection(),
                                    2.0,
                                );
                            }
                        }
                    }
                }
            }
            if table {
                // 单元格: 逐可见行 serde_json parse (真 parser, 消除 memmem 内嵌误判),
                // 取顶层字段紧凑显示; level 列按级别着色;
                // 水平滚动: 左缘切断走 scroll_trim (亚字符平滑)
                let parsed = jsonl::parse_line(raw);
                // 展开开关: + 可展开未展开 / - 已展开 (表格模式专属, 独立展开区)
                // (字体是 GB2312 子集, 无 ▶/▼ 几何形, 用 ASCII +- 保可用)
                let expanded_here = self.expanded.is_expanded(line_no);
                let expandable = parsed.as_ref().is_some_and(jsonl::is_expandable);
                if expanded_here || expandable {
                    let glyph = if expanded_here { "−" } else { "+" };
                    texts.push_text(
                        glyph,
                        area.origin.x + 2.0,
                        y + row_baseline_off,
                        EXPAND_FONT_SIZE,
                        th.text_primary(),
                    );
                }
                for (cx, cw, col) in &cols {
                    let Some(v) = parsed
                        .as_ref()
                        .and_then(|p| p.get(col.name.as_str()))
                        .map(jsonl::cell_display)
                    else {
                        continue;
                    };
                    let color = cell_color(&col.name, &v);
                    let cell_x = cx + 8.0;
                    let left_cut = (text_x - cell_x).max(0.0);
                    let right_edge = (cx + cw - 8.0).min(text_right);
                    // 数字右对齐 (量级可一眼比较); 列左缘被切断或文本截断时回落左对齐
                    if left_cut <= 0.0 && is_numeric(&v) {
                        let (shown, truncated) =
                            danqing::fit::fit_line(&v, right_edge - cell_x, |t| {
                                texts.measure(t, FONT_SIZE)
                            });
                        if !truncated {
                            let w = texts.measure(shown, FONT_SIZE);
                            texts.push_text(
                                shown,
                                right_edge - w,
                                y + baseline_off,
                                FONT_SIZE,
                                color,
                            );
                            continue;
                        }
                    }
                    let (shown, sub) =
                        danqing::fit::scroll_trim(&v, left_cut, |t| texts.measure(t, FONT_SIZE));
                    let draw_x = cell_x + left_cut - sub;
                    let max_w = right_edge - draw_x;
                    if max_w > 0.0 {
                        fit_push(
                            texts,
                            shown,
                            max_w,
                            draw_x,
                            y + baseline_off,
                            FONT_SIZE,
                            color,
                        );
                    }
                }
            } else {
                // 原始模式: 整行级别着色 + 水平左截断 (scroll_trim) + 右截断省略;
                // 行内容宽度边测边长 (max_seen, 水平滚动范围估计)
                let raw = file.line_lossy(line_no);
                let color = level_color(raw.as_bytes());
                let full_w = texts.measure(&raw, FONT_SIZE);
                if full_w > self.max_seen.get() {
                    self.max_seen.set(full_w);
                }
                let (shown, sub) =
                    danqing::fit::scroll_trim(&raw, x_off, |t| texts.measure(t, FONT_SIZE));
                // 选区命中几何 (T3): 起点宽 = x_off - sub (scroll_trim 已算),
                // 免一次 O(行长) 前缀测量; event 无 TextBatch, 靠这份缓存同源
                let base = raw.len() - shown.len();
                let geom = measure_row_geom(texts, &raw, base, x_off - sub, x_off + text_w + 64.0);
                self.row_geom.borrow_mut().insert(i, geom);
                // 文本选区区间 (T3): 与命中高亮同法 (measure 前缀→矩形);
                // 在循环末尾才画 = 与同批命中矩形交叠时选区优先
                if let Some(sel) = self.selection.as_ref().filter(|s| !s.is_empty()) {
                    if let Some((b0, b1)) = selection::row_slice(sel, i, &raw) {
                        let x0 =
                            (text_x + texts.measure(&raw[..b0], FONT_SIZE) - x_off).max(text_x);
                        // 整行选中时复用刚算过的 full_w, 省一次 O(行长) 测量
                        let w1 = if b1 == raw.len() {
                            full_w
                        } else {
                            texts.measure(&raw[..b1], FONT_SIZE)
                        };
                        let x1 = (text_x + w1 - x_off).min(text_right);
                        if x1 > x0 {
                            rects.push_rect(
                                Rect::from_xywh(x0, y + 2.0, x1 - x0, ROW_HEIGHT - 4.0),
                                th.selection(),
                                2.0,
                            );
                        }
                    }
                }
                let draw_x = text_x - sub;
                fit_push(
                    texts,
                    shown,
                    text_right - draw_x,
                    draw_x,
                    y + baseline_off,
                    FONT_SIZE,
                    color,
                );
            }
            i += 1;
        }
        rects.pop_clip();
        texts.pop_clip();

        // 水平滚动条 (T7): 内容宽于视口才出现, 列表区底部 6px
        let max_seen = self.max_seen.get();
        if max_seen > text_w && list_h > 0.0 {
            let track_y = rows_top + list_h - 6.0;
            rects.push_rect(
                Rect::from_xywh(text_x, track_y, text_w, 6.0),
                th.divider(),
                3.0,
            );
            let ratio = (text_w / max_seen).min(1.0);
            let thumb_w = (text_w * ratio).max(THUMB_MIN_H).min(text_w);
            let max_x = (max_seen - text_w).max(1.0);
            let thumb_x = text_x + (x_off / max_x) * (text_w - thumb_w);
            rects.push_rect(
                Rect::from_xywh(thumb_x, track_y, thumb_w, 6.0),
                th.border(),
                3.0,
            );
        }

        // 滚动条: 拇指尺寸 ∝ 视口/全文, 位置 ∝ top_row (显示行域)
        // 仅内容溢出视口时出现 (与水平条同规: 全部可见 = 无条)
        let visible = f64::from(list_h / ROW_HEIGHT).max(1.0);
        if count as f64 > visible && list_h > 0.0 {
            let track_x = area.origin.x + area.size.width - SCROLLBAR_W;
            rects.push_rect(
                Rect::from_xywh(track_x, rows_top, SCROLLBAR_W, list_h),
                th.divider(),
                3.0,
            );
            let ratio = ((visible / count as f64) as f32).min(1.0);
            let thumb_h = (list_h * ratio).max(THUMB_MIN_H).min(list_h);
            let max_top = (count as f64 - visible).max(0.0);
            let t = if max_top > 0.0 {
                (self.top_row / max_top).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            let thumb_y = rows_top + t * (list_h - thumb_h);
            rects.push_rect(
                Rect::from_xywh(track_x, thumb_y, SCROLLBAR_W, thumb_h),
                th.border(),
                3.0,
            );
        }

        // 底栏状态行 (打开耗时/过滤统计 = 截图弹药本体); 顶部 1px 线与列表区分层
        rects.push_rect(
            Rect::from_xywh(area.origin.x, status_y, area.size.width, 1.0),
            header_line(),
            0.0,
        );
        let sy =
            status_y + (STATUS_HEIGHT - aux_line_h) / 2.0 + texts.ascent(f32::from(AUX_FONT_SIZE));
        texts.push_text(
            &self.status,
            area.origin.x + 10.0,
            sy,
            AUX_FONT_SIZE,
            th.text_secondary(),
        );
        // 设置入口 (S2): ⚙ 关于 — 位置计数左侧, hover 可辨
        let settings_label = "⚙ 关于";
        let settings_w = texts.measure(settings_label, AUX_FONT_SIZE);
        let settings_x = area.origin.x + area.size.width - SCROLLBAR_W - 10.0 - settings_w;
        let settings_rect =
            Rect::from_xywh(settings_x - 4.0, status_y, settings_w + 8.0, STATUS_HEIGHT);
        self.settings_btn_rect.set(settings_rect);
        let settings_color = if self.settings_hover.get() {
            th.text_primary()
        } else {
            th.text_secondary()
        };
        texts.push_text(
            settings_label,
            settings_x,
            sy,
            AUX_FONT_SIZE,
            settings_color,
        );
        // 位置计数: 设置入口左侧 (空态无意义, 不画)
        let pos = if !self.has_file {
            String::new()
        } else if count == 0 {
            format!("行 0/{count}")
        } else {
            format!("行 {}/{count}", self.selected + 1)
        };
        let pos_w = texts.measure(&pos, AUX_FONT_SIZE);
        let pos_x = settings_x - 16.0 - pos_w;
        texts.push_text(
            &pos,
            pos_x.max(area.origin.x + 10.0),
            sy,
            AUX_FONT_SIZE,
            th.text_secondary(),
        );
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        let chrome_top = self.chrome_top();
        let list_h = (area.size.height - chrome_top - STATUS_HEIGHT).max(0.0);
        match event {
            // hover 跟踪 (设置按钮 + 列表行): 矩形已含绝对坐标 (paint 计算)
            Event::CursorMoved(position) => {
                self.settings_hover
                    .set(self.settings_btn_rect.get().contains(*position));
                let rel_y = position.y - area.origin.y - chrome_top;
                if (0.0..list_h).contains(&rel_y) {
                    self.hover_row
                        .set((self.top_row + f64::from(rel_y / ROW_HEIGHT)) as u64);
                } else {
                    self.hover_row.set(u64::MAX);
                }
                // 框选跟手 (T3): 按下未抬起期间, 超阈值即升级/更新选区;
                // 命中失败 (拖出列表/不可选行) 冻结 caret 在最后有效点
                if let Some((arow, abyte, pos0)) = self.press {
                    let moved = (position.x - pos0.x).abs() > CLICK_DIST
                        || (position.y - pos0.y).abs() > CLICK_DIST;
                    if self.dragging || moved {
                        self.dragging = true;
                        if let Some((crow, cbyte)) = self.hit_text(area, *position) {
                            self.selection = Some(TextSelection::new((arow, abyte), (crow, cbyte)));
                        }
                    }
                }
                EventResult::Ignored // 不消费, 让其他组件也能响应 hover
            }
            Event::CursorLeft => {
                self.settings_hover.set(false);
                self.hover_row.set(u64::MAX);
                // 按下未拖动就离窗 = 放弃潜伏选区; 框选中离窗保留
                // (窗口最大化下边缘拖出是常态, 回窗继续跟手)
                if !self.dragging {
                    self.press = None;
                }
                EventResult::Ignored
            }
            Event::MouseWheel { delta, shift, .. } => {
                // 横滚源 (T7): 触控板直接给 delta.0; 否则 Shift+纵滚
                // (MouseWheel 修饰键是 danqing 打磨寄生新增, 联动改动两仓待提交)
                let dx = if delta.0 != 0.0 {
                    delta.0
                } else if *shift {
                    delta.1
                } else {
                    0.0
                };
                if dx != 0.0 {
                    // event 无 TextBatch, 视口宽用最小行号槽近似; 钳制目标随 max_seen 生长
                    let viewport_w =
                        (area.size.width - GUTTER_MIN - GUTTER_GAP - SCROLLBAR_W - 16.0).max(1.0);
                    self.x_offset.set(clamp_x(
                        self.x_offset.get() - dx * 40.0,
                        self.max_seen.get(),
                        viewport_w,
                    ));
                    return EventResult::Consumed;
                }
                // 与 danqing Scrollable 同向: delta.1 > 0 = 向上滚
                msgs.push(Box::new(Msg::ScrollRows(-f64::from(delta.1) * 3.0)));
                EventResult::Consumed
            }
            Event::MouseInput {
                pressed: true,
                position,
                button,
                ..
            } => {
                // 设置按钮点击 (S2)
                if self.settings_btn_rect.get().contains(*position) {
                    msgs.push(Box::new(Msg::OpenSettings));
                    return EventResult::Consumed;
                }
                let rel_y = position.y - area.origin.y - chrome_top;
                if (0.0..list_h).contains(&rel_y) {
                    let row = (self.top_row + f64::from(rel_y / ROW_HEIGHT)) as u64;
                    // 行首 ▶/▼ 展开开关区 (左 20px, 表格模式); 其余点击选中
                    let in_glyph = self.table_mode() && position.x - area.origin.x < EXPAND_W;
                    if in_glyph {
                        msgs.push(Box::new(Msg::ToggleExpand(row)));
                    } else {
                        msgs.push(Box::new(Msg::Select(row)));
                        // 文本选区 (T3): 仅左键 + 原始模式 (右键不清选区/不污染
                        // 双击判定 —— 评审 O1); 左键落 gutter = 仅行选中并清选区
                        let text_x = area.origin.x + EXPAND_W + self.gutter_w.get() + GUTTER_GAP;
                        if *button == MouseButton::Left && !self.table_mode() && self.has_file {
                            if position.x >= text_x {
                                self.handle_text_press(area, *position);
                            } else {
                                self.selection = None;
                            }
                        }
                    }
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            Event::MouseInput {
                pressed: false,
                button: MouseButton::Left,
                ..
            } => {
                // 左键抬起: 框选落定 / 潜伏按下作废 (单击不产选区)。
                // 引擎指针捕获保证拖出本区域的抬起也路由到此 (danqing R1)。
                if self.press.is_some() || self.dragging {
                    self.press = None;
                    self.dragging = false;
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            Event::Copy => {
                // 框架 Ctrl+C 链路 (handler → 焦点路径 → selected_text 写 arboard)。
                // 有内容才消费; 空选区 Ignored = 剪贴板保持不动。
                // 超限选区 (R3): 不复制并底栏提示 (防逐行解码冻结 UI)。
                if self.selected_text().is_some() {
                    EventResult::Consumed
                } else if self.selection_over_limit() {
                    msgs.push(Box::new(Msg::Notice(format!(
                        "选区超 {} 万行未复制 (防冻结)",
                        COPY_MAX_LINES / 10000
                    ))));
                    EventResult::Ignored
                } else {
                    EventResult::Ignored
                }
            }
            Event::Key {
                key: Key::Named(NamedKey::Escape),
                pressed: true,
                ..
            } => {
                // 有选区时 Esc 清选区; 无选区 Ignored (框架清焦, 现状)
                if self.selection.as_ref().is_some_and(|s| !s.is_empty()) {
                    self.selection = None;
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            _ => EventResult::Ignored,
        }
    }

    /// 持焦 = 接入框架焦点剪贴板链路 (Ctrl+C → Event::Copy → selected_text)。
    /// 持焦后未消费的键经 `App::propagate_unhandled_keys` 回退应用层,
    /// 应用级导航 (j/k/翻页/`/`/b) 不失灵 (danqing T1 opt-in, LogApp 已开启)。
    fn focusable(&self) -> bool {
        true
    }

    fn focus_id(&self) -> Option<&'static str> {
        Some("log-view")
    }

    /// 命中区域 = 组件全矩形 (窗口坐标, paint 缓存)。
    /// set_by_click 的 hit_focusable 只认 hit_area —— 无此实现则
    /// 点击永不聚焦, focusable/Ctrl+C 链路全断 (TextInput 同法)。
    fn hit_area(&self) -> Option<Rect> {
        Some(self.area.get())
    }

    /// 当前可复制文本: 原始模式 = 非空且不超限的文本选区 (跨行 `\n` 拼接);
    /// 表格模式 = 选中行完整原文 (不受单元格截断省略影响; 选中展开子行时
    /// 归父行原文 —— 子行是父行 JSON 的投影, 复制原文不丢信息);
    /// 其余 (原始模式仅行选中) = None —— Ctrl+C 只认文本选区 (spec 决策)。
    fn selected_text(&self) -> Option<String> {
        if let Some(sel) = &self.selection {
            if !sel.is_empty() {
                if self.selection_over_limit() {
                    return None;
                }
                let file = self.file.as_ref()?;
                return Some(selection::copy_text(sel, &|row| {
                    let (line_no, sub_off) = self.line_at(row);
                    if sub_off > 0 {
                        return String::new(); // 展开子行不进选区文本 (防御)
                    }
                    file.line_lossy(line_no).into_owned()
                }));
            }
        }
        if self.table_mode() && self.has_file && self.selected < self.display_count() {
            let file = self.file.as_ref()?;
            let (line_no, _) = self.line_at(self.selected);
            return Some(file.line_lossy(line_no).into_owned());
        }
        None
    }
}

/// 栏内水平内边距。
const BAR_PAD_X: f32 = 10.0;
/// 前缀标签 ("过滤:"/"搜索:") 与输入区间隙。
const BAR_LABEL_GAP: f32 = 8.0;
/// 栏顶部内偏移 (视觉下沉, 避紧贴标题栏底边)。
const BAR_TOP_OFFSET: f32 = 3.0;
/// 「清空输入」绑定闭包: 从应用状态读 clear revision。
type ClearBinding = Box<dyn Fn(&dyn Any) -> u64>;

/// 当前生效的输入角色。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveBar {
    /// 表格模式过滤栏。
    Filter,
    /// 原始模式 + 搜索栏开启。
    Search,
    /// 无栏。
    Hidden,
}

/// 过滤/搜索栏: 同一槽位两种输入 (表格模式过滤, 原始+搜索开搜索)。
///
/// 参考 danqing-clipboard `bottom_bar.rs`: TextInput 作为字段而非 child 节点,
/// 焦点路径落在本容器上, 经 `wants_ime`/`ime_area`/`selected_text`/`hit_area`/
/// `reset_focus` 转发到内部 TextInput (否则 IME / 剪贴板快照 / 命中全部失效)。
/// 清空经 clear revision 原地 clear (不重建实例, 保焦点态)。
pub(crate) struct Bar {
    filter_ti: TextInput,
    search_ti: TextInput,
    /// 已应用过滤 (sync 设占位文字)。
    filter_applied: String,
    /// 已应用搜索查询 (sync 设占位文字)。
    search_query: String,
    /// 应用侧清空 revision (读值变化时原地 clear)。
    filter_clear_binding: Option<ClearBinding>,
    search_clear_binding: Option<ClearBinding>,
    applied_filter_rev: u64,
    applied_search_rev: u64,
    /// 当前生效角色 (sync 计算)。
    active: ActiveBar,
    /// 前缀标签宽度 (paint 测量缓存, event 转发与 paint 的 input_area 须一致,
    /// 否则点击定位光标会偏一个 label 宽)。
    label_width: std::cell::Cell<f32>,
    /// 主题模式 (从 LogApp 同步)。
    theme: crate::config::AppTheme,
}

impl Bar {
    pub(crate) fn new() -> Self {
        Self {
            filter_ti: Self::fresh_filter(),
            search_ti: Self::fresh_search(),
            filter_applied: String::new(),
            search_query: String::new(),
            filter_clear_binding: None,
            search_clear_binding: None,
            applied_filter_rev: 0,
            applied_search_rev: 0,
            active: ActiveBar::Hidden,
            label_width: std::cell::Cell::new(0.0),
            theme: crate::config::AppTheme::Light,
        }
    }

    fn fresh_filter() -> TextInput {
        Self::base_input().placeholder(
            "输入如 level=ERROR status=50* (AND · 尾缀 * 前缀通配) · Enter 应用 · Esc 清除 · Ctrl+T 切回",
            Color::rgb(0.45, 0.45, 0.48), // placeholder_fg
        )
    }

    fn fresh_search() -> TextInput {
        Self::base_input().placeholder(
            "输入正则 · Enter 应用 · Esc 关闭 (GBK/Latin-1 文件退化为字面量)",
            Color::rgb(0.45, 0.45, 0.48), // placeholder_fg
        )
    }

    fn base_input() -> TextInput {
        TextInput::themed(&LightTheme)
            .font_size(FONT_SIZE)
            .chromeless()
            .color(Color::rgb(0.20, 0.20, 0.20)) // filter_fg
            .caret_color(Color::rgb(0.10, 0.10, 0.12)) // caret_fg
            .selection_color(Color::rgba(0.24, 0.42, 0.66, 0.24)) // selection_bg
            .padding(Edges::symmetric(2.0, 0.0))
    }

    /// 绑定过滤清空信号: 应用侧 revision 变化时原地 clear。
    pub(crate) fn bind_clear_filter<S: 'static>(mut self, f: impl Fn(&S) -> u64 + 'static) -> Self {
        self.filter_clear_binding = Some(Box::new(move |state: &dyn Any| {
            let state = state
                .downcast_ref::<S>()
                .expect("Bar::bind_clear_filter 状态类型不匹配");
            f(state)
        }));
        self
    }

    /// 绑定搜索清空信号: 应用侧 revision 变化时原地 clear。
    pub(crate) fn bind_clear_search<S: 'static>(mut self, f: impl Fn(&S) -> u64 + 'static) -> Self {
        self.search_clear_binding = Some(Box::new(move |state: &dyn Any| {
            let state = state
                .downcast_ref::<S>()
                .expect("Bar::bind_clear_search 状态类型不匹配");
            f(state)
        }));
        self
    }

    /// 前缀标签。
    fn label(&self) -> &'static str {
        match self.active {
            ActiveBar::Filter => "过滤:",
            ActiveBar::Search => "搜索:",
            ActiveBar::Hidden => "",
        }
    }

    /// 输入矩形 (label 之后到右缘)。
    fn input_area(&self, area: Rect, label_w: f32) -> Rect {
        let text_x = area.origin.x + BAR_PAD_X + label_w + BAR_LABEL_GAP;
        let w = (area.size.width - (text_x - area.origin.x) - BAR_PAD_X).max(1.0);
        Rect::from_xywh(text_x, area.origin.y, w, area.size.height)
    }

    /// 当前生效输入的引用。
    fn active_input(&self) -> Option<&TextInput> {
        match self.active {
            ActiveBar::Filter => Some(&self.filter_ti),
            ActiveBar::Search => Some(&self.search_ti),
            ActiveBar::Hidden => None,
        }
    }

    /// 当前生效输入的可变引用。
    fn active_input_mut(&mut self) -> Option<&mut TextInput> {
        match self.active {
            ActiveBar::Filter => Some(&mut self.filter_ti),
            ActiveBar::Search => Some(&mut self.search_ti),
            ActiveBar::Hidden => None,
        }
    }
}

impl Default for Bar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Bar {
    fn sync(&mut self, state: &dyn Any) {
        let app = state
            .downcast_ref::<LogApp>()
            .expect("Bar 绑定状态类型不匹配");
        // 生效角色: 表格=过滤, 原始=搜索 (搜索栏始终可见, 不可隐藏); 空态无文件不出栏。
        self.active = if !app.has_file {
            ActiveBar::Hidden
        } else if app.mode == ViewMode::Table {
            ActiveBar::Filter
        } else {
            ActiveBar::Search
        };
        self.theme = app.theme;

        // 清空信号: revision 变化时原地 clear。
        if let Some(binding) = &self.filter_clear_binding {
            let rev = binding(state);
            if rev != self.applied_filter_rev {
                self.applied_filter_rev = rev;
                self.filter_ti.clear();
            }
        }
        if let Some(binding) = &self.search_clear_binding {
            let rev = binding(state);
            if rev != self.applied_search_rev {
                self.applied_search_rev = rev;
                self.search_ti.clear();
            }
        }

        // 应用态 → 占位文字 (空态显示 "已应用" 提示; 有输入时占位消失)。
        self.filter_applied = app.filter_applied.clone();
        self.search_query = app.search_query.clone();
        self.set_filter_placeholder();
        self.set_search_placeholder();
    }

    fn animate(&mut self, ctx: &danqing::AnimationCtx) {
        self.filter_ti.animate(ctx);
        self.search_ti.animate(ctx);
    }

    fn layout(&mut self, constraints: Constraints, texts: &mut TextBatch) -> Size {
        if self.active == ActiveBar::Hidden {
            return Size::new(constraints.max().width, 0.0);
        }
        // 让当前生效的输入框先 layout (缓存 vertical_pad / char_offsets)。
        if let Some(ti) = self.active_input_mut() {
            let _ = ti.layout(constraints, texts);
        }
        Size::new(constraints.max().width, FILTER_BAR_H)
    }

    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        if self.active == ActiveBar::Hidden {
            return;
        }
        let th = self.theme.theme();
        rects.push_rect(
            Rect::from_xywh(area.origin.x, area.origin.y, area.size.width, FILTER_BAR_H),
            th.surface_variant(),
            0.0,
        );
        let line_h = texts.line_height(f32::from(FONT_SIZE));
        let baseline = area.origin.y
            + (FILTER_BAR_H - line_h) / 2.0
            + texts.ascent(f32::from(FONT_SIZE))
            + BAR_TOP_OFFSET;
        let label = self.label();
        texts.push_text(
            label,
            area.origin.x + BAR_PAD_X,
            baseline,
            FONT_SIZE,
            th.text_primary(),
        );
        let label_w = texts.measure(label, FONT_SIZE);
        self.label_width.set(label_w);
        let input_area = self.input_area(area, label_w);
        match self.active {
            ActiveBar::Filter => self.filter_ti.paint(input_area, rects, texts),
            ActiveBar::Search => self.search_ti.paint(input_area, rects, texts),
            ActiveBar::Hidden => {}
        }
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        let active = self.active;
        if active == ActiveBar::Hidden {
            return EventResult::Ignored;
        }
        // 拦截 Enter/Esc/PageUp/PageDown: Enter=应用, Esc=关闭/清除, Page=滚动 (栏聚焦时导航仍可用)。
        if let Event::Key {
            key,
            pressed: true,
            shift,
            ..
        } = event
        {
            match key {
                Key::Named(NamedKey::Enter) => return self.handle_enter(active, *shift, msgs),
                Key::Named(NamedKey::Escape) => return self.handle_escape(active, msgs),
                Key::Named(NamedKey::PageUp) => {
                    msgs.push(Box::new(Msg::ScrollRows(-crate::PAGE_ROWS)));
                    return EventResult::Consumed;
                }
                Key::Named(NamedKey::PageDown) => {
                    msgs.push(Box::new(Msg::ScrollRows(crate::PAGE_ROWS)));
                    return EventResult::Consumed;
                }
                _ => {}
            }
        }
        // 其余转发给当前生效的输入框 (打字/方向键移动光标等)。
        let input_area = self.input_area(area, self.label_width.get());
        let ti = self
            .active_input_mut()
            .expect("active 非 Hidden 必有输入框");
        ti.event(event, input_area, msgs)
    }

    fn focusable(&self) -> bool {
        self.active != ActiveBar::Hidden
    }

    fn focus_id(&self) -> Option<&'static str> {
        Some("log-bar")
    }

    fn wants_ime(&self) -> bool {
        self.active_input().map(|t| t.wants_ime()).unwrap_or(false)
    }

    fn ime_area(&self) -> Option<Rect> {
        self.active_input().and_then(|t| t.ime_area())
    }

    fn selected_text(&self) -> Option<String> {
        self.active_input().and_then(|t| t.selected_text())
    }

    fn hit_area(&self) -> Option<Rect> {
        self.active_input().and_then(|t| t.hit_area())
    }

    fn reset_focus(&mut self) {
        self.filter_ti.reset_focus();
        self.search_ti.reset_focus();
    }
}

impl Bar {
    /// Enter 处理: 过滤=应用; 搜索=空时下/上一命中 (shift), 非空应用。
    fn handle_enter(&self, active: ActiveBar, shift: bool, msgs: &mut MsgQueue) -> EventResult {
        match active {
            ActiveBar::Filter => {
                let q = self.filter_ti.value().trim().to_string();
                msgs.push(Box::new(Msg::ApplyFilter(q)));
            }
            ActiveBar::Search => {
                let q = self.search_ti.value().trim().to_string();
                if q.is_empty() {
                    msgs.push(Box::new(if shift {
                        Msg::SearchPrevHit
                    } else {
                        Msg::SearchNextHit
                    }));
                } else {
                    msgs.push(Box::new(Msg::ApplySearch(q)));
                }
            }
            ActiveBar::Hidden => return EventResult::Ignored,
        }
        EventResult::Consumed
    }

    /// Esc 处理: 栏有内容 → 清内容、保焦点 (方便重新输入);
    /// 栏已空 → 返回 Ignored 让框架清焦, 用户可继续用键导航列表。
    fn handle_escape(&self, active: ActiveBar, msgs: &mut MsgQueue) -> EventResult {
        match active {
            ActiveBar::Filter => {
                if self.filter_ti.value().is_empty() {
                    return EventResult::Ignored;
                }
                msgs.push(Box::new(Msg::ClearFilter));
                EventResult::Consumed
            }
            ActiveBar::Search => {
                if self.search_ti.value().is_empty() {
                    return EventResult::Ignored;
                }
                msgs.push(Box::new(Msg::ClearSearch));
                EventResult::Consumed
            }
            ActiveBar::Hidden => EventResult::Ignored,
        }
    }

    fn set_filter_placeholder(&mut self) {
        if self.filter_applied.is_empty() {
            return;
        }
        let msg = format!("已应用: {} · Esc 清除 · Ctrl+T 切回", self.filter_applied);
        self.filter_ti.set_placeholder(msg);
    }

    fn set_search_placeholder(&mut self) {
        if self.search_query.is_empty() {
            return;
        }
        let msg = format!(
            "{} · Enter 下一命中 · Shift+Enter 上一 · Esc 关闭",
            self.search_query
        );
        self.search_ti.set_placeholder(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_color_prefers_error_over_info() {
        // 行首含 ERROR 与 "informational" 之类干扰时仍判 ERROR
        assert_eq!(
            level_color(b"2026-09-05 INFO ok"),
            Color::rgb(0.12, 0.12, 0.12), // text_default
            "INFO 走默认色"
        );
        let err = level_color(b"2026-09-05 ERROR disk full");
        assert!(err.r > 0.7, "ERROR 判红: {err:?}");
        let fatal = level_color(b"FATAL boom");
        assert!(fatal.r > 0.7, "FATAL 判红");
        let warn = level_color(b"WARN slow query");
        assert!(warn.r > 0.6 && warn.g > 0.4, "WARN 判黄: {warn:?}");
        let dbg = level_color(b"DEBUG cache miss");
        assert!(dbg.r < 0.6, "DEBUG 判灰");
    }

    #[test]
    fn clamp_x_bounds() {
        assert_eq!(clamp_x(-5.0, 1000.0, 100.0), 0.0, "负归零");
        assert_eq!(clamp_x(5000.0, 1000.0, 100.0), 900.0, "越界钳到内容尾");
        assert_eq!(clamp_x(42.0, 1000.0, 100.0), 42.0, "区间内不变");
        assert_eq!(clamp_x(10.0, 50.0, 100.0), 0.0, "内容窄于视口归零");
    }

    #[test]
    fn cell_color_semantics() {
        // level 列: 全级别色带 (INFO 蓝, 与原始模式整行降噪策略不同)
        assert_eq!(cell_color("level", "INFO"), info_fg(), "INFO 蓝");
        assert_eq!(cell_color("level", "ERROR"), err_fg(), "ERROR 红");
        assert_eq!(
            cell_color("severity", "WARN"),
            warn_fg(),
            "severity 同 level"
        );
        // status 列: 按首数字分段, 非数字值不着色
        assert_eq!(cell_color("status", "200"), ok_fg(), "2xx 绿");
        assert_eq!(cell_color("status", "301"), info_fg(), "3xx 蓝");
        assert_eq!(cell_color("http_status", "404"), warn_fg(), "4xx 琥珀");
        assert_eq!(cell_color("status", "503"), err_fg(), "5xx 红");
        assert_eq!(
            cell_color("status", "N/A"),
            Color::rgb(0.12, 0.12, 0.12), // text_default
            "非数字 status 默认色"
        );
        // 其余列一律正文色 (降灰设计已被用户验收判死: 白底小字看不清,
        // klogg/LogViewPlus/Daucloud 对 ts/req_id 均用正文色)
        assert_eq!(cell_color("ts", "2026-09-05"), Color::rgb(0.12, 0.12, 0.12), "ts 正文色");
        assert_eq!(cell_color("msg", "request completed"), Color::rgb(0.12, 0.12, 0.12));
        assert_eq!(
            cell_color("logger", "auth-service"),
            Color::rgb(0.12, 0.12, 0.12),
            "logger 正文色"
        );
        assert_eq!(
            cell_color("path", "/api/v1/orders/84701"),
            Color::rgb(0.12, 0.12, 0.12),
            "path 正文色"
        );
        assert_eq!(
            cell_color("req_id", "1b26690fb26795f6"),
            Color::rgb(0.12, 0.12, 0.12),
            "长 hex 正文色"
        );
        assert_eq!(
            cell_color("trace_id", "550e8400-e29b-41d4-a716-446655440000"),
            Color::rgb(0.12, 0.12, 0.12),
            "UUID 正文色"
        );
    }

    #[test]
    fn is_numeric_gate() {
        assert!(is_numeric("707"), "整数");
        assert!(is_numeric("40.5"), "小数");
        assert!(is_numeric("-3"), "负数");
        assert!(is_numeric("1,234"), "千分位");
        assert!(!is_numeric("1b266"), "hex 标识符不算数字");
        assert!(!is_numeric(""), "空串");
        assert!(!is_numeric("-"), "无数字不算");
        assert!(!is_numeric("200 OK"), "带文本不算");
    }

    #[test]
    fn scroll_trim_prefix_math() {
        let mut texts = TextBatch::new();
        let s = "abcdefghij";
        // 经 danqing::fit::scroll_trim (measure 闭包适配 TextBatch), 验证真实字体测量下适配
        let (whole, sub0) = danqing::fit::scroll_trim(s, 0.0, |t| texts.measure(t, FONT_SIZE));
        assert_eq!(whole, s, "零偏移原样");
        assert_eq!(sub0, 0.0);
        let w5 = texts.measure("abcde", FONT_SIZE);
        let (suf, sub) = danqing::fit::scroll_trim(s, w5, |t| texts.measure(t, FONT_SIZE));
        assert_eq!(suf, "fghij", "恰好 5 字符宽处切断");
        assert!(sub.abs() < 1e-3, "整字符边界无亚偏移: {sub}");
        let char_w = texts.measure("a", FONT_SIZE);
        let (suf, sub) =
            danqing::fit::scroll_trim(s, char_w / 2.0, |t| texts.measure(t, FONT_SIZE));
        assert_eq!(suf, s, "半字符处不切整字符");
        assert!(sub > 0.0 && sub <= char_w, "亚字符偏移平滑: {sub}");
        let (none, _) = danqing::fit::scroll_trim(s, 99999.0, |t| texts.measure(t, FONT_SIZE));
        assert_eq!(none, "", "全滚出为空");
    }

    /// 选区命中: 坐标 → (显示行, 解码字节偏移), 含 gutter 扣除与水平滚动还原。
    /// 几何走 paint 写入的 row_geom 缓存 (测试中手工预填, 与渲染同源约定)。
    #[test]
    fn hit_text_maps_coords_through_gutter_and_xoff() {
        let v = LogView::new();
        v.gutter_w.set(56.0);
        // 行 0: 3 字符, 每字符 10px (内容域 x_end, 字节_end)
        v.row_geom.borrow_mut().insert(
            0,
            RowGeom {
                base_byte: 0,
                offs: vec![(10.0, 1), (20.0, 2), (30.0, 3)],
            },
        );
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        // text_x = 16 + 56 + 12 = 84; 点内容 x=15 → caret 在第 2 字符前 (byte 1)
        assert_eq!(v.hit_text(area, Point::new(84.0 + 15.0, 5.0)), Some((0, 1)));
        // 点在行首 → 首个缓存字符以左 → base_byte
        assert_eq!(v.hit_text(area, Point::new(84.0, 5.0)), Some((0, 0)));
        // 水平滚动后: content_x = px - text_x + x_off = 15 + 100 = 115 → 超末字符 → byte 3
        v.x_offset.set(100.0);
        assert_eq!(v.hit_text(area, Point::new(84.0 + 15.0, 5.0)), Some((0, 3)));
        // 未缓存行 (表格/展开子行/空白行区) → None (不可选)
        assert_eq!(
            v.hit_text(area, Point::new(84.0 + 15.0, 5.0 + ROW_HEIGHT)),
            None
        );
        // 列表区外 (状态栏) → None
        assert_eq!(v.hit_text(area, Point::new(84.0, 599.0)), None);
    }

    /// 左截断窗口: base_byte 之后才是缓存字符, 左缘点击归 base_byte (不乱指行首)。
    #[test]
    fn hit_text_respects_trimmed_window_base() {
        let v = LogView::new();
        v.gutter_w.set(56.0);
        // 左滚 200px 后, 可见窗口从 byte 20 起, 首字符 x_end=205
        v.row_geom.borrow_mut().insert(
            0,
            RowGeom {
                base_byte: 20,
                offs: vec![(205.0, 21), (215.0, 22)],
            },
        );
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        // content_x = 15 - 84... 直接给: px=text_x → content_x = 0 + x_off
        v.x_offset.set(200.0);
        // content_x = 200 → 全缓存字符在右 → base_byte 20 (不是 0!)
        assert_eq!(v.hit_text(area, Point::new(84.0, 5.0)), Some((0, 20)));
        // content_x = 210 → caret 在 byte 21 字符后 → 21
        assert_eq!(
            v.hit_text(area, Point::new(84.0 + 10.0, 5.0)),
            Some((0, 21))
        );
    }

    /// 复制接线 (T4): 原始模式 = 文本选区拼文本; 表格模式 = 选中行完整原文;
    /// 空选区/原始仅行选中 = None (Ctrl+C 不动作, 剪贴板不动)。
    #[test]
    fn selected_text_raw_selection_and_table_row() {
        let path = std::env::temp_dir().join(format!("danqing-log-sel-{}.log", std::process::id()));
        std::fs::write(&path, "aaa bbb\nccc\nddd eee\n").unwrap();
        let file = LogFile::open(&path).unwrap();
        let mut v = LogView::new();
        v.file = Some(Arc::new(file));
        v.has_file = true;
        // 原始模式: 跨行选区 → 首行后缀 + 末行前缀
        v.selection = Some(TextSelection::new((0, 4), (1, 2)));
        assert_eq!(v.selected_text().as_deref(), Some("bbb\ncc"));
        // 反向选区同内容
        v.selection = Some(TextSelection::new((1, 2), (0, 4)));
        assert_eq!(v.selected_text().as_deref(), Some("bbb\ncc"));
        // 空选区 → None
        v.selection = Some(TextSelection::new((0, 1), (0, 1)));
        assert_eq!(v.selected_text(), None);
        // 原始模式仅行选中 → None (spec: Ctrl+C 只认文本选区)
        v.selection = None;
        v.selected = 2;
        assert_eq!(v.selected_text(), None);
        // 表格模式: 选中行完整原文 (行内容不经单元格截断)
        v.mode = ViewMode::Table;
        v.schema = Some(Arc::new(Schema {
            columns: vec![Column {
                name: "x".into(),
                width_chars: 4,
            }],
        }));
        assert_eq!(v.selected_text().as_deref(), Some("ddd eee"));
        std::fs::remove_file(&path).ok();
    }

    /// Esc 语义 (T4): 有选区 → 清选区并消费; 无选区 → Ignored (框架清焦)。
    #[test]
    fn esc_clears_selection_only_when_present() {
        let mut v = LogView::new();
        let esc = Event::Key {
            key: Key::Named(NamedKey::Escape),
            pressed: true,
            ctrl: false,
            shift: false,
            alt: false,
        };
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut msgs = danqing::widget::MsgQueue::new();
        // 无选区: Ignored (框架随后清焦点)
        assert_eq!(v.event(&esc, area, &mut msgs), EventResult::Ignored);
        // 空选区同样 Ignored
        v.selection = Some(TextSelection::new((1, 1), (1, 1)));
        assert_eq!(v.event(&esc, area, &mut msgs), EventResult::Ignored);
        // 非空选区: 清除并消费
        v.selection = Some(TextSelection::new((0, 0), (1, 2)));
        assert_eq!(v.event(&esc, area, &mut msgs), EventResult::Consumed);
        assert_eq!(v.selection, None);
    }

    /// 框选状态机 (R1 回归): 单击 (未超阈值抬起) 不产选区;
    /// 拖超阈值选区成形, 抬起后落定且按下态清空。
    #[test]
    fn drag_state_machine_forms_and_settles_selection() {
        let mut v = LogView::new();
        v.has_file = true;
        v.gutter_w.set(56.0);
        v.row_geom.borrow_mut().insert(
            0,
            RowGeom {
                base_byte: 0,
                offs: vec![(10.0, 1), (20.0, 2), (30.0, 3)],
            },
        );
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut msgs = danqing::widget::MsgQueue::new();
        let input = |pressed: bool, x: f32, y: f32| Event::MouseInput {
            button: MouseButton::Left,
            pressed,
            position: Point::new(x, y),
        };
        // 单击: 按下即抬起 (未超阈值) → 不产选区, 按下态清空
        // (与后续框选按下拉开 >4px, 避免触发双击判定)
        v.event(&input(true, 200.0, 5.0), area, &mut msgs);
        v.event(&input(false, 200.0, 5.0), area, &mut msgs);
        assert_eq!(v.selection, None);
        assert!(v.press.is_none());
        // 框选: 拖超 4px → 选区成形 (锚点 byte0 → caret byte1)
        v.event(&input(true, 89.0, 5.0), area, &mut msgs);
        v.event(&Event::CursorMoved(Point::new(99.0, 5.0)), area, &mut msgs);
        assert_eq!(v.selection, Some(TextSelection::new((0, 0), (0, 1))));
        assert!(v.dragging);
        // 抬起 → 落定: 选区保留, 按下/框选态清空 (粘滞回归钉死)
        v.event(&input(false, 99.0, 5.0), area, &mut msgs);
        assert_eq!(v.selection, Some(TextSelection::new((0, 0), (0, 1))));
        assert!(v.press.is_none());
        assert!(!v.dragging);
        // 右键按下: 不清既有选区、不产潜伏锚点、不覆写双击记录 (O1 钉死)
        let lc = v.last_click;
        v.event(
            &Event::MouseInput {
                button: MouseButton::Right,
                pressed: true,
                position: Point::new(200.0, 5.0),
            },
            area,
            &mut msgs,
        );
        assert_eq!(v.selection, Some(TextSelection::new((0, 0), (0, 1))));
        assert!(v.press.is_none(), "右键不产潜伏锚点");
        assert_eq!(v.last_click, lc, "右键不污染双击判定");
    }
}
