# todo-v1x-field-picker-ui: 免语法字段查询 UI 任务清单

- @author 十四叔
- @date 2026/09/23
- Spec: `docs/specs/SPEC-v1x-field-picker-ui.md` · Plan: `tasks/plan-v1x-field-picker-ui.md`
- 状态: **五段全闭**（2026-09-23 build+review: /build auto → 双路评审并账
  Critical×1+Required×6 全修 +5 锁; 2026-09-24 code-simplify 行为零变化收口
  8 项, 测试零修改）—— 人工验收记账（E 组）待实机
- 测试基线: **376** → build 收口 388 → review 收口 **393** → simplify 收口
  **393**（零修改）

## Phase 0: 前置修复（T0, table-column-config 漏判搭车）

- [x] **T0: RowList 自绘行列表件 + col_menu 启动快照修复** —— `src/pick_list.rs`
      （sync 闭包缓存 + paint 行/hover + event 合成几何命中→Msg, ≤16 封顶 +
      「还有 N 列」）+ `col_menu_card` 行集换挂（「恢复默认」保底行不动;
      **快照参数整个删除**）。Acceptance ①–⑤ **全过**; 判罪锁
      `col_menu_rows_follow_schema_across_sync`（摘 sync 重建红 `0.0≠84.0`）。
      Verify: `cargo test` + clippy 0。

## Phase 1: 纯逻辑 + 状态链

- [x] **T1: 拼子句 + Msg 链** —— `build_clause`（6 算符 + 前缀尾 `*`, 空值/空字段/
      含空白值拒绝）+ picker 三态 + 6 个 Msg 臂（含 `PickerValueInput` 值镜像 ——
      plan 偏差: 许可页 `LicenseKeyInput` 先例替代自绘复合件, 见 spec 实现记;
      `PickerSubmit` = 组装→空格追加→`apply_filter`）。Acceptance ①–⑤ **全过**。
      Verify: `cargo test` + clippy 0。

## Checkpoint A（T0–T1）✅

- [x] 判据 1–4 全绿; A/B 红两处在案（T0 sync 重建 / 追加拼接
      `"level=ER*"≠"level=ERROR level=ER*"`）; 基线 376 不破; 三件套干净。

## Phase 2: UI 接线

- [x] **T2: 「字段…」按钮 + 弹层接线** —— Bar paint 侧按钮（hint 同款先测后存
      同帧让位, `has_schema` 判据）+ 查询卡（RowList 字段行 + 算符六钮 + 值输入
      镜像 + 「过滤」钮）+ 四件套 + Esc 插层 + 互斥 + 模态守卫 + Enter 同路 +
      换文件关弹层 + 关闭清草稿。Acceptance ①–⑤ **全过**（修程纠偏: Enter 块
      误嵌 Esc 分支死代码当场抓出挪正）。
      Verify: `cargo test` + clippy 0。

## Checkpoint B（T2）(机器部分) ✅

- [x] 判据 5–6 全绿; 三件套干净; 376 不破（收口 **388** = 376 + 12 锁）。

## Phase 3: 收口

- [x] **T3: 文档收口** —— ROADMAP 勾销 / README 能力行 / map / spec 回填 /
      acceptance 续 **E 组五条** / **table-column-config spec 勘误记**（T0 bug
      修复指针, A3 由 E5 兼验）/ todo·CLAUDE.md·记忆。（无性能面。）

## Checkpoint C（T3）(机器部分)

- [x] 机器判据逐条回填（1–7 全过, 388 绿）; 人工验收五条记账进总清单 E 组;
      状态全线落账。
- [ ] **人工验收（记账 → `tasks/acceptance-pending.md` E 组五条）**: ①「字段…」
      点选全链命中正确 ②追加 AND + Esc 清回全量 ③空值拒绝/Esc 不提交 ④`.log`
      无按钮/免费态直用 ⑤T0: 列管理弹层行随文件换（兼 A3 勘误验点）
- [x] 进 review 阶段（`/agent-skills:code-review-and-quality`）—— 2026-09-23
      **双路独立评审**（五轴 + 三区深潜互不知情）均 Request changes; 并账
      **Critical×1+Required×6 全修**: 拼子句拒收面盖住 parse 破坏面（值/字段
      保留字符/Eq 尾星/Prefix 含星, 表驱动对抗锁）/ 托盘互斥补全 / **值镜像退役
      回归 PickerInput 持有者收口**（R3 脱钩窗 + R7 全局 Enter 劫持 + R8 送焦
      三缺陷一次消解, plan 偏差反转）/ RowList 载荷锚定（换数据不送错行）/
      弹层键路门禁同源（导航键/滚轮不穿）。Optional/Nit 全清。+5 锁 → **393 绿**。
- [x] 进 code-simplify 阶段（2026-09-24 收口）—— 8 项行为零变化: 拒收说清
      单一收口 / 三弹层开合+三门禁同源 / 五卡壳收口 / POPOVER_ROWS_MAX /
      BODY_SIZE 同源别名 / RowList 假缓存字段删 / 过期注释更正 ×2 / 可见性
      收紧; **FONT_SIZE 耦合方向裁定**（view = token 家, 消费者指向它正确;
      同值分家已钉）。留档不动: 许可页镜像 R3 同族窗实机再收 / RowList 五闭包
      等第三消费者。393 测试零修改全绿

## 遗留 / 待用户动作

1. 人工验收（记账中 —— E 组五条, 免费层无门控）
2. 值列表点选（Out: 无值发现基建; 实机喊累另起, 触发面 = field-analytics 值域）
