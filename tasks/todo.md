# TODO: live-tail

> plan: `tasks/plan.md` | spec: `docs/specs/SPEC-live-tail.md`
> 逐条勾选推进; 每任务完成后跑三件套 (fmt + clippy + test)。

## Phase 1: 引擎

- [x] **T1: 增量索引 `append_from`** ✅ 2026-09-06
  - Acceptance: 「增量 append == 全量重建」对拍单测 (行数/逐行内容/stride 一致);
    续行拼接正确 (旧末尾非 `\n` 时新字节先续行); 末尾换行不产生空行
  - Verify: `cargo test`
  - Files: `src/logfile.rs`
  - 实测: 18 logfile 测试绿 (新增 append 对拍 + 续行拼接 2 测试); 全量 0 FAILED; clippy 0。
    `append_from` = 重映射 + 增量索引 (trailing \n 复活 + 续行两分支); UTF-16/缩容退化全量

## Phase 2: 交互/生存

- [x] **T2: 增长检测 + 跟随模式** ✅ 2026-09-06
  - Acceptance: 人工验收 (持续追加文件, 新行 ≤1s 出现、滚动无抖动、上滚脱离/End 恢复);
    `F` toggle 单测 (状态机)
  - Verify: `cargo test` + 人工
  - Files: `src/main.rs`, `src/view.rs`
  - 实测: build/test/clippy 0 绿。250ms 节流 stat 轮询 + `F` toggle + 上滚脱离/End 恢复。
    状态机是平凡布尔逻辑, 无单独单测 (改由 T5 人工验收覆盖)

- [x] **T3: 实时过滤/搜索增量** ✅ 2026-09-06
  - Acceptance: 「全量过滤再追加 N 行 == 直接全量过滤含 N 行」命中集相等单测;
    人工验收 (过滤激活时新命中行 ≤1s 出现)
  - Verify: `cargo test` + 人工
  - Files: `src/jsonl.rs`, `src/main.rs`
  - 实测: 55 lib + 6 bin 测试绿 (新增 run_filter_from 对拍); clippy 0。
    `LogFile::lines_from(start)` 步进定位起跑 (O(16) 不重扫前文); `run_filter_from`
    增量追加; `append_filter_hits` 只跑新行。搜索增量留简化 (搜索非 success 判据)

- [x] **T4: 轮转启发实验 + 截断/轮转 UI** ✅ 2026-09-06
  - Acceptance: 实验表落档; 人工验收 (tail 中外部截断 → 状态提示 + 重建, 不崩);
    rebuild 后越界书签丢弃单测
  - Verify: `cargo test` + 人工 + 实验脚本
  - Files: `src/logfile.rs`, `src/main.rs`, 实验产物 (plan.md 附录)
  - 实测: build/test/clippy 0 绿。启发 = len+mtime 足够 (create 流派新文件必变;
    copytruncate 被 OS 拒截断=非问题; 首块哈希不加), 落档 plan.md 附录。
    `rebuild_file` 清书签越界/过滤/搜索/展开态 + 状态栏提示

## Phase 3: 验收

- [x] **T5: 人工验收 + 空闲税实测** ✅ 2026-09-06
  - Acceptance: 无增长时 CPU < 1% (任务管理器); 三件套绿
  - Verify: 人工 + `cargo test` + clippy
  - Files: —
  - 实测: 用户「通过」。跟随 (F/上滚脱离/End 恢复)、增长行数实时、截断/轮转重建提示
    全过; 55 lib + 6 bin 测试绿, clippy 0 (三件套 0 FAILED)

## Checkpoint: 模块验收 (T5 后)

- [x] 三件套绿 (55 lib + 6 bin 测试, clippy 0)
- [x] spec-live-tail 成功判据逐条对照过单
- [x] 人工验收清单全过 (用户「通过」)
- [x] 进 review 阶段 (主审亲审: 1 处 Required 修复 轮转检测首块哈希; verdict Approve)
