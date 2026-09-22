//! @author 十四叔
//! @date 2026/09/05
//!
//! 行锚定虚拟视口组件: 只布局/绘制可见行, 任何文件大小下渲染成本恒定。
//!
//! 为什么不复用 danqing Scrollable: 其 scroll_offset 为 f32 像素坐标,
//! 10M 行 × 20px = 2 亿逻辑像素远超 f32 精确整数域 (2^24≈16.7M),
//! 底部滚动会出现亚行漂移 —— 恰好在 POC 要证明的 1GB+ 场景失守。
//! 行锚定 (u64 行号 + 行内小数偏移) 在任意文件大小下精度无损。
//!
//! 布局 (v1 后过滤/搜索栏是独立 sibling, 不在此组件内):
//! `Column[ Bar(真 TextInput, 内容高 32) · LogView(本组件, fill) ]`
//!  LogView 负责: [表头 24px(仅表格)] [虚拟化行] [状态栏 26px]。
//! - 原始模式: 整行文本 + 级别着色, 与 POC v0 一致;
//! - 表格模式 (前提②): 表头 + 虚拟化行, 列定义来自 JSONL 采样 (jsonl::Schema),
//!   单元格经 memmem 字段提取 (零 parse), 显示行→文件行号经 filtered 命中表映射
//!   (无过滤时恒等), 行号槽恒显真实行号。
//!
//! POC 边界: 无水平滚动 (超宽单元格截断省略, 溢出右缘的列整列不画),
//! 无鼠标拖滚动条 (滚轮/键盘/点击), 无搜索/过滤命中高亮。

use std::any::Any;
use std::sync::Arc;
use std::time::Instant;

use danqing::widget::{EventResult, MsgQueue, TextInput, Widget};
use danqing::{
    Color, Constraints, CursorIcon, Edges, Event, Key, LightTheme, MouseButton, NamedKey, Point,
    Rect, RectBatch, Size, TextBatch, Theme,
};

use danqing::selection::{self, TextSelection};
use danqing_log::expand::{self, ExpandMap};
use danqing_log::jsonl::{self, Column, Schema, SubRow};
use danqing_log::logfile::LogFile;

use crate::{LogApp, Msg, ViewMode};

/// 复制行数上限 (R3 业务护栏): 滚轮甩底可造出全文件选区, 无上限复制 = 逐行
/// 解码 + 逐行分配, UI 冻结分钟级。10 万行 ≈ 16MB 文本, 剪贴板与内存都安全。
/// (text::selection 下沉时留产品侧 —— 框架 copy_text 本身无行数限制。)
const COPY_MAX_LINES: u64 = 100_000;

/// 行高 (逻辑像素)。24 = 可读性底线: 14px 正文上下仍留呼吸, 终端感消失。
pub(crate) const ROW_HEIGHT: f32 = 24.0;
/// 正文字号 (实机验收定档 14: 像素吸附落地后用户拍板; 行高 24 容得下)。
const FONT_SIZE: u16 = 14;
/// 行号/状态栏字号 (12: 竞品基准的可读底线, 11 在白底上偏吃力)。
const AUX_FONT_SIZE: u16 = 12;
/// 行号槽最小宽度。
const GUTTER_MIN: f32 = 56.0;
/// 文本与行号槽间距。
const GUTTER_GAP: f32 = 12.0;
/// 展开标识区宽度 (行首 +/-, 独立于行号槽, 不与行号重叠)。
const EXPAND_W: f32 = 20.0;
/// 展开标识字号 (比正文大一号, 12px 太小看不清)。
const EXPAND_FONT_SIZE: u16 = 16;
/// 展开标识的两个字符: 折叠 / 展开。**必须全是 ASCII**。
///
/// 内嵌 `ofl-mono.ttf` 是 **GB2312 子集**, 不在里面的字符是 **0×0 空字形** ——
/// 静默不画、不报错、不 panic。探针实测 MISSING: `−` (U+2212) / `✕` (U+2715) /
/// `▶`(U+25B6) / `▼`(U+25BC)。
/// **2026-09-15 用户实机报「展开之后 `-` 没有显示」, 正是代码里写了 `−` (U+2212)**
/// —— 而紧挨着的注释本来就写着「用 ASCII `+-` 保可用」, 是代码没照注释做。
/// 产品侧够不着框架的字体探针, 故把「只许 ASCII」提成常量, 由
/// `expand_glyph_is_ascii_and_lights_up_on_hover` 钉住。
const GLYPH_COLLAPSED: &str = "+";
const GLYPH_EXPANDED: &str = "-";
/// 底栏状态行高度。
const STATUS_HEIGHT: f32 = 26.0;

/// 更新角标直径 (逻辑像素; 圆点半径取直径之半)。SPEC-update-badge D1, 拟态值待验收 (e) 定档。
const UPDATE_DOT_D: f32 = 6.0;
/// 角标与「⚙ 设置」文字右缘的间隙。
const UPDATE_DOT_GAP: f32 = 2.0;

/// 角标几何 (纯函数): 挂「⚙ 设置」**文字**右上 —— X 在文字右缘外 [`UPDATE_DOT_GAP`],
/// Y 与**文字顶**齐平 (锚在文字, 不锚状态行: 26px 行顶会把点浮到标签上方 —— 评审抓出
/// 的 D1 偏差)。**不进布局**: 自绘路径绝对定位, 显隐零位移 (pomodoro 是 widget 树,
/// 才需要「常占槽位、颜色显隐」的对策, 这里不需要)。命中矩形由 [`settings_hit_rect`] 常算吞并。
fn update_dot_rect(text_right: f32, text_top: f32) -> Rect {
    Rect::from_xywh(
        text_right + UPDATE_DOT_GAP,
        text_top,
        UPDATE_DOT_D,
        UPDATE_DOT_D,
    )
}

/// 设置入口命中矩形 (纯函数): 文字 + 4px 内垫, **常算吞并角标** —— 角标显隐零位移,
/// 且整个可见角标都点得到 (半个点不中 = D3「点击照旧开设置卡」的缺口, 评审抓出)。
fn settings_hit_rect(settings_x: f32, settings_w: f32, text_top: f32, row_top: f32) -> Rect {
    let dot = update_dot_rect(settings_x + settings_w, text_top);
    let left = settings_x - 4.0;
    let right = (settings_x + settings_w + 4.0).max(dot.origin.x + dot.size.width);
    Rect::from_xywh(left, row_top, right - left, STATUS_HEIGHT)
}

/// 画更新角标 (D1): 有提示画 accent 圆点, 无提示**什么都不画** (零痕迹)。
/// 几何与 [`update_dot_rect`] 同源; 断言走 `RectBatch` 实例内省, 不设返回值。
fn paint_update_dot(
    rects: &mut RectBatch,
    text_right: f32,
    text_top: f32,
    has_hint: bool,
    accent: Color,
) {
    if !has_hint {
        return;
    }
    let dot = update_dot_rect(text_right, text_top);
    rects.push_rect(dot, accent, UPDATE_DOT_D / 2.0);
}
/// 过滤栏高度 (表格模式)。
const FILTER_BAR_H: f32 = 32.0;
/// 表头高度 (表格模式)。
const HEADER_H: f32 = 28.0;
/// 列内边距 (含在列宽里, 单元格文本右留 8)。
const COL_PAD: f32 = 16.0;
/// 右侧滚动条宽度。
const SCROLLBAR_W: f32 = 6.0;
/// 滚动条拇指最小长度 (竖条取高度, 横条取宽度)。
///
/// **2026-09-15 用户实机裁定后由 24 提到 48**: 当时那根竖条上是
/// **6 × 24px** 的拇指 (4771 行 / 964px 轨道 → 真实比例 0.5%, 被夹到底), 而同一屏
/// 的横条拇指是 **647 × 6px**。用户原话:「横向滚动条够了, 纵向滚动条有点小, 不够抓」。
/// **两根条的厚度与命中带完全一样** (6px / 12px), 差的就是长度 —— 所以「小」是长度问题,
/// 改这一处即可, 不动厚度与命中带。
///
/// 代价说清楚: 夹到 48 之后, 凡是可见比例低于 `48/轨道长` 的文件, 拇指**长度不再反映
/// 真实比例** (所有平台的滚动条都这么夹)。换来的是「拖得动」—— 24px 的目标要先精准
/// 落上去才谈得上拖, 而抓不住等于没有这个手势。
const THUMB_MIN_H: f32 = 48.0;
/// 水平滚动条的高度 (竖条用 [`SCROLLBAR_W`], 两者都是 6px 的视觉厚度)。
const SCROLLBAR_H: f32 = 6.0;
/// 滚动条的**可拖宽度** (视觉 6px, 命中给到 12px)。6px 的窄条按像素抓是抓不住的,
/// 而「抓不住」在交互上等同「没这个手势」。
const SCROLLBAR_HIT_W: f32 = 12.0;

/// 垂直滚动条几何 (T17)。
///
/// **paint 与拖拽共用这一份** —— 拖拽就是它的逆运算 ([`VScroll::top_row_at`])。
/// 各推一份式子迟早漂成「拇指画在这儿、抓住却跳到那儿」, 与 M2/S1 那次同一个教训。
#[derive(Clone, Copy, Debug)]
struct VScroll {
    /// 可拖区域 (比视觉轨道宽, 见 [`SCROLLBAR_HIT_W`])。
    hit: Rect,
    /// 轨道顶 = 拇指可移动区间的起点。
    track_top: f32,
    /// 拇指当前顶边 y。
    thumb_y: f32,
    thumb_h: f32,
    /// 拇指可移动的距离 (轨道高 − 拇指高)。0 = 拖不动 (内容刚好铺满)。
    span: f32,
    /// `top_row` 的上界。**与 paint 同口径** (`count − 可见行数`), 不是 app 的
    /// `clamp_top` 那个 `count − 1` —— 只有取 paint 的口径, 逆运算才对得上。
    max_top: f64,
}

/// 内容溢出视口时算出垂直滚动条几何; 不溢出 / 无高度 → `None` (不画也不可拖)。
fn v_scroll(area: Rect, rows_top: f32, list_h: f32, count: u64, top_row: f64) -> Option<VScroll> {
    let visible = f64::from(list_h / ROW_HEIGHT).max(1.0);
    if list_h <= 0.0 || count as f64 <= visible {
        return None;
    }
    let max_top = (count as f64 - visible).max(0.0);
    if max_top <= 0.0 {
        return None;
    }
    let ratio = ((visible / count as f64) as f32).min(1.0);
    let thumb_h = (list_h * ratio).max(THUMB_MIN_H).min(list_h);
    let span = (list_h - thumb_h).max(0.0);
    let t = (top_row / max_top).clamp(0.0, 1.0) as f32;
    let track_x = area.origin.x + area.size.width - SCROLLBAR_W;
    Some(VScroll {
        hit: Rect::from_xywh(
            track_x - (SCROLLBAR_HIT_W - SCROLLBAR_W),
            rows_top,
            SCROLLBAR_HIT_W,
            list_h,
        ),
        track_top: rows_top,
        thumb_y: rows_top + t * span,
        thumb_h,
        span,
        max_top,
    })
}

impl VScroll {
    /// 拇指顶 y → `top_row` —— paint 里那两行的**逆运算**。拖拽只走这一支。
    ///
    /// 往返一致 (验收 ①) 在 `top_row ∈ [0, max_top]` 内成立; 超出 (如 `GotoEnd`
    /// 给的 `count − 1`) 会被 paint 夹到拇指底, 逆运算回来得到 `max_top` ——
    /// 两者都是「在底部」, 但数值不等, 故不变量只在区间内谈。
    fn top_row_at(&self, thumb_top: f32) -> f64 {
        if self.span <= 0.0 {
            return 0.0;
        }
        let t = ((thumb_top - self.track_top) / self.span).clamp(0.0, 1.0);
        f64::from(t) * self.max_top
    }

    /// 该拇指位置是否已在条能被拖到的**最底**。
    ///
    /// 判据取**拇指位置**而不是 `top_row_at(...) >= max_top`: 后者要走一趟浮点
    /// 换算再比, 这里本来就有确切的坐标可看。
    fn at_bottom(&self, thumb_top: f32) -> bool {
        self.span <= 0.0 || thumb_top >= self.track_top + self.span - 0.5
    }
}

/// 水平滚动条几何 (T17)。与 [`VScroll`] 同一条纪律: paint 与拖拽共用一份,
/// 拖拽是它的逆运算。
#[derive(Clone, Copy, Debug)]
struct HScroll {
    /// 可拖区域 (纵向比视觉轨道高, 见 [`SCROLLBAR_HIT_W`])。
    hit: Rect,
    /// 轨道左端 = 拇指可移动区间的起点。
    track_left: f32,
    thumb_x: f32,
    thumb_w: f32,
    /// 拇指可移动的距离 (轨道宽 − 拇指宽)。
    span: f32,
    /// `x_offset` 的上界 (内容宽 − 视口宽)。
    max_x: f32,
}

/// 内容宽于视口时算出水平滚动条几何; 不溢出 → `None`。
fn h_scroll(text_x: f32, text_w: f32, track_y: f32, max_seen: f32, x_off: f32) -> Option<HScroll> {
    if text_w <= 0.0 || max_seen <= text_w {
        return None;
    }
    let ratio = (text_w / max_seen).min(1.0);
    let thumb_w = (text_w * ratio).max(THUMB_MIN_H).min(text_w);
    let span = (text_w - thumb_w).max(0.0);
    let max_x = (max_seen - text_w).max(1.0);
    Some(HScroll {
        // 命中带**向下**延伸 (轨道在列表区最底, 下面是状态栏): 向上会让 6px
        // 压在列表末行上, 那一带就从此选不中行了。竖条没有这个问题 —— 它的带子
        // 在右缘之外, 本来就是内容区以外。
        hit: Rect::from_xywh(text_x, track_y, text_w, SCROLLBAR_HIT_W),
        track_left: text_x,
        thumb_x: text_x + (x_off / max_x) * span,
        thumb_w,
        span,
        max_x,
    })
}

impl HScroll {
    /// 拇指左端 x → `x_offset` —— paint 的逆运算。
    fn x_offset_at(&self, thumb_left: f32) -> f32 {
        if self.span <= 0.0 {
            return 0.0;
        }
        let t = ((thumb_left - self.track_left) / self.span).clamp(0.0, 1.0);
        t * self.max_x
    }
}

/// 指针停在**哪一根**滚动条上 (T17)。
///
/// `Widget::cursor_icon` 没有位置参数 (`mod.rs:181-191`), 所以「只有条上才手型」
/// 只能靠 `CursorMoved` 缓存的位置判断。两条条共用一个槽, 顺带回答了
/// 「哪一根该加深」——分两个 bool 的话这两件事就得各维护一遍。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BarAxis {
    Vertical,
    Horizontal,
}
/// 双击判定窗口 (沿用 danqing title_bar.rs 先例)。
const DOUBLE_CLICK_MS: u128 = 300;
/// 双击位移容差; 同值兼任「按下→框选」升级阈值 (抖动不产选区)。
const CLICK_DIST: f32 = 4.0;

/// 表头下划线 / 状态栏顶线的颜色 —— 取**当前主题**的分割线色。
///
/// 原先恒用 `LightTheme.divider()`（不看当前主题）: 暗色下这两条线用的是浅色主题
/// 那条 (黑 10%), 压在近黑底上等于没画。回归锁 `header_line_follows_theme`。
fn header_line<T: Theme>(th: &T) -> Color {
    th.divider()
}
// ============================ 内容区的「面」阶梯 ============================
//
// 表格区有六层「面」: 页面底 / 斑马 / 表头(含过滤栏) / hover / 选中 / 展开块。
// **它们必须整体一起定** —— 各自单独调数值必然撞车 (2026-09-13 用户实机截图审查,
// 一次照出两处撞车: 暗色表头↔斑马 Δ`L*` 0.08、浅色展开块↔斑马 0.19)。
//
// **判据是「对相邻面」, 不是「对页面底」。** 这是那次审查最重要的收获:
// 记档里写的全是「对页面底 −3.68」这类数, 但屏幕上跟展开块挨着的**不是页面底,
// 是斑马行** —— 对底合格、对邻居不合格, 于是它一直隐形。
//
// 各面的实测 `L*` (取色器从真机截图量的, 不是算的):
//
// ```text
//            浅色             暗色
//   页面底    96.95            9.04
//   斑马      93.46 (−3.49)   12.48 (+3.44)
//   表头      90.32 (−6.62)   18.14 (+9.10)
//   hover     87.12 (−9.83)   24.21 (+15.17)
//   选中      86.33 (−10.62)  29.69 (+20.65)
//   展开块    83.07 (−13.88)   5.26 (−3.79)   ← 与底反向, 是「略深」
// ```
//
// 这张表的每一步都是**解出来的**, 不是挑的: 每面须与页面底 ≥3、且两两 ≥3。
//
// 回归锁 `table_surfaces_are_separated_from_their_neighbours` (≥3.0 Δ`L*`)。
//
// **已知未达标的一对, 明写在此**: 浅色 hover ↔ 选中 只有 **0.79**。
// 浅色那段可用明度区间养不起六个两两 ≥3 的面 —— 六个面最少要 15 个 `L*` 点,
// 而近白底 (96.95) 到还能算「浅色 UI」的区间只剩约 14 点。
// 二者的区分交给**第二条通道**: 选中是 accent 冷青、hover 是中性灰绿。
// 真要拉平得重排整条阶梯 (含改框架的 `selection` α), 属独立决策, 没夹带。
//
// **暗色那段更窄, 窄到必须让表头往外走**: 底色 `L*` 只有 9.04, 而「底↔斑马↔表头」
// 两点各要 3.0 就得 6.0 —— 原区间总共只有 6.30, 八个 8-bit 步长就吃掉了余量,
// 取整后必有一个方向掉到 2.99。故把表头 (框架 `surface_variant`) 提到 +9.10。
// ==========================================================================

/// 斑马纹底色 —— 表格模式隔行一条, 只做**结构**提示 (「读到哪一行」)。
///
/// **为什么要自己定而不是用 `surface_variant()`** (2026-09-13 用户实机报):
/// 那支 token 同时被 hover 用着, 两者同色会撞车 —— 悬停奇数行时颜色完全不变,
/// 悬停偶数行时 hovered 行与左右斑马行连成一片。浅色更糟: 该 token 是
/// **不透明色** `#EEF6F2`, 与底色 `#F0F8F6` 只差 2/255 (Δ`L*` −0.74), 等于没有。
///
/// 与 `bookmark_color` / `expand_block_bg` 同一处理: **产品语义放产品侧**。
/// 回归锁 `row_band_and_hover_are_separate_channels`。
fn row_band_bg(theme: crate::config::AppTheme) -> Color {
    match theme {
        // 白底往深走一档 (底色近白, 没有往上提的余地)。ΔL* −3.49。
        crate::config::AppTheme::Light => Color::from_srgb8(0xE6, 0xEE, 0xEC),
        // 近黑底往亮走一档, ΔL* +3.20。
        //
        // **2026-09-13 由 `#26262D` (Δ`L*` +6.38) 收到这里。** 原值是照
        // `surface_variant()` **当时的**渲染结果取的 (「模块 2 校准过的那条
        // 保持不变」), 但随后 `surface_variant` 自己被调到 +6.42 —— 两者撞成
        // **Δ`L*` 0.08, 表头与斑马行完全同色**。收到 +3.20 后与表头差 3.10,
        // 且顺带让暗色斑马的台阶 (3.20) 与浅色 (3.49) 对齐 —— 原值 6.38 是
        // 浅色的近两倍, 两个主题本来就不一致。
        crate::config::AppTheme::Dark => Color::from_srgb8(0x20, 0x20, 0x26),
    }
}

/// 行 hover 底色 —— 指针所在行, 必须**一眼看出是它**。
///
/// 台阶刻意比斑马**大一档** (浅色 Δ`L*` −9.1 / 斑马 −3.5; 暗色 +15.2 / 斑马 +6.4),
/// 这样无论 hover 到奇数行还是偶数行, 与相邻斑马行都拉得开 —— 原先两者共用
/// `surface_variant()`, 三行会连成一整块。
/// 回归锁 `row_band_and_hover_are_separate_channels`。
fn row_hover_bg(theme: crate::config::AppTheme) -> Color {
    match theme {
        crate::config::AppTheme::Light => Color::from_srgb8(0xD4, 0xDC, 0xDA),
        crate::config::AppTheme::Dark => Color::from_srgb8(0x39, 0x39, 0x40),
    }
}

/// 搜索命中行底色 —— **与选中行同族但弱一档**, 让「搜索留下的痕迹」与「当前选中」
/// 在画面上可分 (2026-09-14 实机 M0 P11/P14: 原先两者同用 `th.selection()`,
/// 命中行把 hover 与选中行都盖掉)。
///
/// 派生规则: **同 RGB, α 减半** —— 浅色 0.30 → 0.15, 暗色 0.20 → 0.10。
/// 减半不是拍的: 命中行要「比一般选中行更弱、但仍在」; α 减半后合成亮度离底色
/// 约一半, 与选中行拉开一档, 又不至于淡到看不出。
///
/// **为什么放产品侧** (与 `row_band_bg` / `row_hover_bg` 同一规矩, 见本文件 126 行
/// 「产品语义放产品侧」): 「搜索命中行」是日志查看器的语义, 不是通用 UI 语义;
/// 框架 `Theme` 只给**通用**调色板, 不替产品定语义色。
fn hit_row_bg(theme: crate::config::AppTheme) -> Color {
    let th = theme.theme();
    let sel = th.selection();
    let a = match theme {
        crate::config::AppTheme::Light => 0.15, // sel α 0.30 减半
        crate::config::AppTheme::Dark => 0.10,  // sel α 0.20 减半
    };
    Color::rgba(sel.r, sel.g, sel.b, a)
}

/// 单元格高亮的两笔颜色 (底色, 描边)。
///
/// **底笔与行选中同 token 是有意的** —— 单元格是行内**更具体**的选中, 底色沿用
/// 选中带, 读起来是「这一行里, 这一格」。
/// **但只画底色等于没有反馈** (2026-09-14 实机/审查 #2): 双击单元格必然同时选中
/// 该行 (调用方先推 `Msg::Select`), 行底色已经铺了 `selection()`, 同色再叠一层
/// 完全看不出选中的是哪一列 —— 而 Ctrl+C 只复制这一格, 视觉 (整行) 与结果
/// (单格) 自相矛盾。故**必须**另起一笔描边把格子圈出来。
/// 回归锁 `cell_highlight_border_is_a_second_stroke`。
fn cell_highlight_colors(th: &impl Theme) -> (Color, Color) {
    (th.selection(), th.accent())
}

/// 展开块底色 —— 比内容区底**略深**一档, 把子行与真实行分开。
///
/// 展开的子行原先与真实行**长得一模一样** (只差没有行号), 用户分不清
/// 「这坨是第 1 行展开的」还是「又是几行日志」。
///
/// **产品语义, 放产品侧** (框架没有「展开块」这个概念), 与 `bookmark_color`
/// 同一处理 —— 不扩公开 `Theme` trait。SPEC §3 原写「本模块必须给框架加 token」,
/// 那是基于当时以为要**两个**不透明底色 (内容区 + 展开块) 的判断;
/// 用户 2026-09-13 把范围收窄到只剩这一件事后, 那条结论不再成立。
/// 回归锁 `expand_block_bg_is_a_slightly_darker_step_in_both_themes`。
fn expand_block_bg(theme: crate::config::AppTheme) -> Color {
    match theme {
        // 白底 (#F0F8F6) 上深一档, ΔL* −13.95 —— 仍留冷青调, 与主题同温。
        //
        // **2026-09-13 由 `#E4EEEA` (Δ`L*` −3.68) 加深到这里。** 原值是对
        // **页面底**取得的, 而表格里紧挨着展开块的**是斑马行** —— 对斑马只有
        // Δ`L*` **0.19**, 等于没画。T1 想解决的「分不清这坨是展开的还是又几行
        // 日志」, 在浅色下一直没解决 (暗色对斑马是 10.17, 所以只有浅色坏)。
        //
        // 加深多少**不是挑出来的, 是解出来的**: 展开块必须与斑马 (−3.49)、
        // 表头 (−7.00)、hover (−9.12)、选中 (−10.62) 各差 ≥3。解空间只有
        // **≤ −13.62** 或 **≥ +2.8** 两段, 中间是空的 —— 原值 −3.68 正掉在空档里。
        // 用户裁定「保持略深」, 故取深井那一支。
        crate::config::AppTheme::Light => Color::from_srgb8(0xC8, 0xD1, 0xCD),
        // 近黑底 (#191920) 上再深一档, ΔL* −3.79。**余地很窄**: 底色 L* 只有 9.04,
        // 再深很快就到黑 —— 故取值偏保守, 具体手感待真机截图定 (设计提案门)。
        crate::config::AppTheme::Dark => Color::from_srgb8(0x11, 0x11, 0x17),
    }
}

/// 展开块底色的**矩形** + 这一段连续子行到哪结束 (开区间末位)。
///
/// **一段连续子行只出 `一个` 矩形** —— 这是本条的存在理由, 不是顺手优化。
///
/// 原先按子行逐行铺: 相邻两行在**逻辑坐标**上严丝合缝 (间距就是 [`ROW_HEIGHT`]),
/// 但每个矩形都会做**边缘抗锯齿**, 而它们是**各自**与下面的底色混合的 —— 两次
/// 半透明叠不出一次全不透明。于是行交界处留下一道 1–2px 的浅色横线
/// (2026-09-13 用户实机报「浅色主题, 行展开区域出现行间隔」; 截图实测那条缝
/// 渲染成 `(207,216,212)`, 而块色 `(200,209,205)`、页面底 `(240,248,246)`)。
///
/// **不是新缺陷, 是深色块把它照出来的**: 浅色块色原为 `#E4EEEA` (对底 Δ`L*` 3.68),
/// 缝与块色只差 ~2/255 —— 看不见; 换成 `#C8D1CD` (Δ`L*` 13.88) 后一眼就是「行间隔」。
/// 与 D1、面阶梯属同一类: **换个取值, 早先就错的东西才现形**。
///
/// `limit` 是**可见行上界**, 由调用方给: 一段可能长达十几万行 (一个巨大 JSON 全展开),
/// 而只有可见的几十行会被画出来 —— 扫到底纯属白费, 这是每帧都跑的热路径。
/// 返回值里的矩形底边因此可能落在视口下方, 交给 clip 裁掉。
///
/// 段首若在视口上方 (`is_sub_row(start - 1)` 为真, 滚动到底时会遇到),
/// **往上多铺一行**: 那条边界整个被 clip 挡掉, 不会画到别的行上, 却能免得
/// 一道缝正好落在视口第一行。
///
/// 回归锁 `expand_block_rects_put_a_contiguous_run_in_one_rect`。
fn expand_block_rects(
    start: u64,
    limit: u64,
    y_of: impl Fn(u64) -> f32,
    is_sub_row: impl Fn(u64) -> bool,
    x: f32,
    width: f32,
) -> Vec<Rect> {
    let mut out = Vec::new();
    let mut i = start;
    while i < limit {
        if !is_sub_row(i) {
            i += 1;
            continue;
        }
        // 段首可能在视口上方 —— 上面还有同名子行就往上多铺一行, 交给 clip 裁。
        let top = if i > 0 && is_sub_row(i - 1) {
            y_of(i) - ROW_HEIGHT
        } else {
            y_of(i)
        };
        // 一段连续子行 = **一个**矩形
        while i < limit && is_sub_row(i) {
            i += 1;
        }
        let bottom = y_of(i);
        out.push(Rect::from_xywh(x, top, width, bottom - top));
    }
    out
}

/// 书签行号色 —— 两主题各一支金。
///
/// **有意不套 `Theme::accent`**: accent 已经用于选中行 / 焦点边框 / 指示线,
/// 书签套上去会把**第三类语义**混进「选中/强调」那一个通道 —— 扫一眼分不出
/// 哪行是书签、哪行是选中。框架没有书签 token, **也不为它扩 trait**:
/// 这是产品语义, 放产品侧。回归锁 `bookmark_color_is_its_own_channel_per_theme`。
fn bookmark_color(theme: crate::config::AppTheme) -> Color {
    match theme {
        // 原值原样保留: 深金配白底, 用户验收过。
        crate::config::AppTheme::Light => Color::rgb(0.75, 0.60, 0.10),
        // 近黑底上要提亮 —— 同一支深金在暗色下会糊进背景。
        crate::config::AppTheme::Dark => Color::rgb(0.95, 0.78, 0.28),
    }
}

/// 语义色板 (级别/状态着色): **两套, 按底色明暗选**。
///
/// 原先这五支是写死的常量, 且清一色是「浅底上的深饱和色」—— 那是**只对浅色成立**
/// 的取值。暗色下它们压近黑底 (2026-09-13 实测, 对底 `#191920`):
/// ERROR **3.03** / INFO 3.70 / OK 4.28 / WARN 5.10 / DEBUG 5.46 ——
/// 除后两支外**全部低于 WCAG AA (4.5)**。用户实机报「ERROR 色看得眼花」,
/// 一半是这个原因, 另一半是选区带 (见 `danqing` 的
/// `selection_band_does_not_step_too_far_from_the_background`)。
///
/// 暗色那一套按实测挑: 换完后 ERROR 6.32 / WARN 10.47 / OK 10.03 / INFO 6.88 /
/// DEBUG 6.89, 全部过 AA。
///
/// **浅色的欠账同样成立, 且根子更深** —— 那套值只对**白底**成立, 换成页面底
/// `#F0F8F6` 就掉下来: 实测 WARN 3.25 / OK 3.78 / DEBUG 3.09 (INFO 4.38 也只差一点,
/// 只有 ERROR 5.34 是够的)。2026-09-13 一并修掉。
///
/// **判据 (两个主题同一把尺)**: 语义色是**文字**, 但按它实际压着的**面**分两档 ——
/// **常驻面** (页面底 / 斑马行) 过 WCAG AA 4.5; **瞬时面** (hover / 选中行) 不低于 3.0。
/// 瞬时态放宽到 3.0, 与框架侧 `selection_band_does_not_step_too_far_from_the_background`
/// 从两边一夹, 是暗色那次就定下的口径; 浅色这次照搬 —— 两个主题第一次用同一把尺。
///
/// **斑马行必须进常驻档**, 别只量页面底: 表里一半的行就是斑马底, 只量页面底
/// 等于漏掉一半的行 —— 这与「面阶梯」那次翻车是**同一个错**
/// (那里是判据对页面底、屏幕上挨着的是斑马, 见 [`row_band_bg`])。
///
/// **为什么不扩 `Theme` trait**: 级别/状态是**日志语义**, 不是通用设计 token
/// (与 [`bookmark_color`] 同一条判据)。放产品侧。
///
/// **为什么按底色亮度选而不是按 [`crate::config::AppTheme`] 匹配**: 这五支要压的
/// 是**底色**, 判据就是「它够不够亮能承受深饱和色」。写死成对 `AppTheme` 的 match,
/// 等于把这个事实编码成一个人工表, 将来加第三个主题还得记得回来改。
/// 阈值 0.5 落在两个内置主题的巨大空档里 (浅色底 `L` ≈ 0.93, 暗色底 ≈ 0.010)。
///
/// **浅色有两条口径, 且它们不一样 —— 这是查出来的, 不是设计出来的。**
/// 「整行着色」与「单元格着色」原先各写了一套字面量 (实测 `ΔE76`:
/// ERROR 3.43 / WARN 4.15 / DEBUG 1.65, JND ≈ 2.3 —— 前两支**可感知**)。
/// 两条路径**仍然分开**, 各自按上面那把尺独立压暗 (合并与否是独立决策 D3,
/// 见 `tasks/todo-open-decisions.md`)。这次压暗**几乎没有动到分叉本身** ——
/// 实测 `ΔE76`: ERROR 3.43 → **3.70** / WARN 4.15 → **3.42** / DEBUG 1.65 → 1.20,
/// 前两支仍**在 JND 之上**可感知。**注意这与「分值」无关**: 两条口径都过 AA 了,
/// 但它们仍然不是同一个颜色。要么并成一支, 要么认下这个分叉, 别指望它自己消失。
/// 暗色是全新的, 没有历史包袱, 故两条路径共用一套。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LevelPalette {
    /// ERROR / FATAL / 5xx。
    pub(crate) error: Color,
    /// WARN / 4xx。
    pub(crate) warn: Color,
    /// 2xx。
    pub(crate) ok: Color,
    /// INFO / 3xx。
    pub(crate) info: Color,
    /// DEBUG / TRACE。
    pub(crate) trace: Color,
}

impl LevelPalette {
    /// 浅色 · **整行着色**口径 (原 `level_color` 里的字面量)。
    ///
    /// 2026-09-13 按上面那把尺压暗: **保持 HSL 色相与饱和度, 只降亮度**
    /// (二分求最低亮度, 再留一点余量让护栏不贴在阈值上)。所以是**同一批颜色变深**,
    /// 不是换了一套色板 —— 用户熟悉的 `WARN` 仍是那个琥珀、`INFO` 仍是那个蓝。
    /// ERROR 本来就过线, **一个字节没动**。
    ///
    /// 修前 → 修后 (对页面底 `#F0F8F6` / 斑马 `#E6EEEC`):
    /// ERROR 5.34/4.88 不动 · WARN 3.25/2.97 → 5.05/4.62 ·
    /// OK 3.78/3.46 → 5.06/4.63 · INFO 4.38/4.00 → 5.05/4.62 ·
    /// DEBUG 3.09/2.83 → 5.04/4.61。回归锁
    /// [`tests::light_semantic_colors_clear_wcag_aa_on_light_surfaces`]。
    pub(crate) fn light_line() -> Self {
        Self {
            error: Color::from_srgb8(0xBF, 0x2E, 0x2E),
            warn: Color::from_srgb8(0x89, 0x63, 0x0F),
            ok: Color::from_srgb8(0x23, 0x78, 0x45),
            info: Color::from_srgb8(0x33, 0x6B, 0xAE),
            trace: Color::from_srgb8(0x69, 0x69, 0x71),
        }
    }

