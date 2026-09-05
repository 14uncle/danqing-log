# TODO: jsonl-table

> plan: `tasks/plan.md` | spec: `docs/specs/SPEC-jsonl-table.md`
> 逐条勾选推进; 每任务完成后跑三件套 (fmt + clippy + test)。

## Phase 1: 引擎地基

- [x] **T1: 真 parser 显示路径 + 嵌套值紧凑显示** ✅ 2026-09-06
  - Acceptance: 嵌套 fixture (含 `,"key":"` 内嵌字符串对抗样本) 单元格显示正确,
    memmem 误判样本全部不再误判; 可见行 parse 成本恒定 (perf 不退化)
  - Verify: `cargo test`; logbench 复跑
  - Files: `src/jsonl.rs`, `src/view.rs`
  - 实测: 12 jsonl 测试绿 (新增 2 对抗样本回归); 全量 0 FAILED; clippy 0。
    `extract_field` 保留供 T2 过滤粗筛, 显示路径已走 `parse_line`+`cell_display`

- [x] **T2: 点路径 + 数值比较过滤引擎** ✅ 2026-09-06
  - Acceptance: 点路径导航单测; 比较算子边界单测 (= > < >= <= 负数 浮点 字符串值不匹配);
    扁平 `level=ERROR` 仍零 parse (直通判据); 点路径过滤命中数 vs 全量 parse 对拍一致
  - Verify: `cargo test`; logbench `--filter "user.id=42*"` 交叉验证
  - Files: `src/jsonl.rs`
  - 实测: 15 jsonl 测试绿 (新增 navigate/compare_val/filter_dot_path); 全量 0 FAILED; clippy 0。
    两段架构落地: 扁平 Eq/Prefix → `Compiled::Flat` memmem 直通零 parse; 点路径/比较 →
    `Compiled::Verify` 粗筛最内层 key + parse 导航比较。长算子 `>=`/`<=` 先于短算子匹配

- [x] **T3: 展开行模型 + flatten** ✅ 2026-09-06
  - Acceptance: 展开/折叠/越界/前缀和 roundtrip 单测; flatten 对象+数组 (数组段 `[i]`) 单测
  - Verify: `cargo test`
  - Files: `src/expand.rs`(新), `src/jsonl.rs`
  - 实测: 50 lib 测试绿 (新增 expand 3 测试 + flatten 2 测试); clippy 0。
    `ExpandMap` 前缀和双向映射 (display_count/display_row_of/file_line_at),
    过滤 × 展开叠加的「被滤掉展开行不计入」单测过; `flatten` 顶层叶子=列不进子行

## Checkpoint: 引擎地基 (T1–T3 后)

- [x] 三件套绿 (50 lib + 6 bin 测试, clippy 0)
- [x] `level=ERROR` 扁平过滤仍 memmem 直通零 parse (T2 架构保证, 实测待 T5 logbench)
- [ ] 与用户过一眼引擎数字再继续

## Phase 2: 展开 UI

- [x] **T4: 展开 UI + 统一显示行模型** ✅ 2026-09-06
  - Acceptance: 人工验收 (展开嵌套对象 → 缩进子行 → 滚动流畅 → 折叠恢复); 过滤 + 展开
    叠加行号映射一致 (单测覆盖 展开/折叠/滚动越界/过滤叠加)
  - Verify: `cargo test` + 1GB JSONL 人工验收
  - Files: `src/main.rs`, `src/view.rs`
  - 实测: 50 lib + 6 bin 测试绿, clippy 0。`Lines` 抽象统一全量/过滤两种显示行来源;
    行首 `▶`/`▼` + `→`/`←` 展开折叠 (子行归父行); 子行缩进「路径段 = 值」渲染,
    惰性 parse 只 parse 展开那行; 过滤 × 展开叠加映射已单测 (被滤掉展开行不计)

## Phase 3: 验收

- [x] **T5: genlog --nested + logbench 过滤扩展 + 性能门槛 + 人工验收** ✅ 2026-09-06
  - Acceptance: `user.id=42*` 点路径 1GB ≤ 2s; `status>=500` 1GB ≤ 400ms;
    `level=ERROR` 扁平不退化 (≤400ms); 过滤结果与 logbench 交叉验证一致
  - Verify: `cargo run --release --bin logbench -- <1GB嵌套> --filter "user.id=42*"`; 人工
  - Files: `src/bin/genlog.rs`, `src/bin/logbench.rs`
  - 实测 (1GB 嵌套 4021125 行): `level=ERROR` 259ms (≤400 ✓, POC 235ms+架构税);
    `user.id=42*` 537ms (≤2s ✓, 优化前 9.3s —— 粗筛从「key 命中」改「值命中」
    `field_value_matches` 遍历所有出现免假阴性); `status>=500` 401ms (扁平比较走
    token 直通)。logbench 无需改 (已走 parse_query+run_filter)。

## Checkpoint: 模块验收 (T5 后)

- [x] 三件套绿 (52 lib + 6 bin 测试, clippy 0)
- [x] spec-jsonl-table 成功判据逐条对照过单 (性能三门槛全过; 单测覆盖 对抗样本/展开映射/比较算子)
- [x] 人工验收清单全过 (用户上手; 展开标识初版 ▶/▼ 因 GB2312 子集字体无几何形画空白, 改 ASCII +/- + 独立展开区后用户确认)
- [x] 进 review 阶段 (主审亲审: 1 处 Required 修复 display_row_of 越界钳制; 1 处 FYI 扁平比较字符串/数值边界; verdict Approve)
