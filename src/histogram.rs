//! @author 十四叔
//! @date 2026/09/12
//!
//! 级别计数侧栏 (level-histogram 模块, spec: docs/specs/SPEC-level-histogram.md):
//! 左侧常驻窄栏, 6 行 = 级别名 + 计数 + 横条。
//!
//! 为什么是**对数刻度**横条: 线性刻度下 Info 4544133 会把 Fatal 4760 压成亚像素,
//! 而后者恰恰是唯一要看的那一根。计数数字始终以文本完整显示, 横条只表相对量级。
//!
//! hover 只给**可点行** (2026-09-13 验收改判: 原「不做 hover」被实机证伪) ——
//! 反馈本身即「哪几行能点」的说明书, 故只读行不得有反馈; 点过之后另有
//! 「当前生效行高亮」与「✕ 清除筛选」行。
//!
//! .log / 无级别列的 JSONL 下侧栏**只读** (D3: 点选限 JSONL 字段过滤通路)。
//! 只读态底部给一行「仅统计·不可点选」说明 (2026-09-14: 两种模式侧栏长得一样,
//! 用户实机把「活着但不能点」读成了「坏了」)。
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
/// 「清除筛选」行与 6 个桶之间的间距 (行序号 = 6, 见 [`row_rect`])。
const CLEAR_ROW_GAP: f32 = 10.0;
/// 只读态底部说明文案。直接回答用户实机的两条困惑:「有没有在统计」(仅统计) +
/// 「为什么点不动」(不可点选)。守卫 `readonly_hint_fits_sidebar_width` 钉住它
/// 一行放得下侧栏 —— 放不下就是截断, 还不如不写。
const READONLY_HINT: &str = "仅统计·不可点选";

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

/// 「清除筛选」行的序号 (紧接 6 个桶之后)。
const CLEAR_ROW: usize = Level::ALL.len();

/// 第 `i` 行的命中矩形: `0..6` 是桶 (序同 [`Level::ALL`]), [`CLEAR_ROW`] 是
/// 清除行 (仅在有生效筛选时出现, 见 paint)。
fn row_rect(area: Rect, i: usize) -> Rect {
    let y = if i == CLEAR_ROW {
        area.origin.y + PAD_Y + Level::ALL.len() as f32 * ROW_H + CLEAR_ROW_GAP
    } else {
        area.origin.y + PAD_Y + i as f32 * ROW_H
    };
    Rect::from_xywh(area.origin.x, y, area.size.width, ROW_H)
}

