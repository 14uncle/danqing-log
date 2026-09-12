//! @author 十四叔
//! @date 2026/09/12
//!
//! 级别计数侧栏 (level-histogram 模块, spec: docs/specs/SPEC-level-histogram.md):
//! 左侧常驻窄栏, 6 行 = 级别名 + 计数 + 横条。
//!
//! 为什么是**对数刻度**横条: 线性刻度下 Info 4544133 会把 Fatal 4760 压成亚像素,
//! 而后者恰恰是唯一要看的那一根。计数数字始终以文本完整显示, 横条只表相对量级。
//!
//! 为什么**不做 hover**: spec 的 Open Question 倾向不做 (简单到不需要解释);
//! 可点行的可发现性改由「当前生效行高亮」承担 —— 点过之后有反馈。
//!
//! 布局: 本组件是 LogView 的 **sibling** (顶层 `Row[Histogram, LogView.fill]`),
//! 不侵入 LogView 内部的坐标数学 —— 后者只是拿到一个更窄的 `area`。

use std::any::Any;

use danqing::widget::{EventResult, MsgQueue, Widget};
use danqing::{
    Color, Constraints, Event, MouseButton, Point, Rect, RectBatch, Size, TextBatch, Theme,
};

use danqing_log::levels::{self, Level, LevelCounts, LevelQueries};

use crate::view;
use crate::{LogApp, Msg};

/// 侧栏宽度 (逻辑像素)。
pub(crate) const HIST_WIDTH: f32 = 112.0;
/// 左右内边距。
const PAD_X: f32 = 10.0;
/// 顶部内边距。
const PAD_Y: f32 = 8.0;
/// 行高 (标签行 + 横条行 + 行距)。
const ROW_H: f32 = 28.0;
/// 标签/计数字号。
const LABEL_SIZE: u16 = 12;
/// 横条高度。
const BAR_H: f32 = 6.0;
/// 标签行与横条之间的间距。
const BAR_GAP: f32 = 3.0;
/// 非零计数的最小可见条宽 (对数刻度下 1 与 1e6 也只差一档)。
const MIN_BAR_W: f32 = 2.0;
/// 容得下侧栏的最小窗口内容宽: 再窄就自动折叠, 优先保内容区。
const MIN_CONTENT_WIDTH: f32 = 640.0;

/// 侧栏的有效宽度: 关掉 (`Ctrl+L`), 或窗口窄到容不下 → 0。
///
/// 窄窗自动折叠的必要性: 侧栏是固定宽, 窗口 400px 时内容区只剩 288px,
/// 而新用户未必知道有 `Ctrl+L` —— 卡在没法看的布局里比看不到直方图糟。
/// `available` 是**整个 Row 的可用宽** (Fit 子项拿到的是宽松约束)。
///
/// paint/event 不重判这个函数, 而是看 layout 给出的实际宽度 (`area.size.width`)
/// —— 判定只有一处, 不存在「宽度 0 却还在画/还在吃点击」的漏判。
fn effective_width(visible: bool, available: f32) -> f32 {
    if visible && available >= MIN_CONTENT_WIDTH {
        HIST_WIDTH
    } else {
        0.0
    }
}

/// 第 `i` 行的命中矩形 (序同 [`Level::ALL`])。
fn row_rect(area: Rect, i: usize) -> Rect {
    Rect::from_xywh(
        area.origin.x,
        area.origin.y + PAD_Y + i as f32 * ROW_H,
        area.size.width,
        ROW_H,
    )
}

/// 命中测试: 点落在第几行; 行外 (或行号越界) → None。
fn row_at(area: Rect, p: Point) -> Option<usize> {
    (0..Level::ALL.len()).find(|i| row_rect(area, *i).contains(p))
}

/// 横条宽度比例 (0..=1), 对数刻度 (底 10)。
///
/// `log10(1 + c) / log10(1 + max)`: 加 1 是为了让 `c == 1` 也落在可见区间
/// (log10(1) = 0 会让单行计数完全不可见), 且 0 计数仍返回 0。
pub(crate) fn bar_fraction(count: u64, max: u64) -> f32 {
    if count == 0 || max == 0 {
        return 0.0;
    }
    if count >= max {
        return 1.0;
    }
    ((1.0 + count as f64).log10() / (1.0 + max as f64).log10()) as f32
}

