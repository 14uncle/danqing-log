//! @author 十四叔
//! @date 2026/09/06
//!
//! 轻量设置卡：danqing::Overlay 承载 scrim/居中/模态门控 + **不透明**卡
//! (多页签: 常规 / 快捷键 / 关于)。
//!
//! 为什么分页签: 单列堆叠在加上「快捷键」段后会把卡片顶得很高 (矮窗口下顶到边),
//! 而分页签后每一页各自只有原来那么高 —— 也用上了框架自带 `Tabs` (自绘 tab 栏 + 指示线)。
//! 关闭：✕ 按钮 / Esc (app 级两阶段) / 点遮罩。

use std::any::Any;

use danqing::widget::{
    Box as UiBox, Center, CloseButton, Column, Dropdown, EventResult, MsgQueue, Overlay, Padding,
    Row, Tabs, Text, Widget,
};
use danqing::{
    Color, Constraints, Edges, Event, Key, NamedKey, Point, Rect, RectBatch, Size, TextBatch, Theme,
};

use crate::LogApp;
use crate::Msg;
use crate::config::{self, AppTheme};

/// 卡片宽度。
const CARD_WIDTH: f32 = 360.0;
/// 卡片左右内边距。
const CARD_PAD_X: f32 = 24.0;

/// 内容区宽度 = 卡片宽 - 左右内边距。
///
/// 抽成函数而不是在 `settings_card` 里就地算: **测试必须用同一个宽度**,
/// 换一个宽度去量页签内容等于没量。
fn content_width() -> f32 {
    CARD_WIDTH - CARD_PAD_X * 2.0
}
/// 页签**内容区**的固定高度 —— 各页签必须同高, 否则切换时卡片会跳。
///
/// 取「最高那一页 + 余量」, 数值来自**实测** (`panel_contents_fit_fixed_height`
/// 量的就是这三页): 常规 36 / 快捷键 125 / 关于 133.5 (有更新提示 165.5)。
/// 180 = 165.5 + 14.5 余量。
///
/// 曾取 216, 理由写的是「常规页 v1.x 要加授权行, 留余量免得再动常量」——
/// **那条理由是错的**: 常规页实测只有 36px, 加两行也够不着上限; 真正贴着上限的
/// 是关于页, 而关于页的内容是固定的、不会长。空留的 50px 全变成了卡片下沿的空白。
/// 高度加在内容上而不是整个 Tabs 上 —— 这样 tab 栏与面板间距是外加的,
/// 各页签的高度基准才一致。
/// **内容超过此值不会裁切, 而是溢出画到卡片外** —— 框架 `Box`/`Column` 都不裁剪
/// (`paint` 只是原样转交子组件; 全框架只有 `Scrollable`/`icon_input` 走 clip)。
/// 所以这个值**必须**盖住最高那一页; 真要加高某一页, 先看这条测试红不红。
const PANEL_CONTENT_H: f32 = 180.0;
/// 正文字号。
const BODY_SIZE: u16 = 14;

/// 设置卡浮层：danqing::Overlay 承载 scrim/居中/模态门控 (簇 C 下沉)。
pub(crate) fn settings_overlay(theme: config::AppTheme) -> impl Widget {
    let t = theme.theme();
    Overlay::themed(&t, Center::new(settings_card(theme)).fill_max())
        .bind_open(|app: &LogApp| app.settings_open)
        .on_scrim_click(|| Msg::CloseSettings)
}

