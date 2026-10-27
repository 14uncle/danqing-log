# plan-checkbox-widget: 框架复选框 + `[x]` 换挂 任务拆解

- @author 十四叔
- @date 2026/09/28
- Spec: `docs/specs/SPEC-checkbox-widget.md`（2026-09-28「go」批准：三裁定全按推荐
  —— D1 两处都换 / D2 完整 widget + 静态画法 / D3 accent 实心 + 白勾；Open Q①② 按推荐）
- 状态: **待过目** —— 批准后进 build（`/build auto` 惯例：零 commit、TDD 循环不减；
  **例外**: T2 含 danqing push + 本仓 lock 复钉提交，即 spec D8 联动链路，批 plan 即授权）
- 测试基线: 本仓 **495 绿**（2026-09-28 CLAUDE.md）；框架记档 **603**（09-20，
  T1 开工先实测复核）

## 组件表

| 仓库 | 文件 | 职责 | 改动面 |
|---|---|---|---|
| danqing | `src/widget/form/checkbox.rs`（新） | `Checkbox` widget + `CheckboxColors` + 静态画法 `paint_box` + 单测 | 新文件（含测试） |
| danqing | `src/widget/form/mod.rs` / `src/widget/mod.rs` | 导出两级 | 各 1 行 |
| danqing | `examples/showcase.rs` | Checkbox 演示区（Switch 区同构） | 状态 + Msg + section |
| danqing-log | `src/pick_list.rs` | `with_checkbox` 第五闭包 + sync 缓存 + paint 盒 | 加法不改契约 |
| danqing-log | `src/settings.rs` | 两处 rows_fn 删 `[x]` 前缀 + 装 `with_checkbox` + 守卫 | 2 闭包 + 测试 |
| 文档 | spec 实现记 / acceptance-pending H 组 / CLAUDE.md / todo | 收口 | T5 |

## 关键实现事实（2026-09-28 开工核实）

1. **`push_diagonal` 是 `pub(crate)`**（`danqing/src/widget/mod.rs:53`）—— checkbox.rs
   同 crate 直接用，**零可见性改动**；勾 = 两笔对角线（短笔下行 + 长笔上行），
   CloseButton × 符同算法。
2. **导出链两级**：`form/mod.rs` `pub use checkbox::Checkbox;` + `widget/mod.rs`
   `pub use form::{… Checkbox}`；`lib.rs` 的 `pub mod widget` 已公开——widget 的既定
   公开通路即此（Switch 同款，lib.rs 不动）。widget checklist 13 条
   （`danqing/docs/CONTEXT/widget-guidelines.md` §8）：form/ 目录 / `new()`+`themed()`
   双构造 / builder 链 / `bind_*`+`sync` / 单测 / **showcase 登记**。
3. **showcase Switch 区先例**（`examples/showcase.rs:673` 起）：状态 bool +
   `Msg::SwitchToggle` + section fn + `bind_theme`。Checkbox 演示区同构
   （勾中/未选静态各一 + 可交互一）。
4. **RowList 四消费者盘点**：`col_menu_rows`（settings.rs:514）/ `picker_field_rows`
   （:565）/ `merge_source_rows`（:883）/ 测试 `test_list`。`with_swatch` = 第四闭包
   先例（`Option<Color>`：None = 该行不画；不装 = 零变化）；`with_checkbox` 第五闭包
   同形：`Fn(&LogApp, &str) -> Option<bool>`，None = 不画盒，Some(c) = 画盒取勾态。
5. **行内几何**：现状 `text_x = r.x + 8`（+ `SWATCH_W`=18 若有色块）。换挂后：
   8 → 色块 18 → 盒 `CHECK_W`=20（盒 14 + 间隔 6）→ 文案。净宽变化 ≈ **−8px**
   （评审实测更正, 原估 −6px： `[x] ` 前缀在 FONT_SIZE 14 下实测 28px 退役 vs
   盒位 20px）——行内容变窄，无溢出回归风险，但同源守卫仍立（防未来漂移，G-d 教训）。
