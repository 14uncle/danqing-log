//! @author 十四叔
//! @date 2026/09/06
//!
//! 轻量设置卡: scrim 遮罩 + 居中玻璃卡 + 关于/版本/反馈。
//! 关闭: ✕ 按钮 / Esc / 点遮罩。

use std::any::Any;

use danqing::widget::{
    Box as UiBox, Center, CloseButton, Column, EventResult, MsgQueue, Padding, Row, Stack, Text,
    Widget,
};
use danqing::{
    Color, Constraints, Edges, Event, Key, NamedKey, Point, Rect, RectBatch, Size, TextBatch,
};

use crate::LogApp;
use crate::Msg;

/// 卡片宽度。
const CARD_WIDTH: f32 = 360.0;
/// 正文字号。
const BODY_SIZE: u16 = 14;

fn text_primary() -> Color {
    Color::rgb(0.12, 0.12, 0.12)
}
fn text_secondary() -> Color {
    Color::rgb(0.40, 0.40, 0.42)
}
fn accent() -> Color {
    Color::rgb(0.18, 0.35, 0.60)
}
fn card_bg() -> Color {
    Color::rgba(0.0, 0.0, 0.0, 0.04)
}
fn scrim() -> Color {
    Color::rgba(0.0, 0.0, 0.0, 0.25)
}
fn hover_bg() -> Color {
    Color::rgba(0.0, 0.0, 0.0, 0.06)
}

/// 设置卡浮层: 全窗 scrim + 居中玻璃卡。
/// 内含 open 态, sync 从 LogApp.settings_open 读取;
/// 关闭时零高零宽, 不拦截事件。
pub(crate) fn settings_overlay() -> SettingsOverlay {
    SettingsOverlay::new()
}

/// 设置卡浮层组件: 检查 settings_open 态, 关闭时不可见不可交互。
pub(crate) struct SettingsOverlay {
    open: bool,
    /// 内部子树: scrim + 卡片。
    inner: Box<dyn Widget>,
}

impl SettingsOverlay {
    fn new() -> Self {
        Self {
            open: false,
            inner: Box::new(
                Stack::new()
                    .child(Scrim::new())
                    .child(Center::new(settings_card()).fill_max()),
            ),
        }
    }
}

impl Widget for SettingsOverlay {
    fn sync(&mut self, state: &dyn Any) {
        let app = state
            .downcast_ref::<LogApp>()
            .expect("SettingsOverlay 绑定状态类型不匹配");
        self.open = app.settings_open;
        if self.open {
            self.inner.sync(state);
        }
    }

    fn animate(&mut self, ctx: &danqing::AnimationCtx) {
        if self.open {
            self.inner.animate(ctx);
        }
    }

    fn layout(&mut self, constraints: Constraints, texts: &mut TextBatch) -> Size {
        if self.open {
            self.inner.layout(constraints, texts)
        } else {
            // 关闭时占零空间, 不影响底层布局。
            Size::new(constraints.max().width, 0.0)
        }
    }

    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        if self.open {
            self.inner.paint(area, rects, texts);
        }
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        if self.open {
            self.inner.event(event, area, msgs)
        } else {
            EventResult::Ignored
        }
    }

    fn children(&self) -> &[danqing::widget::Node] {
        if self.open {
            self.inner.children()
        } else {
            &[]
        }
    }

    fn children_mut(&mut self) -> &mut [danqing::widget::Node] {
        if self.open {
            self.inner.children_mut()
        } else {
            &mut []
        }
    }

    fn focusable(&self) -> bool {
        self.open
    }
}

/// 玻璃卡片: 关闭行 + 关于 + 版本行 + 反馈链接。
fn settings_card() -> impl Widget {
    let pad = Edges {
        top: 24.0,
        right: 24.0,
        bottom: 16.0,
        left: 24.0,
    };
    UiBox::new(card_bg())
        .radius(12.0)
        .child(Padding::new(
            pad,
            Column::new()
                .gap(12.0)
                .cross_stretch()
                .child(close_row())
                .child(about_section())
                .child(version_row())
                .child(feedback_row()),
        ))
        .width(CARD_WIDTH)
}

