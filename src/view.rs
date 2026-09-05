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
//! 双模式:
//! - 原始模式: 整行文本 + 级别着色, 与 POC v0 一致;
//! - 表格模式 (前提②): [过滤栏 32px][表头 24px][虚拟化行][状态栏 26px] 四区,
//!   列定义来自 JSONL 采样 (jsonl::Schema), 单元格经 memmem 字段提取 (零 parse),
//!   显示行→文件行号经 filtered 命中表映射 (无过滤时恒等), 行号槽恒显真实行号。
//!
//! POC 边界: 无水平滚动 (超宽单元格截断省略, 溢出右缘的列整列不画),
//! 无鼠标拖滚动条 (滚轮/键盘/点击), 无搜索/过滤命中高亮。

use std::any::Any;
use std::sync::Arc;

use danqing::widget::{EventResult, MsgQueue, Widget};
use danqing::{Color, Constraints, Event, Rect, RectBatch, Size, TextBatch};

use danqing_log::expand::{self, ExpandMap};
use danqing_log::jsonl::{self, Column, Schema, SubRow};
use danqing_log::logfile::LogFile;

use crate::{LogApp, Msg, ViewMode};

/// 行高 (逻辑像素)。
pub(crate) const ROW_HEIGHT: f32 = 20.0;
/// 正文字号。
const FONT_SIZE: u16 = 13;
/// 行号/状态栏字号。
const AUX_FONT_SIZE: u16 = 11;
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
const HEADER_H: f32 = 24.0;
/// 列内边距 (含在列宽里, 单元格文本右留 8)。
const COL_PAD: f32 = 16.0;
/// 右侧滚动条宽度。
const SCROLLBAR_W: f32 = 6.0;
/// 滚动条拇指最小高度。
const THUMB_MIN_H: f32 = 24.0;

/// 暗色基底 (VS Code 系: 开发者日志场景心智)。
fn bg() -> Color {
    Color::rgb(0.118, 0.118, 0.145)
}
fn text_default() -> Color {
    Color::rgb(0.83, 0.83, 0.83)
}
fn gutter_fg() -> Color {
    Color::rgb(0.38, 0.40, 0.44)
}
fn selection_bg() -> Color {
    Color::rgba(0.24, 0.42, 0.66, 0.35)
}
fn status_fg() -> Color {
    Color::rgb(0.55, 0.57, 0.62)
}
fn scrollbar_track() -> Color {
    Color::rgba(1.0, 1.0, 1.0, 0.04)
}
fn scrollbar_thumb() -> Color {
    Color::rgba(1.0, 1.0, 1.0, 0.18)
}
fn filter_bar_bg() -> Color {
    Color::rgba(1.0, 1.0, 1.0, 0.035)
}
fn filter_fg() -> Color {
    Color::rgb(0.75, 0.78, 0.82)
}
fn header_fg() -> Color {
    Color::rgb(0.62, 0.65, 0.72)
}
fn header_line() -> Color {
    Color::rgba(1.0, 1.0, 1.0, 0.08)
}
/// 搜索命中行内区间底色 (琥珀)。
fn hit_bg() -> Color {
    Color::rgba(0.95, 0.75, 0.25, 0.30)
}
/// 表格模式命中行底色 (淡琥珀; 行内区间高亮只在原始模式)。
fn hit_row_bg() -> Color {
    Color::rgba(0.95, 0.75, 0.25, 0.08)
}

/// 日志级别着色: 行前 200 字节内找级别关键字 (日志行级别几乎都在行首)。
fn level_color(line: &[u8]) -> Color {
    let head = &line[..line.len().min(200)];
    // 长词优先: FATAL 含 "AT" 之类子串碰撞无所谓 (都是错误级), 但 WARN 要先于 INFO 判
    let has = |pat: &[u8]| memchr::memmem::find(head, pat).is_some();
    if has(b"FATAL") || has(b"ERROR") {
        Color::rgb(0.95, 0.30, 0.30)
    } else if has(b"WARN") {
        Color::rgb(0.85, 0.65, 0.13)
    } else if has(b"DEBUG") || has(b"TRACE") {
        Color::rgb(0.50, 0.52, 0.56)
    } else {
        text_default()
    }
}