6. **D3b 高亮行反白**：RowList paint 的 `is_hi` 行铺 accent 底 + 白字；四消费者中
   **仅** `merge_source_rows` 有 highlight_fn（= `merge_source_selected`，显示列弹层
   highlight 恒 None）。`CheckboxColors` 框架拥有两套：`from_theme`（常态：勾中
   accent 填充 + 白勾 / 未选 border 边框空盒）+ `on_accent(accent)`（高亮行：勾中
   白填充 + accent 勾 / 未选白边框空盒）——RowList sync 各缓存一份，paint 按 is_hi
   选套。
7. **行宽守卫模式**：`merge_time_edit_row_fits_card_width`（settings.rs:2455）=
   layout 自然宽 ≤ `content_width()`（312）。RowList.layout 回 `constraints.max()`
   （占满），守卫须改为**量内容**：代表性最坏 label（合并源 = `src0.jsonl · JSONL ts
   · 断流` 形）measure 宽 + 8 + SWATCH_W + CHECK_W ≤ `content_width()`。
8. **事件路径零改动**（spec D5）：RowList event 不看盒（`row_clickable` 只看行），
   无新事件测试；既有点击锁（同载荷触发/拖出不触发/右键不冒充/尾行不可点）
   全保留即回归证据。
9. **patch/lock 纪律**：开工先 `cp tools/local-patch.toml .cargo/config.toml`；
   danqing push 后**关 patch** `cargo update -p danqing`——若报 `did not match any
   packages`（lock 是 path 态）先跑一次 `cargo check` 重解再 update（2026-09-14 实录）；
   **path 态 lock 不许提交**。

## 任务

### T1: 框架 `checkbox.rs`（danqing，新文件）

`CheckboxColors { border, fill, check }` + `from_theme` + `on_accent`；`Checkbox`
widget 全件（`new()`/`themed()`/`bind(bool)`/`on_toggle(Msg)`/`bind_theme`/焦点环/
hover/pressed/Space·Enter/`focusable`）；常量 `BOX_SIZE=14` / 边框 1.5 / 圆角 3；
勾 = 两笔 `push_diagonal`；**`pub fn paint_box(rects, rect, checked, colors)` 静态画法，
widget 自身 paint 也调它**（单真源）；模块注释写明 D2（widget 本体暂由 showcase 消费，
RowList 消费 paint_box——非死代码）与 D6（无动画的有意偏离）。

Acceptance: ①layout 固定 14×14 + 约束收窄受约束 ②原地按下抬起产 Msg / 拖出不产 /
右键不冒充 ③Space/Enter 持焦才产、无焦不产 ④焦点环 FocusIn 多一笔、FocusOut 消失
⑤`bind`/`bind_theme` sync 换值换色（断言经 `LinearRgba`，Switch `rgba_of` 先例）
⑥**paint_box A/B 判罪锁**：checked 比未选多出勾的实例（摘勾 = 精确红）；未选盒内
无 fill 色填充实例 ⑦widget paint 走 paint_box（读码可见的唯一画法入口）。
Verify: `cd ../danqing && cargo test checkbox` + `cargo clippy --all-targets -- -D warnings` + fmt。

### T2: 框架导出 + showcase + 联动闸门（danqing → 本仓复钉）

两级导出；showcase Checkbox 演示区（Switch 区同构：勾中/未选/可交互各一）；
框架**全量**三件套（基线复核 603）；**push danqing**；本仓关 patch
`cargo update -p danqing` 复钉 + `Cargo.lock` 提交（message 注明关联）。

Acceptance: ①showcase 含 Checkbox 且 `cargo build --example danqing-showcase` 过
（评审勘误: target 真名 `danqing-showcase`; 初稿写 `--example showcase` 是错名,
且当时 `| tail -1` 未核退出码 = 假绿, 已用真名补验 exit 0）
②框架全量绿 ③danqing 已 push ④本仓 lock = `git+…#<sha>` 钉态（非 path）
⑤本仓 495 基线不破（本任务只动 lock）。
Verify: 两仓 `cargo test`。

### T3: 产品 `RowList::with_checkbox` 加法（零换挂，纯加法）