    /// 浅色 · **单元格/状态**口径 (原 `*_fg()` 函数)。
    ///
    /// 同上一把尺、同一套色相, 但**独立压暗** (两条口径的分叉见类型级文档)。
    /// 落到的值与 [`Self::light_line`] 很接近但**不相等** —— 这不是笔误。
    pub(crate) fn light_cell() -> Self {
        Self {
            error: Color::from_srgb8(0xC1, 0x36, 0x36),
            warn: Color::from_srgb8(0x8C, 0x62, 0x04),
            ok: Color::from_srgb8(0x23, 0x78, 0x45),
            info: Color::from_srgb8(0x33, 0x6B, 0xAE),
            trace: Color::from_srgb8(0x69, 0x69, 0x73),
        }
    }

    /// 暗色底一套 —— 提亮到近 `400` 档 (Tailwind 色阶的亮度区间)。
    /// **两条路径共用这一套**: 上面那点分叉是历史遗留, 不值得在暗色里复刻。
    pub(crate) fn dark() -> Self {
        Self {
            error: Color::from_srgb8(0xF8, 0x71, 0x71),
            warn: Color::from_srgb8(0xFB, 0xBF, 0x24),
            ok: Color::from_srgb8(0x4A, 0xDE, 0x80),
            info: Color::from_srgb8(0x60, 0xA5, 0xFA),
            trace: Color::from_srgb8(0x9C, 0xA3, 0xAF),
        }
    }

    /// 按底色亮度挑一套 (整行着色口径)。`th` 只用于判断明暗 —— 色板与主题无关。
    pub(crate) fn for_line<T: Theme>(th: &T) -> Self {
        if is_dark_background(th) {
            Self::dark()
        } else {
            Self::light_line()
        }
    }

    /// 按底色亮度挑一套 (单元格/状态口径)。
    pub(crate) fn for_cell<T: Theme>(th: &T) -> Self {
        if is_dark_background(th) {
            Self::dark()
        } else {
            Self::light_cell()
        }
    }
}

/// 底色算不算「暗」—— 阈值 0.5 落在两个内置主题的巨大空档里
/// (浅色底 `L` ≈ 0.93, 暗色底 ≈ 0.010)。
fn is_dark_background<T: Theme>(th: &T) -> bool {
    danqing::relative_luminance(th.background()) < 0.5
}

/// 日志级别着色: 行前 200 字节内找级别关键字 (日志行级别几乎都在行首)。
///
/// `th` **只用于未命中级别时的正文色** —— 级别色板本身是固定语义色, 不随主题走。
fn level_color<T: Theme>(line: &[u8], th: &T) -> Color {
    let head = &line[..line.len().min(200)];
    // 长词优先: FATAL 含 "AT" 之类子串碰撞无所谓 (都是错误级), 但 WARN 要先于 INFO 判
    let has = |pat: &[u8]| memchr::memmem::find(head, pat).is_some();
    let pal = LevelPalette::for_line(th);
    if has(b"FATAL") || has(b"ERROR") {
        pal.error
    } else if has(b"WARN") {
        pal.warn
    } else if has(b"DEBUG") || has(b"TRACE") {
        pal.trace
    } else {
        // 必须走 token: 这里原先写死近黑 `0.12` (= 浅色主题的正文色),
        // 修好双重 gamma 之后它在暗色背景上就是**真的近黑** —— 整列消失。
        th.text_primary()
    }
}

/// level 列单元格着色 (表格模式): INFO 也给蓝 —— 窄列色带是语义扫描线;
/// 原始模式整行着色的降噪策略 (INFO 走默认色) 不同, 两函数有意不共用。
fn level_cell_color<T: Theme>(v: &str, th: &T) -> Color {
    let b = v.as_bytes();
    let has = |pat: &[u8]| memchr::memmem::find(b, pat).is_some();
    let pal = LevelPalette::for_cell(th);
    if has(b"FATAL") || has(b"ERROR") {
        pal.error
    } else if has(b"WARN") {
        pal.warn
    } else if has(b"INFO") {
        pal.info
    } else if has(b"DEBUG") || has(b"TRACE") {
        pal.trace
    } else {
        th.text_primary() // 同 level_color: 写死近黑会在暗色下消失
    }
}

/// status 列按首数字分段: 2xx 绿 / 3xx 蓝 / 4xx 琥珀 / 5xx 红。
fn status_color<T: Theme>(v: &str, th: &T) -> Color {
    let pal = LevelPalette::for_cell(th);
    match v.as_bytes().first() {
        Some(b'2') => pal.ok,
        Some(b'3') => pal.info,
        Some(b'4') => pal.warn,
        Some(b'5') => pal.error,
        _ => th.text_primary(), // 同 level_color: 写死近黑会在暗色下消失
    }
}

/// 纯数字值判定 (含小数点/负号/千分位): 有资格右对齐。
fn is_numeric(v: &str) -> bool {
    let b = v.as_bytes();
    !b.is_empty()
        && b.iter().any(|c| c.is_ascii_digit())
        && b.iter()
            .all(|c| matches!(c, b'0'..=b'9' | b'.' | b'-' | b','))
}

/// 表格单元格语义配色: level/status 按值分段, 其余一律正文色。
/// (曾有时间戳列/hex 标识符降灰设计, 用户验收判死: 白底小字看不清;
/// klogg/LogViewPlus/Daucloud 三家竞品对 ts/req_id 均一视同仁用正文色。)
fn cell_color<T: Theme>(name: &str, v: &str, th: &T) -> Color {
    if name == "level" || name == "severity" {
        return level_cell_color(v, th);
    }
    if (name.contains("status") || name == "code")
        && v.as_bytes().first().is_some_and(u8::is_ascii_digit)
    {
        return status_color(v, th);
    }
    // 正文色走 token —— 写死近黑在暗色主题下与背景同值, 整列消失 (2026-09-13)。
    th.text_primary()
}

/// 一行可见窗口的命中几何 (选区 T3): `base_byte` = 左截断起点的解码字节偏移,
/// `offs` = 窗口内逐字符 (内容域 x_end, 字节_end)。只缓存可见窗口字符 ——
/// 用户点不到的不测, 超长行 (minified JSON) 单帧成本有界。
struct RowGeom {
    base_byte: usize,
    offs: Vec<(f32, usize)>,
}

/// 构建一行可见窗口的命中几何: 从 `base` (左截断字节) 起逐字符累计宽度,
/// 越过 `right_bound` (内容域 x) 即停 —— 右缘外字符用户点不到, 不测。
/// `start_x` = base 处的内容域 x (调用方由 scroll_trim 的 sub 换算, 免重复测量)。
fn measure_row_geom(
    texts: &mut TextBatch,
    raw: &str,
    base: usize,
    start_x: f32,
    right_bound: f32,
) -> RowGeom {
    let mut acc = start_x;
    let mut offs = Vec::new();
    for (rel, ch) in raw[base..].char_indices() {
        let end = base + rel + ch.len_utf8();
        acc += texts.measure(&raw[base + rel..end], FONT_SIZE);
        if acc > right_bound {
            break;
        }
        offs.push((acc, end));
    }
    RowGeom {
        base_byte: base,
        offs,
    }
}

/// 行锚定虚拟列表 (整窗唯一组件, 含搜索栏/过滤栏/表头/底栏状态行与滚动条)。
pub(crate) struct LogView {
    file: Option<Arc<LogFile>>,
    /// 是否已打开真实文件 (false = 无参启动空态, 画欢迎提示)。
    has_file: bool,
    /// Loading 占位文案 (async-open: 无旧文件 + job 在途; 空态分支改画打开进度)。
    loading_label: Option<(String, String)>,
    top_row: f64,
    /// 选中的显示行。
    selected: u64,
    /// 本组件是否持焦 (FocusIn / FocusOut 维护)。
    ///
    /// **三处高亮 (行选中 / 单元格选中 / 文本选区带) 全部 AND 上它** (T14/P19) ——
    /// 不是为了好看, 是为了让「看得见」与「复制得到」由**同一个因**决定:
    /// 框架的 Ctrl+C 只在持焦链路上派发 `Event::Copy`, 所以不持焦时那三样本来就
    /// 复制不到。原先它们照画, 于是屏幕上留着一段「看着是选中的、按 Ctrl+C 却
    /// 什么都不发生」的假象 —— 那正是 P19。
    focused: bool,
    /// 竖条拖拽态 (T17): `Some(grab)` = 正拖着, 值是按下点相对拇指顶的偏移。
    /// 抓在拇指中段拖动时必须保持这个偏移, 否则拇指会「吸」到指针下, 一按就跳。
    v_drag: std::cell::Cell<Option<f32>>,
    /// 横条拖拽态, 同上 (偏移是相对拇指**左端**)。
    h_drag: std::cell::Cell<Option<f32>>,
    /// 指针停在哪一根条上; `None` = 都没停。见 [`BarAxis`]。
    hover_bar: std::cell::Cell<Option<BarAxis>>,
    status: String,
    /// 底栏那行 `status` 是不是**错误** (P27)。2026-09-15 用户裁定「**常驻红**」——
    /// 不给它走 notice 通道: notice 有 4 秒消退期, 而「正则无效」是「你刚按的那下
    /// 没生效」, 不该自己消失 (用户原话)。与 notice 的 `Warn` 档共用 `danger()`,
    /// 但活在另一条通道上。真身是 `app.status_error`, 唯一写入点是
    /// `LogApp::set_status_error`。
    status_error: bool,
    /// 底栏瞬时提示 (M3): 与 `status` **分通道**, 各画各的 —— 常态信息
    /// `text_secondary()` / 提示 `text_primary()` / 警示 `danger()`, 见 paint。
    /// sync 时从 `app.notice` 读; **不得**再拼进 `status` (那会画两遍)。
    notice: Option<(String, crate::NoticeKind)>,
    mode: ViewMode,
    schema: Option<Arc<Schema>>,
    /// 过滤命中的文件行号 (升序); None = 全量。
    filtered: Option<Arc<Vec<u64>>>,
    // ---- 搜索态 (T5) ----
    /// 已应用模式原文 (变化检测) 与编译缓存 (sync 时按需重编译, paint 零成本)。
    search_pattern_src: Option<String>,
    search_re: Option<regex::bytes::Regex>,
    /// 命中文件行号表 (升序), 高亮成员判定走二分。
    search_hits: Option<Arc<Vec<u64>>>,
    /// 文件数据编码 (高亮前缀宽度测量须与行显示同一解码路径)。
    encoding: danqing::encoding::Encoding,
    /// 书签文件行号 (用户量级, 逐帧克隆无感)。
    bookmarks: std::collections::BTreeSet<u64>,
    // ---- 展开态 (jsonl-table T4) ----
    expanded: ExpandMap,
    sub_rows: std::collections::BTreeMap<u64, Vec<SubRow>>,
    // ---- 水平滚动 (T7, 纯视图态: 应用层不参与) ----
    /// 内容左缘偏移像素 (Cell: paint 只读, 事件写入, paint 防御性回钳)。
    x_offset: std::cell::Cell<f32>,
    /// 迄今见过的最大内容宽 (渲染时边测边长; 滚动范围的下界估计, 诚实边界:
    /// 未探索区域的宽度未知, 与编辑器「边走边长」同构)。
    max_seen: std::cell::Cell<f32>,
    // ---- 设置入口 (S2) ----
    /// 设置按钮更新角标显隐 (D1): **sync 填充** (每帧从 `app_update::hint()` 读),
    /// paint 只读字段 —— VersionRow 同款注入惯例, 测试可构造注入、不碰全局 publish。
    has_update_hint: bool,
    /// 设置按钮 hover 态 (event 写, paint 读)。
    settings_hover: std::cell::Cell<bool>,
    /// 设置按钮矩形 (paint 计算, event 用; Cell 跨 paint/event 共享)。
    settings_btn_rect: std::cell::Cell<Rect>,
    /// 鼠标悬停显示行 (u64::MAX = 无; event 写, paint 读)。
    hover_row: std::cell::Cell<u64>,
    /// 指针是否正落在**展开标识列**上 (P3)。与 `hover_row` 分开判: 光标停在行
    /// 文本上不该点亮标识。**这里不 parse 那一行** —— 「这一行到底有没有 glyph」
    /// 交给 paint 判 (它本来就逐可见行 `parse_line` 一次); 鼠标一动就解析整行是
    /// T17 明确修掉过的同类倒退 (那次是 `bars()` 的 `display_count()`)。
    hover_expand: std::cell::Cell<bool>,
    // ---- 文本选区 (T3, 仅原始模式) ----
    /// 文本选区 (锚点/光标点 = 显示行 + 解码字节偏移); None 或空 = 无选区。
    selection: Option<TextSelection>,
    /// 上次 sync 所见的展开态修订号 (M3): `app.expand_rev` 变化 = 显示行映射已变,
    /// 旧选区/(M4)单元格选中的 (显示行, 偏移) 指向错误的行 → 作废。
    last_expand_rev: u64,
    /// 单元格选中 (M4): (显示行, 列下标) —— 表格模式普通行双击产生,
    /// 矩形高亮 + Ctrl+C 复制完整值。存视图层 (与文本选区同层, 不进 LogApp)。
    selected_cell: Option<(u64, usize)>,
    /// 列区间缓存 (M4, paint 写 event 读): 可见列的 (x0, x1, 列下标),
    /// 绝对窗口 x —— 列宽靠 TextBatch 实测, event 侧重算不可能 (D2)。
    /// 非表格模式 = 空 (paint 每帧重置, 顺带清陈旧)。
    col_spans: std::cell::RefCell<Vec<(f32, f32, usize)>>,
    /// 按下待升级 (行, 字节, 屏幕位置): 拖超 CLICK_DIST 升级框选,
    /// 未超即抬起 = 单击 (不产空选区)。
    press: Option<(u64, usize, Point)>,
    /// 框选进行中 (press 已升级; release 落定选区)。
    dragging: bool,
    /// 上次按下 (时刻, 位置) —— 双击判定用。
    last_click: Option<(Instant, Point)>,
    /// 选区命中几何 (paint 写, event 读): 显示行 → 可见窗口逐字符偏移。
    /// event 无 TextBatch, 命中测试与渲染同源靠它 (TextInput char_offsets 同法)。
    row_geom: std::cell::RefCell<std::collections::BTreeMap<u64, RowGeom>>,
    /// 行号槽宽缓存 (paint 算, event 用; measure 只在 paint 可得)。
    gutter_w: std::cell::Cell<f32>,
    /// 组件矩形缓存 (paint 写, 窗口坐标): hit_area 供 set_by_click 聚焦用。
    /// 无它 focusable 形同虚设 —— hit_focusable 只认 hit_area (评审自查发现:
    /// 只加 focusable 不加 hit_area, 点击永不聚焦, Ctrl+C 链路断路)。
    area: std::cell::Cell<Rect>,
    /// 主题模式 (从 LogApp 同步)。
    theme: crate::config::AppTheme,
}

impl LogView {
    pub(crate) fn new() -> Self {
        Self {
            file: None,
            has_file: false,
            loading_label: None,
            top_row: 0.0,
            selected: 0,
            focused: false,
            v_drag: std::cell::Cell::new(None),
            h_drag: std::cell::Cell::new(None),
            hover_bar: std::cell::Cell::new(None),
            status: String::new(),
            status_error: false,
            has_update_hint: false,
            notice: None,
            mode: ViewMode::Raw,
            schema: None,
            filtered: None,
            search_pattern_src: None,
            search_re: None,
            search_hits: None,
            encoding: danqing::encoding::Encoding::Utf8,
            bookmarks: std::collections::BTreeSet::new(),
            expanded: ExpandMap::new(),
            sub_rows: std::collections::BTreeMap::new(),
            x_offset: std::cell::Cell::new(0.0),
            max_seen: std::cell::Cell::new(0.0),
            settings_hover: std::cell::Cell::new(false),
            settings_btn_rect: std::cell::Cell::new(Rect::default()),
            hover_row: std::cell::Cell::new(u64::MAX),
            hover_expand: std::cell::Cell::new(false),
            selection: None,
            last_expand_rev: 0,
            selected_cell: None,
            col_spans: std::cell::RefCell::new(Vec::new()),
            press: None,
            dragging: false,
            last_click: None,
            row_geom: std::cell::RefCell::new(std::collections::BTreeMap::new()),
            gutter_w: std::cell::Cell::new(GUTTER_MIN),
            area: std::cell::Cell::new(Rect::default()),
            theme: crate::config::AppTheme::Light,
        }
    }

    /// 表格模式 = 模式开关开 + 列定义在手 (构造保证同生同灭, 这里仍取交集防御)。
    fn table_mode(&self) -> bool {
        self.mode == ViewMode::Table && self.schema.is_some()
    }

    /// 顶部 chrome 高度: 过滤/搜索栏是独立 sibling (Column 上面), LogView 不含它;
    /// 表格模式只留表头, 原始模式无 chrome。
    fn chrome_top(&self) -> f32 {
        if self.table_mode() { HEADER_H } else { 0.0 }
    }

    /// 显示行来源 (全量恒等或过滤命中)。
    fn lines(&self) -> expand::Lines<'_> {
        match &self.filtered {
            Some(hits) => expand::Lines::Filtered(hits),
            None => expand::Lines::All {
                total: self.file.as_ref().map(|f| f.line_count()).unwrap_or(0),
            },
        }
    }

    /// 显示行数: 文件行数 + 展开子行数。
    fn display_count(&self) -> u64 {
        expand::display_count(self.lines(), &self.expanded)
    }

    /// 显示行 → (文件行, 子行偏移)。偏移 0 = 文件行本身。
    fn line_at(&self, row: u64) -> (u64, usize) {
        expand::file_line_at(row, self.lines(), &self.expanded).unwrap_or((0, 0))
    }

    /// 显示行的复制/分词文本 (M3): 普通行 = 解码原文; 展开子行 = 该子行
    /// `label = value` 串 (构造点 = [`sub_row_text`], paint 与行兜底同用)。
    /// 无文件/子行缺失 → 空串 (复制拼装对零宽切片本就跳过)。
    fn row_content(&self, row: u64) -> String {
        let Some(file) = self.file.as_ref() else {
            return String::new();
        };
        let (line_no, sub_off) = self.line_at(row);
        if sub_off > 0 {
            return self
                .sub_rows
                .get(&line_no)
                .and_then(|v| v.get(sub_off - 1))
                .map(sub_row_text)
                .unwrap_or_default();
        }
        file.line_lossy(line_no).into_owned()
    }

    /// 选区是否超复制上限 (R3, 2026-09-08 评审): 滚轮甩底可造出全文件选区,
    /// 无上限复制 = 逐行解码 + 逐行分配, UI 冻结分钟级 —— 对「1GB 不卡」
    /// 立身之本的产品是口碑级事故。超限 Ctrl+C 不动作 + 底栏提示。
    fn selection_over_limit(&self) -> bool {
        self.selection.as_ref().is_some_and(|s| {
            let ((r0, _), (r1, _)) = s.ordered();
            r1.saturating_sub(r0) + 1 > COPY_MAX_LINES
        })
    }

    /// 指针是否落在滚动条**可能出现**的窄带里 —— 只比坐标, 不算几何、不碰
    /// `display_count`。给 `CursorMoved` 做短路用 (见那里的注释)。
    ///
    /// 判据取「条能出现的位置」而非「条此刻存在」: 宁可多算一次, 不可漏掉 ——
    /// 条不存在时 `bars()` 会返回 `None`, 后面的逻辑本来就出不来结果。
    fn near_bar(area: Rect, list_bottom: f32, p: Point) -> bool {
        let right = area.origin.x + area.size.width - SCROLLBAR_HIT_W;
        p.x >= right || p.y >= list_bottom - SCROLLBAR_HIT_W
    }

    /// 两条滚动条的几何 (T17) —— 命中与拖拽**共用这一处**, 免得 event 里再推一遍
    /// `text_x`/`text_w`/`max_seen` 那串式子。
    ///
    /// `text_w` 与 paint 同式 (`text_right − text_x`), 但 event 里没有 `text_batch`
    /// 可量, 故取自 paint 缓存的 `text_w` (`Self::text_w()` 的真源是一致的)。
    fn bars(&self, area: Rect, list_h: f32) -> (Option<VScroll>, Option<HScroll>) {
        let rows_top = area.origin.y + self.chrome_top();
        let v = v_scroll(area, rows_top, list_h, self.display_count(), self.top_row);
        let text_x = self.text_x(area);
        let text_w = (Self::text_right(area) - text_x).max(1.0);
        let h = h_scroll(
            text_x,
            text_w,
            rows_top + list_h - SCROLLBAR_H,
            self.max_seen.get(),
            self.x_offset.get(),
        );
        (v, h)
    }

    /// 拇指取色 (T17 验收 ④): 常态 `border`, hover / 按住加深到 `text_secondary`
    /// —— 6px 的窄条不给反馈就是「摸不到」。用既有 token, 不新增色槽。
    fn thumb_color<T: Theme>(&self, th: &T, axis: BarAxis) -> Color {
        let dragging = match axis {
            BarAxis::Vertical => self.v_drag.get().is_some(),
            BarAxis::Horizontal => self.h_drag.get().is_some(),
        };
        if dragging || self.hover_bar.get() == Some(axis) {
            th.text_secondary()
        } else {
            th.border()
        }
    }

    /// 复制来源的三级 (T18)。**判据的唯一真身** —— `selected_text` (取文本) 与
    /// 复制回执 (取说明) 都从它出发, 于是「真复制了」与「说复制了」不可能分家。
    fn copy_source(&self) -> Option<CopySource> {
        if let Some(sel) = &self.selection {
            if !sel.is_empty() {
                if self.selection_over_limit() {
                    return None;
                }
                let ((r0, _), (r1, _)) = sel.ordered();
                return Some(CopySource::Text {
                    rows: r1.saturating_sub(r0) + 1,
                });
            }
        }
        if let Some((row, col_idx)) = self.selected_cell {
            if self.cell_value(row, col_idx).is_some() {
                return Some(CopySource::Cell);
            }
        }
        if self.has_file && self.selected < self.display_count() {
            let (line_no, sub_off) = self.line_at(self.selected);
            if sub_off > 0 {
                if self
                    .sub_rows
                    .get(&line_no)
                    .and_then(|v| v.get(sub_off - 1))
                    .is_some()
                {
                    return Some(CopySource::Row);
                }
            } else if self.file.is_some() {
                return Some(CopySource::Row);
            }
        }
        None
    }

    /// **画得出来**的文本选区: 非空 **且** 本组件持焦 (T14/P19)。
    ///
    /// 判据必须与「复制得到」同源 —— 见 `Self::focused` 的注释。**不要**把它当成
    /// `self.selection` 的过滤别名用在复制路径上: `selected_text` 是「复制得到」
    /// 那一侧, 那边**不**看焦点 (框架本来就只在持焦时才派发 `Event::Copy`,
    /// 再加一道就变成「持焦也复制不到」)。
    fn visible_selection(&self) -> Option<&TextSelection> {
        if !self.focused {
            return None;
        }
        self.selection.as_ref().filter(|s| !s.is_empty())
    }

    /// 展开标识区的命中判定与绘制位置 —— **S2 (2026-09-14)**: 原先
    /// paint (`area.origin.x + 2.0`) 与 event (`position.x - area.origin.x < EXPAND_W`)
    /// 两处各写同一个常量, 「同规则同常量」只靠注释维持。抽成单点, 两处同源。
    fn expand_glyph_x(area: Rect) -> f32 {
        area.origin.x + 2.0
    }

    /// 展开标识区命中: 表格模式且 x 落在 [area.origin.x, area.origin.x + EXPAND_W)。
    fn in_expand_glyph(&self, area: Rect, position: Point) -> bool {
        self.table_mode() && position.x - area.origin.x < EXPAND_W
    }

    /// 行文本区左键按下的选区处理 (T3)。双击 (300ms/4px, title_bar 先例) =
    /// 文本区左缘 (绝对窗口 x) = 展开标识区 + 行号槽 + 间距。
    /// **与 paint 同源** —— 原先 paint / 命中测试 / 按下分流各推一遍同一个式子,
    /// 三处任一漂了都会让「点得到的地方」与「画出来的地方」错开。
    fn text_x(&self, area: Rect) -> f32 {
        area.origin.x + EXPAND_W + self.gutter_w.get() + GUTTER_GAP
    }

    /// 文本区右缘。同 [`Self::text_x`]: paint 与滚动条命中必须同源
    /// (T17 的横条可拖区就压在这儿)。
    fn text_right(area: Rect) -> f32 {
        area.origin.x + area.size.width - SCROLLBAR_W - 6.0
    }

    /// 列表区内的相对 y → 显示行。**与 paint 的行锚定同源** (paint 逐行递增,
    /// event 侧只能由 y 反算, 两套算法必须给出同一个行号)。
    /// **不钳上界**: 列表区下方空白会算出越界行, 由各调用方自己挡 ——
    /// `hit_text` 靠几何缓存 (只含真实行), 单元格路径靠 [`Self::cell_value`]
    /// 的 `display_count` 守卫。
    fn row_at(&self, rel_y: f32) -> u64 {
        (self.top_row + f64::from(rel_y / ROW_HEIGHT)) as u64
    }

    /// 双击判定 (300ms / 4px, 沿用 `title_bar` 先例): 与上次按下在时间与位移
    /// 阈值内。文本双击与单元格双击**共用同一套规则** —— 原先两处各抄一份,
    /// 「同规则同常量」只靠注释维持。
    fn is_double_click(&self, now: Instant, position: Point) -> bool {
        self.last_click.is_some_and(|(t, p)| {
            now.duration_since(t).as_millis() <= DOUBLE_CLICK_MS
                && (position.x - p.x).abs() < CLICK_DIST
                && (position.y - p.y).abs() < CLICK_DIST
        })
    }

    /// 框架混合连接器分词整选 (M1); 单击 = 潜伏锚点 (拖超阈值才升级框选) 且清除旧选区。
    /// 调用方已判定: 左键 + 已打开文件 + px 在文本区 + (原始模式全行区 | 表格模式
    /// 仅展开子行, M3)。
    fn handle_text_press(&mut self, area: Rect, position: Point) {
        let now = Instant::now();
        let dbl = self.is_double_click(now, position);
        if dbl {
            if let Some((r, b)) = self.hit_text(area, position) {
                // M3: 文本来源 = 该行复制内容 (子行 = `label = value` 串) ——
                // 与命中几何/渲染同串 (三源一体), token 边界才不偏
                let text = self.row_content(r);
                let (s, e) = selection::token_at(&text, b);
                self.selection = Some(TextSelection::new((r, s), (r, e)));
            }
            self.press = None;
            self.dragging = false;
            self.last_click = None; // 三连击不链式放大, 重新开始计数
        } else {
            self.press = self.hit_text(area, position).map(|(r, b)| (r, b, position));
            self.dragging = false;
            self.selection = None;
            self.last_click = Some((now, position));
        }
    }

    /// 选区命中测试 (T3): 屏幕坐标 → (显示行, 解码字节偏移)。
    ///
    /// event 无 TextBatch, 字符级命中走 paint 缓存的逐字符几何 ([`Self::row_geom`],
    /// TextInput char_offsets 同法); 与渲染同一解码路径, 宽度天然一致。
    /// 返回 None = 该坐标不可选 (列表区外 / 未缓存行 —— 表格模式普通行不缓存,
    /// 展开子行 M3 起缓存可命中)。
    /// 按下路径由调用方挡 gutter 区 (px < text_x 不进选区); 拖动路径的
    /// gutter 内坐标 (content_x 为负) 归 base_byte = 可见窗口首字符 ——
    /// 水平滚动后 != 行首, 语义为「选到最左可见处」。缓存未覆盖的字符区间
    /// (视口右缘外) 归最后可见字符 —— 用户只能选中看得到的文本。
    /// **x_offset 分叉 (M3, D3)**: 子行不参与水平滚动 (paint 无 x_off),
    /// 其 content_x 不加 `x_offset`; 普通行加。
    fn hit_text(&self, area: Rect, pos: Point) -> Option<(u64, usize)> {
        let chrome_top = self.chrome_top();
        let list_h = (area.size.height - chrome_top - STATUS_HEIGHT).max(0.0);
        let rel_y = pos.y - area.origin.y - chrome_top;
        if !(0.0..list_h).contains(&rel_y) {
            return None;
        }
        let row = self.row_at(rel_y);
        let text_x = self.text_x(area);
        let (_, sub_off) = self.line_at(row);
        let content_x = if sub_off > 0 {
            pos.x - text_x
        } else {
            pos.x - text_x + self.x_offset.get()
        };
        let geom = self.row_geom.borrow();
        let g = geom.get(&row)?;
        // 首个 x_end > content_x 的字符即 caret 归属 (TextInput hit_to_index 同语义);
        // 全在左侧 → 末字符后; 全在右侧 (左缘点击) → base_byte。
        let n = g.offs.partition_point(|(x_end, _)| *x_end <= content_x);
        let byte = if n == 0 { g.base_byte } else { g.offs[n - 1].1 };
        Some((row, byte))
    }

    /// 列区间命中 (M4): 屏幕 x → 列下标。读 paint 缓存的绝对窗口坐标
    /// (D2 —— event 无 TextBatch, 列宽重算不可能)。滚出视口的列不在缓存
    /// = 点不到, 天然一致。
    fn col_idx_at(&self, pos: Point) -> Option<usize> {
        self.col_spans
            .borrow()
            .iter()
            .find(|(x0, x1, _)| (*x0..*x1).contains(&pos.x))
            .map(|(_, _, idx)| *idx)
    }

    /// 单元格完整值 (M4): 该行 parse 后取列字段经 `cell_display` —— 与显示
    /// 同源 (字符串裸值/其余紧凑), **不受列宽截断省略影响**。
    /// 行 parse 失败 / 无该字段 / 子行 / **显示行越界** → None
    /// (不产生选中, 也不复制)。
    fn cell_value(&self, row: u64, col_idx: usize) -> Option<String> {
        // 越界行守卫 (2026-09-14 审查 #1): 列表区下方空白处按下时 row 由 y 反算,
        // 可以是任意值, 而 `line_at` 对越界行走 `unwrap_or((0, 0))` 会**塌缩成
        // 文件第 0 行** —— 放行会造出「画面上无高亮 (srow 不等于任何可见 i)、
        // Ctrl+C 却复制第 0 行该列」的隐形选中。hit_text 走 row_geom 缓存
        // (paint 只缓存 < count 的行) 天然挡得住, 单元格路径读的是 col_spans
        // (纯 x 命中, 无行维度), 必须自己挡。
        if row >= self.display_count() {
            return None;
        }
        let (line_no, sub_off) = self.line_at(row);
        if sub_off > 0 {
            return None; // 子行无单元格 (走文本选区通道)
        }
        let col = self.schema.as_deref()?.columns.get(col_idx)?;
        let raw = self.file.as_ref()?.line(line_no);
        jsonl::parse_line(raw)?
            .get(col.name.as_str())
            .map(jsonl::cell_display)
    }

    /// 表格模式普通行的左键按下 (M4): 双击 (300ms/4px, 与文本双击同规则同常量)
    /// 且命中有值单元格 → 选中该格; 单击或双击落空 → 清单元格选中
    /// (行选中走调用方的 Msg::Select, 与此独立)。
    /// **任何单元格按下都作废文本选区** —— 新手势独占 (2026-09-14 实机:
    /// 展开块框选后双击单元格, 旧选区残留, Ctrl+C 复制的是旧选区)。
    fn handle_cell_press(&mut self, row: u64, position: Point) {
        self.selection = None;
        let now = Instant::now();
        let dbl = self.is_double_click(now, position);
        if dbl {
            self.selected_cell = self
                .col_idx_at(position)
                .and_then(|ci| self.cell_value(row, ci).map(|_| (row, ci)));
            self.last_click = None; // 三连击不链式, 重新开始计数 (与文本双击同)
        } else {
            self.selected_cell = None;
            self.last_click = Some((now, position));
        }
    }
}

/// 画一段可能超宽的文本 (截断补省略号), 返回是否截断。
/// 截断逻辑走 [`danqing::fit::fit_line`] (measure 闭包适配 TextBatch)。
fn fit_push(texts: &mut TextBatch, s: &str, max_w: f32, x: f32, baseline: f32, px: u16, c: Color) {
    let (shown, truncated) = danqing::fit::fit_line(s, max_w, |t| texts.measure(t, px));
    texts.push_text(shown, x, baseline, px, c);
    if truncated {
        let w = texts.measure(shown, px);
        texts.push_text("…", x + w, baseline, px, c);
    }
}

/// 水平偏移钳制 (T7): [0, 内容宽 - 视口宽]。独立成函数供单测。
fn clamp_x(x: f32, content_w: f32, viewport_w: f32) -> f32 {
    x.clamp(0.0, (content_w - viewport_w).max(0.0))
}