/// 关闭行: 右对齐 ✕。
fn close_row() -> impl Widget {
    Row::new()
        .cross_stretch()
        .fill(UiBox::new(Color::TRANSPARENT).height(1.0), 1)
        .child(
            CloseButton::new()
                .on_click(|| Msg::CloseSettings)
                .bind_color(|_: &LogApp| text_primary())
                .bind_hover_color(|_: &LogApp| hover_bg()),
        )
}

/// 关于区: 产品名 + 版本号 + 设计一句话。
fn about_section() -> impl Widget {
    Column::new()
        .gap(6.0)
        .cross_stretch()
        .child(Center::new(
            Text::new("丹青日志 LogLens".to_string())
                .font_size(18)
                .bind_color(|_: &LogApp| accent()),
        ))
        .child(Center::new(
            Text::bind(|_app: &LogApp| format!("v{}", env!("CARGO_PKG_VERSION")))
                .font_size(BODY_SIZE)
                .bind_color(|_: &LogApp| text_secondary()),
        ))
        .child(Center::new(
            Text::new("大文件日志/JSONL 查看分析器".to_string())
                .font_size(BODY_SIZE)
                .bind_color(|_: &LogApp| text_secondary()),
        ))
}

/// 版本行: 更新提示 (有新版时显示「有新版本 vX.Y.Z」+ 前往下载按钮)。
fn version_row() -> impl Widget {
    VersionRow::new()
}

/// 反馈链接行。
fn feedback_row() -> impl Widget {
    Link::new("问题反馈", "https://github.com/14uncle/danqing-log/issues")
}

/// Scrim 遮罩: 点击关闭设置卡。
struct Scrim {
    area: Rect,
}

impl Scrim {
    fn new() -> Self {
        Self {
            area: Rect::default(),
        }
    }
}

impl Widget for Scrim {
    fn sync(&mut self, _state: &dyn Any) {}
    fn layout(&mut self, constraints: Constraints, _texts: &mut TextBatch) -> Size {
        constraints.max()
    }
    fn paint(&self, area: Rect, rects: &mut RectBatch, _texts: &mut TextBatch) {
        rects.push_rect(area, scrim(), 0.0);
    }
    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        self.area = area;
        match event {
            Event::MouseInput {
                pressed: true,
                position,
                ..
            } if area.contains(*position) => {
                msgs.push(Box::new(Msg::CloseSettings));
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }
    fn hit_area(&self) -> Option<Rect> {
        Some(self.area)
    }
}

/// 版本行: 有新版时显示提示 + 按钮; 无新版时空白。
struct VersionRow {
    hint_status: String,
    hint_action: &'static str,
    has_hint: bool,
    btn_hover: bool,
    /// Cell 跨 paint/event 共享: paint 测量后写入, event 命中检测读取;
    /// 依赖 paint 在 event 之前调用 (danqing 保证此顺序)。
    btn_area: std::cell::Cell<Rect>,
}

impl VersionRow {
    fn new() -> Self {
        Self {
            hint_status: String::new(),
            hint_action: "",
            has_hint: false,
            btn_hover: false,
            btn_area: std::cell::Cell::new(Rect::default()),
        }
    }
}

impl Widget for VersionRow {
    fn sync(&mut self, _state: &dyn Any) {
        if let Some(hint) = crate::app_update::hint() {
            self.hint_status = hint.status;
            self.hint_action = hint.action;
            self.has_hint = true;
        } else {
            self.has_hint = false;
        }
    }

    fn layout(&mut self, constraints: Constraints, _texts: &mut TextBatch) -> Size {
        if self.has_hint {
            Size::new(constraints.max().width, 32.0)
        } else {
            Size::new(constraints.max().width, 0.0)
        }
    }

