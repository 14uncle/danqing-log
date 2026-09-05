# Implementation Plan: jsonl-table (真 parser + 嵌套展开 + 过滤增强)

> 模块 spec: `../docs/specs/SPEC-jsonl-table.md`; 地图/共享约定/性能基线: `../SPEC.md`。
> core-viewer 已验收, 本 plan 展开第二棒 jsonl-table —— 主炮, 「klogg 速度 ×
> LogViewPlus 结构化」里「结构化」那半的决胜点。live-tail 可与之并行, 独立出 plan。

## Overview

把前提② demo (memmem 提取 + 扁平字段过滤) 升级成正式版: 显示路径换真 parser
(消除 `,"key":"` 内嵌误判)、行内子行嵌套展开、点路径 + 数值比较过滤。任务 5 个,
三阶段: 引擎地基 (T1–T3) → 展开 UI (T4) → 验收 (T5)。

## Architecture Decisions

1. **真 parser 只 parse 可见行**: 单元格渲染逐可见行 serde_json parse (≈50 行/帧,
   恒定成本), 嵌套值安全显示; memmem 提取退役出**显示路径** (消除 POC「`,"key":"`
   内嵌误判」已知边界)。memmem 保留在**过滤粗筛** (性能路径不变)。
2. **过滤两段架构 (性能契约)**: 扁平等值/前缀 (`level=ERROR` / `status=50*`) 走
   memmem 直通, 零 parse (POC 235ms/1GB 不退化); 点路径 / 数值比较才启用 serde_json
   验证 —— 粗筛 = memmem 找最内层 key needle (`"id":`), 把候选压到千行级再 parse 导航。
   识别「无需验证」的扁平查询是直通前提 (决策内建判据, 见 T2)。
3. **展开行模型 = BTreeMap + 前缀和**: `BTreeMap<文件行号, 子行数>` 存展开态;
   显示行 ↔ (文件行, 子行偏移) 双向映射走前缀和 O(log n); 展开内容惰性 parse
   (只 parse 展开的那一行), flatten 成 (深度, 路径段, 值) 子行序列; 展开 1 万节点
   视口成本恒定 (行锚定虚拟化不变)。
4. **展开交互键**: 鼠标点行首 `▶`/`▼`; 键盘 `→` 展开 / `←` 折叠选中行 (ArrowLeft/Right
   表格模式当前未占用; Enter 归过滤应用, 不冲突)。数组路径段显示为 `[0]`。
5. **过滤 × 展开的显示行模型统一**: 显示行 = 过滤命中文件行 ∪ 各命中行的展开子行;
   双向映射在过滤命中表之上再叠展开前缀和, 保证 `file_line_of` / `display_row_of`
   全路径一致 (spec 成功判据「过滤叠加展开行号映射一致」)。
6. **不重构目录, 平铺**: 新增 `src/expand.rs` (展开行模型, 纯逻辑, 与 `search.rs`
   同构); 过滤/parser/flatten 留在 `jsonl.rs`; GUI 接 `main.rs` + `view.rs`。

## Task List

### Phase 1: 引擎地基

- [ ] **T1: 真 parser 显示路径 + 嵌套值紧凑显示** — `jsonl.rs` 加 `parse_line`
  (可见行 serde_json parse); `view.rs` 单元格渲染改走 parse 结果取顶层 key,
  嵌套值 `to_string` 截断省略; `extract_field` 退出显示路径 (保留供过滤粗筛)
  - 验收: 嵌套 fixture (含 `,"key":"` 内嵌字符串的对抗样本) 单元格显示正确,
    memmem 时代的误判样本全部不再误判; 可见行 parse 成本恒定 (perf 不退化)
  - 验证: `cargo test`; logbench 复跑
  - 文件: `src/jsonl.rs`, `src/view.rs` | M

- [ ] **T2: 点路径 + 数值比较过滤引擎** — `jsonl.rs` 扩展 `Clause` (路径 + 算子),
  `parse_query` 解析 `a.b.c=42` / `status>=500` / `duration_ms>1000` / `level=ERR*`;
  两段: 扁平等值/前缀 → memmem 直通; 点路径/比较 → memmem 粗筛最内层 key + serde_json
  导航验证; `navigate(value, path)` + `num_compare`
  - 验收: 点路径导航单测; 比较算子边界单测 (= > < >= <= 负数 浮点 字符串值不匹配);
    扁平 `level=ERROR` 仍零 parse (直通判据); 点路径过滤命中数 vs 全量 parse 对拍一致
  - 验证: `cargo test`; logbench `--filter "user.id=42*"` 交叉验证
  - 文件: `src/jsonl.rs` | M

