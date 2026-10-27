//! @author 十四叔
//! @date 2026/09/28
//!
//! Warn toast 浮层 (SPEC-notice-visibility 腿 A): 「你按的那下没生效」类提示从
//! 底栏一行字升级为视野内浮层。
//!
//! **非模态**: 无 scrim / 不抢焦点 / 不进 Esc 次序表与 `close_popovers` 互斥清单
//! —— 不是模态弹层族成员。唯一消费的事件 = 落在自身矩形内的左键按下
//! (点掉, `Msg::DismissNotice`); 其余一律 `Ignored` 放行。
//!
//! 挂 `Stack` 末位: 框架事件反序分发 + 后画 = 最上层 (stack.rs `rev()`),
//! 故模态弹层开着时 toast 仍可见可点 —— 合并源卡里输错偏移的 Warn 正发生在
//! 弹层开着时, 被盖住 = 最该看到的场景反而看不见 (spec §2 腿 A)。
//!
//! 生命周期零新机制: 现身/消退完全由 `app.notice` 驱动 (单槽 + 4 秒 + tick 到点
//! 清, T18 既有通路), 本组件只是 Warn 的**呈现位** (分派在 view 层, spec D4)。

use std::any::Any;
use std::cell::Cell;

use danqing::event::MouseButton;
use danqing::widget::{EventResult, MsgQueue, Widget};
use danqing::{Color, Constraints, Event, Rect, RectBatch, Size, TextBatch, Theme};

use crate::{LogApp, Msg, NoticeKind};

/// 浮层固定高 (正文 14 + 上下各 ~11)。
const TOAST_H: f32 = 36.0;
/// 横向内边距 (= `spacing_md` 同值, 不引新 token)。
const PAD_X: f32 = 12.0;
/// 左侧语义色条宽。
const BAR_W: f32 = 4.0;
/// 色条与文案间距 (= `spacing_sm` 同值)。
const BAR_GAP: f32 = 8.0;
/// 底缘与状态栏顶的间距 (= `spacing_sm` 同值)。
const BOTTOM_GAP: f32 = 8.0;
/// 圆角。
const RADIUS: f32 = 8.0;
/// 色条垂直内缩 (上下各收, 不顶圆角)。
const BAR_INSET: f32 = 8.0;
/// 宽度硬顶 (spec D8): 另受窗口宽 ×0.6 约束, 取小。
const MAX_W: f32 = 520.0;

/// D8 上限单点: `min(窗口宽×0.6, MAX_W)` —— paint 的文案截断预算与 toast_rect
/// 的矩形钳制是同一条线, 收口防两处各算各的 (测试期望值也走它)。
fn width_cap(area_w: f32) -> f32 {
    (area_w * 0.6).min(MAX_W)
}

/// toast 矩形几何 (底部水平居中, 底缘贴状态栏顶; 宽随文案, 上限 D8)。
/// 独立纯函数供单测; paint 与命中同源 = 都走它 (hit 同源先例: SubmitInput)。
fn toast_rect(area: Rect, content_w: f32) -> Rect {
    let cap = width_cap(area.size.width);
    let w = content_w.min(cap);
    let x = area.origin.x + (area.size.width - w) / 2.0;
    let y = area.origin.y + area.size.height - crate::view::STATUS_HEIGHT - BOTTOM_GAP - TOAST_H;
    Rect::from_xywh(x, y, w, TOAST_H)
}

/// Warn toast 浮层 (模块头有全貌)。`Sync` 每帧从 app 取 notice 与主题色
/// (bind 取值先例 —— 建树冻结铁律: 构造值会烘死)。
pub(crate) struct Toast {
    /// 当前 Warn 文案 (sync 注入)。
    text: String,
    /// notice 是 Warn 才现身 (Info/None 零绘制零拦截)。
    visible: bool,
    bg: Color,
    bar: Color,
    fg: Color,
    /// 命中同源缓存 (paint 写, event 读)。
    hit_rect: Cell<Rect>,
}

impl Toast {
    pub(crate) fn new() -> Self {
        Self {
            text: String::new(),
            visible: false,
            bg: Color::TRANSPARENT,
            bar: Color::TRANSPARENT,
            fg: Color::TRANSPARENT,
            hit_rect: Cell::new(Rect::default()),
        }
    }
}

impl Widget for Toast {
    fn sync(&mut self, state: &dyn Any) {
        let Some(app) = state.downcast_ref::<LogApp>() else {
            return;
        };
        let t = app.theme.theme();
        self.bg = t.background();
        self.bar = t.danger();
        self.fg = t.text_primary();
        match &app.notice {
            Some((text, NoticeKind::Warn)) => {
                self.text = text.clone();
                self.visible = true;
            }
            // Info 留底栏 (spec D1 分级), toast 不现身
            _ => {
                self.visible = false;
                self.text.clear();
            }
        }
    }