    fn paint(&self, area: Rect, _rects: &mut RectBatch, texts: &mut TextBatch) {
        if !self.has_hint {
            return;
        }
        let baseline = area.origin.y
            + (32.0 - texts.line_height(f32::from(BODY_SIZE))) / 2.0
            + texts.ascent(f32::from(BODY_SIZE));
        // 状态文案
        texts.push_text(
            &self.hint_status,
            area.origin.x,
            baseline,
            BODY_SIZE,
            text_secondary(),
        );
        // 按钮
        let status_w = texts.measure(&self.hint_status, BODY_SIZE);
        let btn_x = area.origin.x + status_w + 12.0;
        let btn_w = texts.measure(self.hint_action, BODY_SIZE) + 16.0;
        let btn_color = if self.btn_hover {
            text_primary()
        } else {
            accent()
        };
        texts.push_text(
            self.hint_action,
            btn_x + 8.0,
            baseline,
            BODY_SIZE,
            btn_color,
        );
        self.btn_area
            .set(Rect::from_xywh(btn_x, area.origin.y, btn_w, 32.0));
    }

    fn event(&mut self, event: &Event, area: Rect, _msgs: &mut MsgQueue) -> EventResult {
        if !self.has_hint {
            return EventResult::Ignored;
        }
        let btn = self.btn_area.get();
        match event {
            Event::CursorMoved(p) => {
                let abs_btn = Rect::from_xywh(
                    btn.origin.x + area.origin.x,
                    btn.origin.y + area.origin.y,
                    btn.size.width,
                    btn.size.height,
                );
                self.btn_hover = abs_btn.contains(*p);
                EventResult::Ignored
            }
            Event::CursorLeft => {
                self.btn_hover = false;
                EventResult::Ignored
            }
            Event::MouseInput {
                pressed: true,
                position,
                ..
            } => {
                let abs_btn = Rect::from_xywh(
                    btn.origin.x + area.origin.x,
                    btn.origin.y + area.origin.y,
                    btn.size.width,
                    btn.size.height,
                );
                if abs_btn.contains(*position) {
                    crate::app_update::go_download();
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            _ => EventResult::Ignored,
        }
    }
}

/// 可点击链接: accent 色 + hover 下划线 + 点击开浏览器。
struct Link {
    text: String,
    url: String,
    hovered: bool,
    area: Rect,
}

impl Link {
    fn new(text: &str, url: &str) -> Self {
        Self {
            text: text.to_string(),
            url: url.to_string(),
            hovered: false,
            area: Rect::default(),
        }
    }
}

impl Widget for Link {
    fn sync(&mut self, _state: &dyn Any) {}
    fn layout(&mut self, constraints: Constraints, texts: &mut TextBatch) -> Size {
        let w = texts.measure(&self.text, BODY_SIZE);
        let h = texts.line_height(f32::from(BODY_SIZE));
        let size = constraints.constrain(Size::new(w, h));
        self.area = Rect::new(Point::ZERO, size);
        size
    }
    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        let baseline = area.origin.y
            + (area.size.height - texts.line_height(f32::from(BODY_SIZE))) / 2.0
            + texts.ascent(f32::from(BODY_SIZE));
        texts.push_text(&self.text, area.origin.x, baseline, BODY_SIZE, accent());
        if self.hovered {
            let underline_y = area.origin.y + area.size.height - 1.0;
            rects.push_rect(
                Rect::from_xywh(area.origin.x, underline_y, area.size.width, 1.0),
                accent(),
                0.0,
            );
        }
    }
    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        self.area = area;
        match event {
            Event::CursorMoved(p) => {
                self.hovered = area.contains(*p);
                if self.hovered {
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            Event::CursorLeft => {
                self.hovered = false;
                EventResult::Ignored
            }
            Event::MouseInput {
                pressed: true,
                position,
                ..
            } => {
                if area.contains(*position) {
                    msgs.push(Box::new(Msg::OpenUrl(self.url.clone())));
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            _ => EventResult::Ignored,
        }
    }
    fn focusable(&self) -> bool {
        true
    }
    fn hit_area(&self) -> Option<Rect> {
        Some(self.area)
    }
}

/// Esc 关闭设置卡 (在 view 层的 event 处理中捕获)。
pub(crate) fn handle_settings_key(key: &Key) -> Option<Msg> {
    if matches!(key, Key::Named(NamedKey::Escape)) {
        Some(Msg::CloseSettings)
    } else {
        None
    }
}
