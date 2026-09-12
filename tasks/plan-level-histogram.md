# Implementation Plan: level-histogram (级别计数侧栏)

> spec: `docs/specs/SPEC-level-histogram.md` · todo: `tasks/todo-level-histogram.md`
> 2026-09-12 plan 阶段。四项用户裁决 (D1–D4) + 一项读码后改判 (D6)。

## Overview

把级别分布从「渲染期逐可见行染色」提前到「打开期一趟并行扫描算清」, 在左侧常驻侧栏
呈现 6 桶计数与对数刻度横条; JSONL 模式下点柱条套用既有 `level=<NAME>` 过滤语法。
不改兄弟 crate, 不动已发布口径的索引耗时。

## Architecture Decisions

- **D6 (本 plan 新增, 替代 spec 原 D2 的原始模式分支)**: 计数 = **独立并行扫描**,
  不进索引趟。读码依据 —— `build_line_index_with` 的扫描只对字节做 `\n` 的 memchr,
  **不读行内容**, 「顺带统计」不成立; 而顺序遍历 + 行首 200 字节内匹配 5 个关键词
  是 `run_filter` 74ms/GB 的 8 倍量级 (`run_filter` 是并行的, 见 `jsonl.rs` 的
  `PARALLEL_FILTER_THRESHOLD` / `MAX_FILTER_THREADS`)。独立并行扫描约 50ms 量级,
  索引数字不动, 且**零跨仓改动**。
- **侧栏是 sibling, 不进 LogView 内部坐标数学**。现顶层是
  `Stack[Column[TitleBar.embed(Bar), LogView.fill], Overlay(设置卡)]`; 改为
  `Column[TitleBar.embed(Bar), Row[Histogram(HIST_W), LogView.fill]]` —— LogView
  只拿到一个更窄的 `area`, 其内部 (gutter/x 偏移/命中测试/横滚范围) **一行不改**。
  侧栏自身底部留 `STATUS_HEIGHT` 内边距, 使状态栏在视觉上仍然通栏。
- **并行骨架用 `std::thread::scope`**: `Arc<LogFile>` 已被现有代码跨线程移动
  (`main.rs` 的 filter/search job), 故 `LogFile: Send + Sync` 成立, scope 借用即可,
  不必再 Arc 一次。
- **分段起点定位**: 第 k 段从 `lines_from(k * line_count / N)` 起 —— 其内部是
  二分定位 + memchr 前扫, O(log) 而非 O(lines), 分段本身不引入线性开销。
- **分类器单一谓词**: `classify_level(&[u8]) -> Level` 供两条路径共用 —— 明文传
  行首 200 字节切片, JSONL 传 `extract_field` 取出的字段值。**口径统一靠它**。

## Task List

### Phase 0: 风险尖兵 (先量成本, 再建东西)

- [ ] **T1: 分类器 `classify_level` + 单测**
- [ ] **T2: 并行计数 `count_levels` + 索引零回归实测**

### Checkpoint A: 成本与正确性定档

- [ ] 分类 6 桶之和 == 总行数 (对拍)
- [ ] 并行 == 串行 (对拍)
- [ ] 1GB 明文 / JSONL 计数墙钟实测**落档**
- [ ] 索引 77 / 88ms **未变动** (logbench 对照)
- [ ] **墙钟 > 150ms 则停在此处复核**: 是否改判「先显示文件、计数随后补入」

### Phase 1: 明文第一条垂直切片 (端到端可见)

- [ ] **T3: 接进 OpenJob + 应用状态 + 侧栏组件渲染 (明文)**

### Checkpoint B: 明文端到端

- [ ] 1GB 明文打开 → 侧栏出现, 6 行数字与人工核对一致
- [ ] 索引中不显示脏数 (无「行数已更新、计数还是旧的」窗口)

### Phase 2: JSONL 口径 (第二条垂直切片)

- [ ] **T4: level 类列识别 + 字段计数 + 对抗样本单测**
- [ ] **T5: 点选联动 + 一致性端到端验证**

### Checkpoint C: JSONL 端到端 + 一致性红线

- [ ] `level=INFO` 但正文含 `ERROR` 的对抗样本计入 INFO 桶
- [ ] 点 ERROR 柱条 → 底栏行数 == 柱条数字
- [ ] 无 level 类列的 JSONL → 直方图降级只读 (不撞红线)

### Phase 3: 增量与生存

- [ ] **T6: tail 追加增量计数 + 轮转/重建重算**

### Phase 4: 交互收尾

- [ ] **T7: `Ctrl+L` 显隐 + config 持久化 + 主题适配 + 窄窗口**
- [ ] **T8: 文档收口**

### Checkpoint D: 完成

- [ ] 三件套绿 (fmt + clippy 零警告 + 测试全绿)
- [ ] 人工验收清单逐条过 (spec 成功判据)
- [ ] 交用户 review

## 改动面 (文件级)

| 文件 | 性质 |
|---|---|
| `src/levels.rs` | **新增** —— `Level` 枚举 / `classify_level` / `LevelCounts` / `count_levels` 并行骨架 |
| `src/histogram.rs` | **新增** —— 侧栏组件 (6 行 / 对数横条 / 命中区 / 点击出 Msg) |
| `src/lib.rs` | `pub mod levels` + re-export |
| `src/open.rs` | `OpenOutcome` 增 `level_counts`; worker 内计数; 增量子臂 |
| `src/main.rs` | 状态字段 / `Msg::ToggleHistogram` / `Msg::ApplyLevelFilter` / `Ctrl+L` / 顶层布局改 Row |
| `src/config.rs` | `config.toml` 增侧栏开关字段 |
| `CLAUDE.md` `docs/ROADMAP-v1x.md` `README.md` | 文档收口 |

`src/view.rs` **不在改动面内** (见 D6 的 sibling 决策) —— 若实测发现 LogView 在窄
`area` 下有边界问题, 才回退为内部让位方案, 并记录改判。

## Risks and Mitigations

| 风险 | 影响 | 应对 |
|---|---|---|
| 并行计数墙钟超 150ms | 中 | T2 实测为硬判据; 超线改判「先显示后补计数」(引入侧栏「计算中」态, 需复核) |
| 计数与点选过滤口径不一致 | **高** | D2 红线; T4 对抗样本单测 + T5 端到端一致性命中验收 |
| 索引耗时可被污染 | **高** | D6 隔离在索引趟外; T2 用 logbench 对照 77/88ms 硬判据 |
| 分段起点定位引入额外开销 | 中 | `lines_from` 内部 O(log) 二分; T2 与顺序版对拍时一并测 |
| `LogFile` 非 `Sync` | 低 | 现有 `Arc<LogFile>` 跨线程移动已证 `Send + Sync`; T2 编译期即暴露 |
| 侧栏挤压内容区致横滚变多 | 低 | `Ctrl+L` 可关; 窄窗口最小宽度 T7 定 |
| JSONL 列名五花八门 (无 level 类列) | 中 | 降级只读直方图 (spec Open Question 默认) |

## Open Questions (留实测/复核)

- **计数墙钟超 150ms 是否改判**「先显示文件、计数随后补入」—— T2 实测后裁
- **level 类列名清单**: `level` / `severity` / `lvl` / `loglevel` / `priority` 认哪些? T4 定
- **对数刻度底数**: 2 还是 10; 0 计数行的最小可见条宽
- **窄窗口行为**: 自动折叠, 还是内容区保最小宽度
- **横条 hover 反馈**: 倾向不做, 但可点性的可发现性会打折 —— 人工验收时判
