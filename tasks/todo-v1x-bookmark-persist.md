# todo-v1x-bookmark-persist: 书签持久化 任务清单

- @author 十四叔
- @date 2026/09/23
- Spec: `docs/specs/SPEC-v1x-bookmark-persist.md` · Plan: `tasks/plan-v1x-bookmark-persist.md`
- 状态: **五段全闭**（2026-09-23 一日: 「go」→plan→build→双路评审并账
  Critical×1+Required×6 全修 +4 锁→code-simplify, 全程零 commit）——
  人工验收记账（D 组）
- 测试基线: **365** → build 收口 372 → review 收口 **376**（simplify 行为零变化
  维持 376）

## Phase 1: 磁盘模型（`src/columns.rs`, 零 UI）

- [x] **T1: FileEntry.bookmarks + 磁盘往返 + 载入归一** —— `MAX_BOOKMARKS=256` /
      字段补齐 3 构造点 / `entry_to_value` 恒写 / `entry_from_value` 归一
      （逐元素容错/去重/升序/超限截断保前 256, 坏字段丢字段不丢条）/ `get_entry` /
      `put(FileEntry)`（8 调用点, 测试构造器 `entry()` 收噪）。
      Acceptance: ①roundtrip 三形态 ②归一四态 ③摘写出精确红 ④坏字段不废条
      ⑤LRU/容错既有锁零回退。—— **全过**（`roundtrip_preserves_bookmarks_in_three_forms`
      + `entry_from_json_normalizes_bookmarks_leniently`; 摘写出红 `[]≠[3,5,10]` 在案）。
      Verify: `cargo test` + clippy 0。

## Checkpoint A（T1–T2）✅

- [x] 机器判据 1–6 全绿; A/B 红**三处**（摘写出/摘载入读取/摘 toggle 落盘）在案
      （摘载入红 `[0]≠[1]` A 残留进 B; 摘落盘红「增即落盘」）; 基线 365 不破
      （收口 **372** = 365 + columns 2 + main 5）; 三件套干净（fmt / clippy 0）。

## Phase 2: 全链路接线（`src/main.rs`）

- [x] **T2: 载入/落盘/上限/失效语义** —— `load_state_for_current_file`（同取两态,
      越界剔除不写回）/ `save_state`（6 调用点）/ `toggle_bookmark` 上限守卫 +
      增删即落盘 / **删 apply_fresh 的 `bookmarks.clear()`**（次序陷阱按 plan
      核实⑤拆除）。Acceptance ①–⑥ **全过**（五锁: toggle 落盘重读 / per-路径+
      apply_fresh 替换语义 / 载入越界剔除不写回 / 上限守卫 / rebuild 越界回归）。
      Verify: `cargo test` + clippy 0。

## Phase 3: 测量与收口

- [x] **T3: 文档收口** —— ROADMAP 勾销「书签持久化」/ README（能力行 + 已知
      边界行更新）/ map 行 / spec 实现记 + 机器判据回填 / acceptance-pending
      续 **D 组五条** / todo·CLAUDE.md·记忆落账。
      （无性能面。）

## Checkpoint B（T3）(机器部分)

- [x] spec 成功判据机器部分逐条过（基线不破 + A/B 红在案, 372 绿）
- [ ] **人工验收（记账 → `tasks/acceptance-pending.md` D 组五条）**: ①夹/去书签
      重启还在 ②换文件各记各的 ③删行/轮转越界书签消失其余照旧 ④手坏
      columns.json 如常 ⑤满 256 提示、去掉又能加
- [x] 进 review 阶段（`/agent-skills:code-review-and-quality`）—— 2026-09-23
      **双路独立评审**（五轴全量 + 三区深潜互不知情）均 Request changes; 并账
      **Critical×1+Required×6 全修**（坏文件覆盖抹全记忆→原子落盘+.bak 备份守卫 /
      幽灵行号落盘→Option 守卫+save 过滤 / LRU 静默丢真书签→无书签先挤淘汰保护 /
      落盘失败谎报→bool+「(未落盘)」+set_notice 次序陷阱 / 两锁补半边 / 旧注释
      勘误）+ Optional 修 2·文档化 2 / Nit 同车, +4 锁 → **376 绿**。
      修复清单与 A/B 红见 spec 评审记。
- [x] 进 code-simplify 阶段 —— 2026-09-23 收口: 抽 `backup_if_corrupt`（save_state
      守卫策略块命名化）, 不动清单（entry 三段容错循环 / toggle 四元组后缀 /
      merge 去重上轮定案）见 spec 简化记, 376 绿行为零变化。留实机项: toggle
      写放大 debounce 观察。

## 遗留 / 待用户动作

1. 人工验收（记账中 —— D 组五条, 免费层无门控）
2. 载体改名 `state.json`（Open Q① 已裁**不做**——腿四一次到位; 实机/腿四再启）
