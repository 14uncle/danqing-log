//! @author 十四叔
//! @date 2026/09/06
//!
//! 轻量设置卡：danqing::Overlay 承载 scrim/居中/模态门控 + 玻璃卡 (关于/版本/反馈)。
//! 关闭：✕ 按钮 / Esc (app 级两阶段) / 点遮罩。

use std::any::Any;

use danqing::widget::{
    Box as UiBox, Center, CloseButton, Column, Dropdown, EventResult, MsgQueue, Overlay, Padding,
    Row, Text, Widget,
};
use danqing::{
    Color, Constraints, Edges, Event, Key, NamedKey, Point, Rect, RectBatch, Size, TextBatch, Theme,
};

use crate::LogApp;
use crate::Msg;
use crate::config::{self, AppTheme};

/// 卡片宽度。
const CARD_WIDTH: f32 = 360.0;
/// 正文字号。
const BODY_SIZE: u16 = 14;

/// 设置卡浮层：danqing::Overlay 承载 scrim/居中/模态门控 (簇 C 下沉)。
pub(crate) fn settings_overlay(theme: config::AppTheme) -> impl Widget {
    let t = theme.theme();
    Overlay::themed(&t, Center::new(settings_card(theme)).fill_max())
        .bind_open(|app: &LogApp| app.settings_open)
        .on_scrim_click(|| Msg::CloseSettings)
}

/// 玻璃卡片：关闭行 + 关于 + 版本行 + 主题切换 + 反馈链接。
fn settings_card(theme: config::AppTheme) -> impl Widget {
    let t = theme.theme();
    let pad = Edges {
        top: 24.0,
        right: 24.0,
        bottom: 16.0,
        left: 24.0,
    };
    // 内容区宽度 = 卡片宽 - 左右 padding
    let content_w = CARD_WIDTH - pad.left - pad.right;
    UiBox::new(t.surface())
        .radius(12.0)
        .border_color(t.border())
        .child(Padding::new(
            pad,
            Column::new()
                .gap(16.0)
                .cross_center()
                .child(close_row())
                .child(about_section())
                .child(content_row(version_row(), content_w))
                // 主题行不套 content_row (那是「占满内容宽 + 内部左对齐」):
                // 让 「主题 + 下拉」按内容整体收缩, 交给 Column 的 cross_center 居中,
                // 与关于区三行/反馈链接同处一条中轴。
                .child(theme_dropdown())
                .child(feedback_row()),
        ))
        .width(CARD_WIDTH)
}

/// 内容行：固定宽度居中，内部左对齐。
fn content_row(inner: impl Widget + 'static, width: f32) -> impl Widget {
    Center::new(UiBox::new(Color::TRANSPARENT).width(width).child(inner))
}

/// 关闭行：右对齐 ✕。
fn close_row() -> impl Widget {
    Row::new()
        .cross_center()
        .fill(UiBox::new(Color::TRANSPARENT), 1)
        .child(
            CloseButton::new()
                .on_click(|| Msg::CloseSettings)
                .bind_color(|app: &LogApp| app.theme.theme().text_primary())
                .bind_hover_color(|app: &LogApp| app.theme.theme().surface_variant()),
        )
}

/// 关于区：产品名 + 版本号 + 设计一句话。
fn about_section() -> impl Widget {
    Column::new()
        .gap(6.0)
        .cross_stretch()
        .child(Center::new(
            Text::new("丹青日志 LogLens".to_string())
                .font_size(18)
                .bind_color(|app: &LogApp| app.theme.theme().accent()),
        ))
        .child(Center::new(
            Text::bind(|_app: &LogApp| format!("v{}", env!("CARGO_PKG_VERSION")))
                .font_size(BODY_SIZE)
                .bind_color(|app: &LogApp| app.theme.theme().text_secondary()),
        ))
        .child(Center::new(
            Text::new("大文件日志/JSONL 查看分析器".to_string())
                .font_size(BODY_SIZE)
                .bind_color(|app: &LogApp| app.theme.theme().text_secondary()),
        ))
}

/// 版本行：更新提示 (有新版时显示「有新版本 vX.Y.Z」+ 前往下载按钮)。
fn version_row() -> impl Widget {
    VersionRow::new()
}

/// 反馈链接行。
fn feedback_row() -> impl Widget {
    Center::new(Link::new(
        "问题反馈",
        "https://github.com/14uncle/danqing-log/issues",
    ))
}