/// 设置卡片：关闭行 + 页签 (常规 / 快捷键 / 关于)。
///
/// 卡面上**不再**另有关于区/版本行 —— 自 2026-09-13 起这两样各就其位在
/// 「关于」页签里, 摆在页签外会与页签内容同屏重复 (用户指出)。
fn settings_card(theme: config::AppTheme) -> impl Widget {
    let t = theme.theme();
    let pad = Edges {
        top: 24.0,
        right: CARD_PAD_X,
        bottom: 16.0,
        left: CARD_PAD_X,
    };
    let content_w = content_width();
    // 卡片底色用 `background()` 而非 `surface()`: `surface` 是 `rgba(1,1,1,0.72)`
    // (**半透明**, 框架的玻璃感), `surface_variant` 深色下也是半透明
    // —— 主题里唯一两种配色都**不透明**的就是 `background()` (清屏 fallback 色,
    // 必然是实色)。用户要求面板背景不透明, 故用它。
    //
    // **不描边** (用户 2026-09-13 定)。与底层的区分交给 scrim 就够: 卡外被压暗
    // (实测 (18,18,24)), 卡内是不透明的 `background()` ((25,25,32))。
    // 原先那圈 `border()` 在暗色下会渲染成 (131,131,135) 的亮框, 是**重复**的一道
    // (scrim 已经在做区分) 且过重。注: `UiBox` 不调 `.border_color()` 就完全不画边,
    // 所以这里是删掉而不是把 width 置 0。
    // 底色走**绑定**而非构造值: `view()` 只在启动时求值一次, 构造态的颜色
    // 不会跟着主题切换走 (与标题栏同款的坑)。浅色启动、切到暗色 → 暗色 scrim 上
    // 浮着一张浅色卡。回归锁: `settings_card_background_follows_theme_switch`。
    UiBox::new(t.background())
        .bind_color(|app: &LogApp| app.theme.theme().background())
        .radius(12.0)
        .child(Padding::new(
            pad,
            Column::new()
                .gap(16.0)
                .cross_center()
                .child(close_row())
                .child(
                    Tabs::new(&t)
                        // ⚠ 页签顺序的**唯一真身** —— `LogApp::settings_tab` 存的就是
                        // 这里的下标 (0 = 常规 / 1 = 快捷键 / 2 = 关于)。
                        // main.rs 那两处注释一律指向本处, 别在那边再列一份序号:
                        // 2026-09-13 加「常规」时就因为两处各写了一份而漂过一次。
                        // 顺序理由: 常规在前 (设置卡首屏 = 设置), 关于居末 (产品线惯例)。
                        .tab("常规")
                        .tab("快捷键")
                        .tab("关于")
                        .child(panel_box(general_content()))
                        .child(panel_box(shortcuts_section(content_w)))
                        .child(panel_box(about_content(content_w)))
                        // 页签选择留在应用状态里: 重开卡片停在上次那页 (比每次弹回
                        // 第一页更省事), 且 Esc/点遮罩关闭不丢。
                        .bind(|app: &LogApp| app.settings_tab)
                        // 页签**颜色**必须每帧重取 —— `Tabs::new(&t)` 是构造值,
                        // 不挂这个绑定的话切主题时页签名会停在旧主题色
                        // (浅色启动切暗色 → 深灰字压暗底, 读不了)。
                        // 回归锁: `settings_card_tabs_follow_theme_switch`。
                        .bind_theme(|app: &LogApp| app.theme.theme())
                        .on_change(Msg::SelectSettingsTab),
                ),
        ))
        .width(CARD_WIDTH)
}

/// 「常规」页签: 可配置项的家 (目前只有主题)。
///
/// 单列一行的确是空 —— v1 也确实只有这一个开关。留在原处(`关于`页)才是错的:
/// 那儿是**只读**的产品身份页, 把可点击的开关混进去, 用户没法一眼分辨
/// 「哪些能改、哪些只是展示」。v1.x 的授权行也归这页。
fn general_content() -> impl Widget {
    Column::new()
        .gap(16.0)
        .cross_center()
        .child(theme_dropdown())
}

/// 「关于」页签的内容: 产品名/版本/一句话 + 版本检查 + 反馈。
fn about_content(content_w: f32) -> impl Widget {
    Column::new()
        .gap(16.0)
        .cross_center()
        .child(about_section())
        .child(content_row(version_row(), content_w))
        .child(feedback_row())
}

/// 把一页的内容套进固定高度的透明盒 —— 各页签同高的实现点。
///
/// 各页的**内容**由 `*_content` / `shortcuts_section` 单独返回 (不带这个盒子),
/// 好让 `panel_contents_fit_fixed_height` 能独立量它们的自然高度。
fn panel_box(inner: impl Widget + 'static) -> impl Widget {
    UiBox::new(Color::TRANSPARENT)
        .height(PANEL_CONTENT_H)
        .child(inner)
}