/// 桶 → 横条色。前四档复用行/单元格着色的语义色 (`view.rs`), 保证侧栏色带
/// 与内容色一致; 「其他」桶是「无信息」桶, 由主题的次要色降噪。
fn bucket_color(level: Level, text_secondary: Color) -> Color {
    match level {
        Level::Fatal | Level::Error => view::err_fg(),
        Level::Warn => view::warn_fg(),
        Level::Info => view::info_fg(),
        Level::DebugTrace => view::trace_fg(),
        Level::Other => text_secondary,
    }
}

/// 级别计数侧栏。
pub(crate) struct LevelHistogram {
    counts: LevelCounts,
    /// 每桶的点选子句 (来自当前文件的级别类列; 全 None = 只读侧栏)。
    queries: LevelQueries,
    /// 用户开关 (`Ctrl+L`)。关掉时宽度归零, 与「窄窗自动折叠」同一条路径。
    visible: bool,
    /// 计数是否仍在后台算。
    ///
    /// 未就绪时**必须显示「…」而不是 0** —— 0 会被读成「这个文件真的没有 ERROR」,
    /// 那是假信息。计数改为后台作业后, 这个窗口是常态 (见 main.rs 的
    /// `launch_levels_job`)。
    pending: bool,
    /// 当前生效的过滤对应的桶 (行高亮); None = 无。
    active: Option<Level>,
    // 主题色在 sync 期解析并缓存, paint 期零查表 (与 view.rs 同款)。
    bg: Color,
    text_primary: Color,
    text_secondary: Color,
    /// 生效行底色 (主题的行底色, 与内容区选中行同源)。
    active_bg: Color,
}

impl LevelHistogram {
    pub(crate) fn new() -> Self {
        Self {
            counts: LevelCounts::default(),
            queries: levels::no_level_queries(),
            visible: true,
            pending: false,
            active: None,
            bg: Color::rgb(1.0, 1.0, 1.0),
            text_primary: Color::rgb(0.12, 0.12, 0.12),
            text_secondary: Color::rgb(0.40, 0.40, 0.42),
            active_bg: Color::rgb(0.93, 0.93, 0.94),
        }
    }

    /// 全部桶里的最大计数 (横条归一化的分母)。
    fn max_count(&self) -> u64 {
        Level::ALL
            .iter()
            .map(|l| self.counts.get(*l))
            .max()
            .unwrap_or(0)
    }
}

impl Default for LevelHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for LevelHistogram {
    fn sync(&mut self, state: &dyn Any) {
        let Some(app) = state.downcast_ref::<LogApp>() else {
            return;
        };
        let t = app.theme.theme();
        self.bg = t.background();
        self.text_primary = t.text_primary();
        self.text_secondary = t.text_secondary();
        self.active_bg = t.surface_variant();
        self.counts = *app.level_counts.as_ref();
        self.queries = app.level_queries.clone();
        self.visible = app.histogram_visible;
        self.pending = app.levels_pending;
        // 生效行由**已应用的过滤串**反推, 不另存状态 —— 手打 `level=ERROR*`
        // 与点柱条走同一条判定, 两者行为一致。
        self.active = Level::ALL.iter().copied().find(|l| {
            self.queries[*l as usize]
                .as_deref()
                .is_some_and(|q| q == app.filter_applied)
        });
    }

    fn layout(&mut self, constraints: Constraints, _texts: &mut TextBatch) -> Size {
        let max = constraints.max();
        let h = if max.height.is_finite() {
            max.height
        } else {
            0.0
        };
        Size::new(effective_width(self.visible, max.width), h)
    }

    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        // 归零即不画 —— 与 layout 的判定同一依据 (宽度), 不重复判断 visible。
        if area.size.width < 1.0 {
            return;
        }
        // 整条填主题背景 —— 底栏只画 1px 分隔线不画自己的底色, 故底色在此
        // 与 LogView 的底完全一致, 侧栏与内容区之间无缝。
        rects.push_rect(area, self.bg, 0.0);