- [ ] **T3: 展开行模型 + flatten** — 新 `src/expand.rs`: `ExpandMap` (BTreeMap 文件行
  → 子行数) + 前缀和双向映射 (显示行 ↔ (文件行, 子行偏移)); `jsonl.rs` 加 `flatten`
  (serde_json::Value → (深度, 路径段, 值) 子行序列, 数组段 `[i]`)
  - 验收: 展开/折叠/越界/前缀和 roundtrip 单测; flatten 对象+数组 (数组段 `[i]`) 单测
  - 验证: `cargo test`
  - 文件: `src/expand.rs`(新), `src/jsonl.rs` | M

### Checkpoint: 引擎地基 (T1–T3 后)

- [ ] 三件套绿 (`fmt` + `clippy --all-targets -- -D warnings` + `test`)
- [ ] `level=ERROR` 扁平过滤 perf 不退化 (≤400ms, POC 235ms 基线)
- [ ] 与用户过一眼引擎数字再继续

### Phase 2: 展开 UI

- [ ] **T4: 展开 UI + 统一显示行模型** — `view.rs` 渲染 `▶`/`▼` 行首 + 缩进子行
  (路径段 + 值, 嵌套值紧凑); `main.rs` 持 `ExpandMap` 状态, 点击/`→`/`←` 展开折叠;
  显示行模型统一 (过滤命中表之上叠展开前缀和), 行号槽/选中/滚动全走统一映射
  - 验收: 人工验收 (展开嵌套对象 → 缩进子行 → 滚动流畅 → 折叠恢复); 过滤 + 展开
    叠加行号映射一致 (单测覆盖 展开/折叠/滚动越界/过滤叠加)
  - 验证: `cargo test` + 1GB JSONL 人工验收
  - 文件: `src/main.rs`, `src/view.rs` | M (展开 UI 是集成点, 最易错在映射一致)

### Phase 3: 验收

- [ ] **T5: genlog --nested + logbench 过滤扩展 + 性能门槛 + 人工验收** — genlog
  加 `--nested` 生成 1GB 嵌套 JSONL; logbench `--filter` 支持点路径/比较算子并计时;
  实测性能门槛 + 人工验收清单过单
  - 验收: `user.id=42*` 点路径 1GB ≤ 2s; `status>=500` 1GB ≤ 400ms (与扁平同量级);
    `level=ERROR` 扁平不退化 (≤400ms); 过滤结果与 logbench 交叉验证一致
  - 验证: `cargo run --release --bin logbench -- <1GB嵌套> --filter "user.id=42*"`; 人工
  - 文件: `src/bin/genlog.rs`, `src/bin/logbench.rs` | M

### Checkpoint: 模块验收 (T5 后)

- [ ] 三件套绿
- [ ] spec-jsonl-table 成功判据逐条对照过单
- [ ] 人工验收清单全过 (用户上手)
- [ ] 进 review 阶段 (`/agent-skills:code-review-and-quality`, 全模块)

## Risks and Mitigations

| 风险 | 影响 | 缓解 |
|---|---|---|
| 点路径粗筛候选率失控 (key 太常见) | 高 | 粗筛 needle 用最内层 key 精确匹配; 实测候选率, 失控回退或报错提示 |
| 过滤 × 展开行号映射出错 (最易错点) | 高 | 前缀和 roundtrip + 过滤叠加单测重点覆盖; T4 验收含映射一致 |
| 真 parser 显示路径性能退化 | 中 | 只 parse 可见行 (恒定成本), logbench 复跑 `level=ERROR` 保不退化 |
| 数值比较边界 (负数/浮点/科学计数/字符串值) | 中 | 单测覆盖; 字符串值不参与数值比较 (不匹配) |
| 嵌套展开大对象 flatten 爆栈/慢 | 低 | 惰性 parse 单行; 深度上限防御 (POC 不追求极端嵌套) |

## Open Questions (plan 已答, 备查)

- ~~展开键盘等价键~~ → `→`/`←` + 鼠标 `▶` (决策 4; Enter 归过滤, 不冲突)
- ~~数组根嵌套行显示形态~~ → 数组段 `[0]` 作路径段 (决策 4)