/// 行锚定虚拟列表 (整窗唯一组件, 含搜索栏/过滤栏/表头/底栏状态行与滚动条)。
pub(crate) struct LogView {
    file: Option<Arc<LogFile>>,
    top_row: f64,
    /// 选中的显示行。
    selected: u64,
    status: String,
    mode: ViewMode,
    schema: Option<Arc<Schema>>,
    /// 过滤命中的文件行号 (升序); None = 全量。
    filtered: Option<Arc<Vec<u64>>>,
    filter_input: String,
    filter_applied: String,
    // ---- 搜索态 (T5) ----
    search_open: bool,
    search_input: String,
    search_query: String,
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
    /// 未探索区域的宽度未知, 与编辑器「minimap 边走边长」同构)。
    max_seen: std::cell::Cell<f32>,
}

impl LogView {
    pub(crate) fn new() -> Self {
        Self {
            file: None,
            top_row: 0.0,
            selected: 0,
            status: String::new(),
            mode: ViewMode::Raw,
            schema: None,
            filtered: None,
            filter_input: String::new(),
            filter_applied: String::new(),
            search_open: false,
            search_input: String::new(),
            search_query: String::new(),
            search_pattern_src: None,
            search_re: None,
            search_hits: None,
            encoding: danqing_log::encoding::Encoding::Utf8,
            bookmarks: std::collections::BTreeSet::new(),
            expanded: ExpandMap::new(),
            sub_rows: std::collections::BTreeMap::new(),
            x_offset: std::cell::Cell::new(0.0),
            max_seen: std::cell::Cell::new(0.0),
        }
    }

    /// 表格模式 = 模式开关开 + 列定义在手 (构造保证同生同灭, 这里仍取交集防御)。
    fn table_mode(&self) -> bool {
        self.mode == ViewMode::Table && self.schema.is_some()
    }