/// 内容行：把内容收成固定宽度。
///
/// **不要在这里套 `Center`** (2026-09-13 去掉的): `Center` 在**两个轴上**都居中,
/// 而它若正好是 `panel_box` 的直接子级 —— 快捷键页就是这样 —— 整块内容会被
/// **垂直**居中, tab 栏下面凭空多出 `(PANEL_CONTENT_H - 内容高)/2` 的空档
/// (用户报的「快捷键内容和 tab 间隔大」, 快捷键页那会儿是 45px)。
/// 水平居中的活儿不必它干: 各页 Column 有 `cross_center`, 且这里的
/// `width` 就等于可用宽 (`content_width()` = 卡片宽 - 左右 padding), 本来就填满。
fn content_row(inner: impl Widget + 'static, width: f32) -> impl Widget {
    UiBox::new(Color::TRANSPARENT).width(width).child(inner)
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

/// 快捷键固定键列宽 (成列才好扫; 与作用之间留出对齐感)。
const SHORTCUT_KEY_W: f32 = 120.0;

/// 快捷键一览的「键 → 作用」表 —— 「用户怎么知道有这个键」在界面上的唯一归处
/// (人工验收反馈: 用户无从得知 `Ctrl+L` 能收起侧栏)。
/// 只列**猜不出来**的那几个组合键 (方向键/翻页键不必教); 完整清单在 README。
/// 提为模块级常量: 回归锁 `shortcut_card_bookmark_rows_match_dispatch` 要读它。
const SHORTCUT_KEYS: [(&str, &str); 6] = [
    ("Ctrl+O", "打开文件"),
    ("Ctrl+F", "搜索"),
    ("Ctrl+T", "表格 / 原始模式互切"),
    ("Ctrl+L", "级别侧栏 显示 / 收起"),
    ("Ctrl+B", "添加书签 / 去掉书签"),
    ("Ctrl+G", "跳下一书签"),
];

/// 快捷键一览 —— 放设置卡而不是散在界面各处: 状态栏右下的 ⚙ 是「设置卡」的
/// 唯一入口, 用户找说明会来这儿。
///
/// **不再有「完整清单见 README」引导行** (2026-09-14 用户去掉): 商店版用户
/// 没有仓库语境, 不知道 README 在哪 —— 那行字对他们是指向虚无。故本表即全部
/// 说教, README 的完整清单只服务 GitHub 读者。
fn shortcuts_section(content_w: f32) -> impl Widget {
    let mut col = Column::new().gap(4.0).cross_stretch();
    for (k, a) in SHORTCUT_KEYS {
        col = col.child(shortcut_row(k, a));
    }
    content_row(col, content_w)
}

/// 一行「键 → 作用」: 键固定列宽, 作用在右。
fn shortcut_row(key: &'static str, action: &'static str) -> impl Widget {
    Row::new()
        .cross_center()
        .fill(
            UiBox::new(Color::TRANSPARENT).width(SHORTCUT_KEY_W).child(
                Text::new(key.to_string())
                    .font_size(BODY_SIZE)
                    .bind_color(|app: &LogApp| app.theme.theme().text_primary()),
            ),
            0,
        )
        .child(
            Text::new(action.to_string())
                .font_size(BODY_SIZE)
                .bind_color(|app: &LogApp| app.theme.theme().text_secondary()),
        )
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
                // `Dropdown::new` 内部硬编码 `LightTheme` (dropdown.rs) ——
                // 构造态颜色同样是烘死的, 必须挂绑定才会跟着切主题走。
                .bind_theme(|app: &LogApp| app.theme.theme())
                .on_select(Msg::SelectTheme),
        )
}

