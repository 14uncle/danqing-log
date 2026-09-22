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
//! 「当前生效行高亮」与「清除筛选」行 (纯文字, 前缀符号经实机两轮淘汰:
//! `✕` 不在内嵌字体子集从未渲染, `×` 被判画蛇添足)。
//!
//! .log / 无级别列的 JSONL 下侧栏**只读** (D3: 点选限 JSONL 字段过滤通路)。
//! 只读态在**顶部**给一行「仅统计·不可点选」说明 (2026-09-14: 两种模式侧栏长得一样,
//! 用户实机把「活着但不能点」读成了「坏了」; 首版放底部角落同日被否 —— 谁看得到啊)。
//!
//! 布局: 本组件在 [`crate::sidebar::Sidebar`] 容器里 (直方图吃剩余高度 +
//! 字段分析区自然高), 容器是 LogView 的 **sibling** —— 不侵入 LogView 内部的
//! 坐标数学, 后者只是拿到一个更窄的 `area`。**宽度由容器给** (tight),
//! 本组件不重判折叠 (2026-09-20 实机教训, 见 `sidebar.rs` 模块头)。

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
pub(crate) const PAD_X: f32 = 10.0;
/// 顶部内边距。
pub(crate) const PAD_Y: f32 = 8.0;
/// 行高 (标签行 + 横条行 + 行距)。
pub(crate) const ROW_H: f32 = 28.0;
/// 标签/计数字号。
pub(crate) const LABEL_SIZE: u16 = 12;
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
/// 只读态顶部说明文案。直接回答用户实机的两条困惑:「有没有在统计」(仅统计) +
/// 「为什么点不动」(不可点选)。守卫 `readonly_hint_fits_sidebar_width` 钉住它
/// 一行放得下侧栏 —— 放不下就是截断, 还不如不写。
const READONLY_HINT: &str = "仅统计·不可点选";
/// 只读说明行占的顶部高度: 可见时桶行几何整体下移这么多。
/// **必须是常量**而不是实测行高 —— event 路径没有 `TextBatch`, 量不了。
const HINT_ROW_H: f32 = 20.0;

/// 侧栏的有效宽度: 关掉 (`Ctrl+L`), 或窗口窄到容不下 → 0。
///
/// 窄窗自动折叠的必要性: 侧栏是固定宽, 窗口 400px 时内容区只剩 288px,
/// 而新用户未必知道有 `Ctrl+L` —— 卡在没法看的布局里比看不到直方图糟。
/// `available` 是**整个 Row 的可用宽** (Fit 子项拿到的是宽松约束)。
///
/// paint/event 不重判这个函数, 而是看 layout 给出的实际宽度 (`area.size.width`)
/// —— 判定只有一处, 不存在「宽度 0 却还在画/还在吃点击」的漏判。
pub(crate) fn effective_width(visible: bool, available: f32) -> f32 {
    if visible && available >= MIN_CONTENT_WIDTH {
        HIST_WIDTH
    } else {
        0.0
    }
}

/// 「清除筛选」行的序号 (紧接 6 个桶之后)。
const CLEAR_ROW: usize = Level::ALL.len();

/// 第 `i` 行的命中矩形: `0..6` 是桶 (序同 [`Level::ALL`]), [`CLEAR_ROW`] 是
/// 清除行 (仅在有生效筛选时出现, 见 paint)。`inset` = 顶部说明行占的高度
/// (见 [`LevelHistogram::top_inset`]) —— 行几何只有这一处真身, paint 与
/// 命中测试共用, 各写一套就会「画在这里、点在那里」。
fn row_rect(area: Rect, i: usize, inset: f32) -> Rect {
    let y = if i == CLEAR_ROW {
        area.origin.y + PAD_Y + inset + Level::ALL.len() as f32 * ROW_H + CLEAR_ROW_GAP
    } else {
        area.origin.y + PAD_Y + inset + i as f32 * ROW_H
    };
    Rect::from_xywh(area.origin.x, y, area.size.width, ROW_H)
}

