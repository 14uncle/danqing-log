//! @author 十四叔
//! @date 2026/09/19
//!
//! 字段分析侧栏区 (SPEC-v1x-field-analytics D4/D5) —— 直方图同侧栏的下半段。
//!
//! 两态: **选择器** (schema 列名逐行可点 —— 点字段即分析, 字段行就是「分析」
//! 按钮, D5 门控点位) 与 **结果视图** (统计表 / Top N 计数条 + 作用域行 +
//! 「换字段 / 重跑」)。spec 起草时写的是「字段下拉」, 实现改逐行可点:
//! 下拉是框架控件、内容在建树时冻结, 而 schema 是开文件后才有 —— 逐行可点
//! 与直方图桶行同构 (hover 反馈 / 命中测试同一套手法)。
//!
//! 门控不在本组件: 点字段行产 `Msg::AnalyzeField`, 门在 main.rs 的 handler
//! (免费态弹升级提示且不发起扫描)。本组件只管呈现与命中。

use std::any::Any;
use std::borrow::Cow;
use std::cell::Cell;
use std::sync::Arc;

use danqing::widget::{EventResult, MsgQueue, Node, Widget};
use danqing::{Color, Constraints, Event, Point, Rect, RectBatch, Size, TextBatch, Theme};

use crate::histogram::{LABEL_SIZE, PAD_X, PAD_Y, ROW_H, bar_fraction, effective_width};
use crate::{LogApp, Msg};
use danqing_log::analysis::{Analysis, AnalysisResult};

/// 结果视图顶部两行 (「← 换字段」/「↻ 重跑」) 的行号 —— 命中测试与 paint
/// 共用, 同规则同常量。
const ROW_BACK: usize = 0;
const ROW_RERUN: usize = 1;

/// 选择器态字段行上限 (评审 R6): 无上限时 30-50 列的宽 schema 把直方图挤到
/// 零高、底部字段不可达。超上限的列不进面板 (仍可走字段过滤语法查询) ——
/// 选择器没有滚动机构, 封顶 + 「还有 N 列」行是 spec Open Question 的落地。
const MAX_PICKER_FIELDS: usize = 16;

/// 枚举取值展示的字符上限 (spec 已知局限: 32 字符, 同列宽惯例)。
const ENUM_VALUE_CHARS: usize = 32;

/// 侧栏「字段分析」区。
pub(crate) struct AnalysisPanel {
    /// 侧栏开关 (`Ctrl+L`) —— 关掉时宽度归零 (与直方图同一条路径)。
    visible: bool,
    /// 表格模式且有 schema 才有这区 (.log 不显示, 不是灰掉 —— D4)。
    has_schema: bool,
    /// schema 列名缓存 (只在 schema 指针变化时重建 —— 每帧克隆 16 个串是浪费)。
    columns: Vec<String>,
    /// 缓存所依据的 schema Arc 指针。
    columns_src: usize,
    /// 最近结果 (None = 选择器态)。
    result: Option<Analysis>,
    /// 结果是否基于旧过滤 (过滤串变了但未重跑)。
    stale: bool,
    /// 分析在途 (job 已发起未交付) —— 显示「分析中…」而不是空结果区。
    running: bool,
    /// 悬停行 (仅可点行进) —— **纯视觉**, 光标驱动 (直方图同款);
    /// 点击判定不读它, 读 `pressed` 锚点。
    hover: Cell<Option<usize>>,
    /// 按下锚点行 (可点行才记): 抬起时与命中行比对, 同才触发。
    /// 与 hover 分家 —— 按下拖出再抬起不该触发, 也不该残留高亮。
    pressed: Cell<Option<usize>>,
    // 主题色 sync 期解析缓存, paint 零查表 (与 histogram 同款)。
    bg: Color,
    text_primary: Color,
    text_secondary: Color,
    accent: Color,
}

impl AnalysisPanel {
    pub(crate) fn new() -> Self {
        Self {
            visible: true,
            has_schema: false,
            columns: Vec::new(),
            columns_src: 0,
            result: None,
            stale: false,
            running: false,
            hover: Cell::new(None),
            pressed: Cell::new(None),
            bg: Color::rgb(1.0, 1.0, 1.0),
            text_primary: Color::rgb(0.12, 0.12, 0.12),
            text_secondary: Color::rgb(0.40, 0.40, 0.42),
            accent: Color::rgb(0.18, 0.35, 0.60),
        }
    }

