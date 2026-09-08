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

use danqing::widget::{EventResult, MsgQueue, TextInput, Widget};
use danqing::{
    Color, Constraints, Edges, Event, Key, LightTheme, NamedKey, Rect, RectBatch, Size, TextBatch,
};

use danqing_log::expand::{self, ExpandMap};
use danqing_log::jsonl::{self, Column, Schema, SubRow};
use danqing_log::logfile::LogFile;

use crate::{LogApp, Msg, ViewMode};

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
const EXPAND_W: f32 = 16.0;
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

/// 浅色基底 (白底日志视图)。
fn bg() -> Color {
    Color::rgb(0.98, 0.98, 0.98)
}
fn text_default() -> Color {
    Color::rgb(0.12, 0.12, 0.12)
}
fn gutter_fg() -> Color {
    // 行号可降权但不能淡到看不清 (0.65 白底教训; klogg 行号近正文色)
    Color::rgb(0.45, 0.45, 0.45)
}
fn selection_bg() -> Color {
    Color::rgba(0.24, 0.42, 0.66, 0.24)
}
fn status_fg() -> Color {
    // 底栏/展开子行: 0.40 在白底小字下临界, 加深到近正文
    Color::rgb(0.25, 0.25, 0.25)
}
fn scrollbar_track() -> Color {
    Color::rgba(0.0, 0.0, 0.0, 0.05)
}
fn scrollbar_thumb() -> Color {
    Color::rgba(0.0, 0.0, 0.0, 0.18)
}
fn filter_bar_bg() -> Color {
    Color::rgba(0.0, 0.0, 0.0, 0.04)
}
fn filter_fg() -> Color {
    // 输入文本与前缀标签: 0.30 仍偏浅, 对齐正文对比度
    Color::rgb(0.20, 0.20, 0.20)
}
fn header_fg() -> Color {
    Color::rgb(0.35, 0.35, 0.38)
}
fn header_line() -> Color {
    Color::rgba(0.0, 0.0, 0.0, 0.10)
}
/// 搜索命中行内区间底色 (琥珀)。
fn hit_bg() -> Color {
    Color::rgba(0.95, 0.75, 0.10, 0.35)
}
/// 表格模式命中行底色 (淡琥珀; 行内区间高亮只在原始模式)。
fn hit_row_bg() -> Color {
    Color::rgba(0.95, 0.75, 0.10, 0.12)
}
/// 斑马纹 (奇数显示行, 仅表格模式): 宽表横向跟踪不串行。
fn zebra_bg() -> Color {
    Color::rgba(0.0, 0.0, 0.0, 0.025)
}
/// 鼠标悬停行底色。
fn hover_bg() -> Color {
    Color::rgba(0.0, 0.0, 0.0, 0.045)
}
/// 强调蓝 (选中行左侧竖条)。
fn accent() -> Color {
    Color::rgb(0.24, 0.42, 0.66)
}
/// 表头底色 (与数据区轻分隔)。
fn header_bg() -> Color {
    Color::rgba(0.0, 0.0, 0.0, 0.04)
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
        text_default()
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
        text_default()
    }
}