`pick_list.rs`：第五闭包 `checkbox_fn` + sync 缓存 `checked: Vec<Option<bool>>`
（与 swatches 同法）+ 两套 `CheckboxColors` 缓存 + paint 画盒（`is_hi` 行用
on_accent 套；盒垂直居中 `(ROW_H-14)/2`；文案 x 偏移同源加 `CHECK_W`）。
锁全走 `test_list` 夹具，**四消费者调用点一处不动**。

Acceptance: ①**不装 = 零盒**（A 面）②**装了 = 每可点行恰一盒**（B 面，
swatch 锁同构）③sync 换数据盒态跟随（摘每帧重取 = 精确红，T0 判罪锁同族）
④既有 RowList 全部锁**零修改**绿（加法不改契约的证据）。
Verify: `cargo test` + clippy 0。

### T4: 两处换挂 + 守卫（settings.rs）

`col_menu_rows` / `merge_source_rows`：删 `{mark} ` 前缀（label 回归纯列名/源名·结论），
各装 `with_checkbox`（checked = `!is_hidden` / `!s.hidden`，按载荷查）；
`pick_list.rs:10` 与 `settings.rs:480` 等处「`[x]`/`[ ]` ASCII 勾选」旧注释同步更新
（加新决定清旧文字家法）。

Acceptance: ①**文本前缀退役锁**：两处 rows_fn **产出串**不含 `[x]`/`[ ]`（运行时断言
产出，不 grep 源码——防注释骗锁）；载荷不变 ②勾态正确（隐藏 = 空盒）
③**行宽同源守卫**两处：最坏 label measure + 8 + SWATCH_W + CHECK_W ≤ `content_width()`
④**D3b 反白锁**：高亮行上的盒用白系色（勾中白填充/未选白边框）
⑤既有 settings 锁全绿。
Verify: `cargo test` + clippy 0。

## Checkpoint A（T1–T2，框架半边）

- [ ] T1 机器判据 ①–⑦ 全绿；paint_box 摘勾**精确红**在案；框架全量绿（基线复核值不破）
- [ ] showcase 出现 Checkbox；danqing 已 push；本仓 lock 复钉并提交

## Checkpoint B（T3–T4，产品半边）

- [ ] T3 判据 ①–④ 全绿（不装零盒 / 装了每行一盒 / 摘重取红 / 既有锁零修改）
- [ ] T4 判据 ①–⑤ 全绿；`[x]`/`[ ]` 产出零残留；基线 495 不破（+N 锁）
- [ ] 三件套干净

## Phase 3: 收口

### T5: 文档收口

spec §9 实现记回填（实现与 spec 的偏差如实记）/ `tasks/acceptance-pending.md`
**新 H 组三条**（H1 合并源卡盒渲染+点行切显隐·明暗两主题 / H2 显示列弹层同款 /
H3 字段查询弹层无盒回归；**需付费态 key，与 G 组同窗口实机**）/ CLAUDE.md 状态行 /
todo 落账。（无性能面——每帧多画 ≤ 行数个 14px 矩形，不设 logbench 项；
FEATURE-MATRIX/ROADMAP 不动：无能力增减，纯观感。）

## Checkpoint C（T5）

- [ ] spec 成功判据机器部分逐条回填；H 组三条记账进总清单；状态全线落账
- [ ] 进 review 阶段（`/agent-skills:code-review-and-quality`）→ code-simplify

## 风险与对策

- **画法单真源漂移**（widget paint 与 paint_box 各画一份）: 结构上只许 paint_box
  一个画法入口（widget paint 调它），评审查。
- **高亮行反白漏画**: T4④ 锁；merge 源行是唯一有 highlight 的消费场景，锁打在
  RowList 层（is_hi + checked 组合矩阵）。
- **假绿面（前缀退役锁被注释骗）**: 锁断言 rows_fn **运行时产出串**，不 grep 源码；
  注释清理另列 T4 步骤。
- **patch/lock 陷阱**: 事实 9 全录（did-not-match 解法 / path 态不提交）。
- **showcase 破构建**: `cargo clippy --all-targets` 覆盖 examples，T2 显式
  `cargo build --example showcase`。
