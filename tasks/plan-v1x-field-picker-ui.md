# plan-v1x-field-picker-ui: 免语法字段查询 UI 任务拆解

- @author 十四叔
- @date 2026/09/23
- Spec: `docs/specs/SPEC-v1x-field-picker-ui.md`（2026-09-23「go」批准: 范围三项
  推荐 + T0 前置修复 + Open Q①②按推荐）
- 状态: **待过目** —— 批准后进 build（`/build auto` 惯例: 零 commit、TDD 循环不减）
- 测试基线: **376**（2026-09-23 bookmark-persist 收口实测）

## 组件表

| 文件 | 职责 | 改动面 |
|---|---|---|
| `src/pick_list.rs`（新） | **RowList 自绘行列表件**（sync 缓存行文案/选中 + paint 画行/hover + event 合成几何命中→Msg） | 新小件（Open Q②裁定: 独立文件供两处复用） |
| `src/settings.rs` | col_menu 行集换挂 RowList（**T0 修复**）; FilterForm 卡体装配 | 既有弹层改造 + 新卡 |
| `src/view.rs` | 「字段…」paint 侧按钮（Bar label/hint 同款同源）+ FilterForm 复合件（值 TextInput + Enter 拦截 + 「过滤」钮 —— **Bar 同构**） | 复合件 + 按钮 |
| `src/main.rs` | picker 状态真身（open/field/op）+ Msg 链 + `PickerSubmit` 组装追加 + Esc/模态/互斥 | 状态 + 臂 |
| `src/columns.rs` 等 | **零改动** | — |

## 关键实现事实（2026-09-23 开工核实）

1. **「字段…」按钮落位 = Bar paint 侧**（`view.rs:2829` Bar 是自绘复合件: label/
   hint 自绘 + `input_area` event 转发, 「提示画在哪与点到哪不可能分岔」同源纪律
   在案）——按钮画在过滤行右端, hit rect 同源缓存; **`schema.is_some()` 才画**
   （`.log` 不出, spec 判据 6; 与 export `schema.is_some()` 同哲学）。
2. **值提交 = Bar 模式复合件**（`view.rs:3172-3181` Bar 拦 Enter → `handle_enter`
   读 `filter_ti` 文本发 `Msg::ApplyFilter`）: `FilterForm` 自绘复合件**持有**值
   TextInput, 拦 Enter + paint 侧「过滤」按钮读同一缓冲 —— **兄弟节点读不到
   TextInput 缓冲**, 按钮/Enter 都必须在持有者内收口（这是「点过滤钮拿什么值」
   的唯一解, 别绕）。
3. **动态行 = RowList 自绘件**（框架 `view()` 一次性建树六处文档铁律）: sync 闭包
   取 `(Vec<String> 行文案, Option<usize> 高亮)` + `on_pick(usize, &str) -> Msg`
   工厂; col_menu（`[x]/[ ]` 前缀并进文案）与 picker 字段行共用。**T0 先修
   col_menu**（`col_menu_card` 行集现为启动快照 = bug 本体, `main.rs:2047-2050`
   取值传入建树即冻结）。
4. **拼子句纯函数**: `build_clause(field, op, value) -> Option<String>` ——
   `Op::Eq` 值尾缀 `*` = 前缀（`jsonl::parse_clause` 现语义, `jsonl.rs:472-479`）,
   比较算符原样; 空值/空字段 → `None`（D3 拒绝）。**roundtrip 锁**: 产出串过
   `jsonl::parse_query` 回到 `(path, op, value)` 全等（语法面零发明）。
5. **追加 = `filter_applied` 空格连接**（`parse_query = split_whitespace` AND,
   `jsonl.rs:420-422`）→ `Msg::ApplyFilter(新全串)`（唯一真相路; `parse_filter`
   规范化入口不动）。空查询 = 直接是它; Esc 清回全量是既有语义。
6. **Op 产品侧映射表**（`jsonl::Op` 是 `pub enum`, `jsonl.rs:390` —— 直接复用
   不新造枚举）: 显示文案 `=` / `*` / `>=` / `<=` / `>` / `<`（六钮常显, Open Q①
   裁定）; 前缀 = `Eq` + 值尾 `*`（拼接在 build_clause, 不新增 Op 变体）。
7. **Esc/模态插入点**: `app_key_filter` 现文次序 `upgrade > settings > col_menu >
   export_menu > 栏`（bookmark-persist 未动它）——picker 插在 settings 之后
   (`upgrade > settings > picker > col_menu > export_menu > 栏`), 模态守卫补
   `picker_open`, 与 col_menu/export_menu **双向互斥**（开一关二, 先例全套）。

## 衍生设计