/// 展开子行的文本串 (`label = value`) —— **唯一构造点** (M3 三源一体)。
/// paint 子行分支 / [`LogView::row_content`] / 行兜底复制三处共用: 命中几何是按
/// 这个串测宽的, 复制也按它的字节偏移切片, 改格式只此一处 —— 否则任一处分叉
/// 都会让「双击选中的字符」与「复制出的内容」错位。
fn sub_row_text(r: &SubRow) -> String {
    format!("{} = {}", r.label, r.value)
}

impl Widget for LogView {
    fn sync(&mut self, state: &dyn Any) {
        let app = state
            .downcast_ref::<LogApp>()
            .expect("LogView 绑定状态类型不匹配");
        // 更新角标显隐 (D1): sync 填充, paint 只读字段 (VersionRow 注入惯例 ——
        // 测试可构造注入; 也免去 paint 每帧现查 hint() 的锁与字符串分配)。
        self.has_update_hint = crate::app_update::hint().is_some();
        // 选区失效守卫: 显示行→内容映射变化 (换文件/过滤变化/切模式/展开折叠) 时
        // 旧选区的 (显示行, 偏移) 不再对应原文, 必须作废 (含潜伏按下);
        // 追加 tail 换入新 Arc 同样触发 —— 保守清除胜过复制出错行。
        // 展开折叠走修订号 (M3, expand_rev): ExpandMap 无可比性, 逐帧 diff 不值。
        let file_changed = self
            .file
            .as_ref()
            .is_none_or(|f| !Arc::ptr_eq(f, &app.file));
        let filtered_changed = match (&self.filtered, &app.filtered) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        let expand_changed = app.expand_rev != self.last_expand_rev;
        self.last_expand_rev = app.expand_rev;
        if file_changed || filtered_changed || self.mode != app.mode || expand_changed {
            self.selection = None;
            self.press = None;
            self.dragging = false;
            self.selected_cell = None; // M4: 同守卫块, 同一作废理由
        }
        self.file = Some(Arc::clone(&app.file));
        self.has_file = app.has_file;
        self.loading_label = app.loading_label.clone();
        self.top_row = app.top_row;
        self.selected = app.selected;
        self.status = app.status.clone();
        self.status_error = app.status_error;
        self.notice = app.notice.clone();
        self.mode = app.mode;
        self.schema = app.schema.clone();
        self.filtered = app.filtered.clone();
        self.search_hits = app.search.as_ref().map(|n| Arc::clone(n.hits()));
        self.encoding = app.file.encoding();
        self.bookmarks = app.bookmarks.clone();
        self.expanded = app.expanded.clone();
        self.sub_rows = app.sub_rows.clone();
        self.theme = app.theme;
        // 模式变化才重编译 (正则编译 ms 级, 不能进 paint)。
        // **只认 app.search_pattern 这一串** —— 它就是搜索执行用的那串
        // (`build_search_pattern` 的产物), 故大小写敏感之类的 flag 自动同源,
        // 高亮不可能与命中集不一致。**勿在此另拼 pattern** (spec D8)。
        if self.search_pattern_src != app.search_pattern {
            self.search_pattern_src = app.search_pattern.clone();
            self.search_re = app
                .search_pattern
                .as_deref()
                .and_then(|p| regex::bytes::Regex::new(p).ok());
        }
    }

    fn layout(&mut self, constraints: Constraints, _texts: &mut TextBatch) -> Size {
        constraints.max()
    }

    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        self.area.set(area); // hit_area 供 set_by_click 聚焦 (窗口坐标)
        let Some(file) = &self.file else { return };
        let count = self.display_count();
        let table = self.table_mode();
        let th = self.theme.theme();

        // 背景 + 区域划分
        rects.push_rect(area, th.background(), 0.0);
        let chrome_top = self.chrome_top();
        let list_h = (area.size.height - chrome_top - STATUS_HEIGHT).max(0.0);
        let status_y = area.origin.y + chrome_top + list_h;

        // 行号槽宽: 按文件行数位数实测一次 (行号恒为文件真实行号, 与过滤无关)
        let digits = format!("{}", file.line_count()).len();
        let sample = "8".repeat(digits.max(4));
        let gutter_w = (texts.measure(&sample, AUX_FONT_SIZE) + 20.0).max(GUTTER_MIN);
        self.gutter_w.set(gutter_w); // 选区命中 (event 无 TextBatch) 同源
        // 展开标识区 + 行号槽 + 间距 (与命中测试/按下分流同一个式子)
        let text_x = self.text_x(area);
        let text_right = Self::text_right(area);
        let text_w = (text_right - text_x).max(1.0);
        // 水平偏移 (T7): paint 防御性回钳 (窗口变宽/内容变窄后 offset 可能越界)
        let x_off = clamp_x(self.x_offset.get(), self.max_seen.get(), text_w);
        self.x_offset.set(x_off);

        let line_h = texts.line_height(f32::from(FONT_SIZE));
        let baseline_off = (ROW_HEIGHT - line_h) / 2.0 + texts.ascent(f32::from(FONT_SIZE));
        let aux_line_h = texts.line_height(f32::from(AUX_FONT_SIZE));
        let aux_baseline_off =
            (ROW_HEIGHT - aux_line_h) / 2.0 + texts.ascent(f32::from(AUX_FONT_SIZE));
        let expand_line_h = texts.line_height(f32::from(EXPAND_FONT_SIZE));
        let row_baseline_off =
            (ROW_HEIGHT - expand_line_h) / 2.0 + texts.ascent(f32::from(EXPAND_FONT_SIZE));

        // 表格模式: 列布局 (列宽 = 采样字符宽实测 + 内边距, ≤16 列常量成本;
        // 水平滚动: 列区整体左移, 滚出左右缘的列整列不画)
        let mut cols: Vec<(f32, f32, &Column)> = Vec::new();
        // 列区间缓存 (M4): 可见列的 (x0, x1, 列下标) 绝对窗口 x, paint 写 event 读
        let mut spans: Vec<(f32, f32, usize)> = Vec::new();
        if table {
            let schema = self.schema.as_deref().expect("表格模式必有 schema");
            let mut x = text_x - x_off;
            let mut total_w = 0.0f32;
            for (col_idx, col) in schema.columns.iter().enumerate() {
                let w = texts.measure(&"8".repeat(col.width_chars), FONT_SIZE) + COL_PAD;
                total_w += w;
                if x + w <= text_x {
                    x += w;
                    continue; // 整列滚出左缘
                }
                if x >= text_right {
                    x += w;
                    continue; // 整列滚出右缘 (继续累计 total_w)
                }
                cols.push((x, w, col));
                spans.push((x, x + w, col_idx));
                x += w;
            }
            if total_w > self.max_seen.get() {
                self.max_seen.set(total_w);
            }
            // 表头: 淡灰底与数据区分层 + 底部 1px 线 (表头随列水平滚动, 左缘切断同单元格)
            let hy = area.origin.y;
            rects.push_rect(
                Rect::from_xywh(area.origin.x, hy, area.size.width, HEADER_H - 1.0),
                th.surface_variant(),
                0.0,
            );
            for (cx, cw, col) in &cols {
                let cell_x = cx + 8.0;
                let left_cut = (text_x - cell_x).max(0.0);
                let (shown, sub) =
                    danqing::fit::scroll_trim(&col.name, left_cut, |t| texts.measure(t, FONT_SIZE));
                let draw_x = cell_x + left_cut - sub;
                let max_w = (cx + cw - 8.0).min(text_right) - draw_x;
                if max_w > 0.0 {
                    fit_push(
                        texts,
                        shown,
                        max_w,
                        draw_x,
                        hy + baseline_off,
                        FONT_SIZE,
                        th.text_secondary(),
                    );
                }
            }
            rects.push_rect(
                Rect::from_xywh(area.origin.x, hy + HEADER_H - 1.0, area.size.width, 1.0),
                header_line(&th),
                0.0,
            );
        }
        // M4: 列区间入缓存 (非表格模式 = 空, 顺带清掉切模式前的陈旧缓存)
        self.col_spans.replace(spans);

        // 可见行窗口: 唯一有渲染成本的部分, 与文件大小无关
        let rows_top = area.origin.y + chrome_top;
        let rows_bottom = rows_top + list_h;
        // 选区 (T3): 命中几何随可见窗口逐帧重建; 行选中视觉不再因选区存在而让位
        // (T6, 2026-09-14 实机 M0 P10: 原先 `has_text_sel` 压制选中行底与 accent 竖条)
        self.row_geom.borrow_mut().clear();
        // 空态欢迎 (无参启动): 列表区居中两行提示; 行循环 count=0 本就不画
        if !self.has_file {
            let mid_y = rows_top + list_h / 2.0;
            // Loading (async-open): 文件名 + 进度行; 否则欢迎语
            let (title, hint) = match &self.loading_label {
                Some((name, progress)) => (name.as_str(), progress.as_str()),
                None => ("丹青日志 LogLens", "按 Ctrl+O 打开日志文件"),
            };
            let title_w = texts.measure(title, 16);
            texts.push_text(
                title,
                area.origin.x + (area.size.width - title_w) / 2.0,
                mid_y - 12.0,
                16,
                th.text_primary(),
            );
            let hint_w = texts.measure(hint, FONT_SIZE);
            texts.push_text(
                hint,
                area.origin.x + (area.size.width - hint_w) / 2.0,
                mid_y + 12.0,
                FONT_SIZE,
                th.text_secondary(),
            );
        }
        // 裁剪: 行内容不溢出到表头/底栏
        let clip = Rect::from_xywh(area.origin.x, rows_top, area.size.width, list_h);
        rects.push_clip(clip);
        texts.push_clip(clip);
        let first = self.top_row.floor() as u64;
        let frac = (self.top_row - first as f64) as f32;
        // 展开块只画到「可见的最后一行 + 2」—— 段可能远超视口, 见 `expand_block_rects`。
        let scan_limit = (first + (frac + list_h / ROW_HEIGHT).ceil() as u64 + 2).min(count);
        // 展开块底色是**最底层**: 整层先铺完, 下面行循环里的斑马/选中/hover 才压得住它。
        // 一段连续子行只出一个矩形 —— 逐行铺会在行交界留下抗锯齿的浅色缝。
        // **S1 (2026-09-14)**: 行 y 映射单点化 —— paint 的 `row_y` 与 event 的
        // `row_at` 原先各推一遍同一个式子 (rows_top + (i-first)*ROW_HEIGHT - frac*ROW_HEIGHT),
        // 三处任一漂了都会让「点得到的地方」与「画出来的地方」错开。
        // 现在 `row_at` 是 event 侧的唯一真身, paint 侧复用它反推 y。
        let row_y = |j: u64| {
            // 与 row_at 的逆运算: row_at(rel_y) = (top_row + rel_y/ROW_HEIGHT) as u64
            // → rel_y = (j - top_row) * ROW_HEIGHT; 绝对 y = rows_top + rel_y
            rows_top + (j as f64 - self.top_row) as f32 * ROW_HEIGHT
        };
        for block in expand_block_rects(
            first,
            scan_limit,
            row_y,
            |j| self.line_at(j).1 > 0,
            area.origin.x,
            area.size.width - SCROLLBAR_W,
        ) {
            rects.push_rect(block, expand_block_bg(self.theme), 0.0);
        }
        let mut i = first;
        loop {
            let y = row_y(i);
            // 行顶部超出可见区底部 → 停止
            if y >= rows_bottom || i >= count {
                break;
            }
            // 行完全在可见区上方 → 跳过 (滚动时首行可能部分溢出)
            if y + ROW_HEIGHT <= rows_top {
                i += 1;
                continue;
            }
            // 行底色层叠: 斑马纹 (仅表格模式 —— 宽表横向跟踪不串行;
            // 原始模式整行是连续文本, 斑马打断阅读, klogg 基准无斑马;
            // 奇数显示行, 绝对行号奇偶, 滚动时不游动)
            // → 命中行底 (hit_row_bg, 弱一档) → 选中 (底色 + 左侧 3px 强调条)
            // → hover (永画, 压过命中行底 —— 「指针现在在哪」必须盖过「搜索留下的痕迹」)
            let row_rect =
                Rect::from_xywh(area.origin.x, y, area.size.width - SCROLLBAR_W, ROW_HEIGHT);
            let (line_no, sub_off) = self.line_at(i);
            let is_sub_row = sub_off > 0;
            // 展开块底色已在上面整层铺完 (2026-09-13): 子行原先与真实行长得一模一样
            // (只差没有行号), 用户分不清「这坨是第 1 行展开的」还是「又是几行日志」。
            // 铺在最下层, 选中/hover 仍能压在上面 (那两态必须保持可见)。
            if table && i % 2 == 1 && !is_sub_row {
                // 斑马纹**不盖展开块**: 块要靠**单一底色**读作「一整块」,
                // 交替条纹会把它切碎、语义又糊回去。
                rects.push_rect(row_rect, row_band_bg(self.theme), 0.0);
            }
            // **三态不是互斥关系** (2026-09-14 实机 M0 P10): 原先
            // `if selected && !has_text_sel {…} else if hover {…}` 有两重压制 ——
            // ① 有文本选区时选中行的底与左 accent 竖条一起消失;
            // ② hover 与选中行共用 else-if, 选中行上 hover 也熄灭。
            // 修复 = 各自独立: 选中行**永画**, hover **永画**, 文本选区照旧只画区间带。
            //
            // 画序 (2026-09-14 实机 M0 P11): 命中行底**先**画, hover **后**画 ——
            // 命中行是「搜索留下的痕迹」, hover 是「指针现在在哪」, 后者必须压过前者。
            // 原先把命中行底画在 hover 之后且同用 `th.selection()`, 命中行上 hover 无反馈。
            if table {
                if let Some(hits) = &self.search_hits {
                    if hits.binary_search(&line_no).is_ok() {
                        rects.push_rect(
                            Rect::from_xywh(
                                area.origin.x,
                                y,
                                area.size.width - SCROLLBAR_W,
                                ROW_HEIGHT,
                            ),
                            hit_row_bg(self.theme),
                            0.0,
                        );
                    }
                }
            }
            if i == self.selected && self.focused {
                rects.push_rect(row_rect, th.selection(), 0.0);
                rects.push_rect(
                    Rect::from_xywh(area.origin.x, y, 3.0, ROW_HEIGHT),
                    th.accent(),
                    0.0,
                );
            }
            if i == self.hover_row.get() {
                // hover 走**独立通道**: 与斑马同色会让「悬停奇数行看不出、
                // 悬停偶数行三行连片」(用户实机报)。见 `row_hover_bg`。
                // 不再与选中行互斥 —— 拖框选经过选中行时 hover 仍可见。
                rects.push_rect(row_rect, row_hover_bg(self.theme), 0.0);
            }
            // 单元格选中高亮 (M4): 列区间的可见部分 (随 paint 缓存, 水平滚动自然跟随;
            // 与行选中可同存 —— 单元格是更具体的选中, 画在上层)
            if let Some((srow, scol)) = self.selected_cell {
                if srow == i && !is_sub_row && self.focused {
                    let spans = self.col_spans.borrow();
                    if let Some((x0, x1, _)) = spans.iter().find(|(_, _, ci)| *ci == scol) {
                        let hx0 = x0.max(text_x);
                        let hx1 = x1.min(text_right);
                        if hx1 > hx0 {
                            let cell_rect =
                                Rect::from_xywh(hx0, y + 2.0, hx1 - hx0, ROW_HEIGHT - 4.0);
                            // 两笔: 底色 (与行选中同 token, 见 cell_highlight_colors)
                            // + 描边 (唯一指出「具体哪一格」的一笔, 按钮焦点环同款原语)
                            let (fill, border) = cell_highlight_colors(&th);
                            rects.push_rect(cell_rect, fill, 2.0);
                            rects.push_rounded_border(cell_rect, border, 2.0, 1.5);
                        }
                    }
                }
            }
            // 展开子行: 缩进路径段 = 值, 无行号/列/搜索高亮
            if is_sub_row {
                if let Some(row) = self.sub_rows.get(&line_no).and_then(|v| v.get(sub_off - 1)) {
                    let indent = (row.depth as f32 - 1.0) * 16.0;
                    let s = sub_row_text(row);
                    let draw_x = text_x + indent;
                    if draw_x < text_right {
                        // 选区命中几何 (M3): 串由 `sub_row_text` 单点构造, 命中/渲染/
                        // 复制三源一体; 子行不参与水平滚动 (无 x_off) → 内容域起点
                        // = indent, hit_text 对子行相应不加 x_offset (D3 分叉)
                        let geom =
                            measure_row_geom(texts, &s, 0, indent, text_right - text_x + 64.0);
                        self.row_geom.borrow_mut().insert(i, geom);
                        // 文本选区区间 (M3): 与 raw 行同法 (measure 前缀→矩形),
                        // 唯二差异 = 串是子行串、无 x_off (子行不水平滚动)
                        if let Some(sel) = self.visible_selection() {
                            if let Some((b0, b1)) = selection::row_slice(sel, i, &s) {
                                let x0 = (draw_x + texts.measure(&s[..b0], FONT_SIZE)).max(text_x);
                                let x1 =
                                    (draw_x + texts.measure(&s[..b1], FONT_SIZE)).min(text_right);
                                if x1 > x0 {
                                    rects.push_rect(
                                        Rect::from_xywh(x0, y + 2.0, x1 - x0, ROW_HEIGHT - 4.0),
                                        th.selection(),
                                        2.0,
                                    );
                                }
                            }
                        }
                        fit_push(
                            texts,
                            &s,
                            text_right - draw_x,
                            draw_x,
                            y + baseline_off,
                            FONT_SIZE,
                            th.text_secondary(),
                        );
                    }
                }
                i += 1;
                continue;
            }
            // 行号 (文件真实行号; 书签行金色)
            let no = format!("{}", line_no + 1);
            let no_w = texts.measure(&no, AUX_FONT_SIZE);
            let bookmarked = self.bookmarks.contains(&line_no);
            if bookmarked {
                // 书签竖条: 行号槽左缘 3px 满行高。金色行号单兵作战时扫屏不可见
                // (用户实机「这功能体现在哪」), 竖条成列才能用余光扫到。
                // x=EXPAND_W: 与 x=0 的选中 accent 竖条错位, 选中+书签同存时
                // 两条都可见; 表格模式的 +/- 在 [0,EXPAND_W) 内, 不撞。
                rects.push_rect(
                    Rect::from_xywh(area.origin.x + EXPAND_W, y, 3.0, ROW_HEIGHT),
                    bookmark_color(self.theme),
                    0.0,
                );
            }
            let no_color = if bookmarked {
                bookmark_color(self.theme)
            } else {
                th.text_secondary()
            };
            texts.push_text(
                &no,
                area.origin.x + EXPAND_W + gutter_w - 10.0 - no_w,
                y + aux_baseline_off,
                AUX_FONT_SIZE,
                no_color,
            );
            let raw = file.line(line_no);
            // 原始模式: 命中行内区间高亮 (仅命中行跑 regex, 逐可见行恒定成本;
            // 前缀宽度测量与行显示同一解码路径, 宽度一致)
            if !table {
                if let (Some(re), Some(hits)) = (&self.search_re, &self.search_hits) {
                    if hits.binary_search(&line_no).is_ok() {
                        for m in re.find_iter(raw) {
                            let px0 = texts.measure(
                                &danqing::encoding::decode_line(self.encoding, &raw[..m.start()]),
                                FONT_SIZE,
                            );
                            let px1 = px0
                                + texts.measure(
                                    &danqing::encoding::decode_line(
                                        self.encoding,
                                        &raw[m.start()..m.end()],
                                    ),
                                    FONT_SIZE,
                                );
                            let x0 = (text_x + px0 - x_off).max(text_x);
                            let x1 = (text_x + px1 - x_off).min(text_right);
                            if x1 > x0 {
                                rects.push_rect(
                                    Rect::from_xywh(x0, y + 2.0, x1 - x0, ROW_HEIGHT - 4.0),
                                    th.selection(),
                                    2.0,
                                );
                            }
                        }
                    }
                }
            }
            if table {
                // 单元格: 逐可见行 serde_json parse (真 parser, 消除 memmem 内嵌误判),
                // 取顶层字段紧凑显示; level 列按级别着色;
                // 水平滚动: 左缘切断走 scroll_trim (亚字符平滑)
                let parsed = jsonl::parse_line(raw);
                // 展开开关: + 可展开未展开 / - 已展开 (表格模式专属, 独立展开区)
                // (字体是 GB2312 子集, 无 +/- 几何形, 用 ASCII +- 保可用)
                let expanded_here = self.expanded.is_expanded(line_no);
                let expandable = parsed.as_ref().is_some_and(jsonl::is_expandable);
                if expanded_here || expandable {
                    // 字符见 `GLYPH_EXPANDED` / `GLYPH_COLLAPSED` 上的说明
                    // (ASCII 约束是硬要求, 不是风格)。
                    let glyph = if expanded_here {
                        GLYPH_EXPANDED
                    } else {
                        GLYPH_COLLAPSED
                    };
                    // P3 的另一半: **可点却无任何 hover 指示**。悬停时换 accent 色。
                    // 只在真有 glyph 的行点亮 —— `hover_expand` 只说明指针在展开列里,
                    // 「这一行画不画 glyph」是 paint 才知道的事 (见字段注释)。
                    let hot = self.hover_expand.get() && i == self.hover_row.get();
                    texts.push_text(
                        glyph,
                        Self::expand_glyph_x(area),
                        y + row_baseline_off,
                        EXPAND_FONT_SIZE,
                        if hot { th.accent() } else { th.text_primary() },
                    );
                }
                for (cx, cw, col) in &cols {
                    let Some(v) = parsed
                        .as_ref()
                        .and_then(|p| p.get(col.name.as_str()))
                        .map(jsonl::cell_display)
                    else {
                        continue;
                    };
                    let color = cell_color(&col.name, &v, &th);
                    let cell_x = cx + 8.0;
                    let left_cut = (text_x - cell_x).max(0.0);
                    let right_edge = (cx + cw - 8.0).min(text_right);
                    // 数字右对齐 (量级可一眼比较); 列左缘被切断或文本截断时回落左对齐
                    if left_cut <= 0.0 && is_numeric(&v) {
                        let (shown, truncated) =
                            danqing::fit::fit_line(&v, right_edge - cell_x, |t| {
                                texts.measure(t, FONT_SIZE)
                            });
                        if !truncated {
                            let w = texts.measure(shown, FONT_SIZE);
                            texts.push_text(
                                shown,
                                right_edge - w,
                                y + baseline_off,
                                FONT_SIZE,
                                color,
                            );
                            continue;
                        }
                    }
                    let (shown, sub) =
                        danqing::fit::scroll_trim(&v, left_cut, |t| texts.measure(t, FONT_SIZE));
                    let draw_x = cell_x + left_cut - sub;
                    let max_w = right_edge - draw_x;
                    if max_w > 0.0 {
                        fit_push(
                            texts,
                            shown,
                            max_w,
                            draw_x,
                            y + baseline_off,
                            FONT_SIZE,
                            color,
                        );
                    }
                }
            } else {
                // 原始模式: 整行级别着色 + 水平左截断 (scroll_trim) + 右截断省略;
                // 行内容宽度边测边长 (max_seen, 水平滚动范围估计)
                let raw = file.line_lossy(line_no);
                let color = level_color(raw.as_bytes(), &th);
                let full_w = texts.measure(&raw, FONT_SIZE);
                if full_w > self.max_seen.get() {
                    self.max_seen.set(full_w);
                }
                let (shown, sub) =
                    danqing::fit::scroll_trim(&raw, x_off, |t| texts.measure(t, FONT_SIZE));
                // 选区命中几何 (T3): 起点宽 = x_off - sub (scroll_trim 已算),
                // 免一次 O(行长) 前缀测量; event 无 TextBatch, 靠这份缓存同源
                let base = raw.len() - shown.len();
                let geom = measure_row_geom(texts, &raw, base, x_off - sub, x_off + text_w + 64.0);
                self.row_geom.borrow_mut().insert(i, geom);
                // 文本选区区间 (T3): 与命中高亮同法 (measure 前缀→矩形);
                // 在循环末尾才画 = 与同批命中矩形交叠时选区优先
                if let Some(sel) = self.visible_selection() {
                    if let Some((b0, b1)) = selection::row_slice(sel, i, &raw) {
                        let x0 =
                            (text_x + texts.measure(&raw[..b0], FONT_SIZE) - x_off).max(text_x);
                        // 整行选中时复用刚算过的 full_w, 省一次 O(行长) 测量
                        let w1 = if b1 == raw.len() {
                            full_w
                        } else {
                            texts.measure(&raw[..b1], FONT_SIZE)
                        };
                        let x1 = (text_x + w1 - x_off).min(text_right);
                        if x1 > x0 {
                            // T9 (2026-09-14 实机 M0 P28): 超复制上限的选区带
                            // **拖选进行中即**换警示色 —— 原先只在 Ctrl+C 时才报
                            // (view.rs:1621), 用户拖到一半不知道已经越界。
                            let sel_color = if self.selection_over_limit() {
                                th.danger()
                            } else {
                                th.selection()
                            };
                            rects.push_rect(
                                Rect::from_xywh(x0, y + 2.0, x1 - x0, ROW_HEIGHT - 4.0),
                                sel_color,
                                2.0,
                            );
                        }
                    }
                }
                let draw_x = text_x - sub;
                fit_push(
                    texts,
                    shown,
                    text_right - draw_x,
                    draw_x,
                    y + baseline_off,
                    FONT_SIZE,
                    color,
                );
            }
            i += 1;
        }
        rects.pop_clip();
        texts.pop_clip();

        // 水平滚动条 (T7): 内容宽于视口才出现, 列表区底部 6px
        // 几何走 `h_scroll` 单点 (T17) —— 拖拽是它的逆运算, 两处各推一份会漂。
        if let Some(hb) = h_scroll(
            text_x,
            text_w,
            rows_top + list_h - 6.0,
            self.max_seen.get(),
            x_off,
        ) {
            rects.push_rect(
                Rect::from_xywh(hb.track_left, rows_top + list_h - 6.0, text_w, 6.0),
                th.divider(),
                3.0,
            );
            rects.push_rect(
                Rect::from_xywh(hb.thumb_x, rows_top + list_h - 6.0, hb.thumb_w, 6.0),
                self.thumb_color(&th, BarAxis::Horizontal),
                3.0,
            );
        }

        // 滚动条: 拇指尺寸 ∝ 视口/全文, 位置 ∝ top_row (显示行域)
        // 几何走 `v_scroll` 单点 (T17) —— 拖拽是它的逆运算, 两处各推一份会漂。
        if let Some(sb) = v_scroll(area, rows_top, list_h, count, self.top_row) {
            let track_x = area.origin.x + area.size.width - SCROLLBAR_W;
            rects.push_rect(
                Rect::from_xywh(track_x, sb.track_top, SCROLLBAR_W, list_h),
                th.divider(),
                3.0,
            );
            rects.push_rect(
                Rect::from_xywh(track_x, sb.thumb_y, SCROLLBAR_W, sb.thumb_h),
                self.thumb_color(&th, BarAxis::Vertical),
                3.0,
            );
        }

