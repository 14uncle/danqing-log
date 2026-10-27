# plan-v1x-bookmark-persist: 书签持久化 任务拆解

- @author 十四叔
- @date 2026/09/23
- Spec: `docs/specs/SPEC-v1x-bookmark-persist.md`（2026-09-23「go」批准: 范围三项
  推荐 + Open Q①② 全按推荐裁定）
- 状态: **待过目** —— 批准后进 build（`/build auto` 惯例: 零 commit、TDD 循环不减）
- 测试基线: **365**（2026-09-23 table-column-config 收口实测）

## 组件表

| 文件 | 职责 | 改动面 |
|---|---|---|
| `src/columns.rs` | `FileEntry.bookmarks` 字段 + 磁盘往返 + 载入归一（含上限截断） | 扩展既有基建, 不新建文件 |
| `src/main.rs` | `load_state_for_current_file` / `save_state` / `toggle_bookmark` 上限守卫 + apply_fresh 接线次序 | 方法改名 2 + toggle 扩 1 + 删 1 行 clear |
| `src/view.rs` | **零改动**（书签镜像走既有 sync 克隆） | — |
| 文档 | ROADMAP / README / map / spec 回填 / acceptance-pending 续 D 组 | T3 |

## 关键实现事实（2026-09-23 开工核实）

1. **`put` 调用点 9 处**（`columns.rs` 测试 ×8 + `main.rs:712`）; `get` 返回
   `Option<&ColumnConfig>`（`columns.rs:257`, main.rs:677 在用）——加书签后载入要拿
   **整条目**, 计 `get_entry(path) -> Option<&FileEntry>` 新访问器, `get` 降级为
   `.map(|e| &e.config)` 简写（测试调用点零改动）。`put` 签名**改收 `FileEntry`**
   （参数 4 个封顶后再拉长就是恶趣味; 9 处全改）。
2. **`FileEntry` 构造点 3 处**（`put` / `entry_from_value` / 测试字面量）——加
   `bookmarks: Vec<u64>` 全显式补齐, 不引 Default derive（结构小, 显式即文档）。
3. **`toggle_bookmark` 是 LogApp 直调**（`app_key_filter` 键分派 2112/2146, 非 Msg）——
   增删后**直接 `self.save_state()`**（`save_columns(&self)` 同款可从 `&mut self` 调）,
   不加 Msg（零 Msg 面 = 零 update 臂改动）。
4. **load/save 调用点收口**: `load_columns_for_current_file` 仅 `apply_fresh:1253`
   一处; `save_columns` 五处（1851/1856/1861/1877/1885 全在列配置 Msg 臂）+ toggle 新增。
   改名 `load_state_for_current_file` / `save_state` 一次扫净（名字随职责走:
   现在写的不只是列）。
5. **次序陷阱（T2 的刀口）**: `apply_fresh` 现状 = `:1253` 载列配置 → `:1273`
   `bookmarks.clear()` —— 若只改载入不清 clear, **刚载入的书签被下一行清掉**（假绿
   面: 测试若只测 load_state 单元不测 apply_fresh 全链就漏）。修 = 删 clear 行
   （载入 = 覆盖语义, 无条目 = 空集, 与 clear 等价且带上记忆）。
6. **归一粒度沿用评审 C6 定案**: 坏 `bookmarks` **丢字段不丢条**——逐元素
   `as_u64` 容错（非数值/超 u64 丢弃）, 不用 `?` 自爆; 去重升序收编; 超
   `MAX_BOOKMARKS=256` 截断保升序前 256（存量手造数据收编语义）。

## 衍生设计

- **磁盘 Vec / 内存 BTreeSet 的转换点只在三处**: `load_state_for_current_file`
  （Vec→BTreeSet, 顺带越界剔除）、`save_state`（BTreeSet→升序 Vec）、
  `toggle_bookmark`（经 save_state 间接）。磁盘形状稳定 = 升序去重 Vec
  （roundtrip 全等判据的锚）。
- **`MAX_BOOKMARKS` 置 `columns.rs`**（模型约束层, 与 `MIN_COL_W`/`FILE_CAP` 同层）;
  守卫文案「书签已达上限 256, 先去掉一些」放 `toggle_bookmark`（「至少保留一列可见」
  同款: 拒绝 + 说清为什么 + 零变更）。
- **越界剔除两路径同式**: `load_state`（按载入时 `line_count`）与
  `apply_rebuild:1179`（既有 `retain`, 不动）; 剔除后**不写回**（损坏零写回同哲学）。

## 任务

### T1: `FileEntry.bookmarks` + 磁盘往返 + 载入归一（`columns.rs`, 零 UI）

`MAX_BOOKMARKS: usize = 256` / `FileEntry` 加 `bookmarks: Vec<u64>`（3 构造点补齐）/
`entry_to_value` 写出（恒写, 空=空数组; **不省略键** —— roundtrip 全等好判）/
`entry_from_value` 归一（可缺省空; 逐元素容错/去重/升序/超限截断保前 256）/
`get_entry` + `put(FileEntry)`。

Acceptance: ①roundtrip 全等（缺省/空/满三形态）②归一四态锁（非数值丢弃/
重复去重/乱序升序/超限截断）③摘 `entry_to_value` 的 bookmarks 写出 = **精确红**
④坏 `bookmarks` 不废整条（列摆法照留）⑤LRU/损坏容错既有锁零回退。
Verify: `cargo test` + clippy 0。

### T2: LogApp 接线（载入/落盘/上限/失效语义）

`load_columns_for_current_file`→`load_state_for_current_file`（一次读盘同取列摆法
+ 书签; 书签按 `line_count` 越界剔除**不写回**; 无条目 = 默认摆法+空书签）/
`save_columns`→`save_state`（6 调用点扫净）/ `toggle_bookmark` 上限守卫 + 增删成功
即 `save_state` / **删 `apply_fresh` 的 `bookmarks.clear()`**（核实⑤次序陷阱）。

Acceptance: ①toggle 增删即落盘、重读（`load_state`）恢复全等 ②per-路径隔离
（A 书签不带进 B; 换回 A 又在）③越界剔除两路径（load 侧新锁 + rebuild 既有行为
回归锁）④上限 256 满员拒绝+提示+零变更, 删一个又能加 ⑤apply_fresh 全链锁
（载入替换 clear 语义 —— 核实⑤的防漏锁）⑥摘落盘 / 摘载入各**精确红**。
Verify: `cargo test` + clippy 0。

## Checkpoint A（T1–T2）

- [ ] 机器判据 1–6 全绿; A/B 精确红两处（落盘/载入）在案; **基线 365 不破**;
      fmt / clippy 0。

## Phase 3: 测量与收口

### T3: 文档收口

ROADMAP §一勾销「书签持久化」/ README 免费层能力行补书签持久化 / map 行状态 /
spec 实现记 + 成功判据机器部分回填 / `acceptance-pending.md` **续 D 组五条** /
todo·CLAUDE.md·记忆落账。
（本模块无性能面——BTreeSet ≤256 条常量成本, 不设 logbench 项。）

## Checkpoint B（T3）

- [ ] 机器判据逐条回填; 人工验收五条**记账**进总清单 D 组; 状态全线落账。

## 风险与对策

- **次序陷阱**（核实⑤）: 已点名 + T2⑤ 全链锁防漏。
- **roundtrip 全等 vs 归一**: 归一在 `entry_from_value`, `entry_to_value` 写的是
  已归一态 —— roundtrip 测试须用**归一后**的期望值比对（乱序输入 → 升序输出,
  不是原样回）, 测试写法在 T1 就钉死, 免得假红纠缠。
