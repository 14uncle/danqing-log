# todo-v1x-field-picker-ui: 免语法字段查询 UI 任务清单

- @author 十四叔
- @date 2026/09/23
- Spec: `docs/specs/SPEC-v1x-field-picker-ui.md` · Plan: `tasks/plan-v1x-field-picker-ui.md`
- 状态: **plan 待过目** —— 批准后进 build
- 测试基线: **376**（2026-09-23 bookmark-persist 收口实测）→ 收口预计 ~400

## Phase 0: 前置修复（T0, table-column-config 漏判搭车）

- [ ] **T0: RowList 自绘行列表件 + col_menu 启动快照修复** —— `src/pick_list.rs`
      （sync 闭包缓存 + paint 行/hover + event 合成几何命中→Msg, ≤16 封顶 +
      「还有 N 列」）+ `col_menu_card` 行集换挂（「恢复默认」保底行不动）。
      Acceptance: ①sync 重建跟随锁（真 paint, 摘重建精确红）②点行发
      `ToggleColumn` ③hover/高亮 ④封顶尾行 ⑤既有锁零回退。
      Verify: `cargo test` + clippy 0。

## Phase 1: 纯逻辑 + 状态链

- [ ] **T1: 拼子句 + Msg 链** —— `build_clause`（6 算符 + 前缀尾 `*`, 空值/空字段
      拒绝）+ picker 三态 + 5 个 Msg 臂（`PickerSubmit` = 组装→空格追加→
      `apply_filter`）。Acceptance: ①6 算符 + `parse_query` roundtrip ②拒绝两态
      ③追加两态 ④全链命中正确 ⑤摘拼接精确红。
      Verify: `cargo test` + clippy 0。

## Checkpoint A（T0–T1）

- [ ] 判据 1–4 全绿; A/B 红两处在案; 基线 376 不破; 三件套干净。

## Phase 2: UI 接线

- [ ] **T2: FilterForm + 「字段…」按钮 + 弹层接线** —— 复合件（值 TextInput +
      Enter 拦截 + 「过滤」钮 + RowList 字段行 + 算符六钮, Bar 同构）+ paint 侧
      「字段…」（`schema.is_some()` 才画）+ 四件套 + Esc 插层 + 互斥 + 模态守卫 +
      提交关弹层清草稿。Acceptance: ①显示判据真 paint 锁 ②表单双路同路
      ③Esc/scrim 不提交 ④接线锁族 ⑤真 paint 锁（非零平移）。
      Verify: `cargo test` + clippy 0。

## Checkpoint B（T2）(机器部分)

- [ ] 判据 5–6 全绿; 三件套干净; 376 不破。

## Phase 3: 收口

- [ ] **T3: 文档收口** —— ROADMAP 勾销 / README / map / spec 回填 / acceptance
      续 E 组五条 / **table-column-config spec 勘误记**（T0 bug 修复指针, A3 并入
      E5）/ todo·CLAUDE.md·记忆。（无性能面。）

## Checkpoint C（T3）(机器部分)

- [ ] 机器判据逐条回填; 人工验收五条记账进总清单 E 组; 状态全线落账。
- [ ] **人工验收（记账 → `tasks/acceptance-pending.md` E 组五条）**: ①「字段…」
      点选全链命中正确 ②追加 AND + Esc 清回全量 ③空值拒绝/Esc 不提交 ④`.log`
      无按钮/免费态直用 ⑤T0: 列管理弹层行随文件换
- [ ] 进 review 阶段（`/agent-skills:code-review-and-quality`）
- [ ] 进 code-simplify 阶段

## 遗留 / 待用户动作

1. 人工验收（记账中 —— E 组五条, 免费层无门控）
2. 值列表点选（Out: 无值发现基建; 实机喊累另起, 触发面 = field-analytics 值域）
