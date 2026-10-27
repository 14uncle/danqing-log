# todo-v1x-workspace-sessions: 命名工作台会话任务清单

- @author 十四叔
- @date 2026/09/24
- Spec: `docs/specs/SPEC-v1x-workspace-sessions.md` · Plan: `tasks/plan-v1x-workspace-sessions.md`
- 状态: **五段全闭**（2026-09-24 一日: 「go」×2 → /build auto T1–T4 → 双路评审
  并账 Critical×3+Required×5 全修 → code-simplify 3 项零变化收口）——
  人工验收记账（F 组）待实机
- 测试基线: **393** → build 收口 407 → review 收口 **413** → simplify 收口
  **413**（零修改）

## Phase 1: 持久化 + 状态链

- [x] **T1: 账本改名 state.json + sessions 段** —— `columns.rs`（`SessionEntry`
      结构无 `bookmarks` 键 + `sessions` 恒写/坏条丢条不丢账/收集硬顶 +
      `sessions_for_path` / `put_sessions_for_path` 只动本路径切片）+ 改名
      （`default_path` / `state_path` panic 封死随迁 should_panic 锁 /
      `legacy_state_path` **分支配对**读旧写新迁移——双派生方案无统一表达式）。
      Acceptance ①–⑥ **全过**; A/B: 摘恒写 = roundtrip 精确红。
      Verify: `cargo test` + clippy 0。
- [x] **T2: 会话 save/apply/delete 状态链** —— `main.rs`（`sessions`/
      `session_selected` 随路径载入 + `save_session` 快照四样/覆盖/上限 32 +
      `apply_session` 写穿 per-file/全链/`rebuild_expands` 剔除/回顶/书签
      零触碰 + `delete_session` 无确认 + **`DeleteSelectedSession` 指针语义**
      （plan 偏差: 无状态钮读不到选中名, 不绕镜像）+ `ExpandMap::lines()`）。
      Acceptance ①–⑥ **全过**; A/B: 摘写穿红/摘剔除红。
      Verify: `cargo test` + clippy 0。

## Checkpoint A（T1–T2）✅

- [x] 判据 1–3 全绿; A/B 红三处（摘恒写/摘写穿/摘剔除）在案; 基线 393 不破;
      三件套干净（收口 402 中段）。

## Phase 2: UI 接线

- [x] **T3: 会话弹层 + 状态栏入口 + 门控** —— `settings.rs`（`PickerInput`→
      `SubmitInput` 零行为变化泛化 + `sessions_card`: RowList 第三消费者契约
      零变化, 点行=应用+选中 / 命名输入 Enter 与「保存当前」同路 / 「删除」
      作用选中）+ `view.rs` 状态栏「会话」钮（无文件不出, Open Q3）+ 门控
      两道闸（`session_gate` 入口判 + 三臂兜底; 数据永在）+ 弹层族第四员全套
      （Esc 首插/互斥扩员/模态同源/清草稿）。Acceptance ①–⑤ **全过**（真
      paint 锁/门控两态付费永不误弹/持有者收口/picker 泛化零回退/删除说清）。
      Verify: `cargo test` + clippy 0。

## Checkpoint B（T3）(机器部分) ✅

- [x] 判据 4–5 全绿; 三件套干净; 基线不破（收口 **407** = 393 + 14）。

## Phase 3: 收口

- [x] **T4: 文档收口** —— ROADMAP 勾销（已建成注记）/ README 首批腿 + 边界 /
      map / spec 实现记（含 plan 偏差与修程纠偏）+ 判据 1–7 回填 / acceptance
      续 **F 组五条**（F→spec 映射）/ **三处兑现注记**（bookmark-persist 改名
      已知局限 ✅ / col-config 写穿接缝 ✅ / columns.rs strip 接缝注释 ✅）/
      todo·CLAUDE.md·记忆。（无性能面。）

## Checkpoint C（T4）✅

- [ ] 机器判据逐条回填; 人工验收五条记账进总清单 F 组; 状态全线落账。
- [ ] **人工验收（记账 → `tasks/acceptance-pending.md` F 组五条）**: ①四样
      保存→重启→应用回来 ②同文件两会话互切/同名覆盖 ③免费态入口升级提示
      且数据不丢/激活后可用 ④轮转后应用: 布局回来/越界安静消失 ⑤换文件列表
      只显本路径
- [x] 进 review 阶段（2026-09-24 **双路独立评审** —— 五轴全量 + 三区深潜
      互不知情, 均 Request changes; 并账 **Critical×3+Required×5 全修**:
      M1/M2 账本可辨谓词收口+回落同源+旧名退役一次性 / M3 上限 12 与可视取齐
      +降序 / M4 导出点穿复位 / M5 滚轮假绿改真锁 / M6 互斥双向锁 / M7
      clear_search 作废在途 / M8 正则拒收显式清空; Optional/Nit 全清
      (M9–M12 修复+锁, M13/M14/M16 文档与零变化重构, M15 记档)。+6 锁 →
      **413 绿**。
- [x] 进 code-simplify 阶段（2026-09-24 收口）—— 3 项行为零变化: widths
      双向去重（列三字段同模型真身）/ `now_secs` 收口 / 杂项; **不动清单**
      （hidden 解析分叉 = M13 本体 / 同构双锁 / 状态栏镜像块）见 spec 简化记。
      413 测试零修改全绿。留档: M15 删除指针独立选中实机再裁 / M11 窄窗几何
      实机核对 / `OpenColMenu` 不关 settings 既有缺口

## 遗留 / 待用户动作

1. 人工验收（记账中 —— F 组五条; 免费层无门控面走付费态需 key/商店态）
2. 跨文件/全局会话模板（Out: 账面无诉求; LogViewPlus Templates 跨文件面）
3. 多条搜索历史列表（Out 另起: 无基建, 实机喊累再议）
