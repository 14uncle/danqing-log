//! @author 十四叔
//! @date 2026/09/23
//!
//! RowList 自绘行列表件 (SPEC-v1x-field-picker-ui D2): 框架树冻结铁律
//! (`app.view()` 一次性建树不再重建, 每帧只 `sync` 刷值) 下**动态行列表**的正解 ——
//! 行文案/高亮经 sync 闭包每帧取自 LogApp, paint 动态画行, event 合成几何命中。
//!
//! 消费者四处: 「显示列」弹层 (`settings::col_menu_rows`) / 字段查询弹层字段行 /
//! 命名会话卡 / 合并源弹层源行。行 = (显示文案, 载荷), 点击把
//! **载荷**交给 `on_pick` 构造 Msg —— 显示装饰不污染语义身份。
//! 行首可选件 (加法不改契约, 不装 = 零变化): 色块 (`with_swatch`) /
//! 复选框 (`with_checkbox`, SPEC-checkbox-widget —— `[x]`/`[ ]` 文本勾选
//! 2026-09-28 退役, 勾选态移交框架矢量盒 `Checkbox::paint_box` 单真源)。

use std::any::Any;
use std::cell::{Cell, RefCell};

use danqing::event::MouseButton;
use danqing::widget::{Checkbox, CheckboxColors, EventResult, MsgQueue, Widget};
use danqing::{Color, Constraints, Event, Point, Rect, RectBatch, Size, TextBatch, Theme};

use crate::{LogApp, Msg};

/// 行高 (与侧栏行指标同尺度; 弹层行列表的紧凑行)。
pub(crate) const ROW_H: f32 = 28.0;

/// 行内左边距 —— paint 三处 (色块/文案/尾行) 与行宽守卫**同源** (G-d 教训:
/// 各写一份 8.0 会漂)。
pub(crate) const ROW_PAD_X: f32 = 8.0;

/// 行数据源: (显示文案, 载荷) 列表 —— sync 每帧取。
type RowsFn = Box<dyn Fn(&LogApp) -> Vec<(String, String)>>;
/// 高亮载荷 (选中行); None = 无高亮 —— sync 每帧取。
type HighlightFn = Box<dyn Fn(&LogApp) -> Option<String>>;
/// 行首色块 (可选第四闭包, 合并源弹层用): (app, 载荷) → 色; None = 该行不画块。
/// **加法不改契约** —— 既有三消费者不 `with_swatch` 就是零变化 (画法见 paint)。
type SwatchFn = Box<dyn Fn(&LogApp, &str) -> Option<Color>>;
/// 行首复选框 (可选第五闭包, SPEC-checkbox-widget T3): (app, 载荷) → 勾态;
/// None = 该行不画盒。与色块可同装 (次序: 色块→盒→文案, spec D9)。
type CheckboxFn = Box<dyn Fn(&LogApp, &str) -> Option<bool>>;

/// 逐行派生闭包的擦除引用形态 (per_row 参数; clippy type_complexity 收口)。
type PerRowFn<T> = dyn Fn(&LogApp, &str) -> Option<T>;

/// 逐行派生缓存 (swatches/checked 同法收口): 装了闭包 = 逐行取态, 没装 = 空 Vec
/// (paint 里 `.get(i)` 得 None = 该行不画)。**每帧重取** —— 摘掉 = 冻结在首帧
/// 快照 (T0 行集锁与勾态锁的共同判罪点)。
fn per_row<T>(rows: &[(String, String)], f: Option<&PerRowFn<T>>, app: &LogApp) -> Vec<Option<T>> {
    match f {
        Some(f) => rows.iter().map(|(_, p)| f(app, p)).collect(),
        None => Vec::new(),
    }
}