    /// 顶部 chrome 高度: 表格 = 过滤/搜索栏同槽 + 表头; 原始模式开搜索栏时 +栏高。
    fn chrome_top(&self) -> f32 {
        if self.table_mode() {
            FILTER_BAR_H + HEADER_H
        } else if self.search_open {
            FILTER_BAR_H
        } else {
            0.0
        }
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
        self.top_row = app.top_row;
        self.selected = app.selected;
        self.status = app.status.clone();
        self.mode = app.mode;
        self.schema = app.schema.clone();
        self.filtered = app.filtered.clone();
        self.filter_input = app.filter_input.clone();
        self.filter_applied = app.filter_applied.clone();
        self.search_open = app.search_open;
        self.search_input = app.search_input.clone();
        self.search_query = app.search_query.clone();
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

        // 顶栏: 搜索栏开着即占槽 (与过滤栏同槽替换), 否则表格模式画过滤栏
        if self.search_open {
            self.paint_search_bar(area, rects, texts, line_h);
        } else if table {
            self.paint_filter_bar(area, rects, texts, line_h);
        }

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
            // 表头 + 分隔线 (表头随列水平滚动, 左缘切断同单元格)
            let hy = area.origin.y + FILTER_BAR_H;
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
        let first = self.top_row.floor() as u64;
        let frac = (self.top_row - first as f64) as f32;
        let mut i = first;
        loop {
            let y = rows_top + (i - first) as f32 * ROW_HEIGHT - frac * ROW_HEIGHT;
            if y >= rows_top + list_h || i >= count {
                break;
            }
            // 选中行底色
            if i == self.selected {
                rects.push_rect(
                    Rect::from_xywh(area.origin.x, y, area.size.width - SCROLLBAR_W, ROW_HEIGHT),
                    selection_bg(),
                    0.0,
                );
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
                Color::rgb(0.90, 0.75, 0.30)
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
                    let color = if col.name == "level" {
                        level_color(v.as_bytes())
                    } else {
                        text_default()
                    };
                    let cell_x = cx + 8.0;
                    let left_cut = (text_x - cell_x).max(0.0);
                    let (shown, sub) = scroll_trim(texts, &v, left_cut, FONT_SIZE);
                    let draw_x = cell_x + left_cut - sub;
                    let max_w = (cx + cw - 8.0).min(text_right) - draw_x;
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
        if count > 0 {
            let visible = f64::from(list_h / ROW_HEIGHT).max(1.0);
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

        // 底栏状态行 (打开耗时/过滤统计 = 截图弹药本体)
        let sy =
            status_y + (STATUS_HEIGHT - aux_line_h) / 2.0 + texts.ascent(f32::from(AUX_FONT_SIZE));
        texts.push_text(
            &self.status,
            area.origin.x + 10.0,
            sy,
            AUX_FONT_SIZE,
            status_fg(),
        );
        let pos = if count == 0 {
            format!("行 0/{count}")
        } else {
            format!("行 {}/{count}", self.selected + 1)
        };
        let pos_w = texts.measure(&pos, AUX_FONT_SIZE);
        texts.push_text(
            &pos,
            area.origin.x + area.size.width - SCROLLBAR_W - 10.0 - pos_w,
            sy,
            AUX_FONT_SIZE,
            status_fg(),
        );
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        let chrome_top = self.chrome_top();
        let list_h = (area.size.height - chrome_top - STATUS_HEIGHT).max(0.0);
        match event {
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

impl LogView {
    /// 搜索栏 (T5): 与过滤栏同槽同高。输入中显输入+光标, 已应用显查询+翻页提示。
    fn paint_search_bar(
        &self,
        area: Rect,
        rects: &mut RectBatch,
        texts: &mut TextBatch,
        line_h: f32,
    ) {
        rects.push_rect(
            Rect::from_xywh(area.origin.x, area.origin.y, area.size.width, FILTER_BAR_H),
            filter_bar_bg(),
            0.0,
        );
        let baseline =
            area.origin.y + (FILTER_BAR_H - line_h) / 2.0 + texts.ascent(f32::from(FONT_SIZE));
        let label = if !self.search_input.is_empty() {
            format!("搜索: {}▏", self.search_input)
        } else if !self.search_query.is_empty() {
            format!(
                "搜索: {}  (Enter 下一命中 · Shift+Enter 上一 · Esc 关闭)",
                self.search_query
            )
        } else {
            "搜索: 输入正则 · Enter 应用 · Esc 关闭 (GBK/Latin-1 文件退化为字面量)".to_string()
        };
        fit_push(
            texts,
            &label,
            area.size.width - 20.0,
            area.origin.x + 10.0,
            baseline,
            FONT_SIZE,
            filter_fg(),
        );
    }

    /// 过滤栏: 输入中显输入+光标, 已应用显查询+操作提示, 空态显语法提示。
    fn paint_filter_bar(
        &self,
        area: Rect,
        rects: &mut RectBatch,
        texts: &mut TextBatch,
        line_h: f32,
    ) {
        rects.push_rect(
            Rect::from_xywh(area.origin.x, area.origin.y, area.size.width, FILTER_BAR_H),
            filter_bar_bg(),
            0.0,
        );
        let baseline =
            area.origin.y + (FILTER_BAR_H - line_h) / 2.0 + texts.ascent(f32::from(FONT_SIZE));
        let label = if !self.filter_input.is_empty() {
            format!("过滤: {}▏", self.filter_input)
        } else if !self.filter_applied.is_empty() {
            format!(
                "过滤: {}  (已应用 · Esc 清除 · Ctrl+T 切回原始)",
                self.filter_applied
            )
        } else {
            "过滤: 直接输入, 如 level=ERROR status=50* (AND, 尾缀 * 前缀通配) · Enter 应用 · Esc 清除 · Ctrl+T 切回原始"
                .to_string()
        };
        fit_push(
            texts,
            &label,
            area.size.width - 20.0,
            area.origin.x + 10.0,
            baseline,
            FONT_SIZE,
            filter_fg(),
        );
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
        assert!(err.r > 0.9, "ERROR 判红: {err:?}");
        let fatal = level_color(b"FATAL boom");
        assert!(fatal.r > 0.9, "FATAL 判红");
        let warn = level_color(b"WARN slow query");
        assert!(warn.r > 0.8 && warn.g > 0.5, "WARN 判黄: {warn:?}");
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
