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
    /// 悬停行 (仅可点行进)。
    hover: Cell<Option<usize>>,
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
            bg: Color::rgb(1.0, 1.0, 1.0),
            text_primary: Color::rgb(0.12, 0.12, 0.12),
            text_secondary: Color::rgb(0.40, 0.40, 0.42),
            accent: Color::rgb(0.18, 0.35, 0.60),
        }
    }

    /// 选择器态的行数 (标题行 + 字段行)。
    fn picker_rows(&self) -> usize {
        1 + self.columns.len()
    }

    /// 结果态的行数 (顶部两行 + 作用域行 + 内容行)。
    fn result_rows(&self) -> usize {
        let Some(a) = &self.result else { return 0 };
        let content = match &a.result {
            AnalysisResult::Numeric(_) => 7, // count/min/max/mean/p50/p95/p99
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
        rects.push_rect(area, self.bg, 0.0);
        let x = area.origin.x + PAD_X;
        let right = area.origin.x + area.size.width - PAD_X;
        let bar_full_w = (area.size.width - 2.0 * PAD_X).max(1.0);

        // 标题行 (选择器 = 「字段分析」, 结果 = 「字段分析 · <field>」)
        let title = match &self.result {
            Some(a) => format!("字段分析 · {}", a.field),
            None => "字段分析".to_string(),
        };
        let base0 = self.row_rect(area, 0).origin.y + texts.ascent(f32::from(LABEL_SIZE));
        texts.push_text(&title, x, base0, LABEL_SIZE, self.text_secondary);

        match &self.result {
            None => {
                // 选择器: 字段名逐行可点 (与直方图桶行同一交互语言)
                for (i, name) in self.columns.iter().enumerate() {
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
                    texts.push_text(
                        name,
                        x,
                        r.origin.y + texts.ascent(f32::from(LABEL_SIZE)),
                        LABEL_SIZE,
                        color,
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
                texts.push_text(
                    &scope,
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
                            texts.push_text(
                                "分位数为采样估计",
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
                            texts.push_text(val, x, base, LABEL_SIZE, self.text_primary);
                            let cs = count.to_string();
                            let cw = texts.measure(&cs, LABEL_SIZE);
                            texts.push_text(&cs, right - cw, base, LABEL_SIZE, self.text_secondary);
                        }
                        if e.others > 0 || e.capped {
                            let r = self.row_rect(area, content_start + e.top.len());
                            let label = if e.capped {
                                format!("其他（{} 行, 取值过多已合并）", e.others)
                            } else {
                                format!("其他（{} 行）", e.others)
                            };
                            texts.push_text(
                                &label,
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
            let tw = texts.measure(&title, LABEL_SIZE);
            texts.push_text("  分析中…", x + tw, base0, LABEL_SIZE, self.accent);
        }
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
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
        let Some(row) = self.row_at(area, *position) else {
            return EventResult::Ignored;
        };
        if *pressed {
            self.hover.set(Some(row));
            return EventResult::Consumed;
        }
        // 抬起: 命中即触发
        let was = self.hover.replace(None);
        if was != Some(row) {
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
        } else if row >= 1 && row <= self.columns.len() {
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
}