/// 版本行高 —— 只在**有更新提示**时占位, 无提示时返回 0。
/// 抽成常量: `panel_contents_fit_fixed_height` 要拿它算关于页的最坏高度。
const VERSION_ROW_H: f32 = 32.0;

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
            Size::new(constraints.max().width, VERSION_ROW_H)
        } else {
            Size::new(constraints.max().width, 0.0)
        }
    }

    fn paint(&self, area: Rect, _rects: &mut RectBatch, texts: &mut TextBatch) {
        if !self.has_hint {
            return;
        }
        let baseline = area.origin.y
            + (VERSION_ROW_H - texts.line_height(f32::from(BODY_SIZE))) / 2.0
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
            .set(Rect::from_xywh(btn_x, area.origin.y, btn_w, VERSION_ROW_H));
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

#[cfg(test)]
mod tests {
    use super::*;
    use danqing::srgb_to_linear;

    /// 布局一个组件, 返回它**自报的自然高度**(不受固定高盒子影响)。
    fn natural_height(w: &mut impl Widget, c: Constraints, texts: &mut TextBatch) -> f32 {
        w.layout(c, texts).height
    }

    /// 固定高盒子是「各页签同高」的实现, 但它**不会裁切** —— 内容超高会溢出,
    /// 画到卡片外面去 (框架的 `Box`/`Column` 都不裁剪)。这条测试就是那句话的
    /// 可执行版本: 谁把某一页加高到越过 `PANEL_CONTENT_H`, 这里立刻红,
    /// 而不是等他肉眼在卡片外沿发现多出来一行。
    #[test]
    fn panel_contents_fit_fixed_height() {
        let w = content_width();
        let c = Constraints::loose(Size::new(w, 10_000.0));
        let mut texts = TextBatch::default();

        let mut general = general_content();
        let mut shortcuts = shortcuts_section(w);
        let mut about = about_content(w);

        let cases = [
            ("常规", natural_height(&mut general, c, &mut texts)),
            ("快捷键", natural_height(&mut shortcuts, c, &mut texts)),
            // 关于页会随「有新版本」长出一行, 按**最坏情况**(提示存在)算
            (
                "关于(含更新提示)",
                natural_height(&mut about, c, &mut texts) + VERSION_ROW_H,
            ),
        ];
        for (name, h) in cases {
            assert!(
                h <= PANEL_CONTENT_H,
                "{name} 页自然高度 {h} > PANEL_CONTENT_H={PANEL_CONTENT_H}: \
                 会溢出固定盒、画到卡片外 —— 请调大该常量"
            );
        }
    }

    /// 版本行的实测高度必须等于 `VERSION_ROW_H` —— 上一条拿它当最坏情况增量,
    /// 常量与实现脱钩就等于白算。
    #[test]
    fn version_row_height_matches_const() {
        let mut row = VersionRow::new();
        row.has_hint = true;
        let mut texts = TextBatch::default();
        let h = row
            .layout(Constraints::loose(Size::new(300.0, 100.0)), &mut texts)
            .height;
        assert_eq!(h, VERSION_ROW_H);
    }

    /// 回归锁 (2026-09-14): 合并行 `("Ctrl+B · Ctrl+G", "切换书签 / 下一书签")` 拆成两行时,
    /// Ctrl+G 的作用被抄成「切换书签」—— 那是 Ctrl+B (`toggle_bookmark`) 的动词;
    /// Ctrl+G 实为 `goto_next_bookmark` 跳下一书签 (main.rs 键分发与 README 完整清单均为此义)。
    /// 卡上这两行的动词不可互换; 断言钉语义关键词而不是整串, 给措辞微调留余地。
    #[test]
    fn shortcut_card_bookmark_rows_match_dispatch() {
        let action_of = |key: &str| {
            SHORTCUT_KEYS
                .iter()
                .find(|(k, _)| *k == key)
                .unwrap_or_else(|| panic!("快捷键卡缺 {key} 行"))
                .1
        };
        assert!(
            action_of("Ctrl+B").contains("添加"),
            "Ctrl+B = 切换书签 (添加/去掉)"
        );
        assert!(
            action_of("Ctrl+G").contains("下一"),
            "Ctrl+G = 跳下一书签, 不是切换"
        );
    }

    /// 切主题后**设置卡里的页签栏文字色也要跟着变**。
    ///
    /// 页签栏是 `Tabs`, 它原本**只有 `bind(active_index)`, 没有任何主题绑定** ——
    /// 构造时烘死的 4 个色不随切换而变: 浅色启动切暗色, 未选中页签名会用浅色主题的
    /// 深灰压在暗色卡面上, 读不了。
    ///
    /// 断言用「**旧主题的色必须一个不剩**」而不是「新主题的色存在」:
    /// 卡里另有别的 `Text` 绑着同一支 token (版本行 / 快捷键说明), 它们本来就对,
    /// 拿「存在」判会**永真**。本会话已经吃过一次「断言根本不会失败」的亏。
    #[test]
    fn settings_card_tabs_follow_theme_switch() {
        use danqing::theme::DarkTheme;

        let mut card = settings_card(AppTheme::Dark); // 构造用暗色
        let mut light_app = crate::LogApp::new_empty();
        light_app.theme = AppTheme::Light; // 每帧状态是浅色
        card.sync(&light_app);

        let mut texts = TextBatch::default();
        let mut rects = RectBatch::new();
        let size = card.layout(Constraints::loose(Size::new(CARD_WIDTH, 600.0)), &mut texts);
        card.paint(Rect::new(Point::ZERO, size), &mut rects, &mut texts);

        let stale = DarkTheme.text_secondary();
        let stale = [
            srgb_to_linear(stale.r),
            srgb_to_linear(stale.g),
            srgb_to_linear(stale.b),
        ];
        let left = texts
            .instance_colors()
            .iter()
            .filter(|c| {
                (c.r - stale[0]).abs() < 1e-3
                    && (c.g - stale[1]).abs() < 1e-3
                    && (c.b - stale[2]).abs() < 1e-3
            })
            .count();
        assert_eq!(
            left, 0,
            "切到浅色后卡里不该还剩暗色主题的 text_secondary —— 有 {left} 处即页签栏没挂 bind_theme"
        );
    }

    /// 设置卡底色必须**每帧**从应用状态重取 —— 与标题栏同款的坑。
    ///
    /// `view()` 只在启动时求值一次, 所以 `settings_card(theme)` 里
    /// `UiBox::new(t.background())` 烘进去的是**启动那一刻**的底色: 浅色启动、
    /// 切到暗色 → 暗色 scrim 上浮着一张浅色卡。
    /// **构造用一个主题、sync 用另一个**才测得到绑定本身 (两边同主题的话,
    /// 就算没绑定也照样绿)。
    #[test]
    fn settings_card_background_follows_theme_switch() {
        use danqing::theme::LightTheme;

        let mut card = settings_card(AppTheme::Dark); // 构造用暗色
        let mut light_app = crate::LogApp::new_empty();
        light_app.theme = AppTheme::Light; // 每帧状态是浅色
        card.sync(&light_app);

        let mut texts = TextBatch::default();
        let mut rects = RectBatch::new();
        let size = card.layout(Constraints::loose(Size::new(CARD_WIDTH, 600.0)), &mut texts);
        card.paint(Rect::new(Point::ZERO, size), &mut rects, &mut texts);

        let b = LightTheme.background();
        let want = [
            srgb_to_linear(b.r),
            srgb_to_linear(b.g),
            srgb_to_linear(b.b),
            b.a,
        ];
        assert!(
            rects
                .instance_colors()
                .iter()
                .any(|c| c.iter().zip(want.iter()).all(|(x, y)| (x - y).abs() < 1e-3)),
            "切到浅色后卡面底色应变成浅色主题的 background()"
        );
    }

    /// 设置卡**不描边** —— 用户 2026-09-13 的裁定。
    ///
    /// 与底层的区分交给 scrim 就够 (卡外被压暗、卡内是不透明的 `background()`),
    /// 再描一圈边是重复的一道。守的是「别为了定义感又把边加回来」:
    /// `UiBox` 的 `border_width` 默认就是 `1.0`, 加回来只要一行 `.border_color(...)`。
    #[test]
    fn settings_card_paints_no_border() {
        let theme = AppTheme::Dark;
        let mut card = settings_card(theme);
        let mut rects = RectBatch::new();
        let mut texts = TextBatch::default();
        let size = card.layout(Constraints::loose(Size::new(CARD_WIDTH, 600.0)), &mut texts);
        card.paint(Rect::new(Point::ZERO, size), &mut rects, &mut texts);

        // 实例里存的是 **linear** 值, 比较前同样解码 —— 否则比的是两个色彩空间
        // (这正是修好双重 gamma 之前那批断言的形态)。
        let b = theme.theme().border();
        let want = [
            srgb_to_linear(b.r),
            srgb_to_linear(b.g),
            srgb_to_linear(b.b),
            b.a,
        ];
        let painted_border = rects.instance_colors().iter().any(|c| {
            c.iter()
                .zip(want.iter())
                .all(|(x, y)| (x - y).abs() < 0.001)
        });
        assert!(
            !painted_border,
            "设置卡不应画边框 —— 区分交给 scrim (见 settings_card 的注释)"
        );
    }
}