    /// 选择器态实际展示的字段行数 (封顶 MAX_PICKER_FIELDS)。
    fn shown_fields(&self) -> usize {
        self.columns.len().min(MAX_PICKER_FIELDS)
    }

    /// 选择器态的行数 (标题行 + 字段行 + 超限时的「还有 N 列」行)。
    fn picker_rows(&self) -> usize {
        1 + self.shown_fields() + usize::from(self.columns.len() > MAX_PICKER_FIELDS)
    }

    /// 结果态的行数 (顶部两行 + 作用域行 + 内容行)。
    fn result_rows(&self) -> usize {
        let Some(a) = &self.result else { return 0 };
        let content = match &a.result {
            // count/min/max/mean/p50/p95/p99 + 采样标注行 (评审 R1: 漏算它会让
            // 面板自然高度少一行, 「分位数为采样估计」被窗口底边裁掉 —— 而这条
            // 恰在旗舰实测路径 (超 reservoir 上限) 上必现)。
            AnalysisResult::Numeric(s) => 7 + usize::from(s.sampled),
            AnalysisResult::Enum(e) => e.top.len() + usize::from(e.others > 0 || e.capped),
        };
        // +作用域行 + (跳过/采样标注行)
        2 + 1 + content + 1
    }

    /// 内容自然高度 (无 schema / 侧栏关 → 0, 布局坍缩不占地)。
    fn natural_height(&self) -> f32 {
        if !self.has_schema {
            return 0.0;
        }
        let rows = if self.result.is_some() {
            self.result_rows()
        } else {
            self.picker_rows()
        };
        2.0 * PAD_Y + rows as f32 * ROW_H
    }

    /// 行矩形 (选择器与结果共用同一行几何: 顶部 PAD_Y 起, 逐行 ROW_H)。
    fn row_rect(&self, area: Rect, i: usize) -> Rect {
        Rect::from_xywh(
            area.origin.x,
            area.origin.y + PAD_Y + i as f32 * ROW_H,
            area.size.width,
            ROW_H,
        )
    }

    fn row_at(&self, area: Rect, p: Point) -> Option<usize> {
        if p.x < area.origin.x || p.x >= area.origin.x + area.size.width {
            return None;
        }
        let y = p.y - area.origin.y - PAD_Y;
        if y < 0.0 {
            return None;
        }
        let i = (y / ROW_H) as usize;
        let rows = if self.result.is_some() {
            self.result_rows()
        } else {
            self.picker_rows()
        };
        (i < rows).then_some(i)
    }

    /// 该行可点吗 —— hover 留痕与按下锚点共用同一判据 (直方图
    /// `is_row_clickable` 同款: 可点性是单一事实源)。
    fn row_clickable(&self, row: usize) -> bool {
        if self.result.is_some() {
            row == ROW_BACK + 1 || row == ROW_RERUN + 1
        } else {
            row >= 1 && row <= self.shown_fields()
        }
    }
}

impl Widget for AnalysisPanel {
    fn sync(&mut self, state: &dyn Any) {
        let Some(app) = state.downcast_ref::<LogApp>() else {
            return;
        };
        let t = app.theme.theme();
        self.bg = t.background();
        self.text_primary = t.text_primary();
        self.text_secondary = t.text_secondary();
        self.accent = t.accent();
        self.visible = app.histogram_visible;
        self.has_schema = app.has_file && app.schema.is_some();
        // 列名缓存: schema 指针变了才重建
        let src = app.schema.as_ref().map_or(0, |s| Arc::as_ptr(s) as usize);
        if src != self.columns_src {
            self.columns_src = src;
            self.columns = app.schema.as_ref().map_or_else(Vec::new, |s| {
                s.columns.iter().map(|c| c.name.clone()).collect()
            });
        }
        self.result = app.analysis_result.clone();
        self.running = app.analysis_running;
        // 过滤串变了且没重跑 = 旧结果 (D8: 留着但标注, 不静默作废)
        self.stale = self.result.is_some() && app.analysis_filter_src != app.filter_applied;
    }

