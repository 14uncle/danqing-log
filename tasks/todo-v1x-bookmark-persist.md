# todo-v1x-bookmark-persist: 书签持久化 任务清单

- @author 十四叔
- @date 2026/09/23
- Spec: `docs/specs/SPEC-v1x-bookmark-persist.md` · Plan: `tasks/plan-v1x-bookmark-persist.md`
- 状态: **plan 待过目** —— 批准后进 build
- 测试基线: **365**（2026-09-23 table-column-config 收口实测）→ 收口预计 ~380

## Phase 1: 磁盘模型（`src/columns.rs`, 零 UI）

- [ ] **T1: FileEntry.bookmarks + 磁盘往返 + 载入归一** —— `MAX_BOOKMARKS=256` /
      字段补齐 3 构造点 / `entry_to_value` 恒写 / `entry_from_value` 归一
      （逐元素容错/去重/升序/超限截断保前 256, 坏字段丢字段不丢条）/ `get_entry` /
      `put(FileEntry)`（9 调用点）。
      Acceptance: ①roundtrip 三形态 ②归一四态 ③摘写出精确红 ④坏字段不废条
      ⑤LRU/容错既有锁零回退。
      Verify: `cargo test` + clippy 0。

## Checkpoint A（T1–T2）

- [ ] 机器判据 1–6 全绿; A/B 红两处（落盘/载入）在案; 基线 365 不破; 三件套干净。

## Phase 2: 全链路接线（`src/main.rs`）

- [ ] **T2: 载入/落盘/上限/失效语义** —— `load_state_for_current_file`（同取两态,
      越界剔除不写回）/ `save_state`（6 调用点）/ `toggle_bookmark` 上限守卫 +
      增删即落盘 / **删 apply_fresh 的 `bookmarks.clear()`**（次序陷阱, plan 核实⑤）。
      Acceptance: ①toggle 落盘+重读全等 ②per-路径隔离 ③越界剔除两路径 ④上限
      拒绝+提示+零变更 ⑤apply_fresh 全链锁 ⑥摘落盘/摘载入各精确红。
      Verify: `cargo test` + clippy 0。

## Phase 3: 测量与收口

- [ ] **T3: 文档收口** —— ROADMAP 勾销 / README / map / spec 实现记回填 /
      acceptance-pending 续 D 组五条 / todo·CLAUDE.md·记忆落账。
      （无性能面。）

## Checkpoint B（T3）(机器部分)

- [ ] spec 成功判据机器部分逐条过（基线不破 + A/B 红在案）
- [ ] **人工验收（记账 → `tasks/acceptance-pending.md` D 组五条）**: ①夹/去书签
      重启还在 ②换文件各记各的 ③删行/轮转越界书签消失其余照旧 ④手坏
      columns.json 如常 ⑤满 256 提示、去掉又能加
- [ ] 进 review 阶段（`/agent-skills:code-review-and-quality`）
- [ ] 进 code-simplify 阶段

## 遗留 / 待用户动作

1. 人工验收（记账中 —— D 组五条, 免费层无门控）
2. 载体改名 `state.json`（Open Q① 已裁**不做**——腿四一次到位; 实机/腿四再启）