/// status 列按首数字分段: 2xx 绿 / 3xx 蓝 / 4xx 琥珀 / 5xx 红。
fn status_color(v: &str) -> Color {
    match v.as_bytes().first() {
        Some(b'2') => ok_fg(),
        Some(b'3') => info_fg(),
        Some(b'4') => warn_fg(),
        Some(b'5') => err_fg(),
        _ => text_default(),
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
    text_default()
}

/// 行锚定虚拟列表 (整窗唯一组件, 含搜索栏/过滤栏/表头/底栏状态行与滚动条)。
pub(crate) struct LogView {
    file: Option<Arc<LogFile>>,
    /// 是否已打开真实文件 (false = 无参启动空态, 画欢迎提示)。
    has_file: bool,
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
    encoding: danqing_log::encoding::Encoding,
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
}

impl LogView {
    pub(crate) fn new() -> Self {
        Self {
            file: None,
            has_file: false,
            top_row: 0.0,
            selected: 0,
            status: String::new(),
            mode: ViewMode::Raw,
            schema: None,
            filtered: None,
            search_pattern_src: None,
            search_re: None,
            search_hits: None,
            encoding: danqing_log::encoding::Encoding::Utf8,
            bookmarks: std::collections::BTreeSet::new(),
            expanded: ExpandMap::new(),
            sub_rows: std::collections::BTreeMap::new(),
            x_offset: std::cell::Cell::new(0.0),
            max_seen: std::cell::Cell::new(0.0),
            settings_hover: std::cell::Cell::new(false),
            settings_btn_rect: std::cell::Cell::new(Rect::default()),
            hover_row: std::cell::Cell::new(u64::MAX),
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
}

/// 截断到可用宽度: 先整行测量, 超宽则二分最长可容纳前缀 (字符边界对齐)。
/// 返回 (展示切片, 是否被截断) —— 截断时调用方补省略号由 paint 统一处理。
fn fit_line<'a>(texts: &mut TextBatch, s: &'a str, max_w: f32, px: u16) -> (&'a str, bool) {
    if texts.measure(s, px) <= max_w {
        return (s, false);
    }
    let mut lo = 0usize;
    let mut hi = s.len();
    while lo < hi {
        let mut mid = (lo + hi).div_ceil(2);
        while !s.is_char_boundary(mid) {
            mid -= 1;
        }
        if texts.measure(&s[..mid], px) <= max_w {
            lo = mid;
        } else {
            hi = mid.saturating_sub(1);
            while hi > 0 && !s.is_char_boundary(hi) {
                hi -= 1;
            }
        }
    }
    // 给省略号腾位
    let ell = texts.measure("…", px);
    while lo > 0 && texts.measure(&s[..lo], px) + ell > max_w {
        lo -= 1;
        while !s.is_char_boundary(lo) {
            lo -= 1;
        }
    }
    (&s[..lo], true)
}

/// 画一段可能超宽的文本 (截断补省略号), 返回是否截断。
fn fit_push(texts: &mut TextBatch, s: &str, max_w: f32, x: f32, baseline: f32, px: u16, c: Color) {
    let (shown, truncated) = fit_line(texts, s, max_w, px);
    texts.push_text(shown, x, baseline, px, c);
    if truncated {
        let w = texts.measure(shown, px);
        texts.push_text("…", x + w, baseline, px, c);
    }
}

/// 水平滚动左截断 (T7): 内容左缘在 w 像素处被切断, 返回 (后缀, 亚字符偏移)。
/// 调用方把后缀画在 `x - sub` —— 亚像素平滑, 无需裁剪层 (danqing 无 scissor,
/// 直接画负 x 会压行号槽)。
fn scroll_trim<'a>(texts: &mut TextBatch, s: &'a str, w: f32, px: u16) -> (&'a str, f32) {
    if w <= 0.0 {
        return (s, 0.0);
    }
    // 二分: 最长宽度 ≤ w 的前缀
    let mut lo = 0usize;
    let mut hi = s.len();
    while lo < hi {
        let mut mid = (lo + hi).div_ceil(2);
        while !s.is_char_boundary(mid) {
            mid -= 1;
        }
        if texts.measure(&s[..mid], px) <= w {
            lo = mid;
        } else {
            hi = mid.saturating_sub(1);
            while hi > 0 && !s.is_char_boundary(hi) {
                hi -= 1;
            }
        }
    }
    let sub = w - texts.measure(&s[..lo], px);
    (&s[lo..], sub.max(0.0))
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
        self.file = Some(Arc::clone(&app.file));
        self.has_file = app.has_file;
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
        let Some(file) = &self.file else { return };
        let count = self.display_count();
        let table = self.table_mode();

        // 背景 + 区域划分
        rects.push_rect(area, bg(), 0.0);
        let chrome_top = self.chrome_top();
        let list_h = (area.size.height - chrome_top - STATUS_HEIGHT).max(0.0);
        let status_y = area.origin.y + chrome_top + list_h;

        // 行号槽宽: 按文件行数位数实测一次 (行号恒为文件真实行号, 与过滤无关)
        let digits = format!("{}", file.line_count()).len();
        let sample = "8".repeat(digits.max(4));
        let gutter_w = (texts.measure(&sample, AUX_FONT_SIZE) + 20.0).max(GUTTER_MIN);
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
                header_bg(),
                0.0,
            );
            for (cx, cw, col) in &cols {
                let cell_x = cx + 8.0;
                let left_cut = (text_x - cell_x).max(0.0);
                let (shown, sub) = scroll_trim(texts, &col.name, left_cut, FONT_SIZE);
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
                        header_fg(),
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
        // 空态欢迎 (无参启动): 列表区居中两行提示; 行循环 count=0 本就不画
        if !self.has_file {
            let mid_y = rows_top + list_h / 2.0;
            let title = "丹青日志 LogLens";
            let hint = "按 Ctrl+O 打开日志文件";
            let title_w = texts.measure(title, 16);
            texts.push_text(
                title,
                area.origin.x + (area.size.width - title_w) / 2.0,
                mid_y - 12.0,
                16,
                text_default(),
            );
            let hint_w = texts.measure(hint, FONT_SIZE);
            texts.push_text(
                hint,
                area.origin.x + (area.size.width - hint_w) / 2.0,
                mid_y + 12.0,
                FONT_SIZE,
                gutter_fg(),
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
                rects.push_rect(row_rect, zebra_bg(), 0.0);
            }
            if i == self.selected {
                rects.push_rect(row_rect, selection_bg(), 0.0);
                rects.push_rect(
                    Rect::from_xywh(area.origin.x, y, 3.0, ROW_HEIGHT),
                    accent(),
                    0.0,
                );
            } else if i == self.hover_row.get() {
                rects.push_rect(row_rect, hover_bg(), 0.0);
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
                            status_fg(),
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
                            hit_row_bg(),
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
                gutter_fg()
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
                                &danqing_log::encoding::decode_line(
                                    self.encoding,
                                    &raw[..m.start()],
                                ),
                                FONT_SIZE,
                            );
                            let px1 = px0
                                + texts.measure(
                                    &danqing_log::encoding::decode_line(
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
                                    hit_bg(),
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
                    let glyph = if expanded_here { "-" } else { "+" };
                    texts.push_text(
                        glyph,
                        area.origin.x + 3.0,
                        y + aux_baseline_off,
                        AUX_FONT_SIZE,
                        text_default(),
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
                            fit_line(texts, &v, right_edge - cell_x, FONT_SIZE);
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
                    let (shown, sub) = scroll_trim(texts, &v, left_cut, FONT_SIZE);
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
                let (shown, sub) = scroll_trim(texts, &raw, x_off, FONT_SIZE);
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
                scrollbar_track(),
                3.0,
            );
            let ratio = (text_w / max_seen).min(1.0);
            let thumb_w = (text_w * ratio).max(THUMB_MIN_H).min(text_w);
            let max_x = (max_seen - text_w).max(1.0);
            let thumb_x = text_x + (x_off / max_x) * (text_w - thumb_w);
            rects.push_rect(
                Rect::from_xywh(thumb_x, track_y, thumb_w, 6.0),
                scrollbar_thumb(),
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
                scrollbar_track(),
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
                scrollbar_thumb(),
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
            status_fg(),
        );
        // 设置入口 (S2): ⚙ 关于 — 位置计数左侧, hover 可辨
        let settings_label = "⚙ 关于";
        let settings_w = texts.measure(settings_label, AUX_FONT_SIZE);
        let settings_x = area.origin.x + area.size.width - SCROLLBAR_W - 10.0 - settings_w;
        let settings_rect =
            Rect::from_xywh(settings_x - 4.0, status_y, settings_w + 8.0, STATUS_HEIGHT);
        self.settings_btn_rect.set(settings_rect);
        let settings_color = if self.settings_hover.get() {
            text_default()
        } else {
            status_fg()
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
            status_fg(),
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
                EventResult::Ignored // 不消费, 让其他组件也能响应 hover
            }
            Event::CursorLeft => {
                self.settings_hover.set(false);
                self.hover_row.set(u64::MAX);
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
                    // 行首 ▶/▼ 展开开关区 (左 16px, 表格模式); 其余点击选中
                    let in_glyph = self.table_mode() && position.x - area.origin.x < 16.0;
                    if in_glyph {
                        msgs.push(Box::new(Msg::ToggleExpand(row)));
                    } else {
                        msgs.push(Box::new(Msg::Select(row)));
                    }
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            _ => EventResult::Ignored,
        }
    }
}

/// 栏内水平内边距。
const BAR_PAD_X: f32 = 10.0;
/// 前缀标签 ("过滤:"/"搜索:") 与输入区间隙。
const BAR_LABEL_GAP: f32 = 8.0;
/// 光标色 (浅色栏上深色)。
fn caret_fg() -> Color {
    Color::rgb(0.10, 0.10, 0.12)
}
/// 占位文字色 (可降权但 0.55 在白底 13px 下看不清, 用户验收打回)。
fn placeholder_fg() -> Color {
    Color::rgb(0.45, 0.45, 0.48)
}

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
        }
    }

    fn fresh_filter() -> TextInput {
        Self::base_input().placeholder(
            "输入如 level=ERROR status=50* (AND · 尾缀 * 前缀通配) · Enter 应用 · Esc 清除 · Ctrl+T 切回",
            placeholder_fg(),
        )
    }

    fn fresh_search() -> TextInput {
        Self::base_input().placeholder(
            "输入正则 · Enter 应用 · Esc 关闭 (GBK/Latin-1 文件退化为字面量)",
            placeholder_fg(),
        )
    }

    fn base_input() -> TextInput {
        TextInput::themed(&LightTheme)
            .font_size(FONT_SIZE)
            .chromeless()
            .color(filter_fg())
            .caret_color(caret_fg())
            .selection_color(selection_bg())
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
        rects.push_rect(
            Rect::from_xywh(area.origin.x, area.origin.y, area.size.width, FILTER_BAR_H),
            filter_bar_bg(),
            0.0,
        );
        let line_h = texts.line_height(f32::from(FONT_SIZE));
        let baseline =
            area.origin.y + (FILTER_BAR_H - line_h) / 2.0 + texts.ascent(f32::from(FONT_SIZE));
        let label = self.label();
        texts.push_text(
            label,
            area.origin.x + BAR_PAD_X,
            baseline,
            FONT_SIZE,
            filter_fg(),
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

    /// Esc 处理: 搜索=关闭, 过滤=清除。返回 Ignored 让框架清焦
    /// (焦点路由中 Esc 不被焦点组件消费 ⇒ 清焦, 用户可继续用键导航列表)。
    fn handle_escape(&self, active: ActiveBar, msgs: &mut MsgQueue) -> EventResult {
        match active {
            ActiveBar::Filter => msgs.push(Box::new(Msg::ClearFilter)),
            ActiveBar::Search => msgs.push(Box::new(Msg::ClearSearch)),
            ActiveBar::Hidden => return EventResult::Ignored,
        }
        EventResult::Ignored
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
            text_default(),
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
            text_default(),
            "非数字 status 默认色"
        );
        // 其余列一律正文色 (降灰设计已被用户验收判死: 白底小字看不清,
        // klogg/LogViewPlus/Daucloud 对 ts/req_id 均用正文色)
        assert_eq!(cell_color("ts", "2026-09-05"), text_default(), "ts 正文色");
        assert_eq!(cell_color("msg", "request completed"), text_default());
        assert_eq!(
            cell_color("logger", "auth-service"),
            text_default(),
            "logger 正文色"
        );
        assert_eq!(
            cell_color("path", "/api/v1/orders/84701"),
            text_default(),
            "path 正文色"
        );
        assert_eq!(
            cell_color("req_id", "1b26690fb26795f6"),
            text_default(),
            "长 hex 正文色"
        );
        assert_eq!(
            cell_color("trace_id", "550e8400-e29b-41d4-a716-446655440000"),
            text_default(),
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
        let (whole, sub0) = scroll_trim(&mut texts, s, 0.0, FONT_SIZE);
        assert_eq!(whole, s, "零偏移原样");
        assert_eq!(sub0, 0.0);
        let w5 = texts.measure("abcde", FONT_SIZE);
        let (suf, sub) = scroll_trim(&mut texts, s, w5, FONT_SIZE);
        assert_eq!(suf, "fghij", "恰好 5 字符宽处切断");
        assert!(sub.abs() < 1e-3, "整字符边界无亚偏移: {sub}");
        let char_w = texts.measure("a", FONT_SIZE);
        let (suf, sub) = scroll_trim(&mut texts, s, char_w / 2.0, FONT_SIZE);
        assert_eq!(suf, s, "半字符处不切整字符");
        assert!(sub > 0.0 && sub <= char_w, "亚字符偏移平滑: {sub}");
        let (none, _) = scroll_trim(&mut texts, s, 99999.0, FONT_SIZE);
        assert_eq!(none, "", "全滚出为空");
    }
}