/// 命中测试: 点落在第几行; 行外 (或行号越界) → None。
///
/// 纯几何 —— 是否**可点**由状态决定 (子句表有无 / 有没有生效的筛选),
/// 见 [`LevelHistogram::is_row_clickable`]。
fn row_at(area: Rect, p: Point, inset: f32) -> Option<usize> {
    (0..=CLEAR_ROW).find(|i| row_rect(area, *i, inset).contains(p))
}

/// 「清除筛选」文本在给定矩形内**两轴居中**的落点 (返回 x 与 baseline y)。
/// 调用方传**悬停底色块** (用户眼里的「按钮」), 不是行矩形 —— 两者中心差 2.5px。
/// 抽成纯函数, 两条守卫各管一层:
/// `centered_text_origin_puts_the_box_middle_in_the_rect` (公式) 与
/// `clear_row_label_ink_is_centered_in_the_button` (调用点, 量**画出来的 ink**)。
fn centered_text_origin(row: Rect, text_w: f32, line_h: f32, ascent: f32) -> (f32, f32) {
    let x = row.origin.x + (row.size.width - text_w) / 2.0;
    let baseline = row.origin.y + (row.size.height - line_h) / 2.0 + ascent;
    (x, baseline)
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
    /// `active` 重算所依据的**已应用过滤原串**。
    ///
    /// 缓存理由: `active` 的判定要过规范化 (`parse_filter`), 那是分配型操作,
    /// 不该进每帧的 `sync`。判定依据只有两个来源 —— 用户改了过滤串、或换文件
    /// 导致子句表变了 —— 两者任一变化才重算 (子句表的变化由 `queries` 自身比对)。
    active_src: String,
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
            active_src: String::new(),
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

    /// 顶部说明行占的高度 (0 或 [`HINT_ROW_H`]) —— 桶行几何整体下移这么多。
    /// paint 与 event 都经它取 inset 喂给 [`row_rect`]/[`row_at`], 判定只有一处。
    fn top_inset(&self) -> f32 {
        if self.readonly_hint_visible() {
            HINT_ROW_H
        } else {
            0.0
        }
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
        let queries_changed = self.queries != app.level_queries;
        self.queries = app.level_queries.clone();
        self.pending = app.levels_pending;
        self.file_open = app.has_file;
        // 生效行由**已应用的过滤串**反推, 不另存状态 —— 手打 `LEVEL=ERROR*`
        // 与点柱条走同一条判定, 两者行为一致。
        //
        // **比的是规范化之后的子句集, 不是原串** (2026-09-15 review 抓):
        // 键名大小写不同 (`LEVEL=ERROR*`) 的手打查询筛的是**同一批行**, 指示器与
        // `清除筛选` 必须跟着亮 —— 拿原串比会出现最别扭的一种状态: 结果确实被筛了,
        // 侧栏却说没有筛选生效、清除行也点不动。
        // 两侧都过 `parse_filter` (同一道规范化), 等值判定才与匹配口径同源。
        if queries_changed || self.active_src != app.filter_applied {
            self.active_src = app.filter_applied.clone();
            let applied = app.parse_filter(&app.filter_applied);
            self.active = Level::ALL.iter().copied().find(|l| {
                self.queries[*l as usize]
                    .as_deref()
                    .is_some_and(|q| app.parse_filter(q) == applied)
            });
        }
    }

    fn layout(&mut self, constraints: Constraints, _texts: &mut TextBatch) -> Size {
        let max = constraints.max();
        let h = if max.height.is_finite() {
            max.height
        } else {
            0.0
        };
        // 宽度**拿来即用** (截到 HIST_WIDTH): 折叠判定 (Ctrl+L / 窄窗) 在
        // 侧栏容器做一次 —— 它拿得到整个 Row 的可用宽, 本组件拿不到
        // (见 `sidebar.rs` 模块头: 在这里重判会把自己的宽误判成窄窗)。
        Size::new(max.width.min(HIST_WIDTH), h)
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
        let inset = self.top_inset();

        // 只读说明行 (.log / 无级别列 JSONL): 放**顶部** —— 底部角落同日被实机否掉
        // (「谁看得到啊」)。颜色取状态栏同款的次级正文色 (实机反馈: 正文色太亮),
        // 与底部「Ctrl+L 收起」一支色, 整栏只读时视觉一致。桶行几何经 `inset` 下移。
        if self.readonly_hint_visible() {
            texts.push_text(
                READONLY_HINT,
                bar_x,
                area.origin.y + PAD_Y + texts.ascent(f32::from(LABEL_SIZE)),
                LABEL_SIZE,
                self.text_secondary,
            );
        }

        for (i, level) in Level::ALL.iter().enumerate() {
            // S3 (2026-09-14): 行几何**只认 `row_rect` 一个真身** —— 原先这里
            // 内联 `area.origin.y + PAD_Y + inset + i*ROW_H`, 与 `row_rect`/`row_at`
            // 各推一遍, 「同规则同常量」只靠注释维持。收口后 paint/命中/高亮三处同源。
            let row_rect = row_rect(area, i, inset);
            let row_y = row_rect.origin.y;
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

        // 有生效筛选时多一行「清除筛选」—— 人工验收的第二条反馈: 用户点完级别
        // 想退回全部数据时, 唯一的办法是去过滤框按 Esc。这行把回路摆在明处
        // (再点生效那行也能清, 但那是隐式的)。
        if self.active.is_some() {
            let row = row_rect(area, CLEAR_ROW, inset);
            // 悬停底色块 = 用户眼里的「按钮」。它与行矩形**不同心**
            // (块 = [ry-2, ry+24], 中心 ry+11; 行 = [ry, ry+28], 中心 ry+14) ——
            // 曾按行矩形居中文本, 实机读作「按钮内偏下」(差 2.6px)。
            // 故文本居中的参照物是这个块, 不是行矩形; 文本位置不随悬停跳变。
            let btn = Rect::from_xywh(
                area.origin.x + 2.0,
                row.origin.y - 2.0,
                area.size.width - 4.0,
                ROW_H - 2.0,
            );
            let hovered = self.hover.get() == Some(CLEAR_ROW);
            if hovered {
                rects.push_rect(btn, self.active_bg, 3.0);
            }
            // 不带前缀符号: 原 `✕` 不在内嵌 Sarasa 子集里 (栅格化 0×0 空字形,
            // 从未渲染), 换 `×` 又被实机判「画蛇添足」—— 纯文字 (2026-09-14)。
            let label = "清除筛选";
            let (tx, baseline) = centered_text_origin(
                btn,
                texts.measure(label, LABEL_SIZE),
                line_h,
                texts.ascent(f32::from(LABEL_SIZE)),
            );
            texts.push_text(
                label,
                tx,
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
                    .set(row_at(area, *p, self.top_inset()).filter(|i| self.is_row_clickable(*i)));
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
        let Some(i) = row_at(area, *position, self.top_inset()) else {
            return EventResult::Ignored;
        };
        // 不可点 (明文模式 / 无子句的桶 / 没有生效筛选时的清除行) —— 吞掉点击,
        // 不让它穿透到底下的列表 (点在有东西的地方不该毫无回应地选中底下的行)。
        // M3 (2026-09-14 实机 M0 P21): 吞掉时**说清为什么** —— 原先静默,
        // 用户不知道是没点中还是程序没响应。
        if !self.is_row_clickable(i) {
            let reason = if i == CLEAR_ROW {
                "无生效筛选可清除"
            } else {
                "本级别不可点选 (仅统计)"
            };
            msgs.push(Box::new(Msg::Notice(
                reason.into(),
                crate::NoticeKind::Info,
            )));
            return EventResult::Consumed;
        }
        if i == CLEAR_ROW {
            // 「清除筛选」走与「再点生效行」同一套切换语义 (可点判定已保证
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
        let last_bucket = row_rect(area, Level::ALL.len() - 1, 0.0);
        let clear = row_rect(area, CLEAR_ROW, 0.0);
        assert!(
            clear.origin.y >= last_bucket.origin.y + last_bucket.size.height,
            "清除行必须完全在最后一个桶之下"
        );
        // 命中测试也要能落到它
        let p = Point {
            x: clear.origin.x + 4.0,
            y: clear.origin.y + 4.0,
        };
        assert_eq!(row_at(area, p, 0.0), Some(CLEAR_ROW));
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
            let r = row_rect(area, i, 0.0);
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
            let r = row_rect(area, i, 0.0);
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
        let r = row_rect(area, CLEAR_ROW, 0.0);
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

    /// **指示器跟着规范化走, 不跟原串走** (2026-09-15 review 抓的缺口)。
    ///
    /// 键名大小写不同 (`LEVEL=ERROR*`) 的手打查询, 过滤结果与 `level=ERROR*`
    /// 逐行相同; 若拿原串比, 侧栏会进入最别扭的状态 —— 结果确实被筛了, 却没有任何
    /// 桶行高亮, 且「清除筛选」点不动 (它只在 `active.is_some()` 时可点)。
    #[test]
    fn active_bucket_follows_normalized_filter_not_raw_string() {
        let mut app = LogApp::new_empty();
        app.schema = Some(std::sync::Arc::new(crate::jsonl::Schema {
            columns: vec![crate::jsonl::Column {
                name: "level".into(),
                width_chars: 5,
            }],
        }));
        app.level_queries = levels::level_queries_for("level");
        let mut w = LevelHistogram::new();

        app.filter_applied = "LEVEL=ERROR*".into();
        w.sync(&app);
        assert_eq!(
            w.active,
            Some(Level::Error),
            "键名大小写不同仍是同一个筛选 → 桶行必须高亮"
        );
        assert!(w.is_row_clickable(CLEAR_ROW), "有生效筛选 → 清除行必须可点");

        // 大小写一致的写法照旧 (不得因归一化反而失配)
        app.filter_applied = "level=ERROR*".into();
        w.sync(&app);
        assert_eq!(w.active, Some(Level::Error), "同写法照旧点亮");

        // 真正不等价的查询不得点亮
        app.filter_applied = "status=500".into();
        w.sync(&app);
        assert_eq!(w.active, None, "另一个查询不是这个桶");

        // 空过滤 = 无生效
        app.filter_applied.clear();
        w.sync(&app);
        assert_eq!(w.active, None);
        assert!(!w.is_row_clickable(CLEAR_ROW), "无筛选时清除行不可点");
    }

    /// 「清除筛选」行点击要发出消息 (走与「再点生效行」同一套切换语义)。
    #[test]
    fn clear_row_click_emits_message() {
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 600.0);
        let mut w = LevelHistogram::new();
        w.queries = levels::level_queries_for("level");
        w.active = Some(Level::Error); // 模拟 `level=ERROR*` 已生效

        let r = row_rect(area, CLEAR_ROW, 0.0);
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

    /// 只读侧栏 (全 None 子句表) 下点击仍必须被**吞掉** (不穿透去选中底下的日志行),
    /// 但不再静默 —— M3 (P21) 起每次吞掉都附一条说明。
    ///
    /// 本测试原名 `readonly_sidebar_swallows_clicks_without_message`, 断言「不发
    /// 任何消息」。P21 把「静默吞掉」判成了缺陷 (用户分不清「没点中」与「程序没
    /// 响应」), 故**跟着改判**: 「不穿透」这条不变式原样保留, 「不发声」那条反转
    /// 成「必须恰好说一条, 且是带原因的 Notice」。
    #[test]
    fn readonly_sidebar_swallows_clicks_but_says_why() {
        let mut w = LevelHistogram::new();
        assert!(w.queries.iter().all(Option::is_none), "新建即只读");
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 600.0);
        for i in 0..Level::ALL.len() {
            let r = row_rect(area, i, 0.0);
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
                "第 {i} 行点击应被吞 (不穿透)"
            );
            assert_eq!(q.len(), 1, "第 {i} 行吞掉时须**恰好**说一条原因");
            let msg = q[0].downcast_ref::<Msg>().expect("消息应是 Msg");
            assert!(
                matches!(msg, Msg::Notice(text, _) if !text.is_empty()),
                "第 {i} 行的拒绝须带上原因 (非空 Notice)"
            );
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
            let r = row_rect(area, i, 0.0);
            let p = Point {
                x: r.origin.x + r.size.width / 2.0,
                y: r.origin.y + r.size.height / 2.0,
            };
            assert_eq!(row_at(area, p, 0.0), Some(i), "第 {i} 行应命中");
            assert_eq!(Level::ALL[i], Level::ALL[i]);
        }
        // 顶部内边距内不中
        assert_eq!(row_at(area, Point { x: 5.0, y: 1.0 }, 0.0), None);
        // 全部行之下不中
        let below = PAD_Y + Level::ALL.len() as f32 * ROW_H + 1.0;
        assert_eq!(row_at(area, Point { x: 5.0, y: below }, 0.0), None);
        // 右侧之外不中
        assert_eq!(
            row_at(
                area,
                Point {
                    x: HIST_WIDTH + 1.0,
                    y: 20.0
                },
                0.0
            ),
            None
        );
    }

    /// 行矩形不重叠且按序下排 —— 否则邻行点击会串。
    #[test]
    fn row_rects_do_not_overlap() {
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 600.0);
        for i in 1..Level::ALL.len() {
            let prev = row_rect(area, i - 1, 0.0);
            let cur = row_rect(area, i, 0.0);
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

    /// 只读说明行在顶部占位时, 桶行整体下移 [`HINT_ROW_H`] 且**命中测试跟着走** ——
    /// 「画在这里、点在那里」就是 paint 与 event 各写一套几何时的事故形态。
    #[test]
    fn readonly_hint_shifts_rows_and_hit_testing_follows() {
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 600.0);
        let plain = LevelHistogram::new(); // 空态: 无 inset
        let mut ro = LevelHistogram::new();
        ro.file_open = true; // 有文件 + 全 None + 非 pending → 只读说明可见
        assert_eq!(plain.top_inset(), 0.0);
        assert_eq!(ro.top_inset(), HINT_ROW_H);
        for i in 0..Level::ALL.len() {
            let shifted = row_rect(area, i, ro.top_inset());
            let unshifted = row_rect(area, i, plain.top_inset());
            assert_eq!(
                shifted.origin.y - unshifted.origin.y,
                HINT_ROW_H,
                "第 {i} 行应整体下移 HINT_ROW_H"
            );
            // 新位置命中第 i 行
            let p = Point {
                x: shifted.origin.x + 4.0,
                y: shifted.origin.y + 4.0,
            };
            assert_eq!(
                row_at(area, p, ro.top_inset()),
                Some(i),
                "下移后第 {i} 行命中不对"
            );
            // 旧位置不再命中第 i 行 (落进上一行或说明行)
            let old_p = Point {
                x: unshifted.origin.x + 4.0,
                y: unshifted.origin.y + 4.0,
            };
            assert_ne!(
                row_at(area, old_p, ro.top_inset()),
                Some(i),
                "旧位置不应再命中第 {i} 行"
            );
        }
    }

    /// **纯函数那一层**: 给定盒子宽度, `centered_text_origin` 把它放在矩形正中
    /// (两轴)。浮点断言留 ε, 不比精确值。
    ///
    /// **只管公式, 不管喂进去的数**。这一条此前叫
    /// `clear_row_text_is_centered_in_its_button` —— **名过其实**: 它喂的是
    /// **合成矩形** (`x = 3.0`, 真实调用点是 `area.x + 2.0`) 和**写死的 `48.0`**,
    /// 从不碰 `texts.measure()` 也不碰真实 `area`。而本模块**恰恰栽在「宽度错」上过**
    /// (`✕` U+2715 是 0×0 空字形却占着 6px advance, 把整串文本顶偏)。
    /// 名字改准, 免得下一个读的人以为调用点被覆盖了 —— 「宣称有守卫比没守卫更坏」。
    #[test]
    fn centered_text_origin_puts_the_box_middle_in_the_rect() {
        let btn = Rect::from_xywh(2.0, 100.0, HIST_WIDTH - 4.0, ROW_H - 2.0);
        let (x, baseline) = centered_text_origin(btn, 48.0, 15.0, 11.58);
        assert!(
            (x - (2.0 + (HIST_WIDTH - 4.0 - 48.0) / 2.0)).abs() < 0.01,
            "水平居中于按钮"
        );
        // 文本行盒 [baseline-ascent, baseline-ascent+line_h] 的中点 = 按钮中点
        let text_mid = baseline - 11.58 + 15.0 / 2.0;
        assert!(
            (text_mid - (100.0 + (ROW_H - 2.0) / 2.0)).abs() < 0.01,
            "垂直居中于按钮"
        );
    }

    /// **调用点那一层** (2026-09-15 用户裁定「彻底版」): 量的是**画出来的字形矩形**
    /// (`TextBatch::instance_rects`, 本批新加), 不是算出来的盒子 ——
    /// 「清除筛选」的 ink 包围盒中心必须与悬停色块中心重合。
    ///
    /// **为什么非要量 ink**: `measure` 给的是 **advance 之和**, 眼睛看的是 **ink**。
    /// `push_text` 按 `round(pen_x + bearing_x)` 落点、按 `info.width` 定宽, 两者天生
    /// 不等; 更要命的是缺字 —— `✕` (U+2715) 不在内嵌子集里, 是 **0×0 空字形却照样
    /// 占 6px advance**, 于是「按 advance 居中」的文本在屏上是偏的, 而当时
    /// **没有任何一把尺能量到**, 只能靠人眼在手写基准里比。这条把「看着居中」
    /// 第一次变成可断言的事。
    #[test]
    fn clear_row_label_ink_is_centered_in_the_button() {
        let area = Rect::from_xywh(0.0, 0.0, HIST_WIDTH, 600.0);
        let paint = |active: Option<Level>, hover: Option<usize>| {
            let mut w = LevelHistogram::new();
            w.active = active;
            w.hover.set(hover);
            let mut rects = RectBatch::new();
            let mut texts = TextBatch::new();
            w.paint(area, &mut rects, &mut texts);
            (rects.instance_rects(), texts.instance_rects())
        };

        let (_, base_txt) = paint(None, None);
        // **色块的对照组必须也带 active** —— 生效行自己就有一个色块, 拿「无 active」
        // 那一版比会把两个块一起算成「悬停带来的」(第一版就是这么写错的)。
        // (字形的对照用 `None` 那版: 它没有清除行, 差值就是那 4 个字形。)
        let (active_rects, with_txt) = paint(Some(Level::Info), None);
        let (hot_rects, _) = paint(Some(Level::Info), Some(CLEAR_ROW));

        // ① 底色块 = **悬停**前后唯一多出来的那个矩形
        let extra: Vec<Rect> = hot_rects
            .iter()
            .copied()
            .filter(|r| !active_rects.contains(r))
            .collect();
        assert_eq!(extra.len(), 1, "悬停应恰好多一个底色块, 实得 {extra:?}");
        let block = extra[0];

        // ② 「清除筛选」的 4 个字形 = 有 active 时多出来的那一串。它插在底部提示
        //    **之前**, 故出现在序列中段 —— 按第一处差异定位, 不按下标硬取。
        assert_eq!(
            with_txt.len(),
            base_txt.len() + 4,
            "「清除筛选」应是 4 个字形 (少数一个 = 有字形没画出来)"
        );
        let i = base_txt
            .iter()
            .zip(with_txt.iter())
            .position(|(a, b)| a != b)
            .expect("有 active 时应当多出标签");
        let label = &with_txt[i..i + 4];

        // ③ ink 包围盒中心 == 色块中心
        let left = label.iter().map(|g| g.origin.x).fold(f32::MAX, f32::min);
        let right = label
            .iter()
            .map(|g| g.origin.x + g.size.width)
            .fold(f32::MIN, f32::max);
        let ink_c = (left + right) / 2.0;
        let btn_c = block.origin.x + block.size.width / 2.0;
        assert!(
            (ink_c - btn_c).abs() <= 0.5,
            "「清除筛选」ink 中心 {ink_c} 与色块中心 {btn_c} 差 {:.1}px —— \
             按 advance 居中不等于看着居中",
            (ink_c - btn_c).abs()
        );
    }
}