        // 底栏状态行 (打开耗时/过滤统计 = 截图弹药本体); 顶部 1px 线与列表区分层
        rects.push_rect(
            Rect::from_xywh(area.origin.x, status_y, area.size.width, 1.0),
            header_line(&th),
            0.0,
        );
        let sy =
            status_y + (STATUS_HEIGHT - aux_line_h) / 2.0 + texts.ascent(f32::from(AUX_FONT_SIZE));
        // M3 (2026-09-14 实机 M0 P27): notice 与常态信息**分通道** —— 原先整个
        // status 字符串一个颜色, 错误在视觉上不存在。
        //
        // 三档取色: 警示 `danger()` / 提示 `text_primary()` / 常态 `text_secondary()`。
        // 提示**不能**也用 `text_secondary()` —— 那就与常态同色, 等于没分通道
        // (T11 的验收判据正是「同屏可辨」)。常态那一档不再随有无 notice 变化:
        // 「降噪」既没有更暗的 token 可用, 又会让整行文字在提示出现时集体变一下。
        let notice_color = self.notice.as_ref().map(|(_, kind)| match kind {
            crate::NoticeKind::Warn => th.danger(),
            crate::NoticeKind::Info => th.text_primary(),
        });
        texts.push_text(
            &self.status,
            area.origin.x + 10.0,
            sy,
            AUX_FONT_SIZE,
            // P27: 错误态用 `danger()` —— 与打开耗时/过滤统计**不再同色**。
            // 这正是 P27 原文点名的毛病: 「错误在视觉上不存在」。
            if self.status_error {
                th.danger()
            } else {
                th.text_secondary()
            },
        );
        if let Some((notice_text, _)) = &self.notice {
            let status_w = texts.measure(&self.status, AUX_FONT_SIZE);
            texts.push_text(
                notice_text,
                area.origin.x + 10.0 + status_w + 16.0,
                sy,
                AUX_FONT_SIZE,
                notice_color.unwrap_or_else(|| th.text_primary()),
            );
        }
        // 设置入口 (S2): ⚙ 设置 — 位置计数左侧, hover 可辨
        let settings_label = "⚙ 设置";
        let settings_w = texts.measure(settings_label, AUX_FONT_SIZE);
        let settings_x = area.origin.x + area.size.width - SCROLLBAR_W - 10.0 - settings_w;
        let text_top = sy - texts.ascent(f32::from(AUX_FONT_SIZE));
        let settings_rect = settings_hit_rect(settings_x, settings_w, text_top, status_y);
        self.settings_btn_rect.set(settings_rect);
        let settings_color = if self.settings_hover.get() {
            th.text_primary()
        } else {
            th.text_secondary()
        };
        texts.push_text(
            settings_label,
            settings_x,
            sy,
            AUX_FONT_SIZE,
            settings_color,
        );
        // 更新角标 (SPEC-update-badge D1): 有新版时文字右上亮 6px accent 圆点, 无新版零痕迹。
        // 两轨通用 —— 显隐读 sync 填充的字段 (注入惯例), 不在 paint 里现查 hint()。
        paint_update_dot(
            rects,
            settings_x + settings_w,
            text_top,
            self.has_update_hint,
            th.accent(),
        );
        // 位置计数: 设置入口左侧 (空态无意义, 不画)
        let pos = if !self.has_file {
            String::new()
        } else if count == 0 {
            format!("行 0/{count}")
        } else {
            format!("行 {}/{count}", self.selected + 1)
        };
        let pos_w = texts.measure(&pos, AUX_FONT_SIZE);
        let pos_x = settings_x - 16.0 - pos_w;
        texts.push_text(
            &pos,
            pos_x.max(area.origin.x + 10.0),
            sy,
            AUX_FONT_SIZE,
            th.text_secondary(),
        );
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        let chrome_top = self.chrome_top();
        let list_h = (area.size.height - chrome_top - STATUS_HEIGHT).max(0.0);
        match event {
            // hover 跟踪 (设置按钮 + 列表行): 矩形已含绝对坐标 (paint 计算)
            Event::CursorMoved(position) => {
                self.settings_hover
                    .set(self.settings_btn_rect.get().contains(*position));
                // T17: 拖拽跟手 —— 拇指顶 = 指针 − 抓握偏移, 再走逆运算得 top_row。
                // 夹取在 `top_row_at` / `x_offset_at` 里 (验收 ②), 故拖出轨道也不越界。
                //
                // **先按位置短路, 再算几何**: `bars()` 要 `display_count()`, 而它在
                // 过滤态是 O(命中行数) 的 —— 鼠标一动就付一次, 对「1GB 不卡」是实打实
                // 的倒退 (宽过滤下命中数百万行)。指针不在条可能出现的窄带里就整个跳过。
                // 拖拽中不能跳 (指针会离开窄带), 故先看拖拽态。
                let dragging = self.v_drag.get().is_some() || self.h_drag.get().is_some();
                if !dragging
                    && !Self::near_bar(area, area.origin.y + chrome_top + list_h, *position)
                {
                    self.hover_bar.set(None);
                } else {
                    let (vbar, hbar) = self.bars(area, list_h);
                    self.hover_bar.set(
                        vbar.filter(|s| s.hit.contains(*position))
                            .map(|_| BarAxis::Vertical)
                            .or_else(|| {
                                hbar.filter(|s| s.hit.contains(*position))
                                    .map(|_| BarAxis::Horizontal)
                            }),
                    );
                    if let (Some(grab), Some(sb)) = (self.v_drag.get(), vbar) {
                        let ty = position.y - grab;
                        msgs.push(Box::new(Msg::ScrollTo {
                            top: sb.top_row_at(ty),
                            at_bottom: sb.at_bottom(ty),
                        }));
                        self.hover_row.set(u64::MAX); // 拖条时不该同时高亮行 (两套反馈别打架)
                        return EventResult::Consumed;
                    }
                    if let (Some(grab), Some(hb)) = (self.h_drag.get(), hbar) {
                        // 横滚量是**视图局部状态** (`x_offset`), 不经应用层 —— 与横条
                        // 走 `Msg::ScrollTo` 的竖条不同 (横向偏移本来就不进 LogApp)。
                        self.x_offset.set(hb.x_offset_at(position.x - grab));
                        self.hover_row.set(u64::MAX);
                        return EventResult::Consumed;
                    }
                }
                let rel_y = position.y - area.origin.y - chrome_top;
                let in_list = (0.0..list_h).contains(&rel_y);
                if in_list {
                    self.hover_row.set(self.row_at(rel_y));
                } else {
                    self.hover_row.set(u64::MAX);
                }
                // P3: 展开标识可点, 原先悬停时屏上零反馈。只认最左那 `EXPAND_W`
                // 一列 (`in_expand_glyph` 内含 table_mode 判定), 且指针得真在列表里
                // —— 列表下方空白反算出的越界行不该点亮任何东西。
                self.hover_expand
                    .set(in_list && self.in_expand_glyph(area, *position));
                // 框选跟手 (T3): 按下未抬起期间, 超阈值即升级/更新选区;
                // 命中失败 (拖出列表/不可选行) 冻结 caret 在最后有效点
                if let Some((arow, abyte, pos0)) = self.press {
                    let moved = (position.x - pos0.x).abs() > CLICK_DIST
                        || (position.y - pos0.y).abs() > CLICK_DIST;
                    if self.dragging || moved {
                        self.dragging = true;
                        if let Some((crow, cbyte)) = self.hit_text(area, *position) {
                            self.selection = Some(TextSelection::new((arow, abyte), (crow, cbyte)));
                        }
                    }
                }
                EventResult::Ignored // 不消费, 让其他组件也能响应 hover
            }
            Event::CursorLeft => {
                self.settings_hover.set(false);
                self.hover_row.set(u64::MAX);
                // T17: 第三个缓存也要清 —— 漏了它, 指针甩出窗口后拇指会保持
                // 加深态、`cursor_icon` 仍返回手型, 直到下一次进窗才复位。
                self.hover_bar.set(None);
                self.hover_expand.set(false); // P3: 同第三个缓存, 离窗必须一起清
                // 按下未拖动就离窗 = 放弃潜伏选区; 框选中离窗保留
                // (窗口最大化下边缘拖出是常态, 回窗继续跟手)
                if !self.dragging {
                    self.press = None;
                }
                EventResult::Ignored
            }
            Event::MouseWheel { delta, shift, .. } => {
                // 横滚源 (T7): 触控板直接给 delta.0; 否则 Shift+纵滚
                // (MouseWheel 修饰键是 danqing 打磨寄生新增, 联动改动两仓待提交)
                let dx = if delta.0 != 0.0 {
                    delta.0
                } else if *shift {
                    delta.1
                } else {
                    0.0
                };
                if dx != 0.0 {
                    // event 无 TextBatch, 视口宽用最小行号槽近似; 钳制目标随 max_seen 生长
                    let viewport_w =
                        (area.size.width - GUTTER_MIN - GUTTER_GAP - SCROLLBAR_W - 16.0).max(1.0);
                    self.x_offset.set(clamp_x(
                        self.x_offset.get() - dx * 40.0,
                        self.max_seen.get(),
                        viewport_w,
                    ));
                    return EventResult::Consumed;
                }
                // 与 danqing Scrollable 同向: delta.1 > 0 = 向上滚。
                // 换算走 `crate::wheel_rows` 单点 (T17 收口) —— 与「未认领的滚轮」
                // 那一路共用, 否则在侧栏滚和在这里滚会不是一个手感。
                msgs.push(Box::new(Msg::ScrollRows(crate::wheel_rows(delta.1))));
                EventResult::Consumed
            }
            Event::MouseInput {
                pressed: true,
                position,
                button,
                ..
            } => {
                // 只认左键 (T15/P29): 原先不筛 button, 右键/中键与左键**同效**
                // (选中行 / 开设置卡)。缺陷不在「右键没有菜单」—— 界面从未暗示
                // 右键能做什么, 所以右键无反应不违第 3 问; 缺陷在**左键的语义被
                // 一个没有任何 affordance 承诺的手势触发了**, 那是实打实的误导。
                //
                // 返回 `Ignored` 而不是 `Consumed`: 我们确实什么都没做, 不冒充
                // 「已认领」; 顺带把右键事件留给应用层 —— 将来要加右键菜单
                // (ROADMAP) 时这里不用再改一次。
                // 附注: 框架的 `set_by_click` 在**任何**按下的都会跑
                // (`handler.rs:973-977`), 右键也会按位置改焦点 —— 那是框架的
                // 归属, 不属本项。
                if *button != MouseButton::Left {
                    return EventResult::Ignored;
                }
                // 设置按钮点击 (S2)
                if self.settings_btn_rect.get().contains(*position) {
                    msgs.push(Box::new(Msg::OpenSettings));
                    return EventResult::Consumed;
                }
                // 滚动条按下 (T17)。在行命中**之前**: 条压在列表右缘/底缘之上,
                // 先判条才不会被行抢走 (条只有 6px 宽, 抢走就再也抓不到)。
                // 竖条先判只是**习惯性**的次序 —— 两者的命中带在 x 上只在
                // `width - SCROLLBAR_HIT_W` 这一条零宽边界相接, 并不真的重叠
                // (横条向下延伸后才如此; 别以为这里有一场优先级竞争)。
                let (vbar, hbar) = self.bars(area, list_h);
                if let Some(sb) = vbar.filter(|s| s.hit.contains(*position)) {
                    // 抓在拇指上保持抓握点; 抓在轨道空白处则让拇指心对齐指针
                    // (平台惯例, 且这样「点一下就跳过去」而不是要拖两下)
                    let on_thumb = position.y >= sb.thumb_y && position.y < sb.thumb_y + sb.thumb_h;
                    let grab = if on_thumb {
                        position.y - sb.thumb_y
                    } else {
                        sb.thumb_h / 2.0
                    };
                    self.v_drag.set(Some(grab));
                    let ty = position.y - grab;
                    msgs.push(Box::new(Msg::ScrollTo {
                        top: sb.top_row_at(ty),
                        at_bottom: sb.at_bottom(ty),
                    }));
                    return EventResult::Consumed;
                }
                if let Some(hb) = hbar.filter(|s| s.hit.contains(*position)) {
                    let on_thumb = position.x >= hb.thumb_x && position.x < hb.thumb_x + hb.thumb_w;
                    let grab = if on_thumb {
                        position.x - hb.thumb_x
                    } else {
                        hb.thumb_w / 2.0
                    };
                    self.h_drag.set(Some(grab));
                    self.x_offset.set(hb.x_offset_at(position.x - grab));
                    return EventResult::Consumed;
                }
                let rel_y = position.y - area.origin.y - chrome_top;
                // **「列表矩形之内、真实行数之外」= 末行下方空白** (P20)。
                // 2026-09-15 用户实机报「还是没看到出声」: 原判据只问 `rel_y` 落没
                // 落在矩形里, 于是点空白被当成「点中了一个越界行」——
                // `Msg::Select(越界行)` 在 app 侧被 `row < display_count` 挡掉
                // (`main.rs:1288`), 于是**既不发生什么也不出声**, 而这里照旧返回
                // `Consumed`。正是 P20 要消灭的那类沉默。
                // **行数判据必须带上, 且只能排在 `rel_y` 之后**: `rel_y` 为负时
                // `row_at` 里的 f64→u64 强转会**饱和到 0**, 单看行数会把列表上方的
                // 点击误判成第 0 行。
                if (0.0..list_h).contains(&rel_y) && self.row_at(rel_y) < self.display_count() {
                    let row = self.row_at(rel_y);
                    // 行首 +/- 展开开关区 (左 20px, 表格模式); 其余点击选中
                    // S2: 命中判定与绘制位置同源 (`expand_glyph_x` / `in_expand_glyph`)
                    let in_glyph = self.in_expand_glyph(area, *position);
                    if in_glyph {
                        // M3 (2026-09-14 实机 M0 P22): 表格**无 glyph 的行**点行首
                        // 展开区照样发 `ToggleExpand` → 零反应。现在校验可展开性,
                        // 不可展开则说清为什么。
                        let file_line = self.line_at(row).0;
                        let raw = self.file.as_ref().map(|f| f.line(file_line));
                        let expandable = raw.as_ref().is_some_and(|r| {
                            jsonl::parse_line(r)
                                .as_ref()
                                .is_some_and(jsonl::is_expandable)
                        });
                        if expandable {
                            msgs.push(Box::new(Msg::ToggleExpand(row)));
                        } else {
                            msgs.push(Box::new(Msg::Notice(
                                "本行无嵌套可展".into(),
                                crate::NoticeKind::Info,
                            )));
                        }
                    } else {
                        msgs.push(Box::new(Msg::Select(row)));
                        let text_x = self.text_x(area);
                        let sub_row = self.line_at(row).1 > 0;
                        // 文本选区 (T3/M3): 仅左键 + 已打开文件 + (原始模式全行区
                        // | 表格模式仅展开子行); 右键不清选区/不污染双击判定
                        // (评审 O1); 左键落 gutter = 仅行选中并清选区
                        if *button == MouseButton::Left
                            && (!self.table_mode() || sub_row)
                            && self.has_file
                        {
                            if position.x >= text_x {
                                self.handle_text_press(area, *position);
                            } else {
                                self.selection = None;
                            }
                            self.selected_cell = None; // M4: 文本选区动作清单元格选中
                        } else if *button == MouseButton::Left
                            && self.table_mode()
                            && self.has_file
                            && !sub_row
                            && position.x >= text_x
                        {
                            // 单元格双击 (M4): 表格模式普通行的文本区左键
                            self.handle_cell_press(row, *position);
                        } else if *button == MouseButton::Left && self.table_mode() {
                            // 表格普通行的 gutter/行号区单击: 也清单元格选中
                            // (单击他处 = 放弃, 与文本选区的 gutter 惯例一致)
                            self.selected_cell = None;
                        }
                    }
                    EventResult::Consumed
                } else {
                    // M3 (2026-09-14 实机 M0 P20): 点列表区**末行下方空白**被吞
                    // 时说清为什么 —— 原先 `Ignored` 静默, 用户不知道是没点中
                    // 还是程序没响应。
                    // 现在两个来源共用这一支: ① 落在列表矩形**之外** (状态栏那一带);
                    // ② 落在矩形**之内但超出真实行数** (「末行下方空白」的本体 ——
                    // 2026-09-15 用户实机报它没出声, 因为原先只判了 ①)。
                    msgs.push(Box::new(Msg::Notice(
                        "此处无行".into(),
                        crate::NoticeKind::Info,
                    )));
                    EventResult::Ignored
                }
            }
            Event::MouseInput {
                pressed: false,
                button: MouseButton::Left,
                ..
            } => {
                // T17: 拖条收手。与文本选区各走各的 —— 滚动条按下时压根没进
                // `self.press`, 两条状态机不会互相污染。
                if self.v_drag.get().is_some() || self.h_drag.get().is_some() {
                    self.v_drag.set(None);
                    self.h_drag.set(None);
                    return EventResult::Consumed;
                }
                // 左键抬起: 框选落定 / 潜伏按下作废 (单击不产选区)。
                // 引擎指针捕获保证拖出本区域的抬起也路由到此 (danqing R1)。
                if self.press.is_some() || self.dragging {
                    self.press = None;
                    self.dragging = false;
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            Event::Copy => {
                // 框架 Ctrl+C 链路 (handler → 焦点路径 → selected_text 写 arboard)。
                // 有内容才消费; 空选区 Ignored = 剪贴板保持不动。
                // 超限选区 (R3): 不复制并底栏提示 (防逐行解码冻结 UI)。
                //
                // T18 (P17): 判据换 `copy_source` 而不是 `selected_text().is_some()` ——
                // 后者会把整段文本**先取出来再丢掉** (框架随后还要再取一次),
                // 十万行选区就是 16MB 白做两遍。顺带这条路径成了回执的落点:
                // 返 `Consumed` 的**充要条件**就是框架接下来确实会写剪贴板。
                if let Some(src) = self.copy_source() {
                    msgs.push(Box::new(Msg::Notice(
                        copy_receipt(src),
                        crate::NoticeKind::Info,
                    )));
                    EventResult::Consumed
                } else if self.selection_over_limit() {
                    msgs.push(Box::new(Msg::Notice(
                        format!("选区超 {} 万行未复制 (防冻结)", COPY_MAX_LINES / 10000),
                        crate::NoticeKind::Warn,
                    )));
                    EventResult::Ignored
                } else {
                    EventResult::Ignored
                }
            }
            // 焦点态 (T14/P19): 三处高亮的**唯一**开关, 见 `Self::focused`。
            // 原先这两支落到 `_ => Ignored`, 于是「谁持焦」这个信息根本没进组件。
            Event::FocusIn => {
                self.focused = true;
                EventResult::Consumed
            }
            Event::FocusOut => {
                self.focused = false;
                EventResult::Consumed
            }
            Event::Key {
                key: Key::Named(NamedKey::Escape),
                pressed: true,
                ..
            } => {
                // Esc = 放弃当前手势: 清文本选区 / 单元格选中 / **潜伏按下**
                // (潜伏按下也算 —— 否则按住左键中途 Esc 再继续拖仍会升级成框选);
                // 全无 → Ignored (框架清焦, 现状)
                if self.selection.as_ref().is_none_or(|s| s.is_empty())
                    && self.selected_cell.is_none()
                    && self.press.is_none()
                    && !self.dragging
                {
                    EventResult::Ignored
                } else {
                    self.selection = None;
                    self.selected_cell = None;
                    self.press = None;
                    self.dragging = false;
                    EventResult::Consumed
                }
            }
            _ => EventResult::Ignored,
        }
    }

    /// 持焦 = 接入框架焦点剪贴板链路 (Ctrl+C → Event::Copy → selected_text)。
    /// 持焦后未消费的键经 `App::propagate_unhandled_keys` 回退应用层,
    /// 应用级导航 (j/k/翻页/`/`/b) 不失灵 (danqing T1 opt-in, LogApp 已开启)。
    fn focusable(&self) -> bool {
        true
    }

    fn focus_id(&self) -> Option<&'static str> {
        Some("log-view")
    }

    /// 命中区域 = 组件全矩形 (窗口坐标, paint 缓存)。
    /// set_by_click 的 hit_focusable 只认 hit_area —— 无此实现则
    /// 点击永不聚焦, focusable/Ctrl+C 链路全断 (TextInput 同法)。
    fn hit_area(&self) -> Option<Rect> {
        Some(self.area.get())
    }

    /// 与 FocusOut 同语义 (T14/P19)。
    ///
    /// **当前组件树里没有调用点** —— 框架只在 `overlay` / `multi_panel` / `tabs`
    /// 三处调 `reset_focus`, 而 `LogView` 既不在设置卡的 `Overlay` 内容里、也不在
    /// 任何面板里, 收不到这一发。留着它是**接口完整性**: `focused` 一旦与真实焦点
    /// 脱钩, 屏上就会留下「看着选中、按 Ctrl+C 却没反应」的假象 (正是 P19),
    /// 而本组件哪天被放进面板, 没有它就会踩这个坑。
    /// (本批第一版把注释写成「面板隐藏时收不到 FocusOut」, 那是把设想当成了事实。)
    fn reset_focus(&mut self) {
        self.focused = false;
    }

    /// 指针停在滚动条上 (T17) 或展开标识列上 (P3) → 手型。
    ///
    /// 注意这是**整节点**表态的 API (`mod.rs:181-191`): 它没有位置参数, 于是
    /// 「只有条上/标识上才手型」只能靠 `CursorMoved` 缓存的位置 (`hover_bar` /
    /// `hover_expand`) 判断。框架每帧重算光标, 与那两份缓存同频, 不会用到过期位置。
    fn cursor_icon(&self) -> Option<CursorIcon> {
        if self.hover_bar.get().is_some() || self.hover_expand.get() {
            Some(CursorIcon::Pointer)
        } else {
            None
        }
    }

    /// 当前可复制文本, 三级优先 (2026-09-14 翻案旧 spec「Ctrl+C 只认文本选区」):
    /// ① 非空且不超限的文本选区 (跨行 `\n` 拼接, 逐行走 [`Self::row_content`] —
    ///   普通行原文、展开子行供子行串, raw 模式跨界框选混排自然成立);
    /// ② 单元格选中 (M4) → 该字段完整值 (不受列宽截断省略影响; 选中后行数据
    ///   失效则防御性落到 ③);
    /// ③ 行选中 (**两模式统一**) → [`Self::row_content`]: 普通行 = 解码原文
    ///   (表格模式同样拿完整原文, 不受单元格截断省略影响),
    ///   子行 = 该子行 `label = value` 串 (**#5 展开块 Ctrl+C 的落点**);
    ///   子行数据缺失 → None (不复制空串冒充)。
    /// 全无 → None (Ctrl+C 不动作, 剪贴板不动)。
    ///
    /// **「选哪一级」不在这里判** —— 走 [`Self::copy_source`], 与复制回执同源。
    /// 本函数只负责把选中的那一级**取成字符串** (那两处 `?` 在 `copy_source`
    /// 返回 `Some` 的前提下不可能落空, 留着只为 `Option` 收口)。
    fn selected_text(&self) -> Option<String> {
        match self.copy_source()? {
            CopySource::Text { .. } => {
                let sel = self.selection.as_ref()?;
                Some(selection::copy_text(sel, &|row| self.row_content(row)))
            }
            CopySource::Cell => {
                // 该列水平滚出视口时不高亮但仍复制 —— 与行选中「滚出屏幕仍能复制」
                // 同一语义 (选中不因视口移动而失效; col_spans 只含可见列)
                let (row, col_idx) = self.selected_cell?;
                self.cell_value(row, col_idx)
            }
            CopySource::Row => {
                let (line_no, sub_off) = self.line_at(self.selected);
                if sub_off > 0 {
                    self.sub_rows
                        .get(&line_no)
                        .and_then(|v| v.get(sub_off - 1))
                        .map(sub_row_text)
                } else {
                    Some(self.file.as_ref()?.line_lossy(line_no).into_owned())
                }
            }
        }
    }
}

/// 栏内水平内边距。
const BAR_PAD_X: f32 = 10.0;
/// 前缀标签 ("过滤:"/"搜索:") 与输入区间隙。
const BAR_LABEL_GAP: f32 = 8.0;
/// 栏顶部内偏移 (视觉下沉, 避紧贴标题栏底边)。
const BAR_TOP_OFFSET: f32 = 3.0;
/// 栏持焦时的**键义提示** (P33)。
///
/// 栏持焦后 `Space`=输入空格、`Home/End`=移光标 —— 与失焦时 (翻页 / 跳首末)
/// 语义相反, 而 `↑↓`/`PgUp`/`PgDn` 仍滚列表。三种键三种去向, 不说就没人知道。
/// 这几个键**不是被拒绝**, 是被输入框收下了 —— 所以这里写「归谁」而不是
/// 「为什么不行」, 也就不能走 M3 的 notice 通道: 那会每次打空格都在底栏刷一条。
const BAR_KEY_HINT: &str = "↑↓ 滚列表 · Space/Home/End 归输入框";
/// 键义提示与输入区之间的间隙。
const BAR_HINT_GAP: f32 = 16.0;
/// 栏持焦时底边**焦点线**的高 (P6; 颜色取 `Theme::accent()`, 见 `Bar::paint`)。
/// 输入框是 `chromeless` 的, 不自绘焦点描边, 持焦的唯一证据原先只有**半周期闪烁**
/// 的 caret (熄灭那半周期里零指示)。定死 2px 且画在栏自己的 32px 之内 —— 不挤动
/// 任何已有几何。
const BAR_FOCUS_LINE: f32 = 2.0;
/// 过滤栏空态占位 (未应用过滤时; 应用后换成 "已应用: ..." 提示, 故须可复原)。
const FILTER_PLACEHOLDER: &str =
    "输入如 level=ERROR status=50* (AND · 尾缀 * 前缀通配) · Enter 应用 · Esc 清除 · Ctrl+T 切回";
/// 搜索栏空态占位 (同上, 应用后换成查询词提示)。
const SEARCH_PLACEHOLDER: &str = "输入正则 · Enter 应用 · Esc 关闭 (GBK/Latin-1 文件退化为字面量)";

/// 过滤占位文字: 已应用 → 提示词; 未应用 (含 Esc 清除后) → 复原空态文案。
fn filter_placeholder_text(applied: &str) -> String {
    if applied.is_empty() {
        FILTER_PLACEHOLDER.to_string()
    } else {
        format!("已应用: {applied} · Esc 清除 · Ctrl+T 切回")
    }
}

/// 搜索占位文字, 语义同上。
fn search_placeholder_text(query: &str) -> String {
    if query.is_empty() {
        SEARCH_PLACEHOLDER.to_string()
    } else {
        format!("{query} · Enter 下一命中 · Shift+Enter 上一 · Esc 关闭")
    }
}

/// 复制来源的三级 (T18)。与 [`LogView::copy_source`] 一一对应 ——
/// `selected_text` 取文本、回执取说明, 两条路读的是同一个判定。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CopySource {
    /// 文本选区, 跨 `rows` 个显示行。
    Text { rows: u64 },
    /// 单元格选中 (表格模式)。
    Cell,
    /// 行选中 (含展开子行)。
    Row,
}

/// 复制**成功回执**文案 (P17/T18)。
///
/// 中性色走 M3 的 notice 通道 (与警示色分开); 说「复制了什么」而不只是「已复制」——
/// 三级来源对用户的含义完全不同 (一段选区 / 一格 / 一整行)。
fn copy_receipt(src: CopySource) -> String {
    match src {
        CopySource::Text { rows } => format!("已复制选区 {rows} 行"),
        CopySource::Cell => "已复制单元格".to_string(),
        CopySource::Row => "已复制该行".to_string(),
    }
}

/// 「清空输入」绑定闭包: 从应用状态读 clear revision。
type ClearBinding = Box<dyn Fn(&dyn Any) -> u64>;

/// 当前生效的输入角色。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ActiveBar {
    /// 表格模式过滤栏。
    Filter,
    /// 原始模式 + 搜索栏开启。
    Search,
    /// 无栏。
    Hidden,
}

/// 过滤/搜索栏: 同一槽位两种输入 (表格模式过滤, 原始+搜索开搜索)。
///
/// 参考 danqing-clipboard `bottom_bar.rs`: TextInput 作为字段而非 child 节点,
/// 焦点路径落在本容器上, 经 `wants_ime`/`ime_area`/`selected_text`/`hit_area`/
/// `reset_focus` 转发到内部 TextInput (否则 IME / 剪贴板快照 / 命中全部失效)。
/// 清空经 clear revision 原地 clear (不重建实例, 保焦点态)。
pub(crate) struct Bar {
    filter_ti: TextInput,
    search_ti: TextInput,
    /// 已应用过滤 (sync 设占位文字)。
    filter_applied: String,
    /// 已应用搜索查询 (sync 设占位文字)。
    search_query: String,
    /// 应用侧清空 revision (读值变化时原地 clear)。
    filter_clear_binding: Option<ClearBinding>,
    search_clear_binding: Option<ClearBinding>,
    /// 应用侧「回到搜索栏」信号 (T21/P39): revision 变化且**本栏已持焦**时全选。
    search_refocus_binding: Option<ClearBinding>,
    applied_filter_rev: u64,
    applied_search_rev: u64,
    applied_refocus_rev: u64,
    /// 当前生效角色 (sync 计算)。
    active: ActiveBar,
    /// 前缀标签宽度 (paint 测量缓存, event 转发与 paint 的 input_area 须一致,
    /// 否则点击定位光标会偏一个 label 宽)。
    label_width: std::cell::Cell<f32>,
    /// 键义提示的**占位宽** (含间隙; paint 测量缓存, 与 `label_width` 同理 ——
    /// `input_area` 靠它让位, 而 `input_area` 同时供 paint 与 event 转发使用,
    /// 于是「提示画在哪」与「点到哪」不可能分岔)。**0 = 不显示**。
    hint_reserved: std::cell::Cell<f32>,
    /// 主题模式 (从 LogApp 同步)。
    theme: crate::config::AppTheme,
}

impl Bar {
    pub(crate) fn new() -> Self {
        Self {
            filter_ti: Self::fresh_filter(),
            search_ti: Self::fresh_search(),
            filter_applied: String::new(),
            search_query: String::new(),
            filter_clear_binding: None,
            search_clear_binding: None,
            search_refocus_binding: None,
            applied_filter_rev: 0,
            applied_search_rev: 0,
            applied_refocus_rev: 0,
            active: ActiveBar::Hidden,
            label_width: std::cell::Cell::new(0.0),
            hint_reserved: std::cell::Cell::new(0.0),
            theme: crate::config::AppTheme::Light,
        }
    }

    fn fresh_filter() -> TextInput {
        Self::base_input().placeholder(
            FILTER_PLACEHOLDER,
            Color::rgb(0.45, 0.45, 0.48), // placeholder_fg
        )
    }

    fn fresh_search() -> TextInput {
        Self::base_input().placeholder(
            SEARCH_PLACEHOLDER,
            Color::rgb(0.45, 0.45, 0.48), // placeholder_fg
        )
    }

    fn base_input() -> TextInput {
        // 输入色**不写死**: 原先这里钉了 `themed(&LightTheme)` 加三个写死色
        // (正文 0.20 / 光标 0.10 / 选区), 空态看不出来 (占位色是中性灰, 两个主题
        // 上都读得动), 一打字就露 —— 暗色下 (51,51,51) 压在栏底 (38,38,43) 上,
        // WCAG 对比度 **1.19**, 等于看不见。
        // 改成随主题走: 构造值只作首帧兜底, `bind_theme` 每帧重取。
        // 回归锁: `filter_input_color_follows_theme`。
        TextInput::themed(&LightTheme)
            .bind_theme(|app: &crate::LogApp| app.theme.theme())
            .font_size(FONT_SIZE)
            .chromeless()
            .padding(Edges::symmetric(2.0, 0.0))
    }

    /// 绑定过滤清空信号: 应用侧 revision 变化时原地 clear。
    pub(crate) fn bind_clear_filter<S: 'static>(mut self, f: impl Fn(&S) -> u64 + 'static) -> Self {
        self.filter_clear_binding = Some(Box::new(move |state: &dyn Any| {
            let state = state
                .downcast_ref::<S>()
                .expect("Bar::bind_clear_filter 状态类型不匹配");
            f(state)
        }));
        self
    }

    /// 绑定搜索清空信号: 应用侧 revision 变化时原地 clear。
    pub(crate) fn bind_clear_search<S: 'static>(mut self, f: impl Fn(&S) -> u64 + 'static) -> Self {
        self.search_clear_binding = Some(Box::new(move |state: &dyn Any| {
            let state = state
                .downcast_ref::<S>()
                .expect("Bar::bind_clear_search 状态类型不匹配");
            f(state)
        }));
        self
    }

    /// 绑定「回到搜索栏」信号 (T21/P39): revision 变化且本栏已持焦 → 全选内容。
    ///
    /// **只在已持焦时全选**: 首次聚焦由框架走 `focus_request`, 而 `sync` 跑在
    /// 焦点落地**之前** —— 那一刻框里还没有光标, 也没有该全选的东西。
    pub(crate) fn bind_refocus_search<S: 'static>(
        mut self,
        f: impl Fn(&S) -> u64 + 'static,
    ) -> Self {
        self.search_refocus_binding = Some(Box::new(move |state: &dyn Any| {
            let state = state
                .downcast_ref::<S>()
                .expect("Bar::bind_refocus_search 状态类型不匹配");
            f(state)
        }));
        self
    }

    /// 前缀标签。
    fn label(&self) -> &'static str {
        match self.active {
            ActiveBar::Filter => "过滤:",
            ActiveBar::Search => "搜索:",
            ActiveBar::Hidden => "",
        }
    }

    /// 输入矩形 (label 之后到右缘)。
    fn input_area(&self, area: Rect, label_w: f32) -> Rect {
        let text_x = area.origin.x + BAR_PAD_X + label_w + BAR_LABEL_GAP;
        // 持焦时右侧让出键义提示的位置 (hint_reserved 由 paint 测量后写入;
        // 未持焦 / 放不下时为 0)。**单点收口**: paint 与 event 转发都走这里。
        let w = (area.size.width - (text_x - area.origin.x) - BAR_PAD_X - self.hint_reserved.get())
            .max(1.0);
        Rect::from_xywh(text_x, area.origin.y, w, area.size.height)
    }

    /// 当前生效输入是否持焦。`chromeless` 下框架不画描边, 焦点态全由本容器呈现
    /// (P6 底边线 + P33 键义提示), 故这个查询是那两处共同的判据。
    fn input_focused(&self) -> bool {
        self.active_input().map(|t| t.is_focused()).unwrap_or(false)
    }

    /// 当前生效角色的空态占位文案。**只给 P33 的宽度判据用** ——
    /// 空框持焦时输入框画的就是它, 提示要避开的也正是它。
    /// 文案本身仍由 `filter_placeholder_text` / `search_placeholder_text` 单点构造。
    fn active_placeholder(&self) -> String {
        match self.active {
            ActiveBar::Filter => filter_placeholder_text(&self.filter_applied),
            ActiveBar::Search => search_placeholder_text(&self.search_query),
            ActiveBar::Hidden => String::new(),
        }
    }

    /// 当前生效输入的引用。
    fn active_input(&self) -> Option<&TextInput> {
        match self.active {
            ActiveBar::Filter => Some(&self.filter_ti),
            ActiveBar::Search => Some(&self.search_ti),
            ActiveBar::Hidden => None,
        }
    }

    /// 当前生效输入的可变引用。
    fn active_input_mut(&mut self) -> Option<&mut TextInput> {
        match self.active {
            ActiveBar::Filter => Some(&mut self.filter_ti),
            ActiveBar::Search => Some(&mut self.search_ti),
            ActiveBar::Hidden => None,
        }
    }
}

impl Default for Bar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Bar {
    fn sync(&mut self, state: &dyn Any) {
        let app = state
            .downcast_ref::<LogApp>()
            .expect("Bar 绑定状态类型不匹配");
        // 生效角色: 表格=过滤, 原始=搜索 (搜索栏始终可见, 不可隐藏); 空态无文件不出栏。
        self.active = if !app.has_file {
            ActiveBar::Hidden
        } else if app.mode == ViewMode::Table {
            ActiveBar::Filter
        } else {
            ActiveBar::Search
        };
        self.theme = app.theme;

        // 两个输入框是**字段**而非子节点, 框架不会替它们传播 sync ——
        // 挂在 `TextInput` 上的 `bind_theme` 不手动踢一脚就不生效。
        self.filter_ti.sync(state);
        self.search_ti.sync(state);

        // 清空信号: revision 变化时原地 clear。
        if let Some(binding) = &self.filter_clear_binding {
            let rev = binding(state);
            if rev != self.applied_filter_rev {
                self.applied_filter_rev = rev;
                self.filter_ti.clear();
            }
        }
        if let Some(binding) = &self.search_clear_binding {
            let rev = binding(state);
            if rev != self.applied_search_rev {
                self.applied_search_rev = rev;
                self.search_ti.clear();
            }
        }
        // T21 (P39): 「回到搜索栏」不再清草稿 —— 已持焦时改成**全选**, 让用户
        // 直接覆写; 没持焦则什么都不做 (框架会把焦点送进来)。
        if let Some(binding) = &self.search_refocus_binding {
            let rev = binding(state);
            if rev != self.applied_refocus_rev {
                self.applied_refocus_rev = rev;
                if self.search_ti.is_focused() {
                    self.search_ti.select_all();
                }
            }
        }

        // 应用态 → 占位文字 (空态显示 "已应用" 提示; 有输入时占位消失)。
        // 仅在应用值真变化时重设: 初值由 fresh_* 给, 之后归空须复原文案,
        // 值未变则跳过 (sync 每帧跑, 免每帧重建占位串)。
        let filter_applied = app.filter_applied.clone();
        if filter_applied != self.filter_applied {
            self.filter_applied = filter_applied;
            self.set_filter_placeholder();
        }
        let search_query = app.search_query.clone();
        if search_query != self.search_query {
            self.search_query = search_query;
            self.set_search_placeholder();
        }
    }

    fn animate(&mut self, ctx: &danqing::AnimationCtx) {
        self.filter_ti.animate(ctx);
        self.search_ti.animate(ctx);
    }

    fn layout(&mut self, constraints: Constraints, texts: &mut TextBatch) -> Size {
        if self.active == ActiveBar::Hidden {
            return Size::new(constraints.max().width, 0.0);
        }
        // 让当前生效的输入框先 layout (缓存 vertical_pad / char_offsets)。
        if let Some(ti) = self.active_input_mut() {
            let _ = ti.layout(constraints, texts);
        }
        Size::new(constraints.max().width, FILTER_BAR_H)
    }