/// 主题切换下拉选择器。
///
/// 自足组件：展开、弹层渲染、键盘导航、点外收起全部由 `Dropdown` 自管
/// (danqing fb4939f 起弹层走框架弹层通道), 应用侧只留一个选中回调。
/// 迁移前这里是「控件 + 一个 45 行的 Overlay 装配函数 + 3 个 Msg + 2 个状态
/// 字段」, 与 danqing showcase 同一份样板。
fn theme_dropdown() -> impl Widget {
    Row::new()
        .gap(8.0)
        .cross_center()
        .child(
            Text::new("主题".to_string())
                .font_size(BODY_SIZE)
                .bind_color(|app: &LogApp| app.theme.theme().text_secondary()),
        )
        .child(
            Dropdown::new(AppTheme::options())
                .width(120.0)
                .bind_selected(|app: &LogApp| app.theme.index())
                .on_select(Msg::SelectTheme),
        )
}

/// 版本行：有新版时显示提示 + 按钮; 无新版时空白。
struct VersionRow {
    hint_status: String,
    hint_action: &'static str,
    has_hint: bool,
    btn_hover: bool,
    /// Cell 跨 paint/event 共享：paint 测量后写入，event 命中检测读取;
    /// 依赖 paint 在 event 之前调用 (danqing 保证此顺序)。
    btn_area: std::cell::Cell<Rect>,
    text_secondary: Color,
    text_primary: Color,
}

impl VersionRow {
    fn new() -> Self {
        Self {
            hint_status: String::new(),
            hint_action: "",
            has_hint: false,
            btn_hover: false,
            btn_area: std::cell::Cell::new(Rect::default()),
            text_secondary: Color::rgb(0.40, 0.40, 0.42),
            text_primary: Color::rgb(0.12, 0.12, 0.12),
        }
    }
}

impl Widget for VersionRow {
    fn sync(&mut self, state: &dyn Any) {
        if let Some(app) = state.downcast_ref::<LogApp>() {
            let t = app.theme.theme();
            self.text_secondary = t.text_secondary();
            self.text_primary = t.text_primary();
        }
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
            self.text_secondary,
        );
        // 按钮
        let status_w = texts.measure(&self.hint_status, BODY_SIZE);
        let btn_x = area.origin.x + status_w + 12.0;
        let btn_w = texts.measure(self.hint_action, BODY_SIZE) + 16.0;
        let btn_color = if self.btn_hover {
            self.text_primary
        } else {
            self.text_secondary
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

/// 可点击链接行：整行宽幽灵按钮 —— 常显下划线 (裸小字链接发现性太差),
/// hover 整行底色反馈，命中区整行 32px。
struct Link {
    text: String,
    url: String,
    hovered: bool,
    area: Rect,
    hover_bg: Color,
    accent: Color,
}

impl Link {
    fn new(text: &str, url: &str) -> Self {
        Self {
            text: text.to_string(),
            url: url.to_string(),
            hovered: false,
            area: Rect::default(),
            hover_bg: Color::TRANSPARENT,
            accent: Color::rgb(0.18, 0.35, 0.60),
        }
    }
}

/// 链接行高。
const LINK_ROW_H: f32 = 32.0;

impl Widget for Link {
    fn sync(&mut self, state: &dyn Any) {
        if let Some(app) = state.downcast_ref::<LogApp>() {
            let t = app.theme.theme();
            self.hover_bg = t.surface_variant();
            self.accent = t.accent();
        }
    }
    fn layout(&mut self, constraints: Constraints, _texts: &mut TextBatch) -> Size {
        let size = constraints.constrain(Size::new(constraints.max().width, LINK_ROW_H));
        self.area = Rect::new(Point::ZERO, size);
        size
    }
    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        if self.hovered {
            rects.push_rect(area, self.hover_bg, 6.0);
        }
        // 文本整行居中，下划线随行
        let text_w = texts.measure(&self.text, BODY_SIZE);
        let text_x = area.origin.x + (area.size.width - text_w) / 2.0;
        let baseline = area.origin.y
            + (LINK_ROW_H - texts.line_height(f32::from(BODY_SIZE))) / 2.0
            + texts.ascent(f32::from(BODY_SIZE));
        texts.push_text(&self.text, text_x, baseline, BODY_SIZE, self.accent);
        // 常显下划线：链接身份不依赖 hover 才发现
        let underline_y = baseline + texts.descent(f32::from(BODY_SIZE)) + 1.0;
        rects.push_rect(
            Rect::from_xywh(text_x, underline_y, text_w, 1.0),
            self.accent,
            0.0,
        );
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