/// 行列表件 (自绘; 契约见模块注释)。
pub(crate) struct RowList {
    rows_fn: RowsFn,
    highlight_fn: HighlightFn,
    /// 点击回调: 载荷 → Msg。
    on_pick: Box<dyn Fn(&str) -> Msg>,
    /// 封顶行数 (超出折叠进「还有 N …」尾行, 尾行不可点)。
    max_rows: usize,
    /// 尾行文案 (N → 文案)。
    more_fn: Box<dyn Fn(usize) -> String>,
    /// 行首色块 (可选; [`RowList::with_swatch`] 装)。
    swatch_fn: Option<SwatchFn>,
    /// 行首复选框 (可选; [`RowList::with_checkbox`] 装)。
    checkbox_fn: Option<CheckboxFn>,
    // ---- sync 缓存 (paint/event 只读) ----
    rows: Vec<(String, String)>,
    swatches: Vec<Option<Color>>,
    /// 逐行勾态 (装了 checkbox_fn 才填充; None = 该行不画盒)。
    checked: Vec<Option<bool>>,
    /// 常态复选框颜色套 (主题 token, 每帧刷新)。
    checkbox_colors: CheckboxColors,
    /// 高亮行 (accent 底) 反白套 (spec D3b)。
    checkbox_on_accent: CheckboxColors,
    more: usize,
    highlight: Option<String>,
    text_secondary: Color,
    hover_bg: Color,
    accent: Color,
    /// hover 行 (纯视觉; 点击判定不读它)。
    hover: Cell<Option<usize>>,
    /// 按下锚点**载荷** (可点行才记): 抬起时命中行载荷**全等**才触发 ——
    /// 按下拖出再抬不触发; **按下与抬起之间 sync 换数据也不误触发**
    /// (评审 R4: 存行号会让 live-tail 轮转窗口的点击送错列 —— 载荷即行身份)。
    pressed: RefCell<Option<String>>,
}

impl RowList {
    pub(crate) fn new(
        max_rows: usize,
        rows_fn: impl Fn(&LogApp) -> Vec<(String, String)> + 'static,
        highlight_fn: impl Fn(&LogApp) -> Option<String> + 'static,
        on_pick: impl Fn(&str) -> Msg + 'static,
        more_fn: impl Fn(usize) -> String + 'static,
    ) -> Self {
        Self {
            rows_fn: Box::new(rows_fn),
            highlight_fn: Box::new(highlight_fn),
            on_pick: Box::new(on_pick),
            max_rows,
            more_fn: Box::new(more_fn),
            swatch_fn: None,
            checkbox_fn: None,
            rows: Vec::new(),
            swatches: Vec::new(),
            checked: Vec::new(),
            checkbox_colors: CheckboxColors::from_theme(&danqing::LightTheme),
            checkbox_on_accent: CheckboxColors::on_accent(danqing::LightTheme.accent()),
            more: 0,
            highlight: None,
            text_secondary: Color::rgb(0.4, 0.4, 0.42),
            hover_bg: Color::rgb(0.9, 0.9, 0.9),
            accent: Color::rgb(0.18, 0.35, 0.60),
            hover: Cell::new(None),
            pressed: RefCell::new(None),
        }
    }

    /// 可视行数 = 展示行 + 可选尾行。
    fn total_rows(&self) -> usize {
        self.rows.len() + usize::from(self.more > 0)
    }

    /// 行矩形 (顶部起, 逐行 [`ROW_H`])。
    fn row_rect(&self, area: Rect, i: usize) -> Rect {
        Rect::from_xywh(
            area.origin.x,
            area.origin.y + i as f32 * ROW_H,
            area.size.width,
            ROW_H,
        )
    }

    fn row_at(&self, area: Rect, p: Point) -> Option<usize> {
        if p.x < area.origin.x || p.x >= area.origin.x + area.size.width {
            return None;
        }
        let y = p.y - area.origin.y;
        if y < 0.0 {
            return None;
        }
        let i = (y / ROW_H) as usize;
        (i < self.total_rows()).then_some(i)
    }

    /// 该行可点吗 (hover 留痕与按下锚点共用判据 —— 可点性单一事实源)。
    /// 尾行不可点。
    fn row_clickable(&self, row: usize) -> bool {
        row < self.rows.len()
    }

    /// 装行首色块 (加法 builder; 既有消费者零变化)。
    pub(crate) fn with_swatch(
        mut self,
        f: impl Fn(&LogApp, &str) -> Option<Color> + 'static,
    ) -> Self {
        self.swatch_fn = Some(Box::new(f));
        self
    }

    /// 装行首复选框 (加法 builder; 不装 = 零变化, spec D4)。
    pub(crate) fn with_checkbox(
        mut self,
        f: impl Fn(&LogApp, &str) -> Option<bool> + 'static,
    ) -> Self {
        self.checkbox_fn = Some(Box::new(f));
        self
    }

    /// 测试内省: (显示文案, 载荷) 快照 —— 前缀退役锁等「断言产出」用
    /// (框架 `TextBatch::glyph_clips` 同款测试通道)。
    #[cfg(test)]
    pub(crate) fn rows_snapshot(&self) -> &[(String, String)] {
        &self.rows
    }

    /// 色块宽 (含与文案的间隔) —— paint 与文案 x 同源, 行宽守卫同读。
    pub(crate) const SWATCH_W: f32 = 18.0;
    /// 复选框位宽 (盒 14 + 与文案间隔 6) —— paint 与文案 x 同源 (SWATCH_W 同规)。
    pub(crate) const CHECK_W: f32 = Checkbox::BOX_SIZE + 6.0;
}