    fn layout(&mut self, constraints: Constraints, _texts: &mut TextBatch) -> Size {
        // 覆盖层: 占满父约束 (Stack 给全窗口); 自绘几何在 paint 里算。
        Size::new(constraints.max_width, constraints.max_height)
    }

    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        if !self.visible {
            self.hit_rect.set(Rect::default());
            return;
        }
        let px = crate::view::FONT_SIZE;
        let cap = width_cap(area.size.width);
        // 截断走框架 canonical helper (`fit::ellipsize_tail` 内部已为「…」腾位,
        // 总宽 ≤ cap 是其实现保证; 「…」= U+2026 在 Sarasa 子集内, 现网文案实证)。
        let shown =
            danqing::fit::ellipsize_tail(&self.text, cap - BAR_W - BAR_GAP - PAD_X * 2.0, |t| {
                texts.measure(t, px)
            });
        let content_w = texts.measure(&shown, px) + BAR_W + BAR_GAP + PAD_X * 2.0;
        let r = toast_rect(area, content_w);
        self.hit_rect.set(r);
        // 不透明底 (card_shell 同款论据: surface 系半透明 token 不能当浮层底) + 圆角。
        rects.push_rect(r, self.bg, RADIUS);
        // 左色条 (Warn 语义色, 圆角内缩不顶角)。
        rects.push_rect(
            Rect::from_xywh(
                r.origin.x + BAR_GAP,
                r.origin.y + BAR_INSET,
                BAR_W,
                TOAST_H - BAR_INSET * 2.0,
            ),
            self.bar,
            2.0,
        );
        // 文案 (行内垂直居中, view.rs 状态栏同款算法)。
        let line_h = texts.line_height(f32::from(px));
        let baseline = r.origin.y + (TOAST_H - line_h) / 2.0 + texts.ascent(f32::from(px));
        texts.push_text(
            &shown,
            r.origin.x + PAD_X + BAR_W + BAR_GAP,
            baseline,
            px,
            self.fg,
        );
    }

    fn event(&mut self, event: &Event, _area: Rect, msgs: &mut MsgQueue) -> EventResult {
        // 非模态铁律: 不现身时零拦截; 现身时也只拦自身矩形内的左键按下。
        if !self.visible {
            return EventResult::Ignored;
        }
        if let Event::MouseInput {
            button: MouseButton::Left,
            pressed: true,
            position,
        } = event
        {
            if self.hit_rect.get().contains(*position) {
                msgs.push(Box::new(Msg::DismissNotice));
                return EventResult::Consumed;
            }
        }
        EventResult::Ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use danqing::Point;
    use danqing::{App, srgb_to_linear};

    /// 注入一条 Warn notice (走消息链正路, 不碰私有捷径)。
    fn app_with_notice(text: &str, kind: NoticeKind) -> LogApp {
        let mut app = LogApp::new_empty();
        app.update(Msg::Notice(text.to_string(), kind));
        app
    }

    fn synced_toast(app: &LogApp) -> Toast {
        let mut t = Toast::new();
        t.sync(app);
        t
    }

    fn paint_toast(t: &Toast, area: Rect) -> (RectBatch, TextBatch) {
        let mut rects = RectBatch::new();
        let mut texts = TextBatch::default();
        t.paint(area, &mut rects, &mut texts);
        (rects, texts)
    }

    /// 实例色 = linear 数组 (RectBatch 口径); Color → 同形比对 (settings.rs:2018 先例:
    /// 比较前两空间对齐, 否则比的是两个色彩空间)。
    fn paints_color(rects: &RectBatch, c: Color) -> bool {
        let want = [
            srgb_to_linear(c.r),
            srgb_to_linear(c.g),
            srgb_to_linear(c.b),
            c.a,
        ];
        rects.instance_colors().iter().any(|got| {
            got.iter()
                .zip(want.iter())
                .all(|(x, y)| (x - y).abs() < 0.001)
        })
    }

    const AREA: Rect = Rect::from_xywh(0.0, 0.0, 1280.0, 800.0);

    /// Warn → 画底 + 色条 + 文案三样; 底色不透明且 = 主题 background()。
    #[test]
    fn warn_notice_paints_body_bar_and_text() {
        let app = app_with_notice("先点选源再调", NoticeKind::Warn);
        let t = synced_toast(&app);
        let (rects, texts) = paint_toast(&t, AREA);
        let rs = rects.instance_rects();
        assert!(rs.len() >= 2, "Warn 至少画底+色条两矩形, 实得 {}", rs.len());
        let bg = app.theme.theme().background();
        assert!(
            paints_color(&rects, bg) && bg.a == 1.0,
            "底色必须是主题 background() 且不透明 (半透明 token 当浮层底 = 穿透陷阱)"
        );
        assert!(
            paints_color(&rects, app.theme.theme().danger()),
            "左色条用 danger()"
        );
        assert!(
            texts.glyph_clips().count() > 0,
            "文案须真画出字形 (直方图消失教训: 没画过的组件测试看不见)"
        );

        // 暗主题同款断言 (评审 Optional: spec §5 写的是「双主题」底色不透明) ——
        // 两主题 background() 都是 from_srgb8 构造 (不透明是构造保证), 这里补
        // 暗主题一行对齐 spec, 防有人把底色换成半透明 token。
        let mut dark = app_with_notice("先点选源再调", NoticeKind::Warn);
        dark.theme = crate::config::AppTheme::Dark;
        let td = synced_toast(&dark);
        let (rects_d, _) = paint_toast(&td, AREA);
        let bg_d = dark.theme.theme().background();
        assert!(
            paints_color(&rects_d, bg_d) && bg_d.a == 1.0,
            "暗主题底色同样是 background() 且不透明"
        );
    }

    /// Info / None → 零绘制 (分级分派: Info 留底栏, spec D1)。
    #[test]
    fn info_or_none_notice_paints_nothing() {
        let app = app_with_notice("已复制 3 行", NoticeKind::Info);
        let t = synced_toast(&app);
        let (rects, texts) = paint_toast(&t, AREA);
        assert_eq!(rects.instance_rects().len(), 0, "Info 时 toast 零矩形");
        assert_eq!(texts.glyph_clips().count(), 0, "Info 时 toast 零文本");

        let app = LogApp::new_empty();
        let t = synced_toast(&app);
        let (rects, texts) = paint_toast(&t, AREA);
        assert_eq!(rects.instance_rects().len(), 0, "无 notice 时零矩形");
        assert_eq!(
            texts.glyph_clips().count(),
            0,
            "无 notice 时零文本 (与 Info 分支对称)"
        );
    }

    /// 点 toast 矩形内 → DismissNotice + Consumed; 矩形外 → Ignored 放行 (非模态)。
    #[test]
    fn click_inside_dismisses_outside_passes_through() {
        let app = app_with_notice("偏移须是毫秒整数 (如 -3000)", NoticeKind::Warn);
        let mut t = synced_toast(&app);
        let mut texts = TextBatch::default();
        let mut rects = RectBatch::new();
        t.paint(AREA, &mut rects, &mut texts);
        let r = t.hit_rect.get();
        assert!(r.size.width > 0.0, "paint 须缓存命中矩形");

        let mut msgs = MsgQueue::new();
        let inside = Event::MouseInput {
            button: MouseButton::Left,
            pressed: true,
            position: Point::new(
                r.origin.x + r.size.width / 2.0,
                r.origin.y + r.size.height / 2.0,
            ),
        };
        assert_eq!(t.event(&inside, AREA, &mut msgs), EventResult::Consumed);
        assert!(
            msgs.iter()
                .any(|m| matches!(m.downcast_ref::<Msg>(), Some(Msg::DismissNotice))),
            "矩形内点击须出 DismissNotice"
        );

        let mut msgs = MsgQueue::new();
        let outside = Event::MouseInput {
            button: MouseButton::Left,
            pressed: true,
            position: Point::new(4.0, 4.0),
        };
        assert_eq!(t.event(&outside, AREA, &mut msgs), EventResult::Ignored);
        assert_eq!(msgs.len(), 0, "矩形外零消息 (非模态放行)");
    }

    /// 不现身时事件全放行 (Overlay 关态零拦截同款纪律)。
    /// **加固版** (评审 Optional): 先 Warn paint 出真矩形, 再 sync 成 Info
    /// (visible=false 但 `hit_rect` 是**陈旧真矩形**) —— 点旧矩形中心必须
    /// Ignored 零消息: notice 在两帧之间消退/降级时, 旧位置的点击不得被吞。
    /// (原版从未 paint, 零矩形 contains 恒 false, 摘掉 visible 硬闸也绿 = 假绿。)
    #[test]
    fn hidden_toast_intercepts_nothing() {
        let mut app = app_with_notice("先点选源再调", NoticeKind::Warn);
        let mut t = synced_toast(&app);
        let mut rects = RectBatch::new();
        let mut texts = TextBatch::default();
        t.paint(AREA, &mut rects, &mut texts);
        let stale = t.hit_rect.get();
        assert!(stale.size.width > 0.0, "先造出真命中矩形");

        // 降级成 Info (notice 还在但 toast 不现身) —— 不跑 paint, hit_rect 保持陈旧
        app.update(Msg::Notice("已复制".into(), NoticeKind::Info));
        t.sync(&app);
        let mut msgs = MsgQueue::new();
        let click_stale = Event::MouseInput {
            button: MouseButton::Left,
            pressed: true,
            position: Point::new(
                stale.origin.x + stale.size.width / 2.0,
                stale.origin.y + stale.size.height / 2.0,
            ),
        };
        assert_eq!(
            t.event(&click_stale, AREA, &mut msgs),
            EventResult::Ignored,
            "隐身后陈旧矩形位置的点击不得被吞 (visible 硬闸先于 hit_rect 读)"
        );
        assert_eq!(msgs.len(), 0, "零消息");
    }

    /// SPEC-notice-visibility §5 (弹层之上): 模态弹层开着时 toast 的 sync 不被
    /// 污染 —— 它只读 `app.notice`, 与 `merge_menu_open` 等弹层态零耦合。
    /// (「点击仍先到 toast」的那一半在 main.rs 整树行为锁。)
    #[test]
    fn warn_paints_even_when_modal_popover_open() {
        let mut app = app_with_notice("偏移须是毫秒整数 (如 -3000)", NoticeKind::Warn);
        app.merge_menu_open = true; // 模态弹层开态 (直写字段构态, 不触门控)
        let t = synced_toast(&app);
        let (rects, texts) = paint_toast(&t, AREA);
        assert!(
            !rects.instance_rects().is_empty() && texts.glyph_clips().count() > 0,
            "弹层开着时 Warn toast 仍须画 (底+条+文)"
        );
    }

    /// 消退端到端: Warn 到点 expire 后 toast 随之不画 —— 生命周期零新机制
    /// (4 秒通路是 T18 既有的, toast 只是呈现位; deadline 直拨手法同 T18 锁)。
    #[test]
    fn toast_disappears_when_notice_expires() {
        let mut app = app_with_notice("x", NoticeKind::Warn);
        app.notice_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
        app.expire_notice();
        let t = synced_toast(&app);
        let (rects, _) = paint_toast(&t, AREA);
        assert_eq!(rects.instance_rects().len(), 0, "到点消退后 toast 零绘制");
    }

    /// D8 宽度上限: 超长文案矩形宽 ≤ min(窗口×0.6, 520) 且文案带「…」。
    #[test]
    fn long_text_is_clamped_to_cap() {
        let long = "很".repeat(200);
        let app = app_with_notice(&long, NoticeKind::Warn);
        let t = synced_toast(&app);
        let mut rects = RectBatch::new();
        let mut texts = TextBatch::default();
        t.paint(AREA, &mut rects, &mut texts);
        let r = t.hit_rect.get();
        let cap = width_cap(AREA.size.width);
        assert!(
            r.size.width <= cap + 0.5,
            "矩形宽 {} 超上限 {cap}",
            r.size.width
        );
        assert!(
            !t.text.is_empty() && r.size.width > 100.0,
            "截断不是清空, 浮层仍有内容"
        );
        // 窄窗口: 0.6×窗口 < MAX_W 时按窗口走
        let narrow = Rect::from_xywh(0.0, 0.0, 400.0, 800.0);
        t.paint(narrow, &mut rects, &mut texts);
        let rn = t.hit_rect.get();
        assert!(
            rn.size.width <= width_cap(400.0) + 0.5,
            "窄窗矩形宽 {} 超 {}",
            rn.size.width,
            width_cap(400.0)
        );
    }

    /// 几何锁: 底缘贴状态栏顶 (STATUS_HEIGHT 同源, 不抄数值); 水平居中。
    #[test]
    fn toast_sits_on_top_of_status_bar_centered() {
        let app = app_with_notice("会话不存在", NoticeKind::Warn);
        let t = synced_toast(&app);
        let mut rects = RectBatch::new();
        let mut texts = TextBatch::default();
        t.paint(AREA, &mut rects, &mut texts);
        let r = t.hit_rect.get();
        let expect_bottom =
            AREA.origin.y + AREA.size.height - crate::view::STATUS_HEIGHT - BOTTOM_GAP;
        assert!(
            (r.origin.y + r.size.height - expect_bottom).abs() < 0.01,
            "底缘 {} 须贴状态栏顶 {}",
            r.origin.y + r.size.height,
            expect_bottom
        );
        let center_x = r.origin.x + r.size.width / 2.0;
        assert!(
            (center_x - AREA.size.width / 2.0).abs() < 0.5,
            "水平居中: 中心 {center_x} ≠ 窗中 {}",
            AREA.size.width / 2.0
        );
    }
}