        let bar_x = area.origin.x + PAD_X;
        let bar_full_w = (area.size.width - 2.0 * PAD_X).max(1.0);
        let right = area.origin.x + area.size.width - PAD_X;
        let max = self.max_count();
        let line_h = texts.line_height(f32::from(LABEL_SIZE));

        for (i, level) in Level::ALL.iter().enumerate() {
            let row_y = area.origin.y + PAD_Y + i as f32 * ROW_H;
            let baseline = row_y + texts.ascent(f32::from(LABEL_SIZE));

            // 生效行: 整行淡底 (比给横条换色更醒目, 且不动语义色)
            if self.active == Some(*level) {
                rects.push_rect(
                    Rect::from_xywh(
                        area.origin.x + 2.0,
                        row_y - 2.0,
                        area.size.width - 4.0,
                        ROW_H - 2.0,
                    ),
                    self.active_bg,
                    3.0,
                );
            }

            // 级别名 (左) + 计数 (右对齐) —— 可点行用正文色, 只读行降噪
            let color = if self.queries[*level as usize].is_some() {
                self.text_primary
            } else {
                self.text_secondary
            };
            texts.push_text(level.label(), bar_x, baseline, LABEL_SIZE, color);
            let count = if self.pending {
                "…".to_string()
            } else {
                self.counts.get(*level).to_string()
            };
            let count_w = texts.measure(&count, LABEL_SIZE);
            texts.push_text(
                &count,
                right - count_w,
                baseline,
                LABEL_SIZE,
                self.text_secondary,
            );

            // 对数横条 (计数未就绪时不画: 空条比假条诚实)
            let frac = if self.pending {
                0.0
            } else {
                bar_fraction(self.counts.get(*level), max)
            };
            if frac > 0.0 {
                let w = (bar_full_w * frac).max(MIN_BAR_W);
                rects.push_rect(
                    Rect::from_xywh(bar_x, row_y + line_h + BAR_GAP, w, BAR_H),
                    bucket_color(*level, self.text_secondary),
                    2.0,
                );
            }
        }
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        if area.size.width < 1.0 {
            return EventResult::Ignored;
        }
        let Event::MouseInput {
            pressed: true,
            position,
            button: MouseButton::Left,
            ..
        } = event
        else {
            return EventResult::Ignored;
        };
        let Some(i) = row_at(area, *position) else {
            return EventResult::Ignored;
        };
        let level = Level::ALL[i];
        // 不可点 = 明文模式 / 无级别类列 / 该桶无单子句 (DEBUG/其他) —— 吞掉点击,
        // 不让它穿透到底下的列表 (点在有东西的地方不该毫无回应地选中底下的行)。
        if self.queries[level as usize].is_none() {
            return EventResult::Consumed;
        }
        msgs.push(Box::new(Msg::ApplyLevelFilter(level)));
        EventResult::Consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 有效宽度: 开着且够宽才给 112; 关掉或过窄一律 0。
    #[test]
    fn effective_width_collapses_on_toggle_and_narrow_window() {
        assert_eq!(effective_width(true, 1200.0), HIST_WIDTH, "开着且够宽");
        assert_eq!(
            effective_width(true, MIN_CONTENT_WIDTH),
            HIST_WIDTH,
            "恰好够宽"
        );
        assert_eq!(effective_width(false, 1200.0), 0.0, "Ctrl+L 关掉");
        assert_eq!(
            effective_width(true, MIN_CONTENT_WIDTH - 1.0),
            0.0,
            "窄窗自动折叠: 优先保内容区"
        );
        assert_eq!(effective_width(false, 100.0), 0.0, "两者叠加仍是 0");
    }

    /// 折叠态 (宽度 0): 不吞事件、不发消息 —— 否则零宽侧栏会吃掉落在内容区的点击。
    #[test]
    fn collapsed_sidebar_ignores_events() {
        let mut w = LevelHistogram::new();
        let area = Rect::from_xywh(0.0, 0.0, 0.0, 600.0);
        let ev = Event::MouseInput {
            button: MouseButton::Left,
            pressed: true,
            position: Point { x: 0.0, y: 20.0 },
        };
        let mut q = MsgQueue::default();
        assert_eq!(
            w.event(&ev, area, &mut q),
            EventResult::Ignored,
            "折叠态应放行而非吞掉"
        );
        assert!(q.is_empty(), "折叠态不得发消息");
    }