    fn layout(&mut self, constraints: Constraints, _texts: &mut TextBatch) -> Size {
        let max = constraints.max();
        let w = effective_width(self.visible && self.has_schema, max.width);
        Size::new(w, self.natural_height())
    }

    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        if area.size.width < 1.0 || area.size.height < 1.0 {
            return;
        }
        // 裁剪兜底 (评审 R4): 文本以 measure 截断为主, 但 rect span 先于 text span
        // 渲染, 任何漏网的超长内容不得盖画到 LogView 行内容上。
        rects.push_clip(area);
        texts.push_clip(area);
        rects.push_rect(area, self.bg, 0.0);
        let x = area.origin.x + PAD_X;
        let right = area.origin.x + area.size.width - PAD_X;
        let avail_w = (area.size.width - 2.0 * PAD_X).max(1.0);
        let bar_full_w = avail_w;

        // 标题行 (选择器 = 「字段分析」, 结果 = 「字段分析 · <field>」)
        let title = match &self.result {
            Some(a) => format!("字段分析 · {}", a.field),
            None => "字段分析".to_string(),
        };
        let base0 = self.row_rect(area, 0).origin.y + texts.ascent(f32::from(LABEL_SIZE));
        // 「分析中…」尾随标题 —— 在途时给它留出位置, 别让两段叠着溢出。
        let suffix_w = if self.running {
            texts.measure("  分析中…", LABEL_SIZE)
        } else {
            0.0
        };
        let title_fit = fit(texts, &title, (avail_w - suffix_w).max(1.0));
        texts.push_text(
            title_fit.as_ref(),
            x,
            base0,
            LABEL_SIZE,
            self.text_secondary,
        );

