# plan-v1x-field-analytics: 字段分析（腿二）实施计划

- @author 十四叔
- @date 2026/09/19
- Spec: `docs/specs/SPEC-v1x-field-analytics.md`（D1–D8 已定，本计划只排任务不重复论证）
- 任务清单: `tasks/todo-v1x-field-analytics.md`

## Overview

引擎前置（顶层字段扫描器，danqing-logfile）→ 分析器（本仓 lib 纯逻辑）→
侧栏 UI + 门控 → logbench 实测。跨仓联动一次（danqing-logfile 先 push → 本仓复钉）。

## Architecture Decisions（spec 指针）

- D1 引擎前置 = `scan_field` 单遍状态机，差分对拍 serde_json 锁正确性
- D2 reservoir 采样（上限 100 万值，超限标注）+ count/min/max/mean 流式精确
- D3 作用域跟随过滤（行集走单个前向迭代器跳行，行号集天然升序）
- D4 侧栏扩展；D5 门控 = 分析按钮；D6 类型判定前 100 行投票；D7 ≤1.5s/1GiB 单列

## 依赖图

```
T1 scan_field (danqing-logfile) ──→ T2 数值聚合 ──→ T3 枚举聚合+作用域 ──→ T4 侧栏UI+门控 ──→ T5 logbench+实测
```

无并行面：每环咬上一环（T2/T3 可分先后但都吃 T1）。

## 任务

### Phase 1: 引擎前置（danqing-logfile）

- **T1** `scan_field` 顶层字段扫描器 + 差分对拍

### Checkpoint A（T1 后）

- [ ] 差分全等（采样行集 + 对抗样本）；danqing-logfile 三件套绿
- [ ] **联动点**：兄弟仓先 push → 本仓（patch 开着）`cargo update -p danqing-logfile` 复钉（push 需用户授权，build 期 patch 顶着）

### Phase 2: 分析器（本仓 lib）

- **T2** `src/analysis.rs`：类型判定 + 数值聚合（流式四项 + reservoir 分位数 + 采样标注）
- **T3** 枚举聚合（Top 20 + 其他桶 + distinct 上限标注）+ 混合类型跳过 + `AnalysisResult`（含作用域行数/过滤串）+ 行集走法（无过滤全扫 / 有过滤跳行）

### Checkpoint B（T2–T3 后）

- [ ] 小文件手算全等；标注语义测试齐；本仓三件套绿

### Phase 3: UI + 门控 + 测量

- **T4** 侧栏「字段分析」区（`src/analysis_panel.rs` 新组件，侧栏容器改两段）+ AsyncJob 后台跑 + 结果渲染（统计表 / 对数计数条复用 `bar_fraction`）+ 作用域行 + 「基于旧过滤 · 重跑」标注 + 门控（免费态 `ShowUpgradePrompt(FieldAnalytics)`，下拉不拦）+ .log 模式不显示 + 换文件清空
- **T5** `logbench --analyze <field>` + demo-1gb.jsonl 实测（热缓存 ≤1.5s 目标核对）+ `PERFORMANCE_REPORT.md` 补数字

### Checkpoint C（T4–T5 后）

- [ ] spec 成功判据逐条勾（差分/手算/标注/作用域/门控/性能）
- [ ] 人工验收（用户实机：demo-1gb.jsonl 跑 duration_ms / status）
- [ ] 进 review

## Risks and Mitigations

| 风险 | 级别 | 缓解 |
|---|---|---|
| `scan_field` 状态机边界错（转义/嵌套/行尾残件） | 高 | D1 差分对拍 serde_json（采样行集 + 对抗样本 + genlog 全量行抽查）；错即判死重做，不带着错进聚合 |
| 性能不达 1.5s | 中 | D7 备选：memmem 先定位候选再局部验证（粗筛倒置）；凭 T5 实测数据定 |
| 侧栏空间挤压（直方图 + 分析区抢高） | 低 | 量了再写（09-13 教训）；结果区高度实测定稿 |
| AsyncJob 代次语义误用 | 低 | 本场景**正是**要的语义：重复点分析 = 作废旧轮；与购买那次（一次性不可丢）相反，注释写明对照 |
| 宽行文件（563 KiB/行形状）拖慢 | 低 | 目标值按 demo 文件形状定；已知局限已写 spec |

## Open Questions（spec 原文保留）

- reservoir 上限 / 枚举 Top N / 侧栏结果区高度 —— build 凭实测定