    fn paint(&self, area: Rect, rects: &mut RectBatch, texts: &mut TextBatch) {
        if self.active == ActiveBar::Hidden {
            return;
        }
        let th = self.theme.theme();
        rects.push_rect(
            Rect::from_xywh(area.origin.x, area.origin.y, area.size.width, FILTER_BAR_H),
            th.surface_variant(),
            0.0,
        );
        let line_h = texts.line_height(f32::from(FONT_SIZE));
        let baseline = area.origin.y
            + (FILTER_BAR_H - line_h) / 2.0
            + texts.ascent(f32::from(FONT_SIZE))
            + BAR_TOP_OFFSET;
        let label = self.label();
        texts.push_text(
            label,
            area.origin.x + BAR_PAD_X,
            baseline,
            FONT_SIZE,
            th.text_primary(),
        );
        let label_w = texts.measure(label, FONT_SIZE);
        self.label_width.set(label_w);

        // 持焦态两条反馈 (P33 键义提示 / P6 焦点线)。提示宽度**先测后存**,
        // `input_area` 才好在同一帧内让位 —— 顺序反了就会压字一帧。
        //
        // 提示只在**输入框为空**时出现。这不是省事, 是被框架逼出来的:
        // `TextInput::paint` **既不裁剪也不横向滚动** (源码是整串一次性
        // `push_text`, 没有任何 scroll offset), 所以「让位」保护得了命中测试,
        // 保护不了字形 —— 有字时长查询照旧会画进提示的地盘。空态下要避的只剩
        // 占位文案, 那是**可测**的, 于是能给出真正的「放不下就不画」判据。
        let focused = self.input_focused();
        let hint_w = if focused && self.active_input().is_some_and(|t| t.value().is_empty()) {
            let room = area.size.width - (BAR_PAD_X + label_w + BAR_LABEL_GAP) - BAR_PAD_X;
            let w = texts.measure(BAR_KEY_HINT, FONT_SIZE);
            let ph_w = texts.measure(&self.active_placeholder(), FONT_SIZE);
            if room >= ph_w + BAR_HINT_GAP + BAR_LABEL_GAP + w {
                w
            } else {
                0.0
            }
        } else {
            0.0
        };
        self.hint_reserved.set(if hint_w > 0.0 {
            hint_w + BAR_HINT_GAP
        } else {
            0.0
        });

        let input_area = self.input_area(area, label_w);
        match self.active {
            ActiveBar::Filter => self.filter_ti.paint(input_area, rects, texts),
            ActiveBar::Search => self.search_ti.paint(input_area, rects, texts),
            ActiveBar::Hidden => {}
        }

        if hint_w > 0.0 {
            // 右对齐推 x: 提示尾端贴栏的右内边距, 与输入区让出的宽度同源。
            texts.push_text(
                BAR_KEY_HINT,
                area.origin.x + area.size.width - BAR_PAD_X - hint_w,
                baseline,
                FONT_SIZE,
                th.text_secondary(),
            );
        }
        if focused {
            rects.push_rect(
                Rect::from_xywh(
                    area.origin.x,
                    area.origin.y + FILTER_BAR_H - BAR_FOCUS_LINE,
                    area.size.width,
                    BAR_FOCUS_LINE,
                ),
                th.accent(),
                0.0,
            );
        }
    }

    fn event(&mut self, event: &Event, area: Rect, msgs: &mut MsgQueue) -> EventResult {
        let active = self.active;
        if active == ActiveBar::Hidden {
            return EventResult::Ignored;
        }
        // 拦截 Enter/Esc/PageUp/PageDown: Enter=应用, Esc=关闭/清除, Page=滚动 (栏聚焦时导航仍可用)。
        if let Event::Key {
            key,
            pressed: true,
            shift,
            ..
        } = event
        {
            match key {
                Key::Named(NamedKey::Enter) => return self.handle_enter(active, *shift, msgs),
                Key::Named(NamedKey::Escape) => return self.handle_escape(active, msgs),
                Key::Named(NamedKey::PageUp) => {
                    msgs.push(Box::new(Msg::ScrollRows(-crate::PAGE_ROWS)));
                    return EventResult::Consumed;
                }
                Key::Named(NamedKey::PageDown) => {
                    msgs.push(Box::new(Msg::ScrollRows(crate::PAGE_ROWS)));
                    return EventResult::Consumed;
                }
                _ => {}
            }
        }
        // 其余转发给当前生效的输入框 (打字/方向键移动光标等)。
        let input_area = self.input_area(area, self.label_width.get());
        let ti = self
            .active_input_mut()
            .expect("active 非 Hidden 必有输入框");
        ti.event(event, input_area, msgs)
    }

    fn focusable(&self) -> bool {
        self.active != ActiveBar::Hidden
    }

    fn focus_id(&self) -> Option<&'static str> {
        Some("log-bar")
    }

    fn wants_ime(&self) -> bool {
        self.active_input().map(|t| t.wants_ime()).unwrap_or(false)
    }

    fn ime_area(&self) -> Option<Rect> {
        self.active_input().and_then(|t| t.ime_area())
    }

    fn selected_text(&self) -> Option<String> {
        self.active_input().and_then(|t| t.selected_text())
    }

    fn hit_area(&self) -> Option<Rect> {
        self.active_input().and_then(|t| t.hit_area())
    }

    fn reset_focus(&mut self) {
        self.filter_ti.reset_focus();
        self.search_ti.reset_focus();
    }
}

impl Bar {
    /// Enter 处理: 过滤=应用; 搜索=空时下/上一命中 (shift), 非空应用。
    fn handle_enter(&self, active: ActiveBar, shift: bool, msgs: &mut MsgQueue) -> EventResult {
        match active {
            ActiveBar::Filter => {
                let q = self.filter_ti.value().trim().to_string();
                msgs.push(Box::new(Msg::ApplyFilter(q)));
            }
            ActiveBar::Search => {
                let q = self.search_ti.value().trim().to_string();
                if q.is_empty() {
                    msgs.push(Box::new(if shift {
                        Msg::SearchPrevHit
                    } else {
                        Msg::SearchNextHit
                    }));
                } else {
                    msgs.push(Box::new(Msg::ApplySearch(q)));
                }
            }
            ActiveBar::Hidden => return EventResult::Ignored,
        }
        EventResult::Consumed
    }

    /// Esc 处理: 栏有内容 → 清内容、保焦点 (方便重新输入);
    /// 栏已空 → 返回 Ignored 让框架清焦, 用户可继续用键导航列表。
    fn handle_escape(&self, active: ActiveBar, msgs: &mut MsgQueue) -> EventResult {
        match active {
            ActiveBar::Filter => {
                // 有已应用过滤 或 输入框有文字 → 清除; 都没有 → 仅清焦
                if self.filter_applied.is_empty() && self.filter_ti.value().is_empty() {
                    return EventResult::Ignored;
                }
                msgs.push(Box::new(Msg::ClearFilter));
                EventResult::Consumed
            }
            ActiveBar::Search => {
                // 有搜索结果 或 输入框有文字 → 清除; 都没有 → 仅清焦
                if self.search_query.is_empty() && self.search_ti.value().is_empty() {
                    return EventResult::Ignored;
                }
                msgs.push(Box::new(Msg::ClearSearch));
                EventResult::Consumed
            }
            ActiveBar::Hidden => EventResult::Ignored,
        }
    }

    fn set_filter_placeholder(&mut self) {
        self.filter_ti
            .set_placeholder(filter_placeholder_text(&self.filter_applied));
    }

    fn set_search_placeholder(&mut self) {
        self.search_ti
            .set_placeholder(search_placeholder_text(&self.search_query));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use danqing::theme::DarkTheme;

    /// 回归: Esc 清除过滤后占位须复原空态文案, 不能留上次的 "已应用: ..."。
    #[test]
    fn filter_placeholder_resets_when_cleared() {
        let applied = filter_placeholder_text("level=ERROR");
        assert!(applied.contains("level=ERROR"), "应用后提示词: {applied}");
        let cleared = filter_placeholder_text("");
        assert_eq!(cleared, FILTER_PLACEHOLDER, "清除后复原空态文案");
    }

    /// 回归: 同上, 搜索栏。
    #[test]
    fn search_placeholder_resets_when_cleared() {
        let applied = search_placeholder_text(r"\d{4}");
        assert!(applied.contains(r"\d{4}"), "应用后显示查询词: {applied}");
        let cleared = search_placeholder_text("");
        assert_eq!(cleared, SEARCH_PLACEHOLDER, "清除后复原空态文案");
    }

    /// 造一个生效角色 = 过滤的栏 (正式路径下 `active` 由 `sync` 从 LogApp 算出,
    /// 这里直接置位 —— 本节只测 paint/event 两条反馈通路, 与 active 怎么来的无关)。
    fn bar_for_test(focused: bool) -> Bar {
        let mut bar = Bar::new();
        bar.active = ActiveBar::Filter;
        if focused {
            let area = Rect::from_xywh(0.0, 0.0, 800.0, FILTER_BAR_H);
            let mut msgs = danqing::widget::MsgQueue::new();
            bar.event(&Event::FocusIn, area, &mut msgs);
        }
        bar
    }

    /// 「这个矩形以这个颜色被画了」—— 断言 paint 的产出, 不是标志位。
    fn rect_painted(rects: &RectBatch, r: Rect, c: Color) -> bool {
        let want = lin(c);
        let got = rects.instance_rects();
        let colors = rects.instance_colors();
        got.iter()
            .zip(colors.iter())
            .any(|(g, col)| *g == r && *col == want)
    }

    /// 够宽的栏 —— 提示放得下, 让断言不受字体宽度摆动的干扰。
    fn wide_bar_area() -> Rect {
        Rect::from_xywh(0.0, 0.0, 1600.0, FILTER_BAR_H)
    }

    /// P33 / P6 回归锁: 栏持焦时必须**画出**键义提示与底边焦点线。
    ///
    /// 断言的是 paint 产出的**矩形与字形**, 不是标志位 —— 「有状态却没画」正是
    /// 这一类缺陷的原始形态。未持焦态同时作对照: 两条反馈都不该出现。
    #[test]
    fn focused_bar_paints_key_hint_and_focus_line() {
        let area = wide_bar_area();
        let th = crate::config::AppTheme::Light.theme();
        let mut blurred_rects = RectBatch::new();
        let mut blurred_texts = TextBatch::new();
        bar_for_test(false).paint(area, &mut blurred_rects, &mut blurred_texts);

        let bar = bar_for_test(true);
        let mut rects = RectBatch::new();
        let mut texts = TextBatch::new();
        bar.paint(area, &mut rects, &mut texts);

        let line = Rect::from_xywh(0.0, FILTER_BAR_H - BAR_FOCUS_LINE, 1600.0, BAR_FOCUS_LINE);
        assert!(
            rect_painted(&rects, line, th.accent()),
            "持焦时底边应有一条 accent 焦点线: {rects:?}"
        );
        assert!(
            texts.len() > blurred_texts.len(),
            "持焦时须多画出提示字形: {} vs {}",
            texts.len(),
            blurred_texts.len()
        );
        // 提示是最后压入的一串字形, 故末位字形的颜色就是提示色。
        // `TextBatch::instance_colors` 返回的是**线性**分量 (与 `RectBatch` 那支
        // 返回 `[f32;4]` 不同型), 故按字段取 —— 也免得引框架私有的 `LinearRgba`。
        let colors = texts.instance_colors();
        assert_eq!(
            colors
                .last()
                .map(|c| [c.r, c.g, c.b, c.a] == lin(th.text_secondary())),
            Some(true),
            "键义提示须用 text_secondary (与输入文本/占位可辨)"
        );
    }

    /// P33 边界: **框里有字就不画提示** —— 框架的 `TextInput::paint` 既不裁剪也不
    /// 横向滚动 (整串一次性 `push_text`), 「让位」保护得了命中测试、保护不了字形,
    /// 长查询会直接画进提示的地盘。故只在空框时画, 那时要避的只剩占位文案。
    ///
    /// 但**焦点线不跟着消失**: 「焦点在哪」与「键义怎么说」是两件事。
    #[test]
    fn focused_bar_with_text_drops_the_hint_but_keeps_the_focus_line() {
        let area = wide_bar_area();
        let th = crate::config::AppTheme::Light.theme();
        let mut bar = bar_for_test(true);
        let mut msgs = danqing::widget::MsgQueue::new();
        bar.event(
            &Event::Key {
                key: Key::Character("a".to_string()),
                pressed: true,
                shift: false,
                ctrl: false,
                alt: false,
            },
            area,
            &mut msgs,
        );
        assert!(!bar.filter_ti.value().is_empty(), "前提: 框里已有字");
        let mut rects = RectBatch::new();
        let mut texts = TextBatch::new();
        bar.paint(area, &mut rects, &mut texts);

        assert_eq!(bar.hint_reserved.get(), 0.0, "有字时不画提示");
        let line = Rect::from_xywh(0.0, FILTER_BAR_H - BAR_FOCUS_LINE, 1600.0, BAR_FOCUS_LINE);
        assert!(
            rect_painted(&rects, line, th.accent()),
            "焦点线不该跟提示一起消失"
        );
    }

    /// P33 回归锁: 让位宽度与提示绘制**同源** —— 提示占多少, 输入区就退多少。
    ///
    /// 两处各写一份式子迟早漂成「提示压在用户正打的正则上」或「点到的光标位置偏
    /// 一格」(后者是本仓 `label_width` 已经踩过一次的形态)。
    #[test]
    fn focused_bar_gives_the_input_area_room_for_the_key_hint() {
        let area = wide_bar_area();
        let bar = bar_for_test(true);
        let mut rects = RectBatch::new();
        let mut texts = TextBatch::new();
        bar.paint(area, &mut rects, &mut texts);

        let label_w = bar.label_width.get();
        let reserved = bar.hint_reserved.get();
        assert!(reserved > 0.0, "1600px 宽的栏放得下提示");
        let full = area.size.width - (BAR_PAD_X + label_w + BAR_LABEL_GAP) - BAR_PAD_X;
        let got = bar.input_area(area, label_w).size.width;
        assert!(
            (got - (full - reserved)).abs() < 0.01,
            "输入区须正好让出提示占位: 实得 {got}, 应为 {}",
            full - reserved
        );

        let blurred = bar_for_test(false);
        let mut r2 = RectBatch::new();
        let mut t2 = TextBatch::new();
        blurred.paint(area, &mut r2, &mut t2);
        assert_eq!(blurred.hint_reserved.get(), 0.0, "未持焦一分不让");
    }

    /// P33 边界: 窄窗放不下提示时**宁可不画**, 也不让它压在输入文本上;
    /// 但焦点线仍在 (提示放不下 ≠ 焦点不可见)。
    #[test]
    fn narrow_bar_drops_the_key_hint_but_keeps_the_focus_line() {
        let area = Rect::from_xywh(0.0, 0.0, 240.0, FILTER_BAR_H);
        let bar = bar_for_test(true);
        let mut rects = RectBatch::new();
        let mut texts = TextBatch::new();
        bar.paint(area, &mut rects, &mut texts);

        assert_eq!(bar.hint_reserved.get(), 0.0, "240px 宽的栏放不下提示");
        let line = Rect::from_xywh(0.0, FILTER_BAR_H - BAR_FOCUS_LINE, 240.0, BAR_FOCUS_LINE);
        assert!(
            rect_painted(
                &rects,
                line,
                crate::config::AppTheme::Light.theme().accent()
            ),
            "焦点线不该跟提示一起消失"
        );
    }

    /// P33 内容锁: 提示点名的键 = 输入框**真的会吞**的键。
    ///
    /// 不写成「常量含某某字样」那种自证 —— 逐个键真喂给 TextInput, 吞得下的才要求
    /// 提示点名。于是「框架改了键分支」和「有人为了排版把词删掉」都会红。
    #[test]
    fn key_hint_names_every_key_the_input_swallows() {
        let area = Rect::from_xywh(0.0, 0.0, 400.0, FILTER_BAR_H);
        let mut msgs = danqing::widget::MsgQueue::new();
        let mut ti = TextInput::themed(&LightTheme).font_size(FONT_SIZE);
        let mut swallows = |ti: &mut TextInput, key: Key| {
            ti.event(
                &Event::Key {
                    key,
                    pressed: true,
                    shift: false,
                    ctrl: false,
                    alt: false,
                },
                area,
                &mut msgs,
            ) == EventResult::Consumed
        };
        // 被吞 = 语义相对失焦态翻转 (输入空格 / 移光标), 提示必须逐键点名
        for (name, key) in [
            ("Space", Key::Named(NamedKey::Space)),
            ("Home", Key::Named(NamedKey::Home)),
            ("End", Key::Named(NamedKey::End)),
        ] {
            assert!(swallows(&mut ti, key), "{name} 应被输入框吞下 (本锁的前提)");
            assert!(
                BAR_KEY_HINT.contains(name),
                "提示漏了 {name} —— 它持焦后已改归输入框"
            );
        }
        // ↑↓ 是**没被吞**的那一半: 栏持焦时仍滚列表, 同样要点名
        assert!(
            !swallows(&mut ti, Key::Named(NamedKey::ArrowDown)),
            "↓ 不该被输入框吞 —— 它仍要滚列表"
        );
        assert!(BAR_KEY_HINT.contains("↑↓"), "提示须点明 ↑↓ 仍滚列表");
    }

    /// 消息队列里有没有一条说中某句话的 notice。
    fn said(msgs: &[Box<dyn std::any::Any>], needle: &str) -> bool {
        msgs.iter().any(|m| {
            matches!(
                m.downcast_ref::<Msg>(),
                Some(Msg::Notice(t, _)) if t.contains(needle)
            )
        })
    }

    /// M3/T12 (P20): 点列表区**末行下方空白**原先完全沉默 —— 现在说清「此处无行」。
    ///
    /// 归因**刻意不动**: 仍返回 `Ignored`, 点击照旧穿透去清焦点 (归因是 M4 的活)。
    /// 所以这里同时锁住「出了声」与「没改归因」两件事。
    ///
    /// **2026-09-15 用户实机报「还是没看到出声」—— 这条测试当时是假绿**:
    /// 它把点击点放在 `HEADER_H + list_h + 8.0`, 而那已经**越出列表矩形**、落进
    /// 状态栏那一带了 (它还漏算了表格模式的过滤栏: `chrome_top` = `HEADER_H` +
    /// `FILTER_BAR_H`, 比 `HEADER_H` 大), 于是走的是「矩形之外」那一支 ——
    /// **从没覆盖过「矩形之内、末行之下」这个本体**。现在两个位置各测一次,
    /// **都按真实布局 (`chrome_top()`) 算**, 且先断言「点在矩形里」。
    #[test]
    fn click_below_the_last_row_says_there_is_no_row() {
        let (mut v, path) = cell_fixture("m3-p20");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let chrome_top = v.chrome_top();
        let list_h = area.size.height - chrome_top - STATUS_HEIGHT;
        let rows_bottom = area.origin.y + chrome_top + v.display_count() as f32 * ROW_HEIGHT;

        let click = |button_y: f32| Event::MouseInput {
            button: MouseButton::Left,
            pressed: true,
            position: Point::new(300.0, button_y),
        };

        // ① **本体**: 列表矩形之内、末行之下 (夹具只有 2 行, 下面是好大一片空白)
        let inside = rows_bottom + 20.0;
        assert!(
            inside < area.origin.y + chrome_top + list_h,
            "前提: 这个点必须真落在列表矩形**之内** —— 否则又退化成测「矩形之外」了"
        );
        let mut msgs = danqing::widget::MsgQueue::new();
        assert_eq!(
            v.event(&click(inside), area, &mut msgs),
            EventResult::Ignored,
            "空白处点击的归因不变 (仍穿透)"
        );
        assert!(
            said(&msgs, "此处无行"),
            "末行下方空白须说清为什么没反应 (原先静默: 越界行被 app 的 \
             `row < display_count` 挡掉, 于是什么都不发生)"
        );

        // ② 矩形**之外** (状态栏那一带): 同一句, 两条来源共用一支
        let mut msgs = danqing::widget::MsgQueue::new();
        v.event(
            &click(area.origin.y + chrome_top + list_h + 8.0),
            area,
            &mut msgs,
        );
        assert!(said(&msgs, "此处无行"), "矩形外那一支不得改坏");

        std::fs::remove_file(&path).ok();
    }

    /// M3/T11 回归锁: notice 是**第二条通道**, 不是常态串的一段。
    ///
    /// 两个具体缺陷都是写这一节时真发生过的:
    /// ① `refresh_status` 曾把 notice 拼进 `status`, 而 paint 又单独画了一遍
    ///    `notice` —— 同一句话在底栏出现**两次**;
    /// ② `Info` 档曾与常态同用 `text_secondary()` —— **同色即同通道**, P27
    ///    「错误在视觉上不存在」原样复活 (写这段时的 if/else 两个分支干脆写成了
    ///    同一个值, 注释还写着「降噪」)。
    ///
    /// 所以这里断言的是**取色**与**不重复**, 不是「画了没有」。
    #[test]
    fn notice_is_drawn_once_in_a_color_of_its_own() {
        let (mut v, path) = cell_fixture("m3-t11");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let th = v.theme.theme();
        let status = v.status.clone();

        let mut texts = TextBatch::new();
        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts);
        let plain = texts.instance_colors();

        v.notice = Some(("此处无行".into(), crate::NoticeKind::Info));
        let mut texts = TextBatch::new();
        v.paint(area, &mut rects, &mut texts);
        let with_notice = texts.instance_colors();

        let n = "此处无行".chars().count();
        assert_eq!(
            with_notice.len() - plain.len(),
            n,
            "notice 只能多画它自己这一串 —— 多出来的就是被画了两遍: {status:?}"
        );
        // 颜色按**重数差**验, 不按下标取: notice 后面还压着设置入口与位置计数,
        // 而插入点在 status 之后 —— 拿总长当下标会切到尾部那几串上去 (本测试第一版
        // 就是这么错的)。前后两批只差 notice, 故差值就是它的字形数。
        let want = lin(th.text_primary());
        let before = plain
            .iter()
            .filter(|c| [c.r, c.g, c.b, c.a] == want)
            .count();
        let after = with_notice
            .iter()
            .filter(|c| [c.r, c.g, c.b, c.a] == want)
            .count();
        assert_eq!(
            after - before,
            n,
            "Info 档 notice 须以 `text_primary` 画出 {n} 个字形 —— 与常态的 \
             `text_secondary` 同色就是没分通道 (T11 判据: 同屏可辨)"
        );
        assert_ne!(
            lin(th.text_primary()),
            lin(th.text_secondary()),
            "前提: 框架这两个 token 本身就不同色"
        );
        std::fs::remove_file(&path).ok();
    }

    /// M3/T12 (P22): 表格里点**无 glyph 行**的行首展开区 —— 原先照样发
    /// `ToggleExpand`, 落地零反应。
    ///
    /// 两侧都断言: 「不发 ToggleExpand」**且**「有一条说清原因的 notice」。
    /// 只测后者会放过「既出声又照发消息」的半吊子修法, 那样点一下仍然会折叠出
    /// 一段并不存在的展开块。
    #[test]
    fn expand_glyph_on_a_leaf_row_says_why_instead_of_toggling() {
        let (mut v, path) = cell_fixture("m3-p22");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        // 行 1 = `{"level":"INFO"}`, 无嵌套 → 不可展开
        let ev = Event::MouseInput {
            button: MouseButton::Left,
            pressed: true,
            position: Point::new(
                LogView::expand_glyph_x(area) + 2.0,
                HEADER_H + ROW_HEIGHT + 5.0,
            ),
        };
        let mut msgs = danqing::widget::MsgQueue::new();
        v.event(&ev, area, &mut msgs);
        assert!(
            !msgs
                .iter()
                .any(|m| matches!(m.downcast_ref::<Msg>(), Some(Msg::ToggleExpand(_)))),
            "不可展开的行不得发 ToggleExpand (落地零反应正是原缺陷)"
        );
        assert!(said(&msgs, "无嵌套可展"), "须说清为什么展不开");
        std::fs::remove_file(&path).ok();
    }

    /// P3 回归锁 (2026-09-15 用户实机报「点行首 `+/−`, hover 没有 UI 反馈,
    /// 展开之后 `-` 没有显示」)。**两个缺陷一起钉**:
    ///
    /// ① **展开后标识不显示** —— 原先写的是 `−` (U+2212), 不在内嵌 GB2312 子集里,
    ///    是 0×0 空字形, **静默不画也不报错**。产品侧够不着框架的字体探针, 故守卫
    ///    钉一条更强的约束: **只许 ASCII**。U+2212 一旦写回来, 这条立刻红。
    /// ② **可点却无 hover 指示** —— 悬停时须换 `accent` 色。
    #[test]
    fn expand_glyph_is_ascii_and_lights_up_on_hover() {
        // ① 字符集约束
        for g in [GLYPH_COLLAPSED, GLYPH_EXPANDED] {
            assert!(
                g.is_ascii(),
                "展开标识 {g:?} 非 ASCII —— 内嵌子集里没有它, 会被静默画成 0×0"
            );
        }
        // **反向对照**: 证明上面那条断言真分得开 —— `−`(U+2212) 与 `-`(U+002D)
        // 肉眼几乎一样, 这正是它一路活到实机才被发现的原因。
        assert!(
            !'−'.is_ascii(),
            "U+2212 必须判为非 ASCII, 否则上面那条形同虚设"
        );

        // ② hover: 一行**真嵌套** (可展开, 画 `+`) + 一行叶子 (不画 glyph)
        let path =
            std::env::temp_dir().join(format!("danqing-log-p3-{}.jsonl", std::process::id()));
        std::fs::write(
            &path,
            "{\"level\":\"ERROR\",\"ctx\":{\"k\":\"v\"}}\n{\"level\":\"INFO\"}\n",
        )
        .unwrap();
        let file = LogFile::open(&path).unwrap();
        let mut v = LogView::new();
        v.file = Some(Arc::new(file));
        v.has_file = true;
        v.focused = true;
        v.gutter_w.set(56.0);
        v.mode = ViewMode::Table;
        v.schema = Some(Arc::new(Schema {
            columns: vec![Column {
                name: "level".into(),
                width_chars: 5,
            }],
        }));
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let accent = lin(v.theme.theme().accent());
        let accent_glyphs = |v: &mut LogView| {
            let mut texts = TextBatch::new();
            let mut rects = RectBatch::new();
            v.paint(area, &mut rects, &mut texts);
            texts
                .instance_colors()
                .iter()
                .filter(|c| [c.r, c.g, c.b, c.a] == accent)
                .count()
        };

        let cold = accent_glyphs(&mut v);
        // 指针落在第 0 行 (可展开) 的展开列里
        v.hover_expand.set(true);
        v.hover_row.set(0);
        assert_eq!(
            accent_glyphs(&mut v),
            cold + 1,
            "悬停展开标识须**恰好多一个** accent 字形 —— A/B: 摘掉 `hot` 判断, 这条必红"
        );

        // **反向对照**: 指针在展开列之外 (行文本上) 不得点亮标识 —— 否则那是
        // 另一条假反馈 (标识可点、行文本不可点, 两者不能共用一个 hover 信号)
        v.hover_expand.set(false);
        assert_eq!(accent_glyphs(&mut v), cold, "不在展开列上就不得点亮标识");
        std::fs::remove_file(&path).ok();
    }

    /// P27 的**视觉判据** (原文那句「错误在视觉上不存在」): 错误态那一行底栏
    /// 必须换色, 与打开耗时/过滤统计**同屏可辨**。
    ///
    /// 按**重数差**验, 不按下标取 —— 底栏后面还压着设置入口与位置计数。
    #[test]
    fn status_error_is_a_color_of_its_own() {
        let (mut v, path) = cell_fixture("p27");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let secondary = lin(v.theme.theme().text_secondary());
        // 夹具直接 `LogView::new()`, 没同步过 app —— 手动给一行常态底栏
        v.status = "索引 92ms · 2 行".into();
        // **空白不算字形实例** (排字时被跳过) —— 差点把这条断言写成「13 个全走」,
        // 实测差 4 个正好是那 4 个空格。数它要先把空白滤掉。
        let n = v.status.chars().filter(|c| !c.is_whitespace()).count();

        let count_secondary = |v: &mut LogView| {
            let mut texts = TextBatch::new();
            let mut rects = RectBatch::new();
            v.paint(area, &mut rects, &mut texts);
            texts
                .instance_colors()
                .iter()
                .filter(|c| [c.r, c.g, c.b, c.a] == secondary)
                .count()
        };

        v.status_error = false;
        let cold = count_secondary(&mut v);
        v.status_error = true;
        let hot = count_secondary(&mut v);
        assert_eq!(
            cold - hot,
            n,
            "置错误态后, status 那 {n} 个字形须**全部**离开 `text_secondary` \
             (A/B: 去掉 paint 里的分支, 这条必红)"
        );
        std::fs::remove_file(&path).ok();
    }

    /// 画一遍, 数出「选中色」矩形的个数。
    ///
    /// 三处高亮 (行选中底 / 文本选区带 / 单元格底色) **同用 `th.selection()`**
    /// (见 `cell_highlight_colors`), 所以一个数就能盯住三类 —— 任一被画出来都会被
    /// 数到, 也任一漏画都会掉数。
    fn selection_token_rects(v: &LogView) -> usize {
        let mut texts = TextBatch::new();
        let mut rects = RectBatch::new();
        v.paint(
            Rect::from_xywh(0.0, 0.0, 800.0, 600.0),
            &mut rects,
            &mut texts,
        );
        let want = lin(v.theme.theme().selection());
        let got = rects.instance_rects();
        let colors = rects.instance_colors();
        got.iter()
            .zip(colors.iter())
            .filter(|(_, c)| **c == want)
            .count()
    }

    /// 画出来的**文本选区带/单元格底**条数 —— 两者同尺寸 (`ROW_HEIGHT - 4`),
    /// 而**行选中底是整行高** (`ROW_HEIGHT`), 故这个数能把它们分开。
    ///
    /// 为什么需要它: 行选中是 `selected` 决定的, 夹具里默认恒有 —— 只数
    /// 「选中色矩形总数」的话, 行选中会**替**另外两类把断言满足掉。
    fn band_shaped_rects(v: &LogView) -> usize {
        let mut texts = TextBatch::new();
        let mut rects = RectBatch::new();
        v.paint(
            Rect::from_xywh(0.0, 0.0, 800.0, 600.0),
            &mut rects,
            &mut texts,
        );
        let want = lin(v.theme.theme().selection());
        let got = rects.instance_rects();
        let colors = rects.instance_colors();
        got.iter()
            .zip(colors.iter())
            .filter(|(r, c)| **c == want && (r.size.height - (ROW_HEIGHT - 4.0)).abs() < 0.01)
            .count()
    }

    /// T14 验收 ②: 「看得见 ⇔ 复制得到」这条不变量对**三类选中各测一次** ——
    /// 它不是「行选中」那一处的局部约定。
    ///
    /// 由构造保证的机制见 `LogView::focused`: 框架只在持焦链路上派发 `Event::Copy`,
    /// 所以三处高亮全部 AND 上焦点, 两边就是同一个因。
    ///
    /// **本测试第一版是假绿的, 记在这里**: 三类共用表格夹具、且都数「选中色矩形
    /// 总数」, 而文本选区带**只在原始模式与子行上画** —— 表格夹具根本走不到那儿,
    /// 于是第三例实际是被**行选中底**满足的, 对选区带零覆盖。
    /// **A/B 实证**: 摘掉选区带的焦点守卫, 那一版照样全绿。
    /// 现在每类用它**自己那台夹具**, 并改用 `band_shaped_rects` 把行选中排除掉。
    #[test]
    fn the_three_highlights_all_follow_focus() {
        // ① 行选中 (表格夹具)
        let (mut v, path) = cell_fixture("t14-row");
        v.hover_row.set(u64::MAX);
        v.selected = 0;
        v.focused = false;
        assert_eq!(selection_token_rects(&v), 0, "行选中: 失焦时不得画出来");
        v.focused = true;
        assert!(
            selection_token_rects(&v) > 0,
            "行选中: 持焦时必须画出来 (否则是「复制得到却看不见」)"
        );
        std::fs::remove_file(&path).ok();

        // ② 单元格选中 (表格夹具; 行选中置到别的行, 不替它满足)
        let (mut v, path) = cell_fixture("t14-cell");
        v.hover_row.set(u64::MAX);
        v.selected = 1;
        v.selected_cell = Some((0, 1));
        v.focused = false;
        assert_eq!(band_shaped_rects(&v), 0, "单元格: 失焦时不得画出来");
        v.focused = true;
        assert!(band_shaped_rects(&v) > 0, "单元格: 持焦时必须画出来");
        std::fs::remove_file(&path).ok();

        // ③ 文本选区带 —— **必须用原始模式夹具**: 选区带的两处绘制点分别在
        // 「子行」与「非表格」分支里, 表格夹具走不到 (第一版就栽在这儿)。
        let path =
            std::env::temp_dir().join(format!("danqing-log-t14-band-{}.log", std::process::id()));
        std::fs::write(&path, "ERROR line one\nINFO line two\n").unwrap();
        let mut v = LogView::new();
        v.file = Some(Arc::new(LogFile::open(&path).unwrap()));
        v.has_file = true;
        v.gutter_w.set(56.0);
        v.mode = ViewMode::Raw;
        v.selected = 1; // 行选中挪到**别的行**, 不许它冒充选区带
        v.hover_row.set(u64::MAX);
        v.selection = Some(TextSelection::new((0, 0), (0, 2)));
        v.focused = false;
        assert_eq!(band_shaped_rects(&v), 0, "文本选区: 失焦时不得画出来");
        v.focused = true;
        assert!(band_shaped_rects(&v) > 0, "文本选区: 持焦时必须画出来");
        std::fs::remove_file(&path).ok();
    }