        match &self.result {
            None => {
                // 选择器: 字段名逐行可点 (与直方图桶行同一交互语言),
                // 封顶 MAX_PICKER_FIELDS, 超出的列给一行指示 (R6)。
                let shown = self.shown_fields();
                for (i, name) in self.columns.iter().take(shown).enumerate() {
                    let row = i + 1;
                    let r = self.row_rect(area, row);
                    let hovered = self.hover.get() == Some(row);
                    if hovered {
                        rects.push_rect(
                            Rect::from_xywh(
                                r.origin.x + 2.0,
                                r.origin.y + 2.0,
                                area.size.width - 4.0,
                                ROW_H - 4.0,
                            ),
                            self.accent,
                            3.0,
                        );
                    }
                    let color = if hovered {
                        Color::WHITE
                    } else {
                        self.text_primary
                    };
                    let name_fit = fit(texts, name, avail_w);
                    texts.push_text(
                        name_fit.as_ref(),
                        x,
                        r.origin.y + texts.ascent(f32::from(LABEL_SIZE)),
                        LABEL_SIZE,
                        color,
                    );
                }
                if self.columns.len() > shown {
                    let r = self.row_rect(area, shown + 1);
                    let more = format!("… 还有 {} 列", self.columns.len() - shown);
                    texts.push_text(
                        &more,
                        x,
                        r.origin.y + texts.ascent(f32::from(LABEL_SIZE)),
                        LABEL_SIZE,
                        self.text_secondary,
                    );
                }
            }
            Some(a) => {
                // 顶部两行: 换字段 / 重跑
                for (row, label) in [(ROW_BACK, "← 换个字段"), (ROW_RERUN, "↻ 重跑")] {
                    let r = self.row_rect(area, row + 1);
                    let hovered = self.hover.get() == Some(row + 1);
                    let color = if hovered {
                        self.accent
                    } else {
                        self.text_secondary
                    };
                    texts.push_text(
                        label,
                        x,
                        r.origin.y + texts.ascent(f32::from(LABEL_SIZE)),
                        LABEL_SIZE,
                        color,
                    );
                }
                // 作用域行 (D3: 分布数字必须说作用域) + 过期标注 (D8)
                let mut scope = format!("{} 行", a.scope_rows);
                if self.stale {
                    scope.push_str(" · 基于旧过滤");
                }
                if a.skipped > 0 {
                    scope.push_str(&format!(" · 跳过 {} 行", a.skipped));
                }
                let r = self.row_rect(area, 3);
                let scope_fit = fit(texts, &scope, avail_w);
                texts.push_text(
                    scope_fit.as_ref(),
                    x,
                    r.origin.y + texts.ascent(f32::from(LABEL_SIZE)),
                    LABEL_SIZE,
                    self.text_secondary,
                );

                let content_start = 4;
                match &a.result {
                    AnalysisResult::Numeric(s) => {
                        let rows = [
                            ("count", fmt_num(s.count as f64)),
                            ("min", fmt_num(s.min)),
                            ("max", fmt_num(s.max)),
                            ("mean", fmt_num(s.mean)),
                            ("p50", fmt_num(s.p50)),
                            ("p95", fmt_num(s.p95)),
                            ("p99", fmt_num(s.p99)),
                        ];
                        for (i, (label, val)) in rows.iter().enumerate() {
                            let r = self.row_rect(area, content_start + i);
                            let base = r.origin.y + texts.ascent(f32::from(LABEL_SIZE));
                            texts.push_text(label, x, base, LABEL_SIZE, self.text_secondary);
                            let vw = texts.measure(val, LABEL_SIZE);
                            texts.push_text(val, right - vw, base, LABEL_SIZE, self.text_primary);
                        }
                        if s.sampled {
                            let r = self.row_rect(area, content_start + rows.len());
                            let note = fit(texts, "分位数为采样估计", avail_w);
                            texts.push_text(
                                note.as_ref(),
                                x,
                                r.origin.y + texts.ascent(f32::from(LABEL_SIZE)),
                                LABEL_SIZE,
                                self.text_secondary,
                            );
                        }
                    }
                    AnalysisResult::Enum(e) => {
                        let max = e.top.first().map_or(0, |(_, c)| *c);
                        for (i, (val, count)) in e.top.iter().enumerate() {
                            let r = self.row_rect(area, content_start + i);
                            let base = r.origin.y + texts.ascent(f32::from(LABEL_SIZE));
                            let frac = bar_fraction(*count, max);
                            let bw = (bar_full_w * frac).max(2.0);
                            rects.push_rect(
                                Rect::from_xywh(x, r.origin.y + ROW_H - 10.0, bw, 6.0),
                                self.accent,
                                2.0,
                            );
                            let cs = count.to_string();
                            let cw = texts.measure(&cs, LABEL_SIZE);
                            // 取值先按 spec 惯例截 32 字符, 再按剩余宽度截
                            // (计数右对齐, 给它留出位置)。
                            let val_cap: Cow<'_, str> = if val.chars().count() > ENUM_VALUE_CHARS {
                                Cow::Owned(format!(
                                    "{}…",
                                    val.chars().take(ENUM_VALUE_CHARS).collect::<String>()
                                ))
                            } else {
                                Cow::Borrowed(val.as_str())
                            };
                            let val_fit = fit(texts, &val_cap, (avail_w - cw - 8.0).max(1.0));
                            texts.push_text(
                                val_fit.as_ref(),
                                x,
                                base,
                                LABEL_SIZE,
                                self.text_primary,
                            );
                            texts.push_text(&cs, right - cw, base, LABEL_SIZE, self.text_secondary);
                        }
                        if e.others > 0 || e.capped {
                            let r = self.row_rect(area, content_start + e.top.len());
                            let label = if e.capped {
                                format!("其他（{} 行, 取值过多已合并）", e.others)
                            } else {
                                format!("其他（{} 行）", e.others)
                            };
                            let label_fit = fit(texts, &label, avail_w);
                            texts.push_text(
                                label_fit.as_ref(),
                                x,
                                r.origin.y + texts.ascent(f32::from(LABEL_SIZE)),
                                LABEL_SIZE,
                                self.text_secondary,
                            );
                        }
                    }
                }
            }
        }
        // 分析在途: 标题行旁标「分析中…」(结果未到前不拿空区冒充)
        if self.running {
            let tw = texts.measure(title_fit.as_ref(), LABEL_SIZE);
            texts.push_text("  分析中…", x + tw, base0, LABEL_SIZE, self.accent);
        }
        texts.pop_clip();
        rects.pop_clip();
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        // hover: 只在**可点**行上留痕, 光标驱动 (直方图同款) —— 评审 R2:
        // 原先只在按下瞬间置位, 字段行没有悬停反馈 (「可点行无 hover = 用户
        // 不知道能点」), 且按下拖出再抬起会残留高亮。
        match event {
            Event::CursorMoved(p) => {
                self.hover
                    .set(self.row_at(area, *p).filter(|&r| self.row_clickable(r)));
                return EventResult::Ignored;
            }
            Event::CursorLeft => {
                self.hover.set(None);
                return EventResult::Ignored;
            }
            _ => {}
        }
        let Event::MouseInput {
            button,
            pressed,
            position,
            ..
        } = event
        else {
            return EventResult::Ignored;
        };
        if *button != danqing::event::MouseButton::Left {
            return EventResult::Ignored;
        }
        let row = self.row_at(area, *position);
        if *pressed {
            // 锚点只记可点行; 点在有东西的地方一律吞掉, 不穿透到底层列表。
            self.pressed.set(row.filter(|&r| self.row_clickable(r)));
            return if row.is_some() {
                EventResult::Consumed
            } else {
                EventResult::Ignored
            };
        }
        // 抬起: 与按下锚点同一条可点行才触发 (按下拖出再抬起 = 取消)。
        let anchor = self.pressed.take();
        let Some(row) = row else {
            return EventResult::Ignored;
        };
        if anchor != Some(row) {
            return EventResult::Consumed;
        }
        if self.result.is_some() {
            match row {
                r if r == ROW_BACK + 1 => msgs.push(Box::new(Msg::AnalysisBack)),
                r if r == ROW_RERUN + 1 => {
                    if let Some(idx) = self
                        .result
                        .as_ref()
                        .and_then(|a| self.columns.iter().position(|c| c == &a.field))
                    {
                        msgs.push(Box::new(Msg::AnalyzeField(idx)));
                    }
                }
                _ => {}
            }
        } else if row >= 1 && row <= self.shown_fields() {
            msgs.push(Box::new(Msg::AnalyzeField(row - 1)));
        }
        EventResult::Consumed
    }

    fn children(&self) -> &[Node] {
        &[]
    }
    fn children_mut(&mut self) -> &mut [Node] {
        &mut []
    }
}