    /// 只读侧栏 (全 None 子句表) 下点击必须被吞掉, 且不发出任何消息 ——
    /// 否则点空侧栏会穿透去选中底下的日志行。
    #[test]
    fn readonly_sidebar_swallows_clicks_without_message() {
        let mut w = LevelHistogram::new();
        assert!(w.queries.iter().all(Option::is_none), "新建即只读");
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 600.0);
        for i in 0..Level::ALL.len() {
            let r = row_rect(area, i);
            let ev = Event::MouseInput {
                button: MouseButton::Left,
                pressed: true,
                position: Point {
                    x: r.origin.x + 4.0,
                    y: r.origin.y + 4.0,
                },
            };
            let mut q = MsgQueue::default();
            assert_eq!(
                w.event(&ev, area, &mut q),
                EventResult::Consumed,
                "第 {i} 行点击应被吞"
            );
            assert!(q.is_empty(), "只读侧栏不得发消息");
        }
    }

    /// 对数刻度: 0 → 0; 最大 → 1; 单行计数仍可见; 单调不减。
    #[test]
    fn bar_fraction_is_log_scaled_and_visible_at_one() {
        assert_eq!(bar_fraction(0, 4_544_133), 0.0, "0 计数不成条");
        assert_eq!(bar_fraction(0, 0), 0.0, "空文件不除零");
        assert_eq!(bar_fraction(100, 100), 1.0, "最大者满格");
        assert_eq!(bar_fraction(200, 100), 1.0, "超过最大仍钳到满格");

        // 这是选对数刻度的全部理由: 4_760 / 4_544_133 线性下是 0.1% 亚像素,
        // 对数下必须仍占可见宽度。
        let rare = bar_fraction(4_760, 4_544_133);
        assert!(
            rare > 0.4,
            "Fatal 4760 对 Info 4544133 应占 >40% 条宽, 实得 {rare}"
        );

        // 单行计数也要可见 (log10 加 1 的意义)
        assert!(bar_fraction(1, 4_544_133) > 0.0);

        // 单调不减
        let mut prev = -1.0f32;
        for c in [0u64, 1, 10, 100, 1_000, 100_000, 4_544_133] {
            let f = bar_fraction(c, 4_544_133);
            assert!(f >= prev, "计数 {c} 的比例 {f} 小于前一个 {prev}");
            prev = f;
        }
    }

    /// 命中测试: 每行各中一次, 顺序与 `Level::ALL` 对齐; 行外不中。
    #[test]
    fn row_at_maps_rows_in_level_all_order() {
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 600.0);
        for i in 0..Level::ALL.len() {
            let r = row_rect(area, i);
            let p = Point {
                x: r.origin.x + r.size.width / 2.0,
                y: r.origin.y + r.size.height / 2.0,
            };
            assert_eq!(row_at(area, p), Some(i), "第 {i} 行应命中");
            assert_eq!(Level::ALL[i], Level::ALL[i]);
        }
        // 顶部内边距内不中
        assert_eq!(row_at(area, Point { x: 5.0, y: 1.0 }), None);
        // 全部行之下不中
        let below = PAD_Y + Level::ALL.len() as f32 * ROW_H + 1.0;
        assert_eq!(row_at(area, Point { x: 5.0, y: below }), None);
        // 右侧之外不中
        assert_eq!(
            row_at(
                area,
                Point {
                    x: HIST_WIDTH + 1.0,
                    y: 20.0
                }
            ),
            None
        );
    }

    /// 行矩形不重叠且按序下排 —— 否则邻行点击会串。
    #[test]
    fn row_rects_do_not_overlap() {
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 600.0);
        for i in 1..Level::ALL.len() {
            let prev = row_rect(area, i - 1);
            let cur = row_rect(area, i);
            assert!(
                prev.origin.y + prev.size.height <= cur.origin.y,
                "第 {} 行与第 {i} 行重叠",
                i - 1
            );
        }
    }
}