- **RowList 契约**（`pick_list.rs`）: `RowList::new(labels_fn, highlight_fn,
  on_pick)` 三个 `Box<dyn Fn…>`; sync 下调闭包取文案/高亮缓存进 `RefCell/Cell`
  （LogView 同款）; paint 逐行 `ROW_H` 画文案 + hover 底色（选中行 accent 底
  —— analysis picker 同构视觉）; event 合成几何命中 → `on_pick(idx, 缓存文案)。
  ≤ `MAX_PICKER_FIELDS=16` 封顶 + 「还有 N 列」尾行（analysis 先例; 尾行不可点
  或点=无动作）。
- **FilterForm 契约**（`view.rs` 或 `settings.rs`, T2 定形）: 持有 `value_ti:
  TextInput`（bind_clear 复位 rev —— 每次提交/关闭弹层清草稿）+ 挂 `RowList`
  （字段行）+ 算符六钮（静态子件, `on_click -> Msg::PickPickerOp`）+ paint 侧
  「过滤」钮; Enter/点钮同路: 读 `value_ti` 文本 → `Msg::PickerSubmit(value)`。
- **状态真身**（LogApp）: `picker_open: bool` / `picker_field: Option<String>`
  （点字段行存名）/ `picker_op: jsonl::Op`（默认 `Eq`）; 值草稿活在
  `value_ti` 缓冲（提交随 Msg 带出, 与主栏同构 —— 值不进 LogApp 真身, 免
  双源）。
- **T0 回归锁形态**: 构造 `RowList` + `LogApp`(schema=None) → paint 无行;
  换 schema → `sync` + paint → 行出现且文案跟随; 换 schema B → 行换 B ——
  **摘 sync 缓存重建 = 精确红**（启动快照 bug 的判罪锁）。col_menu 侧同款挂上
  即享（另加点行发 `ToggleColumn` 合成几何锁）。

## 任务

### T0: RowList 自绘行列表件 + col_menu 启动快照修复（前置）

`src/pick_list.rs` 新小件（契约见衍生设计）+ `col_menu_card` 行集换挂 RowList
（「恢复默认」静态钮保底行不动）+ `col_menu_row_names` 归 labels_fn 用。

Acceptance: ①sync 缓存重建跟随锁（**真 paint**, schema 就位/换文件行跟随 ——
摘重建精确红）②点行发 `ToggleColumn(name)`（合成几何注入）③hover/高亮态写读
④≤16 封顶 +「还有 N 列」⑤col_menu_row_names/弹层接线既有锁零回退。
Verify: `cargo test` + clippy 0。

### T1: 拼子句纯逻辑 + LogApp 状态/Msg 链

`build_clause`（6 算符 + 前缀尾 `*` + 空值/空字段拒绝）+ picker 三态字段 +
`Msg::{OpenPicker, ClosePicker, PickPickerField(String), PickPickerOp(Op),
PickerSubmit(String)}` 处理臂（`PickerSubmit` = 组装 → `filter_applied` 空格
追加 → `apply_filter`; 空值 notice 拒绝）。

Acceptance: ①拼子句 6 算符各一测 + 前缀 `parse_query` roundtrip 全等 ②空值/
空字段 None ③追加语义两态（空查询直提 / 有查询 AND 连接, roundtrip 锁）④提交
全链（既有过滤锁零回退, 命中数正确）⑤摘拼接 = 精确红。
Verify: `cargo test` + clippy 0。

## Checkpoint A（T0–T1）

- [ ] 判据 1–4 全绿; A/B 红两处（T0 sync 重建 / 拼子句）在案; **基线 376 不破**;
      fmt / clippy 0。

### T2: FilterForm 复合件 + 「字段…」按钮 + 弹层接线

FilterForm（值 TextInput + Enter 拦截 + 「过滤」钮 + RowList 字段行 + 算符六钮
——Bar 同构）+ 「字段…」paint 侧按钮（`schema.is_some()` 才画, hit 同源）+
Overlay 四件套 + Esc 次序插 `picker` + 模态守卫补 `picker_open` + 与
col_menu/export_menu 互斥 + 提交后关弹层/清草稿。

Acceptance: ①「字段…」显示判据（`.log` 不出——真 paint 锁）②表单全链
（点字段/算符/打值/Enter 与「过滤」钮同路）③Esc/scrim 关不提交 + 草稿清
④Esc 次序/互斥/模态守卫锁（col_menu 先例同款）⑤真 paint 接线锁（弹层行/按钮
字形产出, 非零平移原点）。
Verify: `cargo test` + clippy 0。

## Checkpoint B（T2）(机器部分)

- [ ] 判据 5–6 全绿; 三件套干净; **376 不破**。

### T3: 文档收口

ROADMAP 勾销「免语法字段查询 UI」/ README（能力行 + 快捷键表不加行——Out）/
map 行 / spec 实现记 + 机器判据回填 / acceptance-pending 续 **E 组五条** /
**table-column-config spec 勘误记**（T0 bug: 启动快照漏判 + 修复指针, 人工验收
A3 注记「行随文件换」并入 E5）/ todo·CLAUDE.md·记忆落账。
（无性能面——≤16 行常量成本。）

## Checkpoint C（T3）

- [ ] 机器判据逐条回填; 人工验收五条**记账**进总清单 E 组; 状态全线落账。

## 风险与对策

- **「按钮拿什么值」**（核实②）: 兄弟节点读不到 TextInput 缓冲 —— FilterForm
  持有者内收口（Enter/点钮同路读同缓冲）, 已点名不许绕。
- **T0 与 T2 的组件分歧**: RowList 先行（T0 独立小件 + col_menu 修复独立可验）,
  T2 只消费不改契约 —— 免得弹层装配反向污染组件 API。
- **roundtrip 测试口径**（T1）: 前缀算符产出 `字段=值*` 再 parse 回 `(Eq→Prefix,
  value)` —— 断言**期望值**按 `parse_clause` 现语义写（`*` 尾缀 = Prefix 无值星）,
  免得把语法面现状当 bug 纠缠。