/// f64 展示: 整数值不带小数点, 其余保留两位 (分位数不假装比采样更精确)。
fn fmt_num(x: f64) -> String {
    if x.is_nan() {
        return "—".to_string();
    }
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return format!("{x:.0}");
    }
    format!("{x:.2}")
}

/// 文本适配可用宽度: 超宽按字符截断加省略号 (评审 R4 —— 侧栏 112px,
/// 长字段名/长取值/「4021125 行 · 基于旧过滤 · 跳过 12 行」这类作用域行
/// 必撞; 截断是主防, paint 里的 push_clip 是兜底)。
fn fit<'a>(texts: &mut TextBatch, s: &'a str, max_w: f32) -> Cow<'a, str> {
    if texts.measure(s, LABEL_SIZE) <= max_w {
        return Cow::Borrowed(s);
    }
    let ell_w = texts.measure("…", LABEL_SIZE);
    let mut out = String::new();
    for ch in s.chars() {
        let cand_w = texts.measure(&out, LABEL_SIZE)
            + texts.measure(ch.encode_utf8(&mut [0; 4]), LABEL_SIZE);
        if cand_w + ell_w > max_w {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogApp;
    use crate::histogram::HIST_WIDTH;

    fn temp_cfg(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "danqing-log-panel-{}-{tag}.toml",
            std::process::id()
        ))
    }

    #[test]
    fn panel_collapses_without_schema_or_when_sidebar_hidden() {
        let mut app = LogApp::new_empty_at(Some(temp_cfg("collapse")));
        let mut panel = AnalysisPanel::new();
        let c = Constraints::loose(Size::new(HIST_WIDTH, 800.0));
        let mut texts = TextBatch::default();
        // 无文件: 高度 0
        panel.sync(&app);
        assert_eq!(panel.layout(c, &mut texts).height, 0.0);
        // 表格模式有 schema: 有高度
        app.has_file = true;
        app.schema = Some(Arc::new(danqing_log::jsonl::Schema {
            columns: vec![
                danqing_log::jsonl::Column {
                    name: "level".into(),
                    width_chars: 5,
                },
                danqing_log::jsonl::Column {
                    name: "duration_ms".into(),
                    width_chars: 11,
                },
            ],
        }));
        panel.sync(&app);
        let h = panel.layout(c, &mut texts).height;
        assert!(h > 0.0, "有 schema 要有自然高度");
        assert_eq!(panel.columns.len(), 2);
        // 侧栏关掉: 宽度归零
        app.histogram_visible = false;
        panel.sync(&app);
        assert_eq!(panel.layout(c, &mut texts).width, 0.0);
    }

    #[test]
    fn picker_row_click_emits_analyze_msg() {
        let mut app = LogApp::new_empty_at(Some(temp_cfg("click")));
        app.has_file = true;
        app.schema = Some(Arc::new(danqing_log::jsonl::Schema {
            columns: vec![
                danqing_log::jsonl::Column {
                    name: "level".into(),
                    width_chars: 5,
                },
                danqing_log::jsonl::Column {
                    name: "duration_ms".into(),
                    width_chars: 11,
                },
            ],
        }));
        let mut panel = AnalysisPanel::new();
        panel.sync(&app);
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 400.0);
        // 第 2 个字段行 (duration_ms) 的中心点: 标题行 0 + 字段行 1=level, 2=duration_ms
        let mut msgs = MsgQueue::default();
        let center = Point::new(HIST_WIDTH / 2.0, PAD_Y + 2.0 * ROW_H + ROW_H / 2.0);
        let down = Event::MouseInput {
            button: danqing::event::MouseButton::Left,
            pressed: true,
            position: center,
        };
        let up = Event::MouseInput {
            button: danqing::event::MouseButton::Left,
            pressed: false,
            position: center,
        };
        panel.event(&down, area, &mut msgs);
        panel.event(&up, area, &mut msgs);
        let saw = msgs
            .iter()
            .filter_map(|m| m.downcast_ref::<Msg>())
            .find_map(|m| {
                if let Msg::AnalyzeField(i) = m {
                    Some(*i)
                } else {
                    None
                }
            });
        assert_eq!(saw, Some(1), "点第 2 个字段行 → AnalyzeField(1)");
    }

    #[test]
    fn result_marks_stale_when_filter_changed() {
        let mut app = LogApp::new_empty_at(Some(temp_cfg("stale")));
        app.has_file = true;
        app.schema = Some(Arc::new(danqing_log::jsonl::Schema {
            columns: vec![danqing_log::jsonl::Column {
                name: "d".into(),
                width_chars: 1,
            }],
        }));
        app.analysis_result = Some(Analysis {
            field: "d".into(),
            scope_rows: 5,
            skipped: 0,
            result: AnalysisResult::Enum(danqing_log::analysis::EnumStats {
                top: vec![("x".into(), 5)],
                others: 0,
                capped: false,
                total: 5,
            }),
        });
        app.analysis_filter_src = "level=ERROR".to_string();
        let mut panel = AnalysisPanel::new();
        // 过滤串与结果快照一致: 不标
        app.filter_applied = "level=ERROR".to_string();
        panel.sync(&app);
        assert!(!panel.stale);
        // 过滤变了没重跑: 标「基于旧过滤」
        app.filter_applied = String::new();
        panel.sync(&app);
        assert!(panel.stale, "过滤变更后旧结果必须标注 (D8)");
    }

    /// 行账目守卫 (评审 R1): result_rows 必须等于 paint 实际画出的行数 ——
    /// 漏一行 = 该行被窗口底边裁掉 (采样标注恰在旗舰路径上必现)。
    #[test]
    fn result_rows_accounts_for_sampled_note_and_others_row() {
        use danqing_log::analysis::{EnumStats, NumericStats};
        let mut app = LogApp::new_empty_at(Some(temp_cfg("rows")));
        app.has_file = true;
        app.schema = Some(Arc::new(danqing_log::jsonl::Schema {
            columns: vec![danqing_log::jsonl::Column {
                name: "d".into(),
                width_chars: 1,
            }],
        }));
        let mut panel = AnalysisPanel::new();
        // 数值 + 采样: 2(顶部) + 1(作用域) + 7(统计) + 1(采样标注) + 1(标题) = 12
        app.analysis_result = Some(Analysis {
            field: "d".into(),
            scope_rows: 4_000_000,
            skipped: 0,
            result: AnalysisResult::Numeric(NumericStats {
                count: 4_000_000,
                min: 0.0,
                max: 9.0,
                mean: 4.5,
                p50: 4.0,
                p95: 8.0,
                p99: 9.0,
                sampled: true,
            }),
        });
        panel.sync(&app);
        assert_eq!(panel.result_rows(), 12, "数值+采样: 标注行必须计入高度");
        // 数值未采样: 11
        if let Some(a) = &mut app.analysis_result {
            a.result = AnalysisResult::Numeric(NumericStats {
                count: 3,
                min: 0.0,
                max: 9.0,
                mean: 4.5,
                p50: 4.0,
                p95: 8.0,
                p99: 9.0,
                sampled: false,
            });
        }
        panel.sync(&app);
        assert_eq!(panel.result_rows(), 11);
        // 枚举: 2 个 top + 其他行 → 2+1+3+1 = 7
        app.analysis_result = Some(Analysis {
            field: "d".into(),
            scope_rows: 9,
            skipped: 0,
            result: AnalysisResult::Enum(EnumStats {
                top: vec![("a".into(), 5), ("b".into(), 3)],
                others: 1,
                capped: false,
                total: 9,
            }),
        });
        panel.sync(&app);
        assert_eq!(panel.result_rows(), 7, "其他行必须计入高度");
        // 枚举无其他: 2+1+1+1 = 5
        if let Some(a) = &mut app.analysis_result {
            a.result = AnalysisResult::Enum(EnumStats {
                top: vec![("a".into(), 5)],
                others: 0,
                capped: false,
                total: 5,
            });
        }
        panel.sync(&app);
        assert_eq!(panel.result_rows(), 5);
    }

    /// hover 光标驱动 (评审 R2): 可点行留痕、不可点行不留、CursorLeft 清空;
    /// 按下拖出再抬起 = 取消, 不触发也不残留。
    #[test]
    fn hover_follows_cursor_and_drag_out_cancels_click() {
        let mut app = LogApp::new_empty_at(Some(temp_cfg("hover")));
        app.has_file = true;
        app.schema = Some(Arc::new(danqing_log::jsonl::Schema {
            columns: vec![
                danqing_log::jsonl::Column {
                    name: "level".into(),
                    width_chars: 5,
                },
                danqing_log::jsonl::Column {
                    name: "duration_ms".into(),
                    width_chars: 11,
                },
            ],
        }));
        let mut panel = AnalysisPanel::new();
        panel.sync(&app);
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 400.0);
        let mut msgs = MsgQueue::default();
        let row_center =
            |r: usize| Point::new(HIST_WIDTH / 2.0, PAD_Y + r as f32 * ROW_H + ROW_H / 2.0);
        // 光标到字段行 2 → hover 留痕
        panel.event(&Event::CursorMoved(row_center(2)), area, &mut msgs);
        assert_eq!(panel.hover.get(), Some(2));
        // 光标到标题行 (不可点) → 不留痕
        panel.event(&Event::CursorMoved(row_center(0)), area, &mut msgs);
        assert_eq!(panel.hover.get(), None, "不可点行不留 hover");
        // 按下字段行 2, 拖到字段行 1 抬起 → 不触发
        let down = |p: Point| Event::MouseInput {
            button: danqing::event::MouseButton::Left,
            pressed: true,
            position: p,
        };
        let up = |p: Point| Event::MouseInput {
            button: danqing::event::MouseButton::Left,
            pressed: false,
            position: p,
        };
        panel.event(&down(row_center(2)), area, &mut msgs);
        panel.event(&Event::CursorMoved(row_center(1)), area, &mut msgs);
        panel.event(&up(row_center(1)), area, &mut msgs);
        assert!(
            !msgs
                .iter()
                .filter_map(|m| m.downcast_ref::<Msg>())
                .any(|m| matches!(m, Msg::AnalyzeField(_))),
            "按下拖出再抬起不得触发分析"
        );
        // CursorLeft 清空 hover
        panel.event(&Event::CursorMoved(row_center(1)), area, &mut msgs);
        assert_eq!(panel.hover.get(), Some(1));
        panel.event(&Event::CursorLeft, area, &mut msgs);
        assert_eq!(panel.hover.get(), None);
    }

    /// 选择器封顶 (评审 R6): 宽 schema 不得把直方图挤到零高;
    /// 「还有 N 列」行不可点。
    #[test]
    fn picker_caps_rows_and_overflow_row_is_inert() {
        let mut app = LogApp::new_empty_at(Some(temp_cfg("cap")));
        app.has_file = true;
        app.schema = Some(Arc::new(danqing_log::jsonl::Schema {
            columns: (0..30)
                .map(|i| danqing_log::jsonl::Column {
                    name: format!("f{i}"),
                    width_chars: 3,
                })
                .collect(),
        }));
        let mut panel = AnalysisPanel::new();
        panel.sync(&app);
        assert_eq!(panel.picker_rows(), 1 + MAX_PICKER_FIELDS + 1);
        let c = Constraints::loose(Size::new(HIST_WIDTH, 800.0));
        let mut texts = TextBatch::default();
        let h = panel.layout(c, &mut texts).height;
        assert_eq!(
            h,
            2.0 * PAD_Y + (1 + MAX_PICKER_FIELDS + 1) as f32 * ROW_H,
            "30 列 schema 的自然高度必须封顶"
        );
        // 「还有 N 列」行 (row 17): 按下抬起 → 无消息
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 800.0);
        let mut msgs = MsgQueue::default();
        let p = Point::new(
            HIST_WIDTH / 2.0,
            PAD_Y + (MAX_PICKER_FIELDS + 1) as f32 * ROW_H + ROW_H / 2.0,
        );
        let down = Event::MouseInput {
            button: danqing::event::MouseButton::Left,
            pressed: true,
            position: p,
        };
        let up = Event::MouseInput {
            button: danqing::event::MouseButton::Left,
            pressed: false,
            position: p,
        };
        panel.event(&down, area, &mut msgs);
        panel.event(&up, area, &mut msgs);
        assert!(
            !msgs
                .iter()
                .filter_map(|m| m.downcast_ref::<Msg>())
                .any(|m| matches!(m, Msg::AnalyzeField(_))),
            "「还有 N 列」行不可点"
        );
        // 最后一行真实字段 (row 16) 仍可点
        let p16 = Point::new(
            HIST_WIDTH / 2.0,
            PAD_Y + MAX_PICKER_FIELDS as f32 * ROW_H + ROW_H / 2.0,
        );
        let down16 = Event::MouseInput {
            button: danqing::event::MouseButton::Left,
            pressed: true,
            position: p16,
        };
        let up16 = Event::MouseInput {
            button: danqing::event::MouseButton::Left,
            pressed: false,
            position: p16,
        };
        panel.event(&down16, area, &mut msgs);
        panel.event(&up16, area, &mut msgs);
        let saw = msgs
            .iter()
            .filter_map(|m| m.downcast_ref::<Msg>())
            .find_map(|m| {
                if let Msg::AnalyzeField(i) = m {
                    Some(*i)
                } else {
                    None
                }
            });
        assert_eq!(saw, Some(MAX_PICKER_FIELDS - 1), "封顶内的字段行仍可点");
    }

    /// fit: 短文本原样 (借用), 长文本截断加省略号且量得出 ≤ max_w。
    #[test]
    fn fit_truncates_to_width_with_ellipsis() {
        let mut texts = TextBatch::default();
        let short = fit(&mut texts, "level", 92.0);
        assert!(matches!(short, Cow::Borrowed(_)), "短文本不该分配");
        let long = "x".repeat(80);
        let out = fit(&mut texts, &long, 92.0);
        assert!(out.ends_with('…'));
        assert!(
            texts.measure(&out, LABEL_SIZE) <= 92.0,
            "截断结果必须量得出 ≤ max_w"
        );
    }
}
