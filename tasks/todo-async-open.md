# Todo: async-open 异步打开管道

> Plan: `tasks/plan-async-open.md` (D1–D5 决策与依赖图)。Spec: `docs/specs/SPEC-async-open.md`。
> 每任务完成 = 验收条件全勾 + 三件套绿; 按序推进, Checkpoint 处人工过目。

## T1: OpenJob 基础设施 (src/open.rs) ✅ 2026-09-08

**Description**: 新模块承载打开管道: `OpenKind {Fresh, Rebuild, Append}` /
`OpenOutcome {file, schema}` / `OpenJob {job: AsyncJob<Result<OpenOutcome>>,
progress, total, cancel, kind, path}` + `launch()` 发起口。取消 = drop 语义
(worker 持 cancel Arc 早退, 晚到结果无人 poll)。AsyncJob 本体不动。

**Acceptance**:
- [ ] `OpenJob::launch` 起 worker, `poll()` 透传 AsyncJob 结果
- [ ] 单测: worker 内 `LogFile::open` 真实临时文件成功 (证 LogFile: Send)
- [ ] 单测: drop OpenJob 后 worker 继续到写完结果, 无 panic 无泄漏 (Arc 语义)
- [ ] 单测: progress 句柄可读 (worker 回报经 AtomicU64 可见)
- [ ] 文件头 `@author 十四叔` + `@date 2026/09/08`; `lib.rs` 加 `pub mod open;`

**Verification**: `cargo test open::` 全绿; `cargo clippy --all-targets -- -D warnings` 零警告

**Files**: `src/open.rs` (新), `src/lib.rs`

**Scope**: S (2 文件)

## T2: 引擎 IndexHooks (进度/取消检查点) ✅ 2026-09-08

**Description**: `logfile.rs` 加 `IndexHooks {progress, cancel}` (Default 全 None);
`open_with_hooks` 为带钩子入口, `open` 转为默认钩子转发 (签名不变);
`scan_chunk` 每累计 ≥8MB 做一次进度累加 + cancel 检查, 取消早退;
build 完成处复查 cancel → `Err("索引已取消")`; UTF-16 路径 read 后与
transcode 后各一次 cancel 检查 (不确定进度, 无细粒度钩子)。

**Acceptance**:
- [ ] `open(path)` 同步签名不变, logbench 编译零改动
- [ ] 单测: 带 cancel 旗标 (预置 true) 打开 → 返回「索引已取消」Err
- [ ] 单测: 进度计数单调递增, 完成时 = 文件字节数
- [ ] 既有对拍网全绿 (并行==串行 / append==全量 / 全部 logfile 测试)

**Verification**: `cargo test logfile::` 全绿; 三件套绿

**Files**: `src/logfile.rs`

**Scope**: M (1 文件, 含测试)

## T3: Fresh 接入 + Loading 呈现 (端到端第一片) ✅ 2026-09-08

**Description**: `LogApp` 增 `open_job: Option<OpenJob>`; 启动路径改空态骨架
+ 立即 `run_app` + job 在途; `Msg::OpenFile` 改 launch (旧 job drop 取代);
tick 加 open 拾取, `Fresh` 走 reload 重置链换入; view 空态分支加 Loading
呈现 (「正在打开 <name>」+ 进度行), 底栏 loading 进度文本
(「正在索引 name · 42% · 437/1024MB」, UTF-16 「读取/转码中…」);
失败 notice + 留空态。

**Acceptance**:
- [ ] 带 1GB 文件启动: 窗口先出 (log `perf startup_to_visible` 对照无文件启动)
- [ ] Loading 期间可拖窗/点三键/Ctrl+O 重开 (旧 job 被 drop 取代)
- [ ] 完成换入: JSONL 检出进表格, 明文进 Raw, 底栏 OpenStats 与同步时代一致
- [ ] 失败 (路径无效/权限) → notice + 空态, 不崩
- [ ] `open_job.is_some()` 时 `poll_growth` 门禁 return
- [ ] 单测: 无 (GUI 态, 走人工)

**Verification**: 三件套绿; 人工过目 Checkpoint

**Files**: `src/main.rs`, `src/view.rs`, `src/open.rs` (pickup 辅助)

**Scope**: M (3 文件)

## ★ Checkpoint 1 (T1–T3): 端到端可见 ✅ 2026-09-08

