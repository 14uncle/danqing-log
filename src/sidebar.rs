//! @author 十四叔
//! @date 2026/09/20
//!
//! 侧栏容器 (直方图 + 字段分析区): **宽度折叠判定的唯一发生地**。
//!
//! 侧栏两段的宽度都只有两种取值 —— `HIST_WIDTH` 或 0 (收起)。「收起」有两个
//! 来源: `Ctrl+L` 与窗口太窄, 后者要用**整个 Row 的可用宽**去判
//! ([`effective_width`] 的 `available` 参数)。
//!
//! **只有 Row 的直接子项拿得到那个宽** (Fit 子项收到的是宽松约束)。一旦把判定
//! 下放到列的子项, 它们拿到的是已经被收窄的 cross_max (侧栏自己的 112px),
//! 112 < `MIN_CONTENT_WIDTH` (640) —— 「自成宽」就被误判成「窄窗」而整块归零。
//! 2026-09-20 实机: 直方图在腿二改造后整块消失, 根因即此 (v1.0 时它是 Row 的
//! Fit 子项, 拿的是 1920, 一切正常)。
//!
//! 故本容器:
//! 1. 作为 Row 的子项拿到**整个 Row 的可用宽** → 在这里判一次 (宽/塌);
//! 2. 给内部 Column 钉 **tight** 宽 (0 时整块不布局、不画);
//! 3. 子组件一律「拿来即用」(`constraints.max().width` 截到 `HIST_WIDTH`),
//!    不再各自重判折叠 —— 判定只有一处, 不会两处口径漂移。

use std::any::Any;

use danqing::widget::{Column, EventResult, MsgQueue, Node, Widget};
use danqing::{AnimationCtx, Constraints, Event, ImageBatch, Rect, RectBatch, Size, TextBatch};

use crate::LogApp;
use crate::histogram::effective_width;

/// 侧栏容器。
pub(crate) struct Sidebar {
    /// 内部仍是 Column: 高度分配 (直方图吃剩余 / 分析区自然高) 交给它。
    inner: Column,
    /// `Ctrl+L` 侧栏开关 (sync 期从 app 读)。
    visible: bool,
}

impl Sidebar {
    pub(crate) fn new(histogram: impl Widget + 'static, panel: impl Widget + 'static) -> Self {
        Self {
            inner: Column::new().fill(histogram, 1).fill(panel, 0),
            visible: true,
        }
    }

    /// 本帧的侧栏宽 (0 = 收起)。
    fn width_for(&self, available: f32) -> f32 {
        effective_width(self.visible, available)
    }
}

impl Widget for Sidebar {
    fn sync(&mut self, state: &dyn Any) {
        if let Some(app) = state.downcast_ref::<LogApp>() {
            self.visible = app.histogram_visible;
        }
        self.inner.sync(state);
    }

    fn animate(&mut self, ctx: &AnimationCtx) {
        self.inner.animate(ctx);
    }

    fn layout(&mut self, constraints: Constraints, texts: &mut TextBatch) -> Size {
        let max = constraints.max();
        let w = self.width_for(max.width);
        if w < 1.0 {
            return Size::ZERO; // 收起: 不布局 (整块不占地, 内容区吃全宽)
        }
        let h = if max.height.is_finite() {
            max.height
        } else {
            0.0
        };
        // tight: 子项的 cross_max 至少是侧栏宽 —— 它们据此定宽, 不再看窗口宽。
        let inner = self
            .inner
            .layout(Constraints::tight(Size::new(w, h)), texts);
        Size::new(w, inner.height)
    }

    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        if area.size.width < 1.0 {
            return;
        }
        self.inner.paint(area, rects, texts);
    }

    fn paint_image(&self, area: Rect, images: &mut ImageBatch) {
        self.inner.paint_image(area, images);
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        if area.size.width < 1.0 {
            return EventResult::Ignored;
        }
        self.inner.event(event, area, msgs)
    }

    // 组合组件约定: 暴露内部子项 (焦点遍历/命中路径要能进到直方图与分析区)。
    fn children(&self) -> &[Node] {
        self.inner.children()
    }

    fn children_mut(&mut self) -> &mut [Node] {
        self.inner.children_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogApp;
    use crate::analysis_panel::AnalysisPanel;
    use crate::histogram::{HIST_WIDTH, LevelHistogram};

    fn temp_cfg(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "danqing-log-sidebar-{}-{tag}.toml",
            std::process::id()
        ))
    }

    /// 回归锁 (2026-09-20 实机): 判定必须用整个 Row 的宽做一次 —— 若下放到
    /// 列的子项, 它们拿到的是已收窄的 cross_max (112 < 640), 直方图会被
    /// 误判「窄窗」而整块不画 (实机症状: 侧栏只剩字段分析区)。
    #[test]
    fn sidebar_keeps_a_real_width_so_the_histogram_actually_paints() {
        let mut app = LogApp::new_empty_at(Some(temp_cfg("paint")));
        app.has_file = true;
        let mut sb = Sidebar::new(LevelHistogram::new(), AnalysisPanel::new());
        sb.sync(&app);

        let wide = Constraints::loose(Size::new(1920.0, 900.0));
        let mut texts = TextBatch::new();
        let size = sb.layout(wide, &mut texts);
        assert_eq!(size.width, HIST_WIDTH, "宽屏: 侧栏宽 = HIST_WIDTH");
        let area = Rect::from_xywh(0.0, 0.0, size.width, size.height);
        let mut rects = RectBatch::new();
        sb.paint(area, &mut rects, &mut texts);
        assert!(
            texts.glyph_clips().count() > 0,
            "直方图必须真的画出字形 (宽度被误判归零是这条要防的回归)"
        );
    }

    /// 两种收起路径: `Ctrl+L` 与窄窗, 都归零。
    #[test]
    fn sidebar_collapses_on_toggle_and_narrow_window() {
        let mut app = LogApp::new_empty_at(Some(temp_cfg("collapse")));
        app.has_file = true;
        let mut sb = Sidebar::new(LevelHistogram::new(), AnalysisPanel::new());
        let wide = Constraints::loose(Size::new(1920.0, 900.0));
        // Ctrl+L 关掉
        app.histogram_visible = false;
        sb.sync(&app);
        let mut texts = TextBatch::new();
        assert_eq!(sb.layout(wide, &mut texts).width, 0.0, "Ctrl+L 收起");
        // 窄窗 (内容区不足 640) 自动折叠
        app.histogram_visible = true;
        sb.sync(&app);
        let mut texts = TextBatch::new();
        let narrow = Constraints::loose(Size::new(400.0, 900.0));
        assert_eq!(sb.layout(narrow, &mut texts).width, 0.0, "窄窗自动折叠");
        // 收起态 paint 不可写任何东西
        let mut texts = TextBatch::new();
        let mut rects = RectBatch::new();
        sb.paint(
            Rect::from_xywh(0.0, 0.0, 0.0, 900.0),
            &mut rects,
            &mut texts,
        );
        assert_eq!(texts.glyph_clips().count(), 0, "收起态零字形");
    }
}