    /// T14 验收 ①: 「点行 → Esc → Ctrl+C」这条链 —— 高亮必须跟着焦点一起走。
    ///
    /// 端到端在本层只能做到「Esc 之后 paint 不再画高亮」: Ctrl+C 那一半由框架的
    /// 焦点链路负责 (只有持焦才派发 `Event::Copy`), 单测里没有那套调度。
    /// 但两者是**同一个因**, 所以这里同时锁住三件事就锁住了整条链:
    /// ① Esc 必须 `Ignored` (这才是框架清焦点的触发条件, `handler.rs:453-465`);
    /// ② 焦点随后丢失 (框架发 FocusOut);
    /// ③ 屏幕上不再留高亮。
    #[test]
    fn escape_drops_the_highlight_and_hands_focus_back_to_the_framework() {
        let (mut v, path) = cell_fixture("t14-esc");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut msgs = danqing::widget::MsgQueue::new();
        v.hover_row.set(u64::MAX);

        // 点行 (这里只取它的两个后果: 持焦 + 选中)
        v.event(&Event::FocusIn, area, &mut msgs);
        v.selected = 0;
        assert!(v.focused, "点行后本组件持焦");
        assert!(selection_token_rects(&v) > 0, "前提: 行选中在屏上");

        assert_eq!(
            v.event(
                &Event::Key {
                    key: Key::Named(NamedKey::Escape),
                    pressed: true,
                    shift: false,
                    ctrl: false,
                    alt: false,
                },
                area,
                &mut msgs,
            ),
            EventResult::Ignored,
            "Esc 必须交还框架 —— 这正是清焦点的触发条件"
        );
        v.event(&Event::FocusOut, area, &mut msgs); // 框架清焦点后发的那一发

        assert!(!v.focused, "Esc 之后不再持焦");
        assert_eq!(
            selection_token_rects(&v),
            0,
            "屏幕上不得再留高亮 —— 它此刻也确实复制不到 (视觉与可复制性同真同假)"
        );
        std::fs::remove_file(&path).ok();
    }