impl Widget for RowList {
    fn sync(&mut self, state: &dyn Any) {
        let Some(app) = state.downcast_ref::<LogApp>() else {
            return;
        };
        // 行数据**每帧重取** (T0 回归锁的判罪点: 摘掉这步 = 行冻结在首帧快照)
        let all = (self.rows_fn)(app);
        self.more = all.len().saturating_sub(self.max_rows);
        self.rows = all.into_iter().take(self.max_rows).collect();
        // 行首件 (色块/勾态) **每帧重取** (行数据同规: 摘掉 = 冻结在首帧快照)
        self.swatches = per_row(&self.rows, self.swatch_fn.as_deref(), app);
        self.checked = per_row(&self.rows, self.checkbox_fn.as_deref(), app);
        self.highlight = (self.highlight_fn)(app);
        let t = app.theme.theme();
        self.text_secondary = t.text_secondary();
        self.hover_bg = t.surface_variant();
        self.accent = t.accent();
        self.checkbox_colors = CheckboxColors::from_theme(&t);
        self.checkbox_on_accent = CheckboxColors::on_accent(t.accent());
    }

    fn layout(&mut self, constraints: Constraints, _texts: &mut TextBatch) -> Size {
        let h = self.total_rows() as f32 * ROW_H;
        Size::new(constraints.max().width, h)
    }

    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        let line_h = texts.line_height(f32::from(crate::view::FONT_SIZE));
        let base_off = (ROW_H - line_h) / 2.0 + texts.ascent(f32::from(crate::view::FONT_SIZE));
        for (i, (label, payload)) in self.rows.iter().enumerate() {
            let r = self.row_rect(area, i);
            let is_hi = self.highlight.as_deref() == Some(payload.as_str());
            if is_hi {
                rects.push_rect(r, self.accent, 4.0);
            } else if self.hover.get() == Some(i) {
                rects.push_rect(r, self.hover_bg, 4.0);
            }
            // 可点行一律 accent 文字 (评审 Optional: 与 format_btn 同级的可点暗示,
            // 免得纯文本行被读成只读列表); 高亮行反白在 accent 底上 (主题无
            // on-accent token, 白字即两主题下的既定呈现)。
            let color = if is_hi { Color::WHITE } else { self.accent };
            // 行首色块 (with_swatch 装了才画): 10×10 圆角小块, 画块才右移文案
            // (x 偏移与色块同源 —— 两处各写一个 8.0 会漂)。
            let mut text_x = r.origin.x + ROW_PAD_X;
            if let Some(c) = self.swatches.get(i).copied().flatten() {
                rects.push_rect(
                    Rect::from_xywh(
                        r.origin.x + ROW_PAD_X,
                        r.origin.y + (ROW_H - 10.0) / 2.0,
                        10.0,
                        10.0,
                    ),
                    c,
                    2.0,
                );
                text_x += Self::SWATCH_W;
            }
            // 行首复选框 (with_checkbox 装了且该行 Some 才画; 高亮行用反白套 D3b)。
            // 画法 = 框架 `Checkbox::paint_box` 单真源 —— 此处不另起画法。
            if let Some(checked) = self.checked.get(i).copied().flatten() {
                let box_rect = Rect::from_xywh(
                    text_x,
                    r.origin.y + (ROW_H - Checkbox::BOX_SIZE) / 2.0,
                    Checkbox::BOX_SIZE,
                    Checkbox::BOX_SIZE,
                );
                let colors = if is_hi {
                    self.checkbox_on_accent
                } else {
                    self.checkbox_colors
                };
                Checkbox::paint_box(rects, box_rect, checked, colors);
                text_x += Self::CHECK_W;
            }
            texts.push_text(
                label,
                text_x,
                r.origin.y + base_off,
                crate::view::FONT_SIZE,
                color,
            );
        }
        if self.more > 0 {
            let r = self.row_rect(area, self.rows.len());
            let label = (self.more_fn)(self.more);
            texts.push_text(
                &label,
                r.origin.x + ROW_PAD_X,
                r.origin.y + base_off,
                crate::view::FONT_SIZE,
                self.text_secondary,
            );
        }
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        match event {
            Event::CursorMoved(p) => {
                // hover 留痕仅可点行 (尾行不留痕)
                self.hover
                    .set(self.row_at(area, *p).filter(|&i| self.row_clickable(i)));
                EventResult::Ignored
            }
            Event::CursorLeft => {
                self.hover.set(None);
                *self.pressed.borrow_mut() = None; // 按下中离窗 = 放弃 (保守)
                EventResult::Ignored
            }
            Event::MouseInput {
                button: MouseButton::Left,
                pressed: true,
                position,
                ..
            } => match self
                .row_at(area, *position)
                .filter(|&i| self.row_clickable(i))
            {
                Some(i) => {
                    // 锚点记**载荷**不记行号 (评审 R4): 按下与抬起之间 sync
                    // 换数据时, 同行号已是别的列 —— 载荷全等才算同一次点击。
                    *self.pressed.borrow_mut() = Some(self.rows[i].1.clone());
                    EventResult::Consumed
                }
                None => EventResult::Ignored,
            },
            Event::MouseInput {
                button: MouseButton::Left,
                pressed: false,
                position,
                ..
            } => {
                let hit = self
                    .row_at(area, *position)
                    .filter(|&i| self.row_clickable(i));
                let anchor = self.pressed.borrow_mut().take();
                if let (Some(anchor), Some(h)) = (anchor, hit) {
                    if self.rows[h].1 == anchor {
                        msgs.push(Box::new((self.on_pick)(&anchor)));
                    }
                }
                if hit.is_some() {
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            _ => EventResult::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonl::{Column, Schema};
    use std::sync::Arc;

    fn temp_cfg(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "danqing-log-pick-{tag}-{}.toml",
            std::process::id()
        ))
    }

    fn app_with(cols: &[&str]) -> LogApp {
        let mut app = LogApp::new_empty_at(Some(temp_cfg("rowlist")));
        app.has_file = true;
        app.schema = Some(Arc::new(Schema {
            columns: cols
                .iter()
                .map(|n| Column {
                    name: (*n).to_string(),
                    width_chars: 4,
                })
                .collect(),
        }));
        app
    }

    fn test_list() -> RowList {
        RowList::new(
            16,
            |app: &LogApp| {
                app.schema
                    .as_deref()
                    .map(|s| {
                        s.columns
                            .iter()
                            .map(|c| (format!("row-{}", c.name), c.name.clone()))
                            .collect()
                    })
                    .unwrap_or_default()
            },
            |_app: &LogApp| None,
            |payload: &str| Msg::ToggleColumn(payload.to_string()),
            |n| format!("… 还有 {n}"),
        )
    }

    /// 色块锁 (T4 加法): `with_swatch` 装了才画块 —— 每可点行恰一块;
    /// 不装 = 零块 (既有消费者零变化的 A/B 面)。
    #[test]
    fn swatch_paints_one_chip_per_clickable_row_only_when_installed() {
        let cfg = temp_cfg("swatch");
        let mut app = LogApp::new_empty_at(Some(cfg.clone()));
        app.schema = Some(Arc::new(Schema {
            columns: vec![
                Column {
                    name: "a".into(),
                    width_chars: 4,
                },
                Column {
                    name: "b".into(),
                    width_chars: 4,
                },
            ],
        }));
        let c = Constraints::loose(Size::new(300.0, 10_000.0));
        let area = Rect::from_xywh(0.0, 0.0, 300.0, 200.0);
        // 不装: 零块
        let mut list = test_list();
        list.sync(&app);
        let mut rects = RectBatch::new();
        let mut texts = TextBatch::new();
        let _ = list.layout(c, &mut texts);
        list.paint(area, &mut rects, &mut texts);
        assert_eq!(rects.instance_rects().len(), 0, "没装色块 = 零块");
        // 装了: 每可点行恰一块
        let mut list = test_list().with_swatch(|_app, payload| {
            Some(if payload == "a" {
                Color::from_srgb8(0xFF, 0, 0)
            } else {
                Color::from_srgb8(0, 0, 0xFF)
            })
        });
        list.sync(&app);
        let mut rects = RectBatch::new();
        let mut texts = TextBatch::new();
        let _ = list.layout(c, &mut texts);
        list.paint(area, &mut rects, &mut texts);
        assert_eq!(rects.instance_rects().len(), 2, "两可点行 = 两块");
        std::fs::remove_file(&cfg).ok();
    }

    /// 评审判罪锁 (T0): 行**每帧重取** —— 建树后 schema 就位/换文件, 行集跟随。
    /// 摘 sync 缓存重建 = 行冻结在首帧 (启动快照 bug 本体) → 本锁红。
    /// 判据 = layout 高度 + 点击载荷 (TextBatch 无字串内省, 载荷即内容新鲜度)。
    #[test]
    fn rows_follow_app_state_across_sync() {
        let mut list = test_list();
        let mut texts = TextBatch::new();
        let c = Constraints::loose(Size::new(300.0, 10_000.0));
        // 建树时无 schema (启动快照场景) = 零行
        let cfg = temp_cfg("follow");
        let mut app = LogApp::new_empty_at(Some(cfg.clone()));
        list.sync(&app);
        assert_eq!(
            list.layout(c, &mut texts).height,
            0.0,
            "启动无 schema = 零行"
        );
        // schema 就位 (开文件) → 行出现
        app.schema = Some(Arc::new(Schema {
            columns: vec![
                Column {
                    name: "a".into(),
                    width_chars: 4,
                },
                Column {
                    name: "b".into(),
                    width_chars: 4,
                },
                Column {
                    name: "c".into(),
                    width_chars: 4,
                },
            ],
        }));
        list.sync(&app);
        assert_eq!(
            list.layout(c, &mut texts).height,
            3.0 * ROW_H,
            "行跟随 schema 就位"
        );
        // 换文件 → 行换
        app.schema = Some(Arc::new(Schema {
            columns: vec![
                Column {
                    name: "x".into(),
                    width_chars: 4,
                },
                Column {
                    name: "y".into(),
                    width_chars: 4,
                },
            ],
        }));
        list.sync(&app);
        assert_eq!(
            list.layout(c, &mut texts).height,
            2.0 * ROW_H,
            "行跟随换文件"
        );
        // 点击发的是**新** schema 的载荷 (内容新鲜度, 非首帧快照)
        let area = Rect::from_xywh(0.0, 0.0, 300.0, 200.0);
        let mut msgs = MsgQueue::new();
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: true,
                position: Point::new(20.0, 1.0 * ROW_H + 4.0),
            },
            area,
            &mut msgs,
        );
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: false,
                position: Point::new(20.0, 1.0 * ROW_H + 4.0),
            },
            area,
            &mut msgs,
        );
        assert!(
            msgs.iter().any(|m| matches!(
                m.downcast_ref::<Msg>(),
                Some(Msg::ToggleColumn(n)) if n == "y"
            )),
            "点击载荷 = 新 schema 的行 (非启动快照)"
        );
        std::fs::remove_file(&cfg).ok();
    }

    /// 点击 = 按下抬起同行为触发; 按下拖出再抬不触发; 右键不冒充 (P29)。
    #[test]
    fn click_requires_press_release_on_same_row() {
        let mut list = test_list();
        let app = app_with(&["a", "b"]);
        list.sync(&app);
        let area = Rect::from_xywh(0.0, 0.0, 300.0, 200.0);
        let row1 = Point::new(20.0, 1.0 * ROW_H + 4.0);
        let row0 = Point::new(20.0, 4.0);
        let mut msgs = MsgQueue::new();
        // 按行 1 → 拖到行 0 抬起: 不触发
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: true,
                position: row1,
            },
            area,
            &mut msgs,
        );
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: false,
                position: row0,
            },
            area,
            &mut msgs,
        );
        assert!(msgs.is_empty(), "按下拖出再抬不触发");
        // 右键抬起按下全程不触发
        list.event(
            &Event::MouseInput {
                button: MouseButton::Right,
                pressed: true,
                position: row1,
            },
            area,
            &mut msgs,
        );
        list.event(
            &Event::MouseInput {
                button: MouseButton::Right,
                pressed: false,
                position: row1,
            },
            area,
            &mut msgs,
        );
        assert!(msgs.is_empty(), "右键不冒充左键");
        // 同行按下抬起 = 触发 (载荷 = b)
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: true,
                position: row1,
            },
            area,
            &mut msgs,
        );
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: false,
                position: row1,
            },
            area,
            &mut msgs,
        );
        assert!(
            msgs.iter().any(|m| matches!(
                m.downcast_ref::<Msg>(),
                Some(Msg::ToggleColumn(n)) if n == "b"
            )),
            "同行按下抬起触发 on_pick"
        );
    }

    /// 评审 R4: 按下与抬起之间 sync 换数据 —— 同行号已是别的载荷, 点击**不许**
    /// 送错行 (live-tail 轮转窗口); 载荷未变则照常触发。
    #[test]
    fn press_release_survives_row_data_swap_only_for_same_payload() {
        let mut list = test_list();
        let cfg = temp_cfg("swap");
        let mut app = LogApp::new_empty_at(Some(cfg.clone()));
        app.schema = Some(Arc::new(Schema {
            columns: vec![
                Column {
                    name: "old_a".into(),
                    width_chars: 4,
                },
                Column {
                    name: "old_b".into(),
                    width_chars: 4,
                },
            ],
        }));
        list.sync(&app);
        let area = Rect::from_xywh(0.0, 0.0, 300.0, 200.0);
        let row0 = Point::new(20.0, 4.0);
        let mut msgs = MsgQueue::new();
        // 按下第 0 行 (载荷 old_a)
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: true,
                position: row0,
            },
            area,
            &mut msgs,
        );
        // 换数据 (轮转窗口): 同位置第 0 行已是 fresh_x
        app.schema = Some(Arc::new(Schema {
            columns: vec![Column {
                name: "fresh_x".into(),
                width_chars: 4,
            }],
        }));
        list.sync(&app);
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: false,
                position: row0,
            },
            area,
            &mut msgs,
        );
        assert!(msgs.is_empty(), "换数据后抬起不触发 (载荷锚定, 不送错行)");
        // 对照: 数据未变的按下抬起照常触发
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: true,
                position: row0,
            },
            area,
            &mut msgs,
        );
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: false,
                position: row0,
            },
            area,
            &mut msgs,
        );
        assert!(
            msgs.iter().any(|m| matches!(
                m.downcast_ref::<Msg>(),
                Some(Msg::ToggleColumn(n)) if n == "fresh_x"
            )),
            "载荷未变照常触发"
        );
        std::fs::remove_file(&cfg).ok();
    }

    /// hover 只留痕可点行; 封顶折叠尾行**不可点**。
    #[test]
    fn hover_and_more_row_are_visual_only() {
        let mut list = RowList::new(
            2,
            |_app: &LogApp| (0..4).map(|i| (format!("r{i}"), format!("p{i}"))).collect(),
            |_app: &LogApp| None,
            |payload: &str| Msg::ToggleColumn(payload.to_string()),
            |n| format!("… 还有 {n}"),
        );
        let mut app = app_with(&["a"]);
        app.schema = None; // rows_fn 不读 schema
        list.sync(&app);
        let mut texts = TextBatch::new();
        let c = Constraints::loose(Size::new(300.0, 10_000.0));
        assert_eq!(
            list.layout(c, &mut texts).height,
            3.0 * ROW_H,
            "2 展示行 + 1 尾行"
        );
        let area = Rect::from_xywh(0.0, 0.0, 300.0, 200.0);
        let mut msgs = MsgQueue::new();
        // hover 展示行 = 留痕; hover 尾行 = 不留痕
        list.event(&Event::CursorMoved(Point::new(20.0, 4.0)), area, &mut msgs);
        assert_eq!(list.hover.get(), Some(0));
        list.event(
            &Event::CursorMoved(Point::new(20.0, 2.0 * ROW_H + 4.0)),
            area,
            &mut msgs,
        );
        assert_eq!(list.hover.get(), None, "尾行不留痕");
        // 点尾行 = 无动作
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: true,
                position: Point::new(20.0, 2.0 * ROW_H + 4.0),
            },
            area,
            &mut msgs,
        );
        list.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: false,
                position: Point::new(20.0, 2.0 * ROW_H + 4.0),
            },
            area,
            &mut msgs,
        );
        assert!(msgs.is_empty(), "尾行不可点");
    }

    /// 高亮 = 载荷匹配 (选中态写读)。
    #[test]
    fn highlight_follows_payload() {
        let mut list = RowList::new(
            16,
            |app: &LogApp| {
                app.schema
                    .as_deref()
                    .map(|s| {
                        s.columns
                            .iter()
                            .map(|c| (c.name.clone(), c.name.clone()))
                            .collect()
                    })
                    .unwrap_or_default()
            },
            |app: &LogApp| app.columns.order.first().cloned(),
            |payload: &str| Msg::ToggleColumn(payload.to_string()),
            |n| format!("… 还有 {n}"),
        );
        let mut app = app_with(&["sel", "other"]);
        app.columns = danqing_log::columns::ColumnConfig::from_schema(&[
            "sel".to_string(),
            "other".to_string(),
        ]);
        list.sync(&app);
        assert_eq!(list.highlight.as_deref(), Some("sel"), "高亮 = 载荷匹配");
    }

    // ---- T3: with_checkbox (SPEC-checkbox-widget) ----

    /// 探针: 单画一枚盒, 量出 (勾中填充色, 未选边框色) —— 期望值由画法自身产出,
    /// 不手算线性空间 (RectBatch 实例存线性值, 见框架 Switch 测试 rgba_of 先例)。
    fn probe_box_colors(app: &LogApp) -> ([f32; 4], [f32; 4]) {
        let t = app.theme.theme();
        let colors = CheckboxColors::from_theme(&t);
        let a = Rect::from_xywh(0.0, 0.0, Checkbox::BOX_SIZE, Checkbox::BOX_SIZE);
        let mut checked = RectBatch::new();
        Checkbox::paint_box(&mut checked, a, true, colors);
        let fill = checked.instance_colors()[checked
            .instance_rects()
            .iter()
            .position(|r| r.size.width == Checkbox::BOX_SIZE)
            .expect("勾中探针应有整盒填充")];
        let mut unchecked = RectBatch::new();
        Checkbox::paint_box(&mut unchecked, a, false, colors);
        let border = unchecked.instance_colors()[0];
        (fill, border)
    }

    fn count_color(rects: &RectBatch, color: [f32; 4]) -> usize {
        rects
            .instance_colors()
            .iter()
            .filter(|c| **c == color)
            .count()
    }

    fn paint_list(list: &mut RowList, app: &LogApp) -> RectBatch {
        list.sync(app);
        let mut texts = TextBatch::new();
        let _ = list.layout(Constraints::loose(Size::new(300.0, 10_000.0)), &mut texts);
        let mut rects = RectBatch::new();
        list.paint(
            Rect::from_xywh(0.0, 0.0, 300.0, 200.0),
            &mut rects,
            &mut texts,
        );
        rects
    }

    /// 复选框加法锁 (T3, swatch 锁同构): **不装 = 零盒** (A 面);
    /// **装了 = 每可点行恰一盒** (B 面, 全勾数填充 / 全不勾数边框)。
    #[test]
    fn checkbox_paints_one_box_per_row_only_when_installed() {
        let cfg = temp_cfg("checkbox");
        let app = app_with(&["a", "b", "c"]);
        let (fill, border) = probe_box_colors(&app);
        // A 面: 不装 (行可点、无高亮无色块) → RectBatch 零实例
        let mut list = test_list();
        let rects = paint_list(&mut list, &app);
        assert_eq!(
            rects.instance_rects().len(),
            0,
            "不装 = 零矩形 (文案走 TextBatch, 盒/块都没有)"
        );
        // B 面全勾: 每行恰一枚填充盒
        let mut list = test_list().with_checkbox(|_app, _payload| Some(true));
        let rects = paint_list(&mut list, &app);
        assert_eq!(
            count_color(&rects, fill),
            3,
            "三行全勾 = 三枚填充盒 (摘画 = 红)"
        );
        // B 面全不勾: 每行恰一组边框 (单盒边框实例数探针量, 不写死); 零填充
        let mut one = RectBatch::new();
        let a14 = Rect::from_xywh(0.0, 0.0, Checkbox::BOX_SIZE, Checkbox::BOX_SIZE);
        let t = app.theme.theme();
        Checkbox::paint_box(&mut one, a14, false, CheckboxColors::from_theme(&t));
        let per_box = count_color(&one, border);
        let mut list = test_list().with_checkbox(|_app, _payload| Some(false));
        let rects = paint_list(&mut list, &app);
        assert_eq!(count_color(&rects, fill), 0, "全不勾 = 零填充");
        assert_eq!(
            count_color(&rects, border),
            per_box * 3,
            "三行未选 = 三盒边框 (每盒 {per_box} 实例)"
        );
        std::fs::remove_file(&cfg).ok();
    }

    /// None = 该行不画盒: a 勾 / b 未选 / c None → 恰一填充 + 恰一边框组。
    #[test]
    fn checkbox_none_payload_paints_no_box() {
        let cfg = temp_cfg("checkbox-none");
        let app = app_with(&["a", "b", "c"]);
        let (fill, border) = probe_box_colors(&app);
        let mut one = RectBatch::new();
        let a14 = Rect::from_xywh(0.0, 0.0, Checkbox::BOX_SIZE, Checkbox::BOX_SIZE);
        let t = app.theme.theme();
        Checkbox::paint_box(&mut one, a14, false, CheckboxColors::from_theme(&t));
        let per_box = count_color(&one, border);
        let mut list = test_list().with_checkbox(|_app, payload: &str| match payload {
            "a" => Some(true),
            "b" => Some(false),
            _ => None, // c 不画盒
        });
        let rects = paint_list(&mut list, &app);
        assert_eq!(count_color(&rects, fill), 1, "仅 a 一枚填充");
        assert_eq!(
            count_color(&rects, border),
            per_box,
            "仅 b 一盒边框; c 无盒"
        );
        std::fs::remove_file(&cfg).ok();
    }

    /// 勾态**每帧重取** (T0 判罪锁同族): sync 换数据盒态跟随 —— 摘重取 = 冻结红。
    #[test]
    fn checkbox_state_follows_sync() {
        let cfg = temp_cfg("checkbox-sync");
        let mut app = app_with(&["a", "b"]);
        app.columns =
            danqing_log::columns::ColumnConfig::from_schema(&["a".to_string(), "b".to_string()]);
        let (fill, _) = probe_box_colors(&app);
        let mut list = test_list()
            .with_checkbox(|app: &LogApp, payload: &str| Some(!app.columns.is_hidden(payload)));
        let rects = paint_list(&mut list, &app);
        assert_eq!(count_color(&rects, fill), 2, "初始两列全可见 = 两勾");
        app.columns.toggle_hidden("a");
        let rects = paint_list(&mut list, &app);
        assert_eq!(
            count_color(&rects, fill),
            1,
            "隐藏 a 后 sync 重取: 勾态跟随 (摘重取 = 此断言红在 2)"
        );
        std::fs::remove_file(&cfg).ok();
    }

    /// D3b 反白锁: 高亮行 (accent 底) 上的盒反白 —— 勾中 = 白填充盒,
    /// 未选 = 白边框; 非高亮行保持常态套。白在线性空间仍是 (1,1,1,1), 可直断。
    #[test]
    fn checkbox_on_highlighted_row_uses_inverse_colors() {
        let cfg = temp_cfg("checkbox-hi");
        let app = app_with(&["a", "b", "c"]);
        let (fill, _) = probe_box_colors(&app);
        const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
        let mut list = RowList::new(
            16,
            |app: &LogApp| {
                app.schema
                    .as_deref()
                    .map(|s| {
                        s.columns
                            .iter()
                            .map(|c| (c.name.clone(), c.name.clone()))
                            .collect()
                    })
                    .unwrap_or_default()
            },
            |_app: &LogApp| Some("a".to_string()), // a 行高亮
            |payload: &str| Msg::ToggleColumn(payload.to_string()),
            |n| format!("… 还有 {n}"),
        )
        .with_checkbox(|_app, payload: &str| Some(payload != "c")); // a,b 勾中; c 未选
        let rects = paint_list(&mut list, &app);
        let box_at = |row: f32, color: [f32; 4]| {
            rects
                .instance_rects()
                .iter()
                .zip(rects.instance_colors().iter())
                .filter(|(r, _)| r.size.width == Checkbox::BOX_SIZE)
                .any(|(r, c)| {
                    r.origin.y == row * ROW_H + (ROW_H - Checkbox::BOX_SIZE) / 2.0 && *c == color
                })
        };
        assert!(box_at(0.0, WHITE), "高亮+勾中 = 白填充盒 (D3b)");
        assert!(
            !box_at(0.0, fill),
            "高亮行上 accent 填充不可见, 不许用常态套"
        );
        assert!(box_at(1.0, fill), "非高亮+勾中 = accent 填充盒 (常态套)");
        assert!(
            !rects.instance_rects().iter().any(|r| r.origin.y
                == 2.0 * ROW_H + (ROW_H - Checkbox::BOX_SIZE) / 2.0
                && r.size.width == Checkbox::BOX_SIZE),
            "未选行无整盒填充 (仅边框小件)"
        );
        std::fs::remove_file(&cfg).ok();
    }

    /// 暗色主题锁 (评审补锁): 探针与画法都经 `app.theme` 取 token —— 暗色
    /// AppTheme 下盒色套换暗色 (accent/border 都是另一支), 防 09-13 式 token
    /// 再校准时暗色路径静默劣化无人拦 (此前全模块探针只跑浅色)。
    #[test]
    fn checkbox_colors_follow_app_theme_dark() {
        let cfg = temp_cfg("checkbox-dark");
        let mut app = app_with(&["a"]);
        app.theme = crate::config::AppTheme::Dark;
        let (dark_fill, dark_border) = probe_box_colors(&app);
        app.theme = crate::config::AppTheme::Light;
        let (light_fill, light_border) = probe_box_colors(&app);
        assert_ne!(dark_fill, light_fill, "暗色 accent 与浅色不同支");
        assert_ne!(dark_border, light_border, "暗色 border 与浅色不同支");
        // 暗色下真画一遍: 勾中填充 = 暗色探针 fill, 未选边框 = 暗色 border
        app.theme = crate::config::AppTheme::Dark;
        let mut app2_rows = app_with(&["a", "b"]);
        app2_rows.theme = crate::config::AppTheme::Dark;
        let mut list = test_list().with_checkbox(|_app, payload: &str| Some(payload == "a"));
        let rects = paint_list(&mut list, &app2_rows);
        assert_eq!(
            count_color(&rects, dark_fill),
            1,
            "暗色勾中盒 = 暗色 accent 填充"
        );
        assert!(
            count_color(&rects, dark_border) > 0,
            "暗色未选盒 = 暗色 border 边框"
        );
        std::fs::remove_file(&cfg).ok();
    }
}