/// 命中测试: 点落在第几行; 行外 (或行号越界) → None。
///
/// 纯几何 —— 是否**可点**由状态决定 (子句表有无 / 有没有生效的筛选),
/// 见 [`LevelHistogram::is_row_clickable`]。
fn row_at(area: Rect, p: Point) -> Option<usize> {
    (0..=CLEAR_ROW).find(|i| row_rect(area, *i).contains(p))
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

/// 桶 → 横条色。前四档复用**单元格着色**的语义色板 (`view::LevelPalette`),
/// 保证侧栏色带与内容列色一致; 「其他」桶是「无信息」桶, 由主题的次要色降噪。
fn bucket_color(level: Level, palette: &view::LevelPalette, text_secondary: Color) -> Color {
    match level {
        Level::Fatal | Level::Error => palette.error,
        Level::Warn => palette.warn,
        Level::Info => palette.info,
        Level::DebugTrace => palette.trace,
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
    /// 是否有文件打开 (空态 false) —— 只读说明行的显示条件之一, 空态不贴。
    file_open: bool,
    /// 当前生效的过滤对应的桶 (行高亮); None = 无。
    active: Option<Level>,
    /// 鼠标悬停的行 (仅**可点**的行会进这里)。
    ///
    /// 人工验收反馈: 没有 hover 反馈, 用户不知道哪些行能点 (spec 原 Open Question
    /// 「倾向不做 hover」由此改判)。**只在可点行上给反馈**是有意的 —— 反馈本身
    /// 就把「哪几行能点」教给了用户。
    hover: std::cell::Cell<Option<usize>>,
    // 主题色在 sync 期解析并缓存, paint 期零查表 (与 view.rs 同款)。
    bg: Color,
    text_primary: Color,
    text_secondary: Color,
    /// 生效行底色 (主题的行底色, 与内容区选中行同源)。
    active_bg: Color,
    /// 强调色 —— hover 反馈用它 (与「生效行」的底色是两个通道, 不会混)。
    accent: Color,
    /// 语义色板 (横条着色)。与 `text_*` 一样在 sync 期解析 —— 它**随主题变**
    /// (暗色底要提亮, 见 `view::LevelPalette`)。
    palette: view::LevelPalette,
}

impl LevelHistogram {
    pub(crate) fn new() -> Self {
        Self {
            counts: LevelCounts::default(),
            queries: levels::no_level_queries(),
            visible: true,
            pending: false,
            file_open: false,
            active: None,
            hover: std::cell::Cell::new(None),
            bg: Color::rgb(1.0, 1.0, 1.0),
            text_primary: Color::rgb(0.12, 0.12, 0.12),
            text_secondary: Color::rgb(0.40, 0.40, 0.42),
            active_bg: Color::rgb(0.93, 0.93, 0.94),
            accent: Color::rgb(0.18, 0.35, 0.60),
            // 占位, 与上面几支同款 —— 首帧 sync 就会被真主题覆盖。
            palette: view::LevelPalette::light_cell(),
        }
    }

    /// 该行此刻是否可点: 桶行看子句表, 清除行看有没有生效的筛选。
    ///
    /// 只有可点的行才给 hover 反馈 —— 反馈本身即「哪几行能点」的说明书,
    /// 故这个判定必须与 [`Self::event`] 的可点判定**同源**, 不许各写一套。
    fn is_row_clickable(&self, i: usize) -> bool {
        match i {
            CLEAR_ROW => self.active.is_some(),
            _ => self.queries[i].is_some(),
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

    /// 只读说明行是否可见: 有文件打开 + 计数已交付 + 子句表全 None
    /// (明文 .log, 或无级别类列的 JSONL —— 两者都是永久只读)。
    ///
    /// **pending 期间不显示**: 那时只读是暂时的 (JSONL 交付后即变得可点),
    /// 提前贴「不可点选」是假话。**空态不显示**: 没文件时六个 0 行之上
    /// 再贴一行说明是噪音。
    fn readonly_hint_visible(&self) -> bool {
        self.file_open && !self.pending && self.queries.iter().all(Option::is_none)
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
        self.accent = t.accent();
        self.palette = view::LevelPalette::for_cell(&t);
        self.counts = *app.level_counts.as_ref();
        self.queries = app.level_queries.clone();
        self.visible = app.histogram_visible;
        self.pending = app.levels_pending;
        self.file_open = app.has_file;
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

            // 级别名 (左) + 计数 (右对齐)
            //  - 只读行: 次要色降噪 (点不动, 不该显得可交互)
            //  - 可点行: 正文色
            //  - 悬停的可点行: 强调色 —— 人工验收要求「让用户知道是可点击对象」
            let clickable = self.queries[*level as usize].is_some();
            let hovered = clickable && self.hover.get() == Some(i);
            let color = if hovered {
                self.accent
            } else if clickable {
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
                if hovered {
                    self.accent
                } else {
                    self.text_secondary
                },
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
                    bucket_color(*level, &self.palette, self.text_secondary),
                    2.0,
                );
            }
        }

        // 有生效筛选时多一行「✕ 清除筛选」—— 人工验收的第二条反馈: 用户点完级别
        // 想退回全部数据时, 唯一的办法是去过滤框按 Esc。这行把回路摆在明处
        // (再点生效那行也能清, 但那是隐式的)。
        if self.active.is_some() {
            let ry = row_rect(area, CLEAR_ROW).origin.y;
            let baseline = ry + texts.ascent(f32::from(LABEL_SIZE));
            let hovered = self.hover.get() == Some(CLEAR_ROW);
            if hovered {
                rects.push_rect(
                    Rect::from_xywh(
                        area.origin.x + 2.0,
                        ry - 2.0,
                        area.size.width - 4.0,
                        ROW_H - 2.0,
                    ),
                    self.active_bg,
                    3.0,
                );
            }
            texts.push_text(
                "✕ 清除筛选",
                bar_x,
                baseline,
                LABEL_SIZE,
                if hovered {
                    self.text_primary
                } else {
                    self.accent
                },
            );
        }

        // 底部提示: 收起侧栏只有 `Ctrl+L` 一个入口, 而界面上没有任何可见控件 ——
        // 人工验收反馈「用户怎么知道按 Ctrl+L」。放导轨底部, 不挤占计数区。
        // 只读态 (.log / 无级别列 JSONL) 在它上面再加一行说明 —— 两种模式的侧栏
        // 长得一样, 只是一个能点一个不能, 用户实机把「活着但不能点」读成了「坏了」
        // (2026-09-14)。一句话摆明「在统计、点不了」。
        if self.readonly_hint_visible() {
            texts.push_text(
                READONLY_HINT,
                bar_x,
                area.origin.y + area.size.height - 8.0 - line_h - 4.0,
                LABEL_SIZE,
                self.text_secondary,
            );
        }
        let hint = "Ctrl+L 收起";
        texts.push_text(
            hint,
            bar_x,
            area.origin.y + area.size.height - 8.0,
            LABEL_SIZE,
            self.text_secondary,
        );
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        if area.size.width < 1.0 {
            return EventResult::Ignored;
        }
        // hover: 只在**可点**行上留痕。反馈本身就是「哪几行能点」的说明书 ——
        // 人工验收反馈「鼠标挪上去 UI 没有反馈」, 这条即其修法。
        match event {
            Event::CursorMoved(p) => {
                self.hover
                    .set(row_at(area, *p).filter(|i| self.is_row_clickable(*i)));
                return EventResult::Ignored;
            }
            Event::CursorLeft => {
                self.hover.set(None);
                return EventResult::Ignored;
            }
            _ => {}
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
        // 不可点 (明文模式 / 无子句的桶 / 没有生效筛选时的清除行) —— 吞掉点击,
        // 不让它穿透到底下的列表 (点在有东西的地方不该毫无回应地选中底下的行)。
        if !self.is_row_clickable(i) {
            return EventResult::Consumed;
        }
        if i == CLEAR_ROW {
            // 「✕ 清除筛选」走与「再点生效行」同一套切换语义 (可点判定已保证
            // active 为 Some)。
            if let Some(l) = self.active {
                msgs.push(Box::new(Msg::ApplyLevelFilter(l)));
            }
            return EventResult::Consumed;
        }
        msgs.push(Box::new(Msg::ApplyLevelFilter(Level::ALL[i])));
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

    /// 清除行在 6 个桶**之下**, 且不与之重叠 —— 否则点桶会误触清除。
    #[test]
    fn clear_row_sits_below_the_buckets() {
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 600.0);
        let last_bucket = row_rect(area, Level::ALL.len() - 1);
        let clear = row_rect(area, CLEAR_ROW);
        assert!(
            clear.origin.y >= last_bucket.origin.y + last_bucket.size.height,
            "清除行必须完全在最后一个桶之下"
        );
        // 命中测试也要能落到它
        let p = Point {
            x: clear.origin.x + 4.0,
            y: clear.origin.y + 4.0,
        };
        assert_eq!(row_at(area, p), Some(CLEAR_ROW));
    }

    /// **hover 只落在可点行上** —— 这条是「让用户知道是可点击对象」的实现依据:
    /// 反馈本身即说明书, 故只读行不能有反馈 (否则教错)。
    #[test]
    fn hover_only_lands_on_clickable_rows() {
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 600.0);
        let mut q = MsgQueue::default();

        // 只读侧栏 (全 None): 任何行都不该留 hover
        let mut ro = LevelHistogram::new();
        for i in 0..=CLEAR_ROW {
            let r = row_rect(area, i);
            let ev = Event::CursorMoved(Point {
                x: r.origin.x + 4.0,
                y: r.origin.y + 4.0,
            });
            ro.event(&ev, area, &mut q);
        }
        assert_eq!(ro.hover.get(), None, "只读侧栏不得给 hover 反馈");

        // JSONL 口径: 有子句的四行可点, DEBUG/其他 与「清除行」(无生效筛选) 不可点
        let mut w = LevelHistogram::new();
        w.queries = levels::level_queries_for("level");
        for (i, level) in Level::ALL.iter().enumerate() {
            let r = row_rect(area, i);
            let ev = Event::CursorMoved(Point {
                x: r.origin.x + 4.0,
                y: r.origin.y + 4.0,
            });
            w.event(&ev, area, &mut q);
            let want = if levels::field_query("level", *level).is_some() {
                Some(i)
            } else {
                None
            };
            assert_eq!(w.hover.get(), want, "{level:?} 的 hover 反馈不对");
        }
        // 没有生效筛选时, 清除行不可点 → 不给 hover
        let r = row_rect(area, CLEAR_ROW);
        w.event(
            &Event::CursorMoved(Point {
                x: r.origin.x + 4.0,
                y: r.origin.y + 4.0,
            }),
            area,
            &mut q,
        );
        assert_eq!(w.hover.get(), None, "无生效筛选时清除行不可点");
        assert_eq!(
            w.event(&Event::CursorLeft, area, &mut q),
            EventResult::Ignored
        );
        assert_eq!(w.hover.get(), None, "移出后 hover 清空");
    }

    /// 「✕ 清除筛选」行点击要发出消息 (走与「再点生效行」同一套切换语义)。
    #[test]
    fn clear_row_click_emits_message() {
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 600.0);
        let mut w = LevelHistogram::new();
        w.queries = levels::level_queries_for("level");
        w.active = Some(Level::Error); // 模拟 `level=ERROR*` 已生效

        let r = row_rect(area, CLEAR_ROW);
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
            "点击清除行应被消费"
        );
        assert!(!q.is_empty(), "清除行点击必须发出消息");
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

    /// 只读说明的显示条件矩阵: 有文件 + 计数已交付 + 子句表全 None, 三者缺一不可。
    /// pending 期间贴了是假话 (JSONL 交付后即可点), 空态贴了是噪音。
    #[test]
    fn readonly_hint_visibility_matrix() {
        let mut w = LevelHistogram::new();
        assert!(!w.readonly_hint_visible(), "新建 (空态无文件) 不贴");

        w.file_open = true;
        assert!(
            w.readonly_hint_visible(),
            ".log (有文件 + 全 None + 已交付) 应贴"
        );

        w.pending = true;
        assert!(!w.readonly_hint_visible(), "计数在算时不贴 (只是暂时只读)");

        w.pending = false;
        w.queries = levels::level_queries_for("level");
        assert!(!w.readonly_hint_visible(), "JSONL 有子句表不贴");
    }

    /// 只读说明必须一行放得下侧栏 —— 放不下就是截断, 还不如不写。
    /// 字号 / 栏宽 / 文案任一改动超宽, 这里立刻红 (布局数值量了再写, 不估算)。
    #[test]
    fn readonly_hint_fits_sidebar_width() {
        let mut texts = TextBatch::default();
        let w = texts.measure(READONLY_HINT, LABEL_SIZE);
        let avail = HIST_WIDTH - 2.0 * PAD_X;
        assert!(w <= avail, "「{READONLY_HINT}」宽 {w} 超出可用 {avail}");
    }
}
