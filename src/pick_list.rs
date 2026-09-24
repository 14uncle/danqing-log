//! @author 十四叔
//! @date 2026/09/23
//!
//! RowList 自绘行列表件 (SPEC-v1x-field-picker-ui D2): 框架树冻结铁律
//! (`app.view()` 一次性建树不再重建, 每帧只 `sync` 刷值) 下**动态行列表**的正解 ——
//! 行文案/高亮经 sync 闭包每帧取自 LogApp, paint 动态画行, event 合成几何命中。
//!
//! 消费者两处 (T0 修复 + 本模块): 「显示列」弹层行集 (`settings::col_menu_rows`,
//! 修复其启动快照 bug) 与字段查询弹层的字段行。行 = (显示文案, 载荷), 点击把
//! **载荷**交给 `on_pick` 构造 Msg —— 显示前缀 (`[x]`/`[ ]`) 不污染语义身份。

use std::any::Any;
use std::cell::{Cell, RefCell};

use danqing::event::MouseButton;
use danqing::widget::{EventResult, MsgQueue, Widget};
use danqing::{Color, Constraints, Event, Point, Rect, RectBatch, Size, TextBatch, Theme};

use crate::{LogApp, Msg};

/// 行高 (与侧栏行指标同尺度; 弹层行列表的紧凑行)。
pub(crate) const ROW_H: f32 = 28.0;

/// 行数据源: (显示文案, 载荷) 列表 —— sync 每帧取。
type RowsFn = Box<dyn Fn(&LogApp) -> Vec<(String, String)>>;
/// 高亮载荷 (选中行); None = 无高亮 —— sync 每帧取。
type HighlightFn = Box<dyn Fn(&LogApp) -> Option<String>>;

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
    // ---- sync 缓存 (paint/event 只读) ----
    rows: Vec<(String, String)>,
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
            rows: Vec::new(),
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
        self.highlight = (self.highlight_fn)(app);
        let t = app.theme.theme();
        self.text_secondary = t.text_secondary();
        self.hover_bg = t.surface_variant();
        self.accent = t.accent();
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
            texts.push_text(
                label,
                r.origin.x + 8.0,
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
                r.origin.x + 8.0,
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
}