    /// T17 夹具: 200 行原始模式 (足够溢出 600px 视口 → 竖条出现)。
    fn scroll_fixture(tag: &str) -> (LogView, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "danqing-log-scroll-{tag}-{}.log",
            std::process::id()
        ));
        let body: String = (0..200).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&path, body).unwrap();
        let mut v = LogView::new();
        v.file = Some(Arc::new(LogFile::open(&path).unwrap()));
        v.has_file = true;
        v.focused = true;
        v.gutter_w.set(56.0);
        v.mode = ViewMode::Raw;
        (v, path)
    }

    /// 列表区几何 (与 paint/event 同式, 抽在此免得测试自己又推一份)。
    fn list_geom(v: &LogView, area: Rect) -> (f32, f32) {
        let top = area.origin.y + v.chrome_top();
        let h = (area.size.height - v.chrome_top() - STATUS_HEIGHT).max(0.0);
        (top, h)
    }

    /// **拇指的长度要够抓** (2026-09-15 用户实机判据)。
    ///
    /// 事由: 4771 行 / 964px 轨道 → 真实比例 0.5%, 竖条拇指被夹到当时的
    /// `THUMB_MIN_H = 24` —— **6 × 24px**。用户原话「横向滚动条够了, 纵向滚动条有点小,
    /// 不够抓」: 同一屏的横条拇指是 **647 × 6px**, 而**两根条的厚度与命中带完全一样**
    /// (6px / 12px), 差的就是长度。24 抓不住 → 提到 48。
    ///
    /// **钉值, 不钉「有下界」** —— 写成 `>= 0.0` 或 `>= THUMB_MIN_H` 的话, 把常量改回
    /// 24 这条照样绿, 那就不是守卫了 (`wheel_rows_is_clamped_and_sign_flipped`
    /// 踩过同一个坑: 当时写 `<= WHEEL_MAX_ROWS` 改成 50 也不红)。
    #[test]
    fn scroll_thumb_has_a_grabbable_minimum_length() {
        assert_eq!(
            THUMB_MIN_H, 48.0,
            "拇指最小长度是用户实机判过的数 (24 抓不住), 改动要重新过一遍手"
        );
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let list_h = 964.0;
        // 真生效: 超大文件下竖条拇指**就是**这个长度
        let sb = v_scroll(area, area.origin.y, list_h, 4_000_000, 0.0).expect("4M 行必有竖条");
        assert_eq!(sb.thumb_h, THUMB_MIN_H, "4M 行下拇指应夹到最小长度");
        // 反向对照: 行数装得下时**没有**条 —— 别为了凑上一条把条画出来
        assert!(
            v_scroll(area, area.origin.y, list_h, 10, 0.0).is_none(),
            "10 行装得下 964px, 不该有竖条"
        );
    }

    /// T17 验收 ①: 拇指位置 ↔ 内容偏移**互为逆运算**。
    ///
    /// 区间只取 `top_row ∈ [0, max_top]` —— 超出时 paint 把拇指夹到底, 逆运算回来
    /// 得 `max_top` (两者都读作「在底部」但数值不等), 见 [`VScroll::top_row_at`]。
    #[test]
    fn scroll_thumb_and_top_row_are_inverse() {
        let (v, path) = scroll_fixture("inverse");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let (rows_top, list_h) = list_geom(&v, area);
        let count = v.display_count();
        let max_top = v_scroll(area, rows_top, list_h, count, 0.0)
            .expect("前提: 内容溢出, 竖条出现")
            .max_top;
        assert!(max_top > 0.0);
        for frac in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let top = max_top * frac;
            let sb = v_scroll(area, rows_top, list_h, count, top).unwrap();
            let back = sb.top_row_at(sb.thumb_y);
            assert!(
                (back - top).abs() < 0.01,
                "往返应一致: top={top} → thumb_y={} → back={back}",
                sb.thumb_y
            );
        }
        std::fs::remove_file(&path).ok();
    }

    /// T17 验收 ②: 拖到顶 / 底之外一律**夹取**, 不越界。
    #[test]
    fn scroll_drag_clamps_at_both_ends() {
        let (v, path) = scroll_fixture("clamp");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let (rows_top, list_h) = list_geom(&v, area);
        let sb = v_scroll(area, rows_top, list_h, v.display_count(), 0.0).unwrap();
        assert_eq!(sb.top_row_at(sb.track_top - 500.0), 0.0, "拖到顶之上夹到 0");
        let bottom = sb.top_row_at(sb.track_top + sb.span + 500.0);
        assert!(
            (bottom - sb.max_top).abs() < 0.01,
            "拖到底之下夹到 max_top: 实得 {bottom}"
        );
        std::fs::remove_file(&path).ok();
    }

    /// 滚动条那一列上的矩形颜色 (轨道 + 拇指都在同一 x/宽上)。
    fn bar_rect_colors(v: &LogView, area: Rect) -> Vec<[f32; 4]> {
        let mut texts = TextBatch::new();
        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts);
        let track_x = area.origin.x + area.size.width - SCROLLBAR_W;
        let got = rects.instance_rects();
        let colors = rects.instance_colors();
        got.iter()
            .zip(colors.iter())
            .filter(|(r, _)| {
                (r.origin.x - track_x).abs() < 0.01 && (r.size.width - SCROLLBAR_W).abs() < 0.01
            })
            .map(|(_, c)| *c)
            .collect()
    }

    /// T17 验收 ④: 拇指的 hover 态与按下态**画得出来** —— 6px 的窄条不给反馈
    /// 就等于「摸不到」, 而 P30 的原始抱怨正是「在侧栏上滚滚轮列表不动」那类
    /// 「根本不知道这儿能用」的沉默。
    #[test]
    fn scroll_thumb_hover_and_drag_states_are_visible() {
        let (v, path) = scroll_fixture("states");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let th = v.theme.theme();

        let idle = bar_rect_colors(&v, area);
        assert!(idle.contains(&lin(th.border())), "常态: 拇指用 border 色");

        v.hover_bar.set(Some(BarAxis::Vertical));
        assert!(
            bar_rect_colors(&v, area).contains(&lin(th.text_secondary())),
            "悬停: 拇指须加深 (与常态不同色)"
        );

        v.hover_bar.set(None);
        v.v_drag.set(Some(0.0));
        assert!(
            bar_rect_colors(&v, area).contains(&lin(th.text_secondary())),
            "按住: 拇指须加深"
        );
        std::fs::remove_file(&path).ok();
    }

    /// T17: 按下拇指 → 拖动 → 抬起 这一整条手势链走通, 且**不进文本选区状态机**。
    ///
    /// 「不进」这条是防回归的关键: 两条状态机共用左键, 若滚动条按下落到
    /// `self.press` 上, 拖完条会顺手留下一段文本选区 (而它还是隐藏的)。
    #[test]
    fn scroll_bar_drag_tracks_the_pointer_without_touching_selection() {
        let (mut v, path) = scroll_fixture("drag");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let (rows_top, list_h) = list_geom(&v, area);
        let sb = v_scroll(area, rows_top, list_h, v.display_count(), 0.0).unwrap();
        let track_x = area.origin.x + area.size.width - SCROLLBAR_W;
        let grab_point = Point::new(track_x + SCROLLBAR_W / 2.0, sb.thumb_y + sb.thumb_h / 2.0);

        let mut msgs = danqing::widget::MsgQueue::new();
        assert_eq!(
            v.event(
                &Event::MouseInput {
                    button: MouseButton::Left,
                    pressed: true,
                    position: grab_point,
                },
                area,
                &mut msgs,
            ),
            EventResult::Consumed,
            "条上按下须被认领 (否则会去选行)"
        );
        assert!(v.v_drag.get().is_some(), "应进入拖拽态");
        assert!(v.press.is_none() && !v.dragging, "不得进文本选区状态机");

        // 往下拖半个 span → 应发出一个更大的 ScrollTo
        let moved = Point::new(grab_point.x, grab_point.y + sb.span / 2.0);
        msgs.clear();
        v.event(&Event::CursorMoved(moved), area, &mut msgs);
        let top = msgs
            .iter()
            .find_map(|m| match m.downcast_ref::<Msg>() {
                Some(Msg::ScrollTo { top, .. }) => Some(*top),
                _ => None,
            })
            .expect("拖动须发出 ScrollTo");
        assert!(top > 0.0, "往下拖应向下滚, 实得 {top}");
        assert!(v.selection.is_none(), "拖条不得产生文本选区");

        // 抬起 → 退出拖拽态
        msgs.clear();
        v.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: false,
                position: moved,
            },
            area,
            &mut msgs,
        );
        assert!(v.v_drag.get().is_none(), "抬起须退出拖拽态");
        std::fs::remove_file(&path).ok();
    }

    /// T17 验收 ⑤: **横条也要能拖**。
    ///
    /// 只做竖条会留下「两根同貌的拇指, 一根能拖一根不能」—— 那正是本模块要消灭的
    /// 形态。横条的成本不比竖条低 (状态机一样), 所以没有理由只做一半。
    /// T17 夹具: 200 行长行 (内容宽于视口 → 横条出现)。
    fn hscroll_fixture(tag: &str) -> (LogView, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "danqing-log-hscroll-{tag}-{}.log",
            std::process::id()
        ));
        let body: String = (0..200)
            .map(|i| format!("line {i} {}\n", "x".repeat(400)))
            .collect();
        std::fs::write(&path, body).unwrap();
        let mut v = LogView::new();
        v.file = Some(Arc::new(LogFile::open(&path).unwrap()));
        v.has_file = true;
        v.focused = true;
        v.gutter_w.set(56.0);
        v.mode = ViewMode::Raw;
        (v, path)
    }

    /// T17 回归锁: 横条的命中带**向下**延伸, 不吃列表末行的下半截。
    ///
    /// 本批第一版让它向上延伸 (`track_y - 6`, 凑满 12px 可拖带), 于是内容横向
    /// 溢出时, 列表末行最下面那一带点下去只会**开始拖横条** —— 那一带的行选中
    /// 从此没了。竖条没有这个问题: 它的带子在文本右缘之外, 本来就是内容区以外。
    #[test]
    fn horizontal_bar_hit_band_does_not_steal_row_clicks() {
        let (mut v, path) = hscroll_fixture("hitband");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut texts = TextBatch::new();
        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts); // 让 max_seen 落地
        let (rows_top, list_h) = list_geom(&v, area);

        // 轨道**上方 2px** —— 仍在列表区内, 但已不在可拖带上
        let y = rows_top + list_h - SCROLLBAR_H - 2.0;
        let x = LogView::text_x(&v, area) + 40.0;
        let msgs = press_at(&mut v, area, MouseButton::Left, Point::new(x, y));
        assert!(v.h_drag.get().is_none(), "轨道上方的点不该开始拖横条");
        assert!(
            msgs.iter()
                .any(|m| matches!(m.downcast_ref::<Msg>(), Some(Msg::Select(_)))),
            "该点应落到行上 (行选中), 而不是被横条吞掉"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn horizontal_scroll_bar_drags_and_round_trips() {
        let (mut v, path) = hscroll_fixture("drag");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut texts = TextBatch::new();
        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts); // 让 max_seen 落地
        let (rows_top, list_h) = list_geom(&v, area);
        let (_, hb) = v.bars(area, list_h);
        let hb = hb.expect("前提: 内容宽于视口, 横条出现");

        // 往返一致 (与竖条同一条不变量)
        for frac in [0.0, 0.5, 1.0] {
            let want = hb.max_x * frac;
            v.x_offset.set(want);
            let (_, probe) = v.bars(area, list_h);
            let probe = probe.unwrap();
            let back = probe.x_offset_at(probe.thumb_x);
            assert!(
                (back - want).abs() < 0.5,
                "横条往返应一致: {want} → thumb_x={} → {back}",
                probe.thumb_x
            );
        }

        // 抓拇指中段右拖 → 横向滚动; 抬起退出
        v.x_offset.set(0.0);
        let (_, hb) = v.bars(area, list_h);
        let hb = hb.unwrap();
        let y = rows_top + list_h - SCROLLBAR_H / 2.0;
        let p0 = Point::new(hb.thumb_x + 5.0, y);
        let mut msgs = danqing::widget::MsgQueue::new();
        assert_eq!(
            v.event(
                &Event::MouseInput {
                    button: MouseButton::Left,
                    pressed: true,
                    position: p0,
                },
                area,
                &mut msgs,
            ),
            EventResult::Consumed,
            "横条上按下须被认领"
        );
        assert!(v.h_drag.get().is_some(), "应进入横向拖拽态");
        assert!(v.v_drag.get().is_none(), "不得误入竖条拖拽态");

        let p1 = Point::new(p0.x + hb.span / 2.0, y);
        v.event(&Event::CursorMoved(p1), area, &mut msgs);
        assert!(v.x_offset.get() > 0.0, "右拖须横向滚动");

        v.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: false,
                position: p1,
            },
            area,
            &mut msgs,
        );
        assert!(v.h_drag.get().is_none(), "抬起须退出横向拖拽态");
        std::fs::remove_file(&path).ok();
    }

    /// T17: 指针停在条上才给手型 —— `cursor_icon` 无位置参数, 靠 `CursorMoved` 缓存。
    #[test]
    fn scroll_bar_shows_a_pointer_cursor_only_on_the_bar() {
        let (mut v, path) = scroll_fixture("cursor");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let (rows_top, _list_h) = list_geom(&v, area);
        let track_x = area.origin.x + area.size.width - SCROLLBAR_W;
        let mut msgs = danqing::widget::MsgQueue::new();

        v.event(
            &Event::CursorMoved(Point::new(track_x + 2.0, rows_top + 40.0)),
            area,
            &mut msgs,
        );
        assert_eq!(v.cursor_icon(), Some(CursorIcon::Pointer), "条上给手型");

        v.event(
            &Event::CursorMoved(Point::new(200.0, rows_top + 40.0)),
            area,
            &mut msgs,
        );
        assert_eq!(v.cursor_icon(), None, "列表身上不给手型");
        std::fs::remove_file(&path).ok();
    }

    /// T18 (P17) 回归锁: 复制**成功要有回执**, 且三级来源各自说清。
    ///
    /// 原先成功时零反馈 (只在超限时出声), 用户不知道剪贴板里进了什么。
    /// 判据取**消息队列**里那条 Notice, 并要求三级回执**互不相同** ——
    /// 只说「已复制」不说复制了什么的回执, 等于没回答 P17 问的那个「什么」。
    #[test]
    fn copy_success_reports_what_was_copied() {
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        type Setup = fn(&mut LogView);
        let cases: [(&str, Setup); 3] = [
            // 跨两行 —— 回执要能报出「几条」, 单行断言不出这个数
            ("文本选区", |v| {
                v.selection = Some(TextSelection::new((0, 0), (1, 1)));
            }),
            ("单元格", |v| v.selected_cell = Some((0, 1))),
            ("行", |_| {}),
        ];
        let mut receipts: Vec<String> = Vec::new();
        for (i, (name, setup)) in cases.iter().enumerate() {
            let (mut v, path) = cell_fixture(&format!("t18-{i}"));
            setup(&mut v);
            assert!(v.copy_source().is_some(), "{name}: 前提, 该级复制得到");
            let mut msgs = danqing::widget::MsgQueue::new();
            assert_eq!(
                v.event(&Event::Copy, area, &mut msgs),
                EventResult::Consumed,
                "{name}: 有内容须消费 —— 那也是框架真去写剪贴板的充要条件"
            );
            let text = msgs
                .iter()
                .find_map(|m| match m.downcast_ref::<Msg>() {
                    Some(Msg::Notice(t, _)) => Some(t.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{name}: 复制成功须有回执"));
            assert!(!text.is_empty(), "{name}: 回执不得为空串");
            receipts.push(text);
            std::fs::remove_file(&path).ok();
        }
        assert_ne!(receipts[0], receipts[1], "选区与单元格的回执须分得开");
        assert_ne!(receipts[1], receipts[2], "单元格与整行的回执须分得开");
        assert!(
            receipts[0].contains('2'),
            "选区回执要带上「几条」: {}",
            receipts[0]
        );
    }

    /// T21 (P39) 的另一半: 栏**已持焦**时收到「回到搜索栏」→ 全选草稿。
    ///
    /// 判据是 `selected_text()` 拿到整段 —— 也就是「按下任意键会被整段替换」的
    /// 那个状态; 只要 `value()` 还在, 就证明没有清空。
    #[test]
    fn refocus_selects_the_existing_draft() {
        let mut app = LogApp::new_empty();
        let mut bar =
            bar_for_test(false).bind_refocus_search(|app: &LogApp| app.search_refocus_rev);
        // FocusIn 由 `Bar::event` 转发给**当前生效**的那个输入框, 故这里让
        // 生效角色是搜索 (正式路径下由 `sync` 按模式算出来)。
        bar.active = ActiveBar::Search;
        bar.search_ti.set_text("level=ERROR");
        let area = wide_bar_area();
        let mut msgs = danqing::widget::MsgQueue::new();
        bar.event(&Event::FocusIn, area, &mut msgs);
        assert!(bar.search_ti.is_focused(), "前提: 栏已持焦");
        assert!(bar.search_ti.selected_text().is_none(), "前提: 尚无选区");

        bar.sync(&app); // rev 未变 → 不该动
        assert!(bar.search_ti.selected_text().is_none(), "rev 未变不得全选");

        app.search_refocus_rev += 1;
        bar.sync(&app);
        assert_eq!(
            bar.search_ti.selected_text().as_deref(),
            Some("level=ERROR"),
            "已持焦时须全选草稿 (便于直接覆写)"
        );
        assert_eq!(bar.search_ti.value(), "level=ERROR", "且**不得清空**");
    }

    /// 按下某一键于某点, 返回这一发产生的消息。
    fn press_at(v: &mut LogView, area: Rect, button: MouseButton, p: Point) -> Vec<Box<dyn Any>> {
        let mut msgs = danqing::widget::MsgQueue::new();
        v.event(
            &Event::MouseInput {
                button,
                pressed: true,
                position: p,
            },
            area,
            &mut msgs,
        );
        msgs
    }

    /// 消息队里有没有这两种「左键专属」的副作用。
    fn acted_like_left(msgs: &[Box<dyn Any>]) -> bool {
        msgs.iter().any(|m| {
            matches!(
                m.downcast_ref::<Msg>(),
                Some(Msg::Select(_)) | Some(Msg::OpenSettings)
            )
        })
    }

    /// T15 (P29) 回归锁: 右键 / 中键**不**冒充左键。
    ///
    /// 判据取**消息队列**而不是返回值: 返回值是 `Ignored` 也可能是「照样发了消息
    /// 然后说没做」, 只测返回值会放过那种形态 (M3 那批同款教训)。
    /// 底栏 ⚙ 的命中矩形由 paint 缓存, 所以必须先画一遍它才是真的。
    #[test]
    fn right_and_middle_buttons_do_not_act_like_the_left_one() {
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let row_point = Point::new(300.0, HEADER_H + 5.0);
        for (i, (name, button)) in [("右键", MouseButton::Right), ("中键", MouseButton::Middle)]
            .into_iter()
            .enumerate()
        {
            let (mut v, path) = cell_fixture(&format!("t15-{i}"));
            let mut texts = TextBatch::new();
            let mut rects = RectBatch::new();
            v.paint(area, &mut rects, &mut texts); // 让 settings_btn_rect 有真值
            let gear = v.settings_btn_rect.get();
            let gear_point = Point::new(gear.origin.x + 2.0, gear.origin.y + 2.0);

            let on_row = press_at(&mut v, area, button, row_point);
            assert!(!acted_like_left(&on_row), "{name} 点行不得选中该行");
            let on_gear = press_at(&mut v, area, button, gear_point);
            assert!(!acted_like_left(&on_gear), "{name} 点底栏 ⚙ 不得开设置卡");

            std::fs::remove_file(&path).ok();
        }

        // 左键对照: 同样两处必须**照常**生效 (修的是误触发, 不是把左键一起关掉)
        let (mut v, path) = cell_fixture("t15-left");
        let mut texts = TextBatch::new();
        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts);
        let gear = v.settings_btn_rect.get();
        assert!(
            acted_like_left(&press_at(&mut v, area, MouseButton::Left, row_point)),
            "左键仍须选中行"
        );
        assert!(
            acted_like_left(&press_at(
                &mut v,
                area,
                MouseButton::Left,
                Point::new(gear.origin.x + 2.0, gear.origin.y + 2.0)
            )),
            "左键仍须开设置卡"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn level_color_prefers_error_over_info() {
        // 行首含 ERROR 与 "informational" 之类干扰时仍判 ERROR
        assert_eq!(
            level_color(b"2026-09-05 INFO ok", &LightTheme),
            LightTheme.text_primary(),
            "INFO 走默认色"
        );
        // 断言「判到了哪一支」, 而**不是**「那一支长什么样」。
        //
        // 原先这里写的是通道阈值 (`err.r > 0.7` / `warn.g > 0.4` 一类) —— 那是拿
        // 色值当分类的代理, 两个东西一起测。2026-09-13 D2 按 AA 把浅色压暗一遍,
        // WARN 的 `r` 从 0.70 落到 0.54, 这条**假红**了: 分类明明是对的。
        // 分类与取值分开测 —— 取值有 `light_semantic_colors_clear_wcag_aa_*` 管。
        let pal = LevelPalette::light_line();
        for (line, want, what) in [
            (&b"2026-09-05 ERROR disk full"[..], pal.error, "ERROR"),
            (&b"FATAL boom"[..], pal.error, "FATAL"),
            (&b"WARN slow query"[..], pal.warn, "WARN"),
            (&b"DEBUG cache miss"[..], pal.trace, "DEBUG"),
            (&b"TRACE tick"[..], pal.trace, "TRACE"),
        ] {
            assert_eq!(level_color(line, &LightTheme), want, "{what} 没判到对应色");
        }
    }

    /// 线性空间合成后的亮度 (与 GPU 一致)。
    ///
    /// **不要改用公开的 `danqing::composite_over`** —— 那个在 **sRGB 空间**混
    /// (其自身测试 `composite_over_half_white_on_black_is_mid_gray` 把这个行为钉死了),
    /// 而模块 1 之后 GPU 在**线性空间**混。用它量出来的数**不是屏幕上的数**
    /// —— 正是「护栏全绿、屏幕全灰」那类事故的入口。框架那边同样有个私有
    /// `composited_luminance` 走线性, 本函数与它同口径。
    fn composited_luminance(top: Color, base: Color) -> f32 {
        use danqing::srgb_to_linear;
        let mix = |t: f32, b: f32| top.a * t + (1.0 - top.a) * b;
        0.2126 * mix(srgb_to_linear(top.r), srgb_to_linear(base.r))
            + 0.7152 * mix(srgb_to_linear(top.g), srgb_to_linear(base.g))
            + 0.0722 * mix(srgb_to_linear(top.b), srgb_to_linear(base.b))
    }

    /// 一支配色压在给定背景上的对比度 (背景以**渲染后**的亮度参与)。
    fn ratio_on(fg: Color, bg_luminance: f32) -> f32 {
        let l = danqing::relative_luminance(fg);
        let (hi, lo) = if l >= bg_luminance {
            (l, bg_luminance)
        } else {
            (bg_luminance, l)
        };
        (hi + 0.05) / (lo + 0.05)
    }

    /// **暗色的语义色必须过 AA** —— 回归锁 (2026-09-13, 用户实机报)。
    ///
    /// 触发: 用户在暗色下选中一行 ERROR, 红字「看得眼花」。查下来一半是选区带
    /// (已在框架侧锁), 另一半是**这五支从来只有浅色一套值** —— 深饱和色压在近黑底上,
    /// 实测 ERROR 3.03 / INFO 3.70 / OK 4.28, **全部低于 WCAG AA (4.5)**。
    ///
    /// **只卡暗色**: 浅色那套的 WARN 3.18 / DEBUG 2.97 / OK 3.78 同样不过 AA ——
    /// 那条欠账当时**另立**了 (D2), 已于 2026-09-13 收口, 见下一条
    /// [`Self::light_semantic_colors_clear_wcag_aa_on_light_surfaces`]。
    ///
    /// 有牙齿: 修之前 ERROR 3.03, 直接红。
    #[test]
    fn dark_semantic_colors_clear_wcag_aa_on_the_dark_background() {
        const AA: f32 = 4.5;
        let bg = danqing::relative_luminance(DarkTheme.background());
        let pal = LevelPalette::for_cell(&DarkTheme);
        for (name, c) in [
            ("error", pal.error),
            ("warn", pal.warn),
            ("ok", pal.ok),
            ("info", pal.info),
            ("trace", pal.trace),
        ] {
            let ratio = ratio_on(c, bg);
            assert!(
                ratio >= AA,
                "暗色的 {name} 对底色只有 {ratio:.2} < {AA}: {c:?}"
            );
        }
    }

    /// **浅色的语义色同样要过 AA** —— 与暗色那条**同一把尺** (2026-09-13, D2 收口)。
    ///
    /// 触发: 审查浅色截图时发现选中的那一行 `status=400` 读不出来, 一度当成**独立缺陷**
    /// (「选中行上的语义色碰撞」); 算完才清楚**它不是一个新问题, 就是这条欠账显形** ——
    /// 那套值只对**白底**成立, 换成页面底 `#F0F8F6` 就掉下来。
    ///
    /// **判据分两档** (与暗色侧完全一致): **常驻面** (页面底 / 斑马行) 要 4.5,
    /// **瞬时面** (hover / 选中行) 只要 3.0 —— 瞬时态那条与框架侧的
    /// `selection_band_does_not_step_too_far_from_the_background` 从两边一夹。
    ///
    /// **斑马行必须进常驻档**: 表里一半的行就是斑马底。只量页面底 = 漏掉一半的行,
    /// 这与「面阶梯」那次翻车是**同一个错** (判据对页面底, 而屏幕上挨着的是别的面)。
    ///
    /// 两条口径**各测一遍**: 分叉是真实存在的 (D3), 漏测一条等于放走一半的着色路径。
    ///
    /// 有牙齿: 修之前 WARN 3.25 / OK 3.78 / DEBUG 3.09, 三条一起红。
    #[test]
    fn light_semantic_colors_clear_wcag_aa_on_light_surfaces() {
        const AA: f32 = 4.5;
        const FLOOR: f32 = 3.0;
        let th = LightTheme;
        // 底色一律取**渲染后**的亮度 (与屏幕上一致): 斑马/hover 是实色,
        // 选中行要先经**线性**合成 —— 别用 `danqing::composite_over`, 它在 sRGB 空间混。
        let page = danqing::relative_luminance(th.background());
        let zebra = danqing::relative_luminance(row_band_bg(crate::config::AppTheme::Light));
        let hover = danqing::relative_luminance(row_hover_bg(crate::config::AppTheme::Light));
        let selected = composited_luminance(th.selection(), th.background());
        let surfaces = [
            ("页面底", page, AA),
            ("斑马行", zebra, AA),
            ("hover", hover, FLOOR),
            ("选中行", selected, FLOOR),
        ];
        let palettes = [
            ("整行着色", LevelPalette::for_line(&th)),
            ("单元格/状态", LevelPalette::for_cell(&th)),
        ];
        for (path, pal) in palettes {
            for (name, c) in [
                ("error", pal.error),
                ("warn", pal.warn),
                ("ok", pal.ok),
                ("info", pal.info),
                ("trace", pal.trace),
            ] {
                for (surface, bg, bar) in surfaces {
                    let ratio = ratio_on(c, bg);
                    assert!(
                        ratio >= bar,
                        "{path} 口径 {name} 压 {surface} 只有 {ratio:.2} 低于 {bar}: {c:?}"
                    );
                }
            }
        }
    }

    /// **选中行上的语义色也要读得出来** —— 这正是用户截图里的那一格。
    ///
    /// 底色取**合成后**的选区带 (跟屏幕一致), 不是裸 `background()`。
    /// 阈值 3.0 低于 AA 的 4.5: 选中是**瞬时态**, 且框架侧的
    /// `selection_band_does_not_step_too_far_from_the_background` 已经从另一边
    /// 把选区带的强度卡住了 —— 两条锁一夹, 中间这点余量才守得住。
    #[test]
    fn dark_semantic_colors_stay_readable_on_a_selected_row() {
        const FLOOR: f32 = 3.0;
        let band = composited_luminance(DarkTheme.selection(), DarkTheme.background());
        let pal = LevelPalette::for_cell(&DarkTheme);
        for (name, c) in [
            ("error", pal.error),
            ("warn", pal.warn),
            ("ok", pal.ok),
            ("info", pal.info),
            ("trace", pal.trace),
        ] {
            let ratio = ratio_on(c, band);
            assert!(
                ratio >= FLOOR,
                "暗色选中行上的 {name} 只有 {ratio:.2} < {FLOOR}: {c:?}"
            );
        }
    }

    /// 斑马纹与 hover 必须是**两个通道** —— 原先两者共用 `th.surface_variant()`。
    ///
    /// 回归锁 (2026-09-13, 用户实机报): 同色的后果是
    /// **悬停奇数行时颜色完全不变**; **悬停偶数行时** hovered 行与左右两条斑马行
    /// 变成同一色, **三行连成一整块**, 读不出指针在哪行。浅色还额外糟一层 ——
    /// `surface_variant()` 在浅色下 Δ`L*` 只有 **−0.74** (差 2/255), hover 基本看不见。
    ///
    /// 断言三件事: 两两不同 (与底色也不同)、**hover 的台阶必须大于斑马的**
    /// (否则「进到哪一行」读不出来)。
    #[test]
    fn row_band_and_hover_are_separate_channels() {
        for app in [
            crate::config::AppTheme::Light,
            crate::config::AppTheme::Dark,
        ] {
            let bg = danqing::relative_luminance(app.theme().background());
            let band = danqing::relative_luminance(row_band_bg(app));
            let hover = danqing::relative_luminance(row_hover_bg(app));
            assert_ne!(
                band, hover,
                "{app:?}: 斑马与 hover 不能同色 (同色即三行连片)"
            );
            assert_ne!(band, bg, "{app:?}: 斑马要与底色分得开");
            assert_ne!(hover, bg, "{app:?}: hover 要与底色分得开");
            assert!(
                (hover - bg).abs() > (band - bg).abs(),
                "{app:?}: hover 的台阶必须大于斑马的 ({hover:.5} vs {band:.5}, 底 {bg:.5})"
            );
        }
    }

    /// 展开块底色必须比内容区底**略深**, 且两个主题都要成立。
    ///
    /// 回归锁 (2026-09-13, 用户裁定「略深」): 展开的子行原先与真实行**长得一模一样**
    /// (只差没有行号), 用户分不清「这坨是第 1 行展开的」还是「又是几行日志」。
    #[test]
    fn expand_block_bg_is_a_slightly_darker_step_in_both_themes() {
        for app in [
            crate::config::AppTheme::Light,
            crate::config::AppTheme::Dark,
        ] {
            let block = expand_block_bg(app);
            let bg = app.theme().background();
            assert_ne!(block, bg, "{app:?}: 展开块必须与内容区底区分得开");
            assert!(
                danqing::relative_luminance(block) < danqing::relative_luminance(bg),
                "{app:?}: 用户裁定是**略深**, 不是略浅"
            );
        }
    }

    /// 合成后亮度的 `L*` (输入是 [`composited_luminance`] 的输出)。
    fn l_star_of(y: f32) -> f32 {
        if y > 0.008856 {
            116.0 * y.powf(1.0 / 3.0) - 16.0
        } else {
            903.3 * y
        }
    }

    /// **表格各「面」必须与相邻面拉开 ≥3 Δ`L*`** —— 这一轮最重要的一条锁。
    ///
    /// 触发 (2026-09-13, 用户发真机截图要我做整体审查): 一次照出**两处撞车**——
    /// 暗色表头 ↔ 斑马 `ΔL*` **0.08**、浅色展开块 ↔ 斑马 **0.19**。两处都
    /// 「对页面底完全合格」(6.30 / −3.68), 所以既有的那些锁一条都没红。
    ///
    /// **根因是判据错了, 不是取值偏了**: 记档里所有台阶都是相对**页面底**量的,
    /// 但屏幕上跟展开块挨着的不是页面底、是**斑马行**。所以本锁只问邻居:
    ///
    /// - 每个面与**页面底** ≥3 (自己得看得见)
    /// - **两两之间** ≥3 (挨着时得分得开)
    ///
    /// **已知例外 (明写在案, 不是漏掉)**: 浅色 `hover` ↔ `选中` 只有 **1.50**。
    /// 浅色那段可用明度区间养不起六个两两 ≥3 的面 (六个最少要 15 个点, 而近白底
    /// 到「还能算浅色 UI」只剩约 14 点), 故这一对**不锁** —— 它们的区分交给第二条
    /// 通道 (选中是 accent 冷青、hover 是中性灰绿)。要真拉平得重排整条阶梯。
    #[test]
    fn table_surfaces_are_separated_from_their_neighbours() {
        const MIN: f32 = 3.0;

        fn check(app: crate::config::AppTheme) {
            let th = app.theme();
            let bg = th.background();
            let base = l_star_of(danqing::relative_luminance(bg));
            let l = |c: Color| l_star_of(composited_luminance(c, bg));

            let surfaces = [
                ("页面底", base),
                ("斑马", l(row_band_bg(app))),
                ("表头", l(th.surface_variant())),
                ("hover", l(row_hover_bg(app))),
                ("选中", l(th.selection())),
                ("展开块", l(expand_block_bg(app))),
            ];

            for (name, v) in &surfaces[1..] {
                assert!(
                    (v - base).abs() >= MIN,
                    "{app:?}: {name} 对页面底 ΔL* 只有 {:.2} < {MIN} —— 自己就看不见",
                    (v - base).abs()
                );
            }
            for i in 1..surfaces.len() {
                for j in (i + 1)..surfaces.len() {
                    let (na, va) = surfaces[i];
                    let (nb, vb) = surfaces[j];
                    // 已知例外: 浅色 hover ↔ 选中 (见文档注释)。
                    if na == "hover" && nb == "选中" && app == crate::config::AppTheme::Light {
                        continue;
                    }
                    assert!(
                        (va - vb).abs() >= MIN,
                        "{app:?}: {na} ↔ {nb} 的 ΔL* 只有 {:.2} < {MIN} —— 挨在一起时分不开",
                        (va - vb).abs()
                    );
                }
            }
        }
        check(crate::config::AppTheme::Light);
        check(crate::config::AppTheme::Dark);
    }

    /// 书签行号色: 两主题各一支金, 且**不能**等于次级正文色。
    ///
    /// 回归锁 (2026-09-13): 书签色原先是写死的 `0.75,0.60,0.10` —— 那是配白底的,
    /// 近黑底上偏暗。旧清单建议「从 `Theme.accent` 派生」, **没有采纳**:
    /// accent 已经用于选中行 / 焦点边框 / 指示线, 书签套上去会把**第三类语义**
    /// 混进「选中/强调」那一个通道, 扫一眼分不出哪行是书签、哪行是选中。
    #[test]
    fn bookmark_color_is_its_own_channel_per_theme() {
        let light = bookmark_color(crate::config::AppTheme::Light);
        let dark = bookmark_color(crate::config::AppTheme::Dark);
        assert_ne!(light, dark, "两主题各一支金");
        assert_ne!(
            light,
            LightTheme.text_secondary(),
            "书签是独立通道, 不能退化成次级正文色"
        );
        assert_ne!(dark, DarkTheme.text_secondary(), "同上");
        assert_ne!(
            light,
            LightTheme.accent(),
            "不能混进 accent —— 那是选中/强调的通道"
        );
    }

    /// 单元格高亮的**描边**必须与底笔可分辨。
    ///
    /// 回归锁 (2026-09-14 审查 #2): 本模块原先把单元格画成**只有一笔**
    /// `th.selection()` —— 与行选中底色同一个 token, 而双击单元格必然同时选中
    /// 该行 (行底色已在下面铺过), 于是整行一片同色, 看不出选中的是哪一列。
    /// 而 Ctrl+C 只复制这一格, 视觉 (整行) 与结果 (单格) 自相矛盾。
    /// 底笔与行选中同色是有意的, 所以这条锁只钉**描边那一笔必不相同**。
    #[test]
    fn cell_highlight_border_is_a_second_stroke() {
        let (light_fill, light_border) = cell_highlight_colors(&LightTheme);
        assert_ne!(
            light_fill, light_border,
            "浅色: 描边与底色同色 = 看不出选中的是哪一格"
        );
        let (dark_fill, dark_border) = cell_highlight_colors(&DarkTheme);
        assert_ne!(dark_fill, dark_border, "暗色同上");
    }

    /// `header_line` 必须取**当前主题**的分割线色。
    ///
    /// 回归锁 (2026-09-13): 它原先恒用 `LightTheme.divider()`, 完全不看当前主题 ——
    /// 表头下划线与状态栏顶线在暗色下用的是浅色主题那条 (黑 10%), 压在近黑底上
    /// 等于没有。两处调用点 (`:563` / `:890`) 都白描了一道看不见的线。
    #[test]
    fn header_line_follows_theme() {
        assert_eq!(header_line(&LightTheme), LightTheme.divider(), "浅色");
        assert_eq!(header_line(&DarkTheme), DarkTheme.divider(), "暗色");
        assert_ne!(
            header_line(&LightTheme),
            header_line(&DarkTheme),
            "两主题的分割线色必须不同 —— 相同即等于没跟随"
        );
    }

    /// 过滤/搜索栏的输入色必须跟着主题走。
    ///
    /// 回归锁 (2026-09-13): `base_input()` 曾写死 `TextInput::themed(&LightTheme)`
    /// 加三个写死色 (正文 `0.20` / 光标 `0.10` / 选区)。**空态看不出来** ——
    /// 占位色是中性灰, 在两个主题上都读得动 —— 所以这个问题一直藏到打字才发作:
    /// 暗色下输入文字 (51,51,51) 压在栏底 (38,38,43) 上, WCAG 对比度 **1.19**。
    ///
    /// 断言用 `Bar::sync` 走一遍真实路径: 绑定挂在 `TextInput` 上,
    /// 但 `Bar` 把它当字段持有, 框架不会替它传播 `sync` —— 漏调一样是白搭。
    #[test]
    fn filter_input_color_follows_theme() {
        let mut bar = Bar::default();
        let mut app = crate::LogApp::new_empty();
        app.theme = crate::config::AppTheme::Dark;
        bar.sync(&app);
        assert_eq!(
            bar.filter_ti.text_color(),
            danqing::theme::DarkTheme.text_primary(),
            "暗色主题下过滤栏输入文字应是该主题的正文色"
        );

        app.theme = crate::config::AppTheme::Light;
        bar.sync(&app);
        assert_eq!(
            bar.search_ti.text_color(),
            danqing::theme::LightTheme.text_primary(),
            "切回浅色后搜索栏输入文字应跟着变"
        );
    }

    #[test]
    fn body_color_follows_theme_instead_of_hardcoded_near_black() {
        // 回归锁 (2026-09-13): 正文色曾写死 `0.12,0.12,0.12` —— 那是浅色主题的近黑。
        // 双重 gamma 修好后, 它在暗色背景上就**真的是近黑**, 整列消失 (用户实机报的
        // 「暗色只剩 level 列」)。正文色必须跟主题走。
        assert_eq!(
            cell_color("msg", "request completed", &LightTheme),
            LightTheme.text_primary(),
            "浅色: 正文色 = text_primary"
        );
        assert_eq!(
            cell_color("msg", "request completed", &DarkTheme),
            DarkTheme.text_primary(),
            "暗色: 正文色 = text_primary"
        );
        assert_eq!(
            level_color(b"2026-09-05 INFO ok", &DarkTheme),
            DarkTheme.text_primary(),
            "暗色: 未命中级别时同样走 token"
        );
        assert_eq!(
            cell_color("status", "N/A", &DarkTheme),
            DarkTheme.text_primary(),
            "暗色: 非数字 status 同样走 token"
        );
        // 实质保证: 暗色正文色必须**够亮**, 否则「跟了 token」也只是换个名字看不见。
        assert!(
            danqing::relative_luminance(DarkTheme.text_primary()) > 0.5,
            "暗色正文色须足够亮"
        );
    }

    /// **一段连续子行只许出一个底面矩形** —— 回归锁 (2026-09-13, 用户实机报)。
    ///
    /// 触发: 用户报「浅色主题, 行展开区域出现行间隔」。截图实测那条缝**横贯整行**,
    /// 渲染成 `(207,216,212)`, 而块色是 `(200,209,205)`、页面底 `(240,248,246)` ——
    /// 既不是块色也不是底色, 是**两个矩形各自的边缘抗锯齿叠出来的**: 相邻两行的
    /// 矩形在逻辑坐标上严丝合缝, 但每个都自己跟底色做一次半透明混合, 谁也盖不满。
    ///
    /// **不是新缺陷**: 浅色块色原为 `#E4EEEA` (对底 Δ`L*` 3.68), 缝与块色只差约
    /// 2/255 —— 看不见; 换成 `#C8D1CD` (Δ`L*` 13.88) 后一眼就是「行间隔」。
    /// 与 D1、面阶梯同属一类: 换个取值, 早先就错的东西才现形。
    ///
    /// 有牙齿: 改回逐行铺 → 六子行的一段出**六个**矩形, 第一条断言直接红。
    #[test]
    fn expand_block_rects_put_a_contiguous_run_in_one_rect() {
        let y_of = |j: u64| j as f32 * ROW_HEIGHT;
        // 3..9 是一段 (六个子行), 12 单独一段
        let is_sub = |j: u64| (3..9).contains(&j) || j == 12;
        let rects = expand_block_rects(0, 20, y_of, is_sub, 10.0, 100.0);
        assert_eq!(rects.len(), 2, "两段连续子行 = 两个矩形, 实得 {rects:?}");
        assert_eq!(rects[0].origin.y, 3.0 * ROW_HEIGHT, "首段上沿");
        assert_eq!(
            rects[0].size.height,
            6.0 * ROW_HEIGHT,
            "首段要**一整块**盖住六个子行, 中间不许断开"
        );
        assert_eq!(rects[1].origin.y, 12.0 * ROW_HEIGHT, "次段上沿");
        assert_eq!(rects[1].size.height, ROW_HEIGHT);
        assert_eq!(rects[0].origin.x, 10.0, "横向上沿内容区左沿");
        assert_eq!(rects[0].size.width, 100.0);

        // 段首在视口上方 (滚动到底时会遇到): 往上多铺一行, 那道边界才不会
        // 正好落在视口第一行
        let rects = expand_block_rects(5, 20, y_of, is_sub, 0.0, 100.0);
        assert_eq!(rects[0].origin.y, 4.0 * ROW_HEIGHT, "上沿补一行");

        // 段被可见区截断: 底边必须够到 `limit`, 否则视口底部会缺一块
        let rects = expand_block_rects(3, 6, y_of, is_sub, 0.0, 100.0);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].size.height, 3.0 * ROW_HEIGHT, "截到 limit 为止");

        // 没有子行 → 一个矩形都不出 (普通表格每帧都走这条)
        assert!(expand_block_rects(0, 20, y_of, |_| false, 0.0, 100.0).is_empty());
    }

    /// 把 sRGB 色转成 `instance_colors()` 的线性空间 `[r, g, b, a]` (模块 1 修
    /// 双重 gamma 后, `instance_colors()` 返回的是线性分量 —— 见 `rect.rs:398-402`
    /// 的 doc(hidden) 注释: 直接拿 token 的 sRGB 分量比, 断言的是修好之前的旧行为)。
    ///
    /// **不引框架私有类型** (`danqing::render` 是私有模块, `lib.rs:27` 无 `pub`);
    /// 用本仓已有的 `composited_luminance` 同口径手写。
    fn lin(c: danqing::Color) -> [f32; 4] {
        use danqing::srgb_to_linear;
        [
            srgb_to_linear(c.r),
            srgb_to_linear(c.g),
            srgb_to_linear(c.b),
            c.a,
        ]
    }

    /// 更新角标显隐两态对拍 (SPEC-update-badge D1): 有提示恰画 1 个 accent 圆点,
    /// 无提示**零痕迹** (批里一个实例都不许有)。
    ///
    /// 断言走 `RectBatch` 实例内省, 不走返回值 —— A/B: 摘掉 `paint_update_dot` 里的
    /// `push_rect` 本锁精确红; 只断言返回值的话, 「算了几何但没画」会漏网
    /// (P8/P10「有状态却没画」的原始形态)。
    #[test]
    fn settings_button_shows_update_dot_only_when_hint_exists() {
        let accent = Color::from_srgb8(0, 120, 212);

        let mut rects = RectBatch::new();
        paint_update_dot(&mut rects, 100.0, 20.0, false, accent);
        assert!(
            rects.is_empty(),
            "无提示必须零痕迹, 实际画了 {}",
            rects.len()
        );

        let mut rects = RectBatch::new();
        paint_update_dot(&mut rects, 100.0, 20.0, true, accent);
        assert_eq!(rects.len(), 1, "有提示恰画一个圆点");
        // 颜色按 token 传入色验 (linear 口径, 与 instance_colors 同源), 不钉绝对色号。
        assert_eq!(
            rects.instance_colors(),
            vec![lin(accent)],
            "圆点必须用 accent 色"
        );
        // 半径 = 直径之半 (正圆)。
        assert_eq!(rects.instance_radii(), vec![[UPDATE_DOT_D / 2.0; 4]]);
    }

    /// 角标几何 (SPEC-update-badge D1): 6px, 挂**文字**右上 —— 不压文字 (X 在文字右缘外),
    /// Y 与文字顶**齐平** (锚在文字, 不锚行顶); 且**整个角标落在设置入口命中矩形内**
    /// (D3 点击面覆盖可见角标, 评审抓出「半个点不中」的缺口)。paint 与纯几何同源。
    #[test]
    fn update_dot_sits_at_the_label_corner_and_outside_layout() {
        let (text_right, text_top, row_top) = (100.0, 20.0, 10.0);
        let dot = update_dot_rect(text_right, text_top);
        assert_eq!(
            (dot.size.width, dot.size.height),
            (UPDATE_DOT_D, UPDATE_DOT_D),
            "圆点是 6px 正方 (D1)"
        );
        assert!(
            dot.origin.x >= text_right,
            "圆点不得压在文字上: x={} < 文字右缘 {}",
            dot.origin.x,
            text_right
        );
        assert_eq!(
            dot.origin.y, text_top,
            "圆点与**文字顶**齐平 —— 锚在文字, 不锚行顶 (行顶会把点浮到标签上方)"
        );
        // 命中矩形必须吞并角标 (与 paint 同一几何源)。
        let hit = settings_hit_rect(text_right - 50.0, 50.0, text_top, row_top);
        assert!(
            hit.origin.x <= dot.origin.x
                && dot.origin.x + dot.size.width <= hit.origin.x + hit.size.width,
            "角标必须整个落在设置入口命中矩形内 (否则右半点不中)"
        );
        // paint 与纯几何同源: 画进批里的就是 `update_dot_rect` 算的那一个。
        let mut rects = RectBatch::new();
        paint_update_dot(&mut rects, text_right, text_top, true, Color::TRANSPARENT);
        let got = &rects.instance_rects()[0];
        assert_eq!(
            (got.origin.x, got.origin.y, got.size.width, got.size.height),
            (dot.origin.x, dot.origin.y, dot.size.width, dot.size.height),
            "paint 画的必须与纯几何是同一个矩形 (同源)"
        );
    }

    /// **接线锁** (评审 Required): `LogView::paint` 真把角标画出来 —— 只测
    /// `paint_update_dot` 本体时, 把调用行删掉/写死 `false` 全套测试照样绿,
    /// 角标永不显示 (P8/P10「有状态却没画」在接线层复活)。
    /// 两态走**字段注入** (spec §5 惯例, 不碰全局 publish); 删调用行 → 添矩形数 0, 本锁红。
    #[test]
    fn log_view_paint_wires_the_update_dot() {
        let (mut v, path) = cell_fixture("badge");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);

        v.has_update_hint = false;
        let mut rects_plain = RectBatch::new();
        let mut texts = TextBatch::new();
        v.paint(area, &mut rects_plain, &mut texts);

        v.has_update_hint = true;
        let mut rects_dot = RectBatch::new();
        let mut texts = TextBatch::new();
        v.paint(area, &mut rects_dot, &mut texts);

        let key = |r: &Rect| (r.origin.x, r.origin.y, r.size.width, r.size.height);
        let plain: Vec<Rect> = rects_plain.instance_rects();
        let with_dot: Vec<Rect> = rects_dot.instance_rects();
        // f32 无 Eq/Hash (NaN 语义) —— 用 Vec::contains (PartialEq); 夹具矩形量级 O(n²) 无虞。
        let plain_keys: Vec<_> = plain.iter().map(key).collect();
        let added: Vec<_> = with_dot
            .iter()
            .filter(|r| !plain_keys.contains(&key(r)))
            .map(key)
            .collect();
        assert_eq!(
            added.len(),
            1,
            "角标恰添一个矩形 (接线删了/写死 false 在此红): {added:?}"
        );
        assert_eq!(
            (added[0].2, added[0].3),
            (UPDATE_DOT_D, UPDATE_DOT_D),
            "新增矩形是 6px 圆点"
        );
        // 颜色按**重数差**验: 新增的恰是 accent 色实例 (与 helper 锁同口径)。
        let accent = lin(v.theme.theme().accent());
        let count = |cs: Vec<[f32; 4]>| cs.iter().filter(|c| **c == accent).count();
        assert_eq!(
            count(rects_dot.instance_colors()),
            count(rects_plain.instance_colors()) + 1,
            "新增圆点必须是 accent 色"
        );

        std::fs::remove_file(&path).ok();
    }

    /// T6 回归锁 (2026-09-14 实机 M0 P10): 拖框选时**选中行不得消失**。
    ///
    /// 原先 `if selected && !has_text_sel {…} else if hover {…}` 有两重压制:
    /// ① 有文本选区时选中行的底与左 accent 竖条一起熄灭;
    /// ② hover 与选中行共用 else-if, 选中行上 hover 也熄灭。
    /// 修复 = 各自独立: 选中**永画**, hover **永画**, 文本选区只画区间带。
    ///
    /// 断言的是 **paint 出来的矩形序列**, 不是标志位 —— 「有状态却没画」正是
    /// P8/P10 的原始形态, 只测 `v.selected` 会漏掉它。
    #[test]
    fn selected_row_stays_painted_while_text_selection_is_active() {
        let (mut v, path) = cell_fixture("t6-p10");
        let mut texts = TextBatch::new();
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        v.selected = 0;
        v.hover_row.set(u64::MAX); // 无 hover, 排除干扰
        // 有文本选区 (拖框选进行中)
        v.selection = Some(TextSelection::new((0, 0), (0, 2)));

        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts);

        // 选中行 (行 0) 的底矩形 + 左 accent 竖条必须**都在**
        let th = v.theme.theme();
        let sel = th.selection();
        let accent = th.accent();
        let row0_y = HEADER_H; // 表格模式有表头
        let row0_rect = Rect::from_xywh(0.0, row0_y, 800.0 - SCROLLBAR_W, ROW_HEIGHT);
        let accent_rect = Rect::from_xywh(0.0, row0_y, 3.0, ROW_HEIGHT);
        let rects_vec = rects.instance_rects();
        let colors = rects.instance_colors();
        let has_sel_rect = rects_vec
            .iter()
            .zip(colors.iter())
            .any(|(r, c)| *r == row0_rect && *c == lin(sel));
        let has_accent_rect = rects_vec
            .iter()
            .zip(colors.iter())
            .any(|(r, c)| *r == accent_rect && *c == lin(accent));
        assert!(has_sel_rect, "有文本选区时选中行底必须仍在: {rects:?}");
        assert!(has_accent_rect, "有文本选区时 accent 竖条必须仍在");
        std::fs::remove_file(&path).ok();
    }

    /// T6 回归锁: hover **永画**, 不与选中行互斥。
    ///
    /// 原先 hover 与选中行共用 else-if —— 拖框选经过选中行时 hover 消失,
    /// 用户看不见「指针现在在哪」。
    #[test]
    fn hover_row_stays_painted_on_selected_row() {
        let (mut v, path) = cell_fixture("t6-hover");
        let mut texts = TextBatch::new();
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        v.selected = 0;
        v.hover_row.set(0); // hover 就在选中行上

        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts);

        let hover = row_hover_bg(crate::config::AppTheme::Light);
        let row0_y = HEADER_H;
        let row0_rect = Rect::from_xywh(0.0, row0_y, 800.0 - SCROLLBAR_W, ROW_HEIGHT);
        let rects_vec = rects.instance_rects();
        let colors = rects.instance_colors();
        let has_hover = rects_vec
            .iter()
            .zip(colors.iter())
            .any(|(r, c)| *r == row0_rect && *c == lin(hover));
        assert!(has_hover, "选中行上 hover 必须仍画: {rects:?}");
        std::fs::remove_file(&path).ok();
    }

    /// T7 回归锁 (2026-09-14 实机 M0 P11): 命中行底**先**画、hover **后**画,
    /// 且命中行用**弱一档**的 `hit_row_bg` (不是 `th.selection()`)。
    ///
    /// 原先命中行底画在 hover 之后且同用 `th.selection()` —— 命中行上 hover
    /// 无反馈, 「指针现在在哪」被「搜索留下的痕迹」盖掉。
    #[test]
    fn hit_row_is_weaker_and_hover_wins_over_it() {
        let (mut v, path) = cell_fixture("t7-p11");
        let mut texts = TextBatch::new();
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        v.selected = 99; // 无选中行 (99 不在文件里, 排除干扰; 不用 u64::MAX —— paint 里 selected+1 会溢出)
        v.hover_row.set(0); // hover 在命中行上
        v.search_hits = Some(std::sync::Arc::new(vec![0])); // 行 0 是命中行

        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts);

        let th = v.theme.theme();
        let hit = hit_row_bg(crate::config::AppTheme::Light);
        let hover = row_hover_bg(crate::config::AppTheme::Light);
        let row0_y = HEADER_H;
        let row0_rect = Rect::from_xywh(0.0, row0_y, 800.0 - SCROLLBAR_W, ROW_HEIGHT);
        let rects_vec = rects.instance_rects();
        let colors = rects.instance_colors();

        // ① 命中行底用 hit_row_bg, 不是 th.selection()
        let has_hit = rects_vec
            .iter()
            .zip(colors.iter())
            .any(|(r, c)| *r == row0_rect && *c == lin(hit));
        let has_sel_on_hit = rects_vec
            .iter()
            .zip(colors.iter())
            .any(|(r, c)| *r == row0_rect && *c == lin(th.selection()));
        assert!(has_hit, "命中行底必须用 hit_row_bg: {rects:?}");
        assert!(!has_sel_on_hit, "命中行底不得用 th.selection()");

        // ② hover 在命中行**之后**画 (画序: 后者压前者)
        let hit_idx = rects_vec
            .iter()
            .zip(colors.iter())
            .position(|(r, c)| *r == row0_rect && *c == lin(hit));
        let hover_idx = rects_vec
            .iter()
            .zip(colors.iter())
            .position(|(r, c)| *r == row0_rect && *c == lin(hover));
        assert!(hover_idx.is_some(), "命中行上 hover 必须画");
        assert!(
            hover_idx > hit_idx,
            "hover 必须画在命中行之后 (压过它): hit={hit_idx:?} hover={hover_idx:?}"
        );
        std::fs::remove_file(&path).ok();
    }

    /// T7 回归锁 (P14): 「当前命中」与「一般命中」在画面上可分。
    ///
    /// 原先两者同用 `th.selection()`, 多命中时只剩 3px 竖条区分 —— 那在
    /// 暗色下几乎不可见。现在: 一般命中 = hit_row_bg (弱), 当前命中 = selected
    /// (强, 且 hover 永画) —— 但**当前命中行的 hit 底仍画** (选中行也是命中行,
    /// 不把它从命中集里剔出去)。
    #[test]
    fn current_hit_is_stronger_than_other_hits() {
        let (mut v, path) = cell_fixture("t7-p14");
        let mut texts = TextBatch::new();
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        v.selected = 0; // 当前命中 = 行 0
        v.hover_row.set(u64::MAX);
        v.search_hits = Some(std::sync::Arc::new(vec![0, 1])); // 两行都是命中

        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts);

        let th = v.theme.theme();
        let hit = hit_row_bg(crate::config::AppTheme::Light);
        let sel = th.selection();
        let row0_y = HEADER_H;
        let row1_y = HEADER_H + ROW_HEIGHT;
        let row0_rect = Rect::from_xywh(0.0, row0_y, 800.0 - SCROLLBAR_W, ROW_HEIGHT);
        let row1_rect = Rect::from_xywh(0.0, row1_y, 800.0 - SCROLLBAR_W, ROW_HEIGHT);
        let rects_vec = rects.instance_rects();
        let colors = rects.instance_colors();

        // 行 0 (当前命中): 既有 hit 底又有 selected 底 + accent 竖条
        let has_hit0 = rects_vec
            .iter()
            .zip(colors.iter())
            .any(|(r, c)| *r == row0_rect && *c == lin(hit));
        let has_sel0 = rects_vec
            .iter()
            .zip(colors.iter())
            .any(|(r, c)| *r == row0_rect && *c == lin(sel));
        assert!(has_hit0, "当前命中行仍画 hit 底");
        assert!(has_sel0, "当前命中行另画 selected 底 (更强)");

        // 行 1 (一般命中): 只有 hit 底, 无 selected 底
        let has_hit1 = rects_vec
            .iter()
            .zip(colors.iter())
            .any(|(r, c)| *r == row1_rect && *c == lin(hit));
        let has_sel1 = rects_vec
            .iter()
            .zip(colors.iter())
            .any(|(r, c)| *r == row1_rect && *c == lin(sel));
        assert!(has_hit1, "一般命中行画 hit 底");
        assert!(!has_sel1, "一般命中行不得画 selected 底");
        std::fs::remove_file(&path).ok();
    }

    /// T7 回归锁 (P15): 文本选区带与搜索命中区间**同 token** 的已知例外保留,
    /// 但**画序**要钉住: 文本选区带画在命中区间**之后** (选区是用户当前动作,
    /// 必须压过搜索痕迹)。
    ///
    /// 这条不追求「两色可分」 (浅色养不起六个两两 ≥3 的面, 见本文件 108 行
    /// 已知例外) —— 只追求「用户拖框选时看得见自己在拖」。
    ///
    /// **只在原始模式成立** —— 表格模式不做文本选区 (D2 划线), 选区带只在
    /// 原始模式的 `else` 分支里画 (`view.rs:1350-1367`)。表格模式的命中行底
    /// 已由 T7 换成 `hit_row_bg`, 与选区带不同 token, 无需此锁。
    #[test]
    fn text_selection_band_paints_after_hit_span() {
        // 原始模式夹具 (非表格): 有命中行 + 文本选区
        let path =
            std::env::temp_dir().join(format!("danqing-log-t7-p15-raw-{}.log", std::process::id()));
        std::fs::write(&path, "ERROR line one\nINFO line two\n").unwrap();
        let file = LogFile::open(&path).unwrap();
        let mut v = LogView::new();
        v.file = Some(Arc::new(file));
        v.has_file = true;
        v.gutter_w.set(56.0);
        v.mode = ViewMode::Raw; // 原始模式 —— 选区带与命中区间同 token 的唯一场景
        v.selected = 99;
        v.focused = true; // T14/P19: 高亮只在持焦时画
        v.hover_row.set(u64::MAX);
        v.search_hits = Some(std::sync::Arc::new(vec![0]));
        v.search_pattern_src = Some("ERROR".into());
        v.search_re = Some(regex::bytes::Regex::new("ERROR").unwrap());
        // 文本选区在行 0 的前两个字符
        v.selection = Some(TextSelection::new((0, 0), (0, 2)));

        let mut texts = TextBatch::new();
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts);

        let th = v.theme.theme();
        let sel = th.selection();
        let rects_vec = rects.instance_rects();
        let colors = rects.instance_colors();

        // 命中区间与文本选区带**同 token** (sel) —— 两者都该出现。
        // **画序不钉**: 原始模式的命中区间 (`view.rs:1238-1264`) 在选中行/hover
        // (`1131-1143`) 之后画, 而选区带 (`1350-1367`) 又在命中区间之后 ——
        // 但两者**同 token 同 α**, 画序在视觉上不可分, 钉它是过度断言。
        // 只锁「两者都在」; 「选区带压过命中区间」的感知由**位置**提供
        // (选区带是用户拖出来的, 命中区间是搜索留下的)。
        let sel_rects: Vec<_> = rects_vec
            .iter()
            .zip(colors.iter())
            .enumerate()
            .filter(|(_, (r, c))| {
                *c == &lin(sel)
                    && r.origin.y >= 2.0
                    && r.origin.y <= ROW_HEIGHT - 2.0
                    && r.size.height == ROW_HEIGHT - 4.0
            })
            .map(|(i, (r, _))| (i, *r))
            .collect();
        assert!(
            sel_rects.len() >= 2,
            "行 0 上应至少有两个 sel 色矩形 (命中区间 + 选区带): {sel_rects:?}"
        );
        std::fs::remove_file(&path).ok();
    }

    /// T9 回归锁 (2026-09-14 实机 M0 P28): **超复制上限的选区带拖选进行中即
    /// 换警示色** (`th.danger()`), 不再只在 Ctrl+C 时才报。
    ///
    /// 原先只在 `Event::Copy` 里报 (`view.rs:1621-1628`), 用户拖到一半不知道
    /// 已经越界 —— 三问第 3 问「不可用时说清为什么」的违例。
    #[test]
    fn over_limit_selection_paints_with_danger_color() {
        let path =
            std::env::temp_dir().join(format!("danqing-log-t9-limit-{}.log", std::process::id()));
        // 造一个超上限的选区 (COPY_MAX_LINES = 10 万行, 测试文件只有 2 行,
        // 但选区坐标是任意的 —— 超限判定只看行号差, 不看文件实际行数)
        std::fs::write(&path, "l0\nl1\n").unwrap();
        let file = LogFile::open(&path).unwrap();
        let mut v = LogView::new();
        v.file = Some(Arc::new(file));
        v.has_file = true;
        v.gutter_w.set(56.0);
        v.mode = ViewMode::Raw;
        v.selected = 0;
        v.focused = true; // T14/P19: 高亮只在持焦时画
        v.hover_row.set(u64::MAX);
        // 超限选区: (0,0) → (COPY_MAX_LINES, 0) —— 行差 = COPY_MAX_LINES > 上限
        v.selection = Some(TextSelection::new((0, 0), (COPY_MAX_LINES, 0)));
        assert!(v.selection_over_limit(), "夹具应超限");

        let mut texts = TextBatch::new();
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts);

        let th = v.theme.theme();
        let danger = th.danger();
        let rects_vec = rects.instance_rects();
        let colors = rects.instance_colors();

        // 行 0 的选区带必须是 danger 色, 不是 sel 色
        let has_danger = rects_vec.iter().zip(colors.iter()).any(|(r, c)| {
            r.origin.y >= 2.0
                && r.origin.y <= ROW_HEIGHT - 2.0
                && r.size.height == ROW_HEIGHT - 4.0
                && *c == lin(danger)
        });
        let has_sel = rects_vec.iter().zip(colors.iter()).any(|(r, c)| {
            r.origin.y >= 2.0
                && r.origin.y <= ROW_HEIGHT - 2.0
                && r.size.height == ROW_HEIGHT - 4.0
                && *c == lin(th.selection())
        });
        assert!(has_danger, "超限选区带必须用 danger 色: {rects:?}");
        assert!(!has_sel, "超限选区带不得用 sel 色");
        std::fs::remove_file(&path).ok();
    }

    /// T9 回归锁: **未超限**的选区带仍用 `th.selection()` (不误报)。
    #[test]
    fn in_limit_selection_stays_selection_color() {
        let path =
            std::env::temp_dir().join(format!("danqing-log-t9-in-{}.log", std::process::id()));
        std::fs::write(&path, "l0\nl1\n").unwrap();
        let file = LogFile::open(&path).unwrap();
        let mut v = LogView::new();
        v.file = Some(Arc::new(file));
        v.has_file = true;
        v.gutter_w.set(56.0);
        v.mode = ViewMode::Raw;
        v.selected = 0;
        v.focused = true; // T14/P19: 高亮只在持焦时画
        v.hover_row.set(u64::MAX);
        // 未超限选区: (0,0) → (0,1) —— 行差 = 0 < 上限
        v.selection = Some(TextSelection::new((0, 0), (0, 1)));
        assert!(!v.selection_over_limit(), "夹具不应超限");

        let mut texts = TextBatch::new();
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut rects = RectBatch::new();
        v.paint(area, &mut rects, &mut texts);

        let th = v.theme.theme();
        let sel = th.selection();
        let rects_vec = rects.instance_rects();
        let colors = rects.instance_colors();

        let has_sel = rects_vec.iter().zip(colors.iter()).any(|(r, c)| {
            r.origin.y >= 2.0
                && r.origin.y <= ROW_HEIGHT - 2.0
                && r.size.height == ROW_HEIGHT - 4.0
                && *c == lin(sel)
        });
        assert!(has_sel, "未超限选区带仍用 sel 色");
        std::fs::remove_file(&path).ok();
    }

    /// T10 回归锁 (S1): paint 的 `row_y` 与 event 的 `row_at` **同源**。
    ///
    /// 原先两处各推一遍同一个式子 (`rows_top + (i-first)*ROW_HEIGHT - frac*ROW_HEIGHT`),
    /// 现在 `row_y` 是 `row_at` 的逆运算 —— 对拍: 任意显示行 j, `row_y(j)` 算出的 y
    /// 代回 `row_at` 必须得到 j。
    #[test]
    fn row_y_and_row_at_are_inverse() {
        let mut v = LogView::new();
        v.top_row = 3.5; // 非整数滚动位置
        for j in [0, 1, 5, 10, 100] {
            let y = (j as f64 - v.top_row) as f32 * ROW_HEIGHT;
            let back = v.row_at(y);
            assert_eq!(
                back, j,
                "row_y({j}) = {y} 代回 row_at 应得 {j}, 实得 {back}"
            );
        }
    }

    #[test]
    fn clamp_x_bounds() {
        assert_eq!(clamp_x(-5.0, 1000.0, 100.0), 0.0, "负归零");
        assert_eq!(clamp_x(5000.0, 1000.0, 100.0), 900.0, "越界钳到内容尾");
        assert_eq!(clamp_x(42.0, 1000.0, 100.0), 42.0, "区间内不变");
        assert_eq!(clamp_x(10.0, 50.0, 100.0), 0.0, "内容窄于视口归零");
    }

    #[test]
    fn cell_color_semantics() {
        let pal = LevelPalette::for_cell(&LightTheme);
        // level 列: 全级别色带 (INFO 蓝, 与原始模式整行降噪策略不同)
        assert_eq!(
            cell_color("level", "INFO", &LightTheme),
            pal.info,
            "INFO 蓝"
        );
        assert_eq!(
            cell_color("level", "ERROR", &LightTheme),
            pal.error,
            "ERROR 红"
        );
        assert_eq!(
            cell_color("severity", "WARN", &LightTheme),
            pal.warn,
            "severity 同 level"
        );
        // status 列: 按首数字分段, 非数字值不着色
        assert_eq!(cell_color("status", "200", &LightTheme), pal.ok, "2xx 绿");
        assert_eq!(cell_color("status", "301", &LightTheme), pal.info, "3xx 蓝");
        assert_eq!(
            cell_color("http_status", "404", &LightTheme),
            pal.warn,
            "4xx 琥珀"
        );
        assert_eq!(
            cell_color("status", "503", &LightTheme),
            pal.error,
            "5xx 红"
        );
        assert_eq!(
            cell_color("status", "N/A", &LightTheme),
            LightTheme.text_primary(),
            "非数字 status 默认色"
        );
        // 其余列一律正文色 (降灰设计已被用户验收判死: 白底小字看不清,
        // klogg/LogViewPlus/Daucloud 对 ts/req_id 均用正文色)
        assert_eq!(
            cell_color("ts", "2026-09-05", &LightTheme),
            LightTheme.text_primary(),
            "ts 正文色"
        );
        assert_eq!(
            cell_color("msg", "request completed", &LightTheme),
            LightTheme.text_primary()
        );
        assert_eq!(
            cell_color("logger", "auth-service", &LightTheme),
            LightTheme.text_primary(),
            "logger 正文色"
        );
        assert_eq!(
            cell_color("path", "/api/v1/orders/84701", &LightTheme),
            LightTheme.text_primary(),
            "path 正文色"
        );
        assert_eq!(
            cell_color("req_id", "1b26690fb26795f6", &LightTheme),
            LightTheme.text_primary(),
            "长 hex 正文色"
        );
        assert_eq!(
            cell_color(
                "trace_id",
                "550e8400-e29b-41d4-a716-446655440000",
                &LightTheme,
            ),
            LightTheme.text_primary(),
            "UUID 正文色"
        );
    }

    #[test]
    fn is_numeric_gate() {
        assert!(is_numeric("707"), "整数");
        assert!(is_numeric("40.5"), "小数");
        assert!(is_numeric("-3"), "负数");
        assert!(is_numeric("1,234"), "千分位");
        assert!(!is_numeric("1b266"), "hex 标识符不算数字");
        assert!(!is_numeric(""), "空串");
        assert!(!is_numeric("-"), "无数字不算");
        assert!(!is_numeric("200 OK"), "带文本不算");
    }

    #[test]
    fn scroll_trim_prefix_math() {
        let mut texts = TextBatch::new();
        let s = "abcdefghij";
        // 经 danqing::fit::scroll_trim (measure 闭包适配 TextBatch), 验证真实字体测量下适配
        let (whole, sub0) = danqing::fit::scroll_trim(s, 0.0, |t| texts.measure(t, FONT_SIZE));
        assert_eq!(whole, s, "零偏移原样");
        assert_eq!(sub0, 0.0);
        let w5 = texts.measure("abcde", FONT_SIZE);
        let (suf, sub) = danqing::fit::scroll_trim(s, w5, |t| texts.measure(t, FONT_SIZE));
        assert_eq!(suf, "fghij", "恰好 5 字符宽处切断");
        assert!(sub.abs() < 1e-3, "整字符边界无亚偏移: {sub}");
        let char_w = texts.measure("a", FONT_SIZE);
        let (suf, sub) =
            danqing::fit::scroll_trim(s, char_w / 2.0, |t| texts.measure(t, FONT_SIZE));
        assert_eq!(suf, s, "半字符处不切整字符");
        assert!(sub > 0.0 && sub <= char_w, "亚字符偏移平滑: {sub}");
        let (none, _) = danqing::fit::scroll_trim(s, 99999.0, |t| texts.measure(t, FONT_SIZE));
        assert_eq!(none, "", "全滚出为空");
    }

    /// 选区命中: 坐标 → (显示行, 解码字节偏移), 含 gutter 扣除与水平滚动还原。
    /// 几何走 paint 写入的 row_geom 缓存 (测试中手工预填, 与渲染同源约定)。
    #[test]
    fn hit_text_maps_coords_through_gutter_and_xoff() {
        let v = LogView::new();
        v.gutter_w.set(56.0);
        // 行 0: 3 字符, 每字符 10px (内容域 x_end, 字节_end)
        v.row_geom.borrow_mut().insert(
            0,
            RowGeom {
                base_byte: 0,
                offs: vec![(10.0, 1), (20.0, 2), (30.0, 3)],
            },
        );
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        // text_x = 16 + 56 + 12 = 84; 点内容 x=15 → caret 在第 2 字符前 (byte 1)
        assert_eq!(v.hit_text(area, Point::new(84.0 + 15.0, 5.0)), Some((0, 1)));
        // 点在行首 → 首个缓存字符以左 → base_byte
        assert_eq!(v.hit_text(area, Point::new(84.0, 5.0)), Some((0, 0)));
        // 水平滚动后: content_x = px - text_x + x_off = 15 + 100 = 115 → 超末字符 → byte 3
        v.x_offset.set(100.0);
        assert_eq!(v.hit_text(area, Point::new(84.0 + 15.0, 5.0)), Some((0, 3)));
        // 未缓存行 (表格模式普通行/空白行区) → None (不可选);
        // 展开子行 M3 起有缓存可命中, 见 hit_text_sub_row_ignores_x_offset
        assert_eq!(
            v.hit_text(area, Point::new(84.0 + 15.0, 5.0 + ROW_HEIGHT)),
            None
        );
        // 列表区外 (状态栏) → None
        assert_eq!(v.hit_text(area, Point::new(84.0, 599.0)), None);
    }

    /// 左截断窗口: base_byte 之后才是缓存字符, 左缘点击归 base_byte (不乱指行首)。
    #[test]
    fn hit_text_respects_trimmed_window_base() {
        let v = LogView::new();
        v.gutter_w.set(56.0);
        // 左滚 200px 后, 可见窗口从 byte 20 起, 首字符 x_end=205
        v.row_geom.borrow_mut().insert(
            0,
            RowGeom {
                base_byte: 20,
                offs: vec![(205.0, 21), (215.0, 22)],
            },
        );
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        // content_x = 15 - 84... 直接给: px=text_x → content_x = 0 + x_off
        v.x_offset.set(200.0);
        // content_x = 200 → 全缓存字符在右 → base_byte 20 (不是 0!)
        assert_eq!(v.hit_text(area, Point::new(84.0, 5.0)), Some((0, 20)));
        // content_x = 210 → caret 在 byte 21 字符后 → 21
        assert_eq!(
            v.hit_text(area, Point::new(84.0 + 10.0, 5.0)),
            Some((0, 21))
        );
    }

    /// 复制接线: 三级优先 = 文本选区 > (单元格选中, M4) > 行选中 (两模式统一,
    /// 2026-09-14 翻案「只认文本选区」); 越界/无文件 = None (Ctrl+C 不动作)。
    #[test]
    fn selected_text_raw_selection_and_table_row() {
        let path = std::env::temp_dir().join(format!("danqing-log-sel-{}.log", std::process::id()));
        std::fs::write(&path, "aaa bbb\nccc\nddd eee\n").unwrap();
        let file = LogFile::open(&path).unwrap();
        let mut v = LogView::new();
        v.file = Some(Arc::new(file));
        v.has_file = true;
        // 第①级: 跨行选区 → 首行后缀 + 末行前缀
        v.selection = Some(TextSelection::new((0, 4), (1, 2)));
        assert_eq!(v.selected_text().as_deref(), Some("bbb\ncc"));
        // 反向选区同内容
        v.selection = Some(TextSelection::new((1, 2), (0, 4)));
        assert_eq!(v.selected_text().as_deref(), Some("bbb\ncc"));
        // 第③级 (翻案): 原始模式仅行选中 → 整行原文
        v.selection = None;
        v.selected = 2;
        assert_eq!(v.selected_text().as_deref(), Some("ddd eee"));
        // 空选区视同无选区 → 落行兜底 (不是 None)
        v.selection = Some(TextSelection::new((0, 1), (0, 1)));
        assert_eq!(v.selected_text().as_deref(), Some("ddd eee"));
        // ① 优先于 ③: 有文本选区时行选中不抢
        v.selection = Some(TextSelection::new((0, 0), (0, 3)));
        assert_eq!(v.selected_text().as_deref(), Some("aaa"));
        // 表格模式: 选中行完整原文 (行内容不经单元格截断)
        v.selection = None;
        v.mode = ViewMode::Table;
        v.schema = Some(Arc::new(Schema {
            columns: vec![Column {
                name: "x".into(),
                width_chars: 4,
            }],
        }));
        assert_eq!(v.selected_text().as_deref(), Some("ddd eee"));
        // 选中越界 → None (Ctrl+C 不动作, 剪贴板不动)
        v.selected = 99;
        assert_eq!(v.selected_text(), None);
        std::fs::remove_file(&path).ok();
    }

    /// M3 夹具: 2 行 JSONL, 行 0 展开两条子行 →
    /// 显示行 0 = 文件行 0, 1..=2 = 子行, 3 = 文件行 1。
    /// 子行串: row1 = "msg = 订单创建成功" (订 起 byte 6), row2 = "id = 42"。
    /// `tag`: 每测试唯一 —— 并行测试共享同一路径会读到他测试的截断/删除中间态。
    fn sub_row_fixture(tag: &str) -> (LogView, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "danqing-log-sub-{tag}-{}.jsonl",
            std::process::id()
        ));
        std::fs::write(&path, "{\"a\":1}\n{\"b\":2}\n").unwrap();
        let file = LogFile::open(&path).unwrap();
        let mut v = LogView::new();
        v.file = Some(Arc::new(file));
        v.has_file = true;
        // 夹具 = 「用户正在日志区里操作」(持焦)。T14/P19 起三处高亮只在持焦时画,
        // 不置这一位的话下面这一批断言会全变成「测到什么都没画」。
        v.focused = true;
        let mut em = ExpandMap::new();
        em.expand(0, 2);
        v.expanded = em;
        v.sub_rows.insert(
            0,
            vec![
                SubRow {
                    depth: 1,
                    label: "msg".into(),
                    value: "订单创建成功".into(),
                },
                SubRow {
                    depth: 1,
                    label: "id".into(),
                    value: "42".into(),
                },
            ],
        );
        (v, path)
    }

    /// row1 ("msg = 订单创建成功", 24 字节) 的合成几何: 10px/字符,
    /// 中文字符字节端 +3 (与真实 char_indices 对齐)。
    fn sub_row1_geom() -> RowGeom {
        RowGeom {
            base_byte: 0,
            offs: vec![
                (10.0, 1),
                (20.0, 2),
                (30.0, 3),
                (40.0, 4),
                (50.0, 5),
                (60.0, 6),
                (70.0, 9),
                (80.0, 12),
                (90.0, 15),
                (100.0, 18),
                (110.0, 21),
                (120.0, 24),
            ],
        }
    }

    /// 子行命中 (M3/T4): 子行缓存几何后可命中, 且 content_x **不加** x_offset
    /// (子行不参与水平滚动, D3); 同帧普通行仍加 —— 分叉两肢一起锁。
    #[test]
    fn hit_text_sub_row_ignores_x_offset() {
        let (v, path) = sub_row_fixture("hit");
        v.gutter_w.set(56.0);
        let geom = || RowGeom {
            base_byte: 0,
            offs: vec![(10.0, 1), (20.0, 2), (30.0, 3)],
        };
        v.row_geom.borrow_mut().insert(0, geom());
        v.row_geom.borrow_mut().insert(1, geom()); // 子行
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let text_x = EXPAND_W + 56.0 + GUTTER_GAP;
        // 无滚动: 普通行与子行同点同结果 (content_x = 15 → byte 1)
        assert_eq!(
            v.hit_text(area, Point::new(text_x + 15.0, 5.0)),
            Some((0, 1))
        );
        assert_eq!(
            v.hit_text(area, Point::new(text_x + 15.0, 5.0 + ROW_HEIGHT)),
            Some((1, 1))
        );
        // 水平滚动 100px: 普通行 content_x = 115 → 末字符后 byte 3; 子行不动
        v.x_offset.set(100.0);
        assert_eq!(
            v.hit_text(area, Point::new(text_x + 15.0, 5.0)),
            Some((0, 3))
        );
        assert_eq!(
            v.hit_text(area, Point::new(text_x + 15.0, 5.0 + ROW_HEIGHT)),
            Some((1, 1)),
            "子行命中不得随 x_offset 偏移"
        );
        std::fs::remove_file(&path).ok();
    }

    /// 子行双击分词 (M3/T5): 表格模式子行上双击 = 框架新分词作用于**子行串**
    /// (三源一体) —— 中文逐字 / 英文复合词; Ctrl+C 复制选中 token。
    #[test]
    fn sub_row_double_click_selects_token_in_sub_row_text() {
        let (mut v, path) = sub_row_fixture("dbl");
        v.gutter_w.set(56.0);
        v.mode = ViewMode::Table;
        v.schema = Some(Arc::new(Schema {
            columns: vec![Column {
                name: "a".into(),
                width_chars: 4,
            }],
        }));
        v.row_geom.borrow_mut().insert(1, sub_row1_geom());
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut msgs = danqing::widget::MsgQueue::new();
        let text_x = EXPAND_W + 56.0 + GUTTER_GAP;
        let press = |x: f32| Event::MouseInput {
            button: MouseButton::Left,
            pressed: true,
            position: Point::new(x, HEADER_H + ROW_HEIGHT + 5.0),
        };
        // 双击「订」(content_x 65 → caret byte 6 → CJK 逐字)
        v.event(&press(text_x + 65.0), area, &mut msgs);
        v.event(&press(text_x + 65.0), area, &mut msgs);
        assert_eq!(v.selection, Some(TextSelection::new((1, 6), (1, 9))));
        assert_eq!(v.selected_text().as_deref(), Some("订"));
        // 双击「msg」(content_x 5 → base_byte 0 → 复合词)
        v.event(&press(text_x + 5.0), area, &mut msgs);
        v.event(&press(text_x + 5.0), area, &mut msgs);
        assert_eq!(v.selection, Some(TextSelection::new((1, 0), (1, 3))));
        assert_eq!(v.selected_text().as_deref(), Some("msg"));
        std::fs::remove_file(&path).ok();
    }

    /// 子行框选与行复制 (M3/T5): 块内跨子行拖选 → 各子行串 `\n` 拼接;
    /// 无文本选区时选中子行 Ctrl+C = 该子行串 (**#5 回归锁**); 普通行回归原文。
    #[test]
    fn sub_row_drag_selection_and_row_copy() {
        let (mut v, path) = sub_row_fixture("drag");
        v.gutter_w.set(56.0);
        v.mode = ViewMode::Table;
        v.schema = Some(Arc::new(Schema {
            columns: vec![Column {
                name: "a".into(),
                width_chars: 4,
            }],
        }));
        v.row_geom.borrow_mut().insert(1, sub_row1_geom());
        v.row_geom.borrow_mut().insert(
            2,
            RowGeom {
                base_byte: 0,
                offs: vec![
                    (10.0, 1),
                    (20.0, 2),
                    (30.0, 3),
                    (40.0, 4),
                    (50.0, 5),
                    (60.0, 6),
                    (70.0, 7),
                ],
            },
        );
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut msgs = danqing::widget::MsgQueue::new();
        let text_x = EXPAND_W + 56.0 + GUTTER_GAP;
        // 按下子行 1 起点 (byte 0)
        v.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: true,
                position: Point::new(text_x + 5.0, HEADER_H + ROW_HEIGHT + 5.0),
            },
            area,
            &mut msgs,
        );
        // 拖到子行 2 (content_x 65 → byte 6)
        v.event(
            &Event::CursorMoved(Point::new(text_x + 65.0, HEADER_H + 2.0 * ROW_HEIGHT + 5.0)),
            area,
            &mut msgs,
        );
        assert_eq!(v.selection, Some(TextSelection::new((1, 0), (2, 6))));
        // 复制 = 子行 1 整串 + 子行 2 [..6] —— `\n` 拼接
        assert_eq!(
            v.selected_text().as_deref(),
            Some("msg = 订单创建成功\nid = 4")
        );
        // 抬起落定 (回归既有状态机)
        v.event(
            &Event::MouseInput {
                button: MouseButton::Left,
                pressed: false,
                position: Point::new(text_x + 65.0, HEADER_H + 2.0 * ROW_HEIGHT + 5.0),
            },
            area,
            &mut msgs,
        );
        assert!(v.press.is_none() && !v.dragging);
        // #5 回归锁: 选中子行 (无文本选区) Ctrl+C = 该子行串, 不是父行原文
        v.selection = None;
        v.selected = 1;
        assert_eq!(v.selected_text().as_deref(), Some("msg = 订单创建成功"));
        // 回归: 普通行仍复制解码原文
        v.selected = 3;
        assert_eq!(v.selected_text().as_deref(), Some("{\"b\":2}"));
        std::fs::remove_file(&path).ok();
    }

    /// raw 模式跨界框选 (M3): 普通行原文 + 子行串混排
    /// (spec Open Q3 —— 语义自洽, 不特殊处理)。
    #[test]
    fn raw_mode_cross_boundary_selection_mixes_line_and_sub_row_text() {
        let (mut v, path) = sub_row_fixture("cross"); // 默认 Raw 模式
        v.selection = Some(TextSelection::new((0, 0), (1, 9)));
        // 行 0 全行 "{\"a\":1}" + 子行 1 [..9] = "msg = 订"
        assert_eq!(v.selected_text().as_deref(), Some("{\"a\":1}\nmsg = 订"));
        std::fs::remove_file(&path).ok();
    }

    /// 回归 (M3): 表格模式**普通行**仍不产生文本选区 (文本选区是子行专属通道;
    /// M4 的双击单元格走另一条状态, 不在此钉)。
    #[test]
    fn table_mode_normal_row_press_makes_no_text_selection() {
        let (mut v, path) = sub_row_fixture("norm");
        v.gutter_w.set(56.0);
        v.mode = ViewMode::Table;
        v.schema = Some(Arc::new(Schema {
            columns: vec![Column {
                name: "a".into(),
                width_chars: 4,
            }],
        }));
        // 即使有缓存几何也不接 (防御性 —— 正常 paint 不会给普通行缓存)
        v.row_geom.borrow_mut().insert(0, sub_row1_geom());
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut msgs = danqing::widget::MsgQueue::new();
        let text_x = EXPAND_W + 56.0 + GUTTER_GAP;
        let press = |x: f32| Event::MouseInput {
            button: MouseButton::Left,
            pressed: true,
            position: Point::new(x, HEADER_H + 5.0),
        };
        v.event(&press(text_x + 5.0), area, &mut msgs);
        assert_eq!(v.selection, None);
        assert!(v.press.is_none());
        // 双击亦然 (不进 handle_text_press → 无 token 选区)
        v.event(&press(text_x + 5.0), area, &mut msgs);
        assert_eq!(v.selection, None);
        std::fs::remove_file(&path).ok();
    }

    /// expand_rev 守卫 (M3/T6): 展开/折叠改变显示行映射 → sync 作废旧选区
    /// (含潜伏按下); rev 不变 → 保留 (不误杀活着的选区)。
    #[test]
    fn sync_clears_selection_when_expand_rev_changes() {
        let path = std::env::temp_dir().join(format!("danqing-log-rev-{}.log", std::process::id()));
        std::fs::write(&path, "l0\nl1\n").unwrap();
        let mut app = crate::LogApp::new_empty();
        app.file = Arc::new(LogFile::open(&path).unwrap());
        app.has_file = true;
        let mut v = LogView::new();
        v.file = Some(Arc::clone(&app.file));
        v.has_file = true;
        v.last_expand_rev = app.expand_rev;
        v.selection = Some(TextSelection::new((0, 0), (0, 2)));
        v.selected_cell = Some((0, 1));
        // rev 不变 → 都保留
        v.sync(&app);
        assert!(v.selection.is_some() && v.selected_cell.is_some());
        // rev 变化 → 都作废
        app.expand_rev += 1;
        v.sync(&app);
        assert_eq!(v.selection, None);
        assert_eq!(v.selected_cell, None);
        std::fs::remove_file(&path).ok();
    }

    /// M4 夹具: 2 行 JSONL (行 0: level + 长 msg; 行 1: 仅 level 无 msg),
    /// 表格模式双列; 列区间合成缓存 = [(text_x, 200) level, (200, 400) msg]
    /// (绝对窗口 x; paint 缓存的替身 —— event 只读它)。
    /// `tag`: 每测试唯一 —— 并行共享同一路径会读/写到他测试的中间态。
    fn cell_fixture(tag: &str) -> (LogView, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "danqing-log-cell-{tag}-{}.jsonl",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "{\"level\":\"ERROR\",\"msg\":\"订单创建成功,支付网关超时\"}\n{\"level\":\"INFO\"}\n",
        )
        .unwrap();
        let file = LogFile::open(&path).unwrap();
        let mut v = LogView::new();
        v.file = Some(Arc::new(file));
        v.has_file = true;
        // 同 `sub_row_fixture`: 夹具代表「正在日志区里操作」(T14/P19)。
        v.focused = true;
        v.gutter_w.set(56.0);
        v.mode = ViewMode::Table;
        v.schema = Some(Arc::new(Schema {
            columns: vec![
                Column {
                    name: "level".into(),
                    width_chars: 5,
                },
                Column {
                    name: "msg".into(),
                    width_chars: 6,
                },
            ],
        }));
        let text_x = EXPAND_W + 56.0 + GUTTER_GAP;
        v.col_spans
            .replace(vec![(text_x, 200.0, 0), (200.0, 400.0, 1)]);
        (v, path)
    }

    /// 单元格双击 (M4): 命中有值列 → 选中; Ctrl+C 拿**完整值** (列宽 6 字符
    /// 装不下 msg, 复制不受截断影响); 单击他格清除; 双击落空 (列区间外 /
    /// 该行无该字段) 不产选中。
    #[test]
    fn cell_double_click_selects_and_copies_full_value() {
        let (mut v, path) = cell_fixture("dbl");
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut msgs = danqing::widget::MsgQueue::new();
        let press = |x: f32, row: f32| Event::MouseInput {
            button: MouseButton::Left,
            pressed: true,
            position: Point::new(x, HEADER_H + row * ROW_HEIGHT + 5.0),
        };
        // 残留回归 (2026-09-14 实机): 先有文本选区 (展开块框选),
        // 再双击单元格 → 旧选区必须作废 (否则 Ctrl+C 复制的是旧选区)
        v.selection = Some(TextSelection::new((0, 0), (0, 2)));
        // 双击 level 列 (x=150)
        v.event(&press(150.0, 0.0), area, &mut msgs);
        v.event(&press(150.0, 0.0), area, &mut msgs);
        assert_eq!(v.selected_cell, Some((0, 0)));
        assert_eq!(v.selection, None, "单元格按下作废旧文本选区");
        assert_eq!(v.selected_text().as_deref(), Some("ERROR"));
        // 单击 msg 列 → 先清; 再击成双击 → 选中, 复制完整值
        v.event(&press(300.0, 0.0), area, &mut msgs);
        assert_eq!(v.selected_cell, None, "单击他格清除选中");
        v.event(&press(300.0, 0.0), area, &mut msgs);
        assert_eq!(v.selected_cell, Some((0, 1)));
        assert_eq!(
            v.selected_text().as_deref(),
            Some("订单创建成功,支付网关超时"),
            "复制完整值, 不受列宽截断影响"
        );
        // 双击列区间外 → 不产选中
        v.event(&press(500.0, 0.0), area, &mut msgs);
        v.event(&press(500.0, 0.0), area, &mut msgs);
        assert_eq!(v.selected_cell, None);
        // 双击无该字段的格 (行 1 无 msg) → 不产选中
        v.event(&press(300.0, 1.0), area, &mut msgs);
        v.event(&press(300.0, 1.0), area, &mut msgs);
        assert_eq!(v.selected_cell, None, "无该字段的单元格不产选中");
        // 末行下方空白双击 (2026-09-14 审查 #1): row 由 y 反算得越界值, line_at
        // 会把它塌缩成文件第 0 行 —— 必须不产选中, 否则画面上看不到高亮 (srow
        // 等于不了任何可见行), Ctrl+C 却复制第 0 行该列的值
        v.event(&press(150.0, 9.0), area, &mut msgs);
        v.event(&press(150.0, 9.0), area, &mut msgs);
        assert_eq!(v.selected_cell, None, "末行下方空白双击不产选中");
        // gutter/行号区单击 → 清单元格选中 (单击他处 = 放弃)
        v.event(&press(150.0, 0.0), area, &mut msgs);
        v.event(&press(150.0, 0.0), area, &mut msgs);
        assert_eq!(v.selected_cell, Some((0, 0)));
        v.event(&press(EXPAND_W + 5.0, 0.0), area, &mut msgs);
        assert_eq!(v.selected_cell, None, "gutter 单击清单元格选中");
        std::fs::remove_file(&path).ok();
    }

    /// 复制上限守卫 (R3): 超 10 万行的文本选区 Ctrl+C 不动作, 且**不落**第②③级
    /// —— 否则「超限不复制」会被单元格/行兜底悄悄绕过, 白设一道闸。
    #[test]
    fn over_limit_selection_blocks_copy_without_falling_through() {
        let (mut v, path) = cell_fixture("limit");
        v.selected = 0;
        v.selected_cell = Some((0, 0)); // ② 有内容, 同样必须被挡住
        v.selection = Some(TextSelection::new((0, 0), (COPY_MAX_LINES, 0)));
        assert!(v.selection_over_limit());
        assert_eq!(v.selected_text(), None, "超限不复制, 也不落单元格/行兜底");
        // 恰在上限 → 不超限 (边界; 不实跑复制 —— 10 万行拼接无测试价值)
        v.selection = Some(TextSelection::new((0, 0), (COPY_MAX_LINES - 1, 0)));
        assert!(!v.selection_over_limit());
        std::fs::remove_file(&path).ok();
    }

    /// 复制三级优先链 (M2 搭链 / M4 填级): 文本选区 > 单元格选中 > 行选中;
    /// 单元格失效 (列下标越界) → 防御落行兜底。
    #[test]
    fn selected_text_priority_selection_over_cell_over_row() {
        let (mut v, path) = cell_fixture("prio");
        // ③ 行兜底: 完整原文
        v.selected = 0;
        assert_eq!(
            v.selected_text().as_deref(),
            Some("{\"level\":\"ERROR\",\"msg\":\"订单创建成功,支付网关超时\"}")
        );
        // ② 单元格压过行
        v.selected_cell = Some((0, 0));
        assert_eq!(v.selected_text().as_deref(), Some("ERROR"));
        // ① 文本选区压过单元格
        v.selection = Some(TextSelection::new((0, 0), (0, 2)));
        assert_eq!(v.selected_text().as_deref(), Some("{\""));
        // 单元格失效 → 落 ③ 行兜底 (不复制空串冒充)
        v.selection = None;
        v.selected_cell = Some((0, 99));
        assert_eq!(
            v.selected_text().as_deref(),
            Some("{\"level\":\"ERROR\",\"msg\":\"订单创建成功,支付网关超时\"}"),
            "失效单元格防御落行兜底"
        );
        std::fs::remove_file(&path).ok();
    }

    /// Esc 语义 (T4): 有选区 → 清选区并消费; 无选区 → Ignored (框架清焦)。
    #[test]
    fn esc_clears_selection_only_when_present() {
        let mut v = LogView::new();
        let esc = Event::Key {
            key: Key::Named(NamedKey::Escape),
            pressed: true,
            ctrl: false,
            shift: false,
            alt: false,
        };
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut msgs = danqing::widget::MsgQueue::new();
        // 无选区: Ignored (框架随后清焦点)
        assert_eq!(v.event(&esc, area, &mut msgs), EventResult::Ignored);
        // 空选区同样 Ignored
        v.selection = Some(TextSelection::new((1, 1), (1, 1)));
        assert_eq!(v.event(&esc, area, &mut msgs), EventResult::Ignored);
        // 非空选区: 清除并消费
        v.selection = Some(TextSelection::new((0, 0), (1, 2)));
        assert_eq!(v.event(&esc, area, &mut msgs), EventResult::Consumed);
        assert_eq!(v.selection, None);
        // 单元格选中 (M4): Esc 同样清除并消费
        v.selected_cell = Some((0, 1));
        assert_eq!(v.event(&esc, area, &mut msgs), EventResult::Consumed);
        assert_eq!(v.selected_cell, None);
    }

    /// 框选状态机 (R1 回归): 单击 (未超阈值抬起) 不产选区;
    /// 拖超阈值选区成形, 抬起后落定且按下态清空。
    #[test]
    fn drag_state_machine_forms_and_settles_selection() {
        let mut v = LogView::new();
        // **夹具必须挂真文件** (2026-09-15, P20 修复暴露): 下面按下的是第 0 行,
        // 而 P20 起「列表矩形**之内**、真实行数**之外**」也算空白 —— `file = None`
        // 时 `display_count()` 是 0, 那一按会被归成「此处无行」。
        // 原夹具只塞了 `row_geom` 却不给 file, 是「手抄几何 + 空文件」的合成体,
        // 只能活在旧判据下 —— 那正是本仓自记的「所有条目都是合成几何」那笔欠账。
        let path =
            std::env::temp_dir().join(format!("danqing-log-drag-{}.log", std::process::id()));
        std::fs::write(&path, "2026-09-05 12:00:01 ERROR one\n").unwrap();
        v.file = Some(Arc::new(LogFile::open(&path).unwrap()));
        v.has_file = true;
        v.gutter_w.set(56.0);
        v.row_geom.borrow_mut().insert(
            0,
            RowGeom {
                base_byte: 0,
                offs: vec![(10.0, 1), (20.0, 2), (30.0, 3)],
            },
        );
        let area = Rect::from_xywh(0.0, 0.0, 800.0, 600.0);
        let mut msgs = danqing::widget::MsgQueue::new();
        let input = |pressed: bool, x: f32, y: f32| Event::MouseInput {
            button: MouseButton::Left,
            pressed,
            position: Point::new(x, y),
        };
        // 单击: 按下即抬起 (未超阈值) → 不产选区, 按下态清空
        // (与后续框选按下拉开 >4px, 避免触发双击判定)
        v.event(&input(true, 200.0, 5.0), area, &mut msgs);
        v.event(&input(false, 200.0, 5.0), area, &mut msgs);
        assert_eq!(v.selection, None);
        assert!(v.press.is_none());
        // 框选: 拖超 4px → 选区成形 (锚点 byte0 → caret byte1)
        v.event(&input(true, 89.0, 5.0), area, &mut msgs);
        v.event(&Event::CursorMoved(Point::new(99.0, 5.0)), area, &mut msgs);
        assert_eq!(v.selection, Some(TextSelection::new((0, 0), (0, 1))));
        assert!(v.dragging);
        // 抬起 → 落定: 选区保留, 按下/框选态清空 (粘滞回归钉死)
        v.event(&input(false, 99.0, 5.0), area, &mut msgs);
        assert_eq!(v.selection, Some(TextSelection::new((0, 0), (0, 1))));
        assert!(v.press.is_none());
        assert!(!v.dragging);
        // 右键按下: 不清既有选区、不产潜伏锚点、不覆写双击记录 (O1 钉死)
        let lc = v.last_click;
        v.event(
            &Event::MouseInput {
                button: MouseButton::Right,
                pressed: true,
                position: Point::new(200.0, 5.0),
            },
            area,
            &mut msgs,
        );
        assert_eq!(v.selection, Some(TextSelection::new((0, 0), (0, 1))));
        assert!(v.press.is_none(), "右键不产潜伏锚点");
        assert_eq!(v.last_click, lc, "右键不污染双击判定");
        std::fs::remove_file(&path).ok();
    }
}