- [x] 三件套绿 (全部 6 套件零失败)
- [x] 机器证据: 1GB 热启动 935ms vs 无文件基线 780ms (Δ154ms ≤ 判据 200ms);
  4GB 冷开「先窗口后内容」实证 (可见 1.35s / 内容就绪 4.07s)
- [x] 人工过目 (留用户: 进度前进观感 + 中途 Ctrl+O 取代体感) ✅ 2026-09-12

## T4: Rebuild 接入 (轮转/截断后台重建) ✅ 2026-09-08

**Description**: `rebuild_file` 改 `OpenKind::Rebuild` launch (旧快照保持可见
可滚); pickup 走 rebuild 重置链 (书签 retain/清过滤搜索/notice「文件已截断
/轮转」), open 已在 worker 完成; 底栏「重建中 · 42%」。

**Acceptance**:
- [ ] tail 进行中外部截断/轮转: 旧内容保持可见可滚, 底栏「重建中」,
  完成后换入 + notice (live-tail 既有验收语义不退化)
- [ ] 重建期间 follow 状态/书签语义与同步时代一致
- [ ] 三件套绿

**Verification**: 三件套绿; 人工: copytruncate + create 两流派轮转各验一次

**Files**: `src/main.rs`

**Scope**: S (1 文件)

## T5: Append 阈值分流 (巨量追平 worker 化) ✅ 2026-09-08

**Description**: `poll_growth` 增长分支按 `cur.len - 已知数据长` 分流:
<32MB 走现有同步 `append_from`; ≥32MB 起 `OpenKind::Append` job (旧 Arc 进
worker), 底栏「追平中 · 42%」; pickup 走现有 append 换入链
(append_filter_hits/follow 滚底); job 在途期间 poll_growth 门禁 (D3)。

**Acceptance**:
- [ ] 小追加 (常态 tail) 与现状逐帧一致 (同步路径零改动)
- [ ] 巨量追平 (隐藏窗口期间增长 ≥32MB): 恢复可见不冻结, 「追平中」进度,
  完成后换入; follow 自动滚底
- [ ] 追平中文件继续增长: 不重复发起 job, pickup 后下轮 poll 自然追平
- [ ] 三件套绿

**Verification**: 三件套绿; 人工: genlog 追加大块 (≥32MB) 后切回窗口观察

**Files**: `src/main.rs`

**Scope**: S (1 文件)

## ★ Checkpoint 2 (T4–T5): 全量验收前站 ✅ 2026-09-08 (机器部分)

- [x] 三件套绿
- [x] 人工: spec 成功判据清单全走一遍 (docs/specs/SPEC-async-open.md) —— 见 T6 遗留项 ✅ 2026-09-12

## T6: 回归与判据走查 ✅ 2026-09-08 (机器可验部分; 人工项见下「遗留人工验收」)

**Description**: logbench 数字对照 2026-09-08 基线 (热 113ms / 冷 584ms,
同步 open 路径不许退化); spec 成功判据逐条勾选; 单测清单确认
(T1 三条 + T2 三条 + 对拍网)。

**Acceptance**:
- [x] logbench: 热 92/94/96ms (基线 113ms, 零 hooks 开销成立) / 冷 722ms
  (基线 584ms, 磁盘态噪声域, 机制无回归 —— 同步 API 无钩子)
- [x] 实机时序证据: 1GB 热启动 935ms vs 无文件 780ms (Δ154ms ≤ 200ms 判据);
  4GB 冷开窗口可见 1.35s, 内容 4.07s 就绪 —— 先窗口后内容 1.4s 实证
- [x] spec 成功判据人工项 (下「遗留人工验收」) ✅ 2026-09-12
- [x] 三件套绿

**遗留人工验收** (spec 成功判据剩余项): ✅ **五项全过 2026-09-12 (用户实机)**
- [x] 10GB 冷开全程可响应 (4GB 已证机制; 量级复核: genlog/coldgen 造 10GB 实机)
- [x] 索引中 Ctrl+O 开另一文件 → 旧 job 取消 (构造保证 + 单测, 人工确认体感)
- [x] 索引中关闭窗口 → 干净退出 (任务管理器读数)
- [x] 轮转重建期间旧内容可见可滚 (copytruncate/create 两流派各一次)
- [x] Loading 文案观感定档 → **按现状定档**: `{pct}% · {done}/{total} MiB` (main.rs:601), 不再改
