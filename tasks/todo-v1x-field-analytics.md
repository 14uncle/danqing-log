# todo-v1x-field-analytics: 字段分析（腿二）任务清单

- @author 十四叔
- @date 2026/09/19
- Spec: `docs/specs/SPEC-v1x-field-analytics.md` · Plan: `tasks/plan-v1x-field-analytics.md`
- 状态: **review 完成（2026-09-19, 6 Required 全修, 本仓 250 + logfile 68 测试绿）, 待 code-simplify** —— 2026-09-19

## Phase 1: 引擎前置（danqing-logfile）

- [x] **T1: `scan_field` 顶层字段扫描器 + 差分对拍**
  - 内容：`danqing-logfile/src/scan.rs` 新模块——单遍状态机（in-string / escape /
    花括号深度），顶层 key 精确匹配，产出类型化 token（`Num`/`Str`/`Bool`/`Null` +
    原字节切片）；零 Value 树、零分配（切片引用）。注册进其 lib.rs
  - Acceptance：与 serde_json 逐行 parse **差分全等**——采样行集 + 对抗样本
    （`,"level":"` 内嵌于字符串值 / 嵌套对象同名 key 取顶层 / 转义引号 /
    `\uXXXX` / 科学计数法 / 负数 / 行尾缺逗号 / 空对象）+ genlog 产出文件
    全量行抽查；类型判定与 serde 一致
  - Verify：`cargo test`（danqing-logfile）+ 其三件套
  - Files：`danqing-logfile/src/scan.rs`、`danqing-logfile/src/lib.rs`

## Checkpoint A ✅

- [x] 差分全等；danqing-logfile 三件套绿（68 测试，含 scan 11 条）
- [ ] 联动：兄弟仓 push（用户授权）→ 本仓 `cargo update -p danqing-logfile` 复钉 —— **待办，patch 顶着**

## Phase 2: 分析器（本仓 lib）

- [x] **T2: 类型判定 + 数值聚合**
  - 内容：`src/analysis.rs`——目标行集前 100 行采样投票定列型（全数值→数值列，
    否则枚举列）；数值聚合 count/min/max/mean 流式 + reservoir（Vitter R 法，
    上限 100 万值）出 p50/p95/p99，超限标「采样估计」
  - Acceptance：小文件手算全等（含负数/浮点/科学计数法）；reservoir 不超限时
    分位数 == 排序精确值；超限时标注位为真；reservoir 内容随机性用定种子可复现
  - Verify：`cargo test analysis`
  - Files：`src/analysis.rs`、`src/lib.rs`（注册）

- [x] **T3: 枚举聚合 + 混合类型 + 作用域行集走法**
  - 内容：枚举 `HashMap` 计数 → Top 20 +「其他」桶（distinct > 1 万时超出并入并
    标注）；bool/null 入枚举；混合类型按多数类型 + 跳过计数；`AnalysisResult`
    （数值|枚举 + 作用域行数 + 过滤串快照）；行集走法：无过滤 `lines_from(0)`
    全扫 / 有过滤单迭代器前向跳行（行号集升序）
  - Acceptance：Top-N 排序正确（计数降序、同计数按值字典序——确定性）；超限标注；
    跳过计数正确；作用域行数 == 行集大小；过滤行集走法与全扫结果一致
    （无过滤 ≡ 全文件的等价测试）
  - Verify：`cargo test analysis`
  - Files：`src/analysis.rs`

## Checkpoint B ✅

- [x] 分析器测试全绿；本仓三件套绿（242）；基线不破

## Phase 3: UI + 门控 + 测量

- [x] **T4: 侧栏「字段分析」区 + 门控**
  - 内容：`src/analysis_panel.rs` 新组件；侧栏容器改两段（直方图区 + 分析区，
    分析区只在表格模式有 schema 时出现——.log 不显示不是灰掉）；**字段行逐行
    可点**（点字段即分析——下拉是建树冻结的框架控件而 schema 开文件后才有，
    已改判，见 spec 实现记）+ 结果区（数值 = 统计表；枚举 = Top 20
    对数计数条，复用 `bar_fraction`）；AsyncJob 后台跑 + 结果作用域行（行数 +
    过滤串快照）+ 过滤变更后标「基于旧过滤 · 重跑」（比 rev/串）；换文件清空；
    门控：免费态点字段行 → `ShowUpgradePrompt(Feature::FieldAnalytics)`（此时
    licensing 的 `#[allow(dead_code)]` 删掉）
  - Acceptance：免费态点分析弹提示且**无扫描发生**（job 未发起断言）；付费态
    出结果；.log 模式无此区；过滤变更后旧结果带标注；换文件清空
  - Verify：`cargo test` + 三件套
  - Files：`src/analysis_panel.rs`（新）、`src/histogram.rs`、`src/main.rs`、`src/view.rs`、`src/lib.rs`

- [x] **T5: logbench --analyze + 实测 + 报告**
  - 内容：`logbench demo-1gb.jsonl --analyze duration_ms`；热缓存跑 3 次取稳态；
    核对 ≤1.5s 目标；`PERFORMANCE_REPORT.md` 补分析行（口径注明文件形状）
  - Acceptance：数字落报告；未达标则按 D7 备选优化后重测（凭数据）
  - Verify：logbench 输出 + 报告 diff
  - Files：`src/bin/logbench.rs`、`PERFORMANCE_REPORT.md`

## Checkpoint C

- [x] spec §成功判据机器部分逐条过（差分/手算/标注/作用域/门控/性能 1001ms ≤ 1.5s）
- [x] 人工验收（用户实机）—— **2026-09-20 通过**（三轮）: demo-1gb.jsonl 数值/枚举两路
      跑通, 免费态弹窗、付费态激活、hover 居中、采样标注行不裁、粘贴不溢出、
      暗色占位可辨、**直方图与字段分析区共存**。四条发现全修, 最重的一条是
      直方图整块消失 → 根因是腿二把它挪进 Column 后宽度折叠判定口径变了
      (详见 spec 人工验收节 + `src/sidebar.rs` 模块头)
- [x] 进 review 阶段（`/agent-skills:code-review-and-quality`）—— 2026-09-19 完成:
      REQUEST CHANGES, 无 Critical, **6 Required 全修**（R1 采样标注行漏算 /
      R2 hover 光标驱动化 / R3 枚举超限行并入其他+capped 语义拆分 / R4 文本截断
      +clip 兜底 / R5 过滤落账串+在途闸 / R6 选择器封顶 16 列）; 修复锁测试 8 条,
      基线 242→250; Optional 6 条记录在案未修（见 spec 评审记）
- [x] 进 code-simplify 阶段（licensing 与 field-analytics 两模块都欠）——
      2026-09-19 完成: 4 处简化（枚举计数器死代码/别名残留/同义 arm 合并/
      COM 起手式提公用）, 行为零变化, 250 绿不破; 详见两份 spec 简化记
