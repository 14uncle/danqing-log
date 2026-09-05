# Implementation Plan: live-tail (tail 跟随 + 实时过滤)

> 模块 spec: `../docs/specs/SPEC-live-tail.md`; 地图/共享约定/性能基线: `../SPEC.md`。
> core-viewer 与 jsonl-table 已验收, 本 plan 展开最后一棒 live-tail。依赖 core-viewer
> T3 的截断/轮转原语 (`FileStat`/`is_stale`/`rebuild`) 与步进索引。

## Overview

`tail -f` 的桌面 GUI 形态: 日志写进来视图跟上去。核心 = 可增长 LogFile (mmap 不随文件
增长, 需重映射 + 增量索引) + 跟随模式 (自动滚底, 上滚脱离) + 轮转生存 (重建换入)。
任务 5 个, 两阶段: 引擎 (T1) → 交互/生存 (T2–T4) → 验收 (T5)。

## Architecture Decisions

1. **可增长 LogFile = 创建新实例 + Arc 快照隔离**: `LogFile::append_from(old, path)`
   重新 mmap (O(1), 页缓存共享) + 只对新字节区间增量索引, 返回**新** LogFile;
   `LogApp.file` 保持 `Arc<LogFile>`, 增长时 `self.file = Arc::new(new)`。worker 线程
   持旧 Arc 快照读, 旧 mmap 由其保活 —— 无 Mutex、无悬垂指针、无数据竞争。代价 =
   每次 append 克隆 stride 表 (~3MB/GB), 250ms 节流下 4 次/秒 = 12MB/s, 可忽略。
2. **增量索引 = 续行拼接 + 步进边界**: 只扫 `[old_len, new_len)`; 旧数据不以 `\n`
   结尾时新字节先续旧行 (不新增行); 每 16 行补一条 stride 表项 (行起始偏移)。
   正确性靠「增量 append == 全量重建」对拍单测钉死。
3. **增长检测 = 250ms 节流 stat 轮询**: danqing tick ~60fps, 但 stat 每 250ms 一次
   (防 60 次/秒系统调用); 隐藏态 (OnDemand Wait) 不轮询, 重新可见时一次性追平。
4. **跟随 = F 键 toggle + 滚动语义**: 跟随态新行到达自动滚到底; 用户向上滚 (top_row
   偏离底部) 即脱离跟随 (不打扰阅读); End 跳底自动恢复跟随。状态栏显 FOLLOW。
5. **轮转检测启发 = len+mtime 先行**: spec Open Question。len 缩小 = 截断/copytruncate;
   len+mtime 变 = create 流派轮转。首块哈希是否加 → T4 实验定 (真实两种流派各测一次)。
6. **增量过滤 = 从旧行数起跑**: `run_filter` 加 `start_line` 参数, 只对新行跑谓词追加
   命中; 搜索命中表同理。语义一致性靠「先全量过滤再追加 N 行 == 直接全量过滤含 N 行」
   单测钉死 (spec 明确要求)。

## Task List

### Phase 1: 引擎

- [ ] **T1: 增量索引 `append_from`** — `LogFile::append_from(old, path) -> Result<LogFile>`:
  重映射全文 + 只对新字节区间增量索引 (续行拼接 + 步进边界) + 更新 FileStat 快照。
  - 验收: 「增量 append == 全量重建」对拍单测 (行数一致、逐行内容一致、stride 一致);
    续行拼接正确 (旧末尾非 `\n` 时新字节先续行); 末尾换行不产生空行
  - 验证: `cargo test`
  - 文件: `src/logfile.rs` | M

### Phase 2: 交互/生存

- [ ] **T2: 增长检测 + 跟随模式** — `tick` 内 250ms 节流 stat 轮询; 检测到增长调
  `append_from` 换入; `F` 键 toggle 跟随 (底栏 FOLLOW 标记); 新行到达跟随态滚底;
  向上滚脱离、End 恢复。
  - 验收: 人工验收 (持续追加文件, 新行 ≤1s 出现、滚动无抖动、上滚脱离/End 恢复);
    `F` toggle 单测 (状态机)
  - 验证: `cargo test` + 人工
  - 文件: `src/main.rs`, `src/view.rs` | M

- [ ] **T3: 实时过滤/搜索增量** — `run_filter` 加 `start_line` 起跑; 过滤激活时增量行
  只跑谓词追加命中表; 搜索命中表同理; 底栏计数实时更新。
  - 验收: 「全量过滤再追加 N 行 == 直接全量过滤含 N 行」命中集相等单测;
    人工验收 (过滤激活时新命中行 ≤1s 出现)
  - 验证: `cargo test` + 人工
  - 文件: `src/jsonl.rs`, `src/main.rs` | M

- [ ] **T4: 轮转启发实验 + 截断/轮转 UI** — 实验 copytruncate vs create 两种流派
  (记录进本文件附录), 定启发 (len+mtime 是否够, 首块哈希是否加); 检测到 len 缩小/轮转
  → 全量 `rebuild` + 状态栏「文件已截断/轮转」; 书签越界丢弃。
  - 验收: 实验表落档; 人工验收 (tail 中外部截断 → 状态提示 + 重建, 不崩);
    rebuild 后越界书签丢弃单测
  - 验证: `cargo test` + 人工 + 实验脚本
  - 文件: `src/logfile.rs`, `src/main.rs`, 实验产物 (本文件附录) | M

### Phase 3: 验收

- [ ] **T5: 人工验收 + 空闲税实测** — 1GB 持续追加 tail; 过滤跟随; 外部截断; 空闲 CPU。
  - 验收: 无增长时 CPU < 1% (任务管理器); 三件套绿
  - 验证: 人工 + `cargo test` + clippy
  - 文件: — | S

### Checkpoint: 模块验收 (T5 后)

- [ ] 三件套绿
- [ ] spec-live-tail 成功判据逐条对照过单
- [ ] 人工验收清单全过 (用户上手)
- [ ] 进 review 阶段 (`/agent-skills:code-review-and-quality`, 全模块)

## Risks and Mitigations

| 风险 | 影响 | 缓解 |
|---|---|---|
| 增量索引与全量不一致 (续行/步进边界) | 高 | 「append == 重建」对拍单测钉死 |
| 轮转启发误判 (mtime 分辨率/同秒轮转) | 中 | T4 两种流派实测定; len 缩小必判截断 |
| Arc 快照隔离下旧 Arc 延迟释放 (worker 存活期) | 低 | worker 短命 (秒级), 旧 mmap 及时回收 |
| 增长过频 (每 250ms 都 append) | 低 | 3MB 克隆 × 4/s 可忽略; 无增长时零 append |
| 跟随滚动抖动 (append 触发视图跳动) | 中 | 跟随态才滚底; 用户上滚即脱离, 不抢滚动 |

## Open Questions (plan 已答, 备查)

- ~~轮转检测启发强度~~ → len 缩小必判截断; len+mtime 变判轮转; 首块哈希 T4 实验后定

## 附录: T4 轮转检测实测 (2026-09-06)

**结论: len+mtime 足够, 首块哈希不加。**

| 流派 | 对「被本工具打开的文件」的实际行为 | 检测 |
|---|---|---|
| create (rename + 新建) | rename 成功 (视图跟句柄走), 新文件 len/mtime 必变 | `is_stale` (len+mtime) 检出 → 全量重建 |
| copytruncate (截断+重写) | **Windows OS 拒绝截断被映射文件** (ERROR_USER_MAPPED_FILE, T3 实测) | 非问题 —— 截断根本发生不了, 轮转工具收到 OS 报错 |

- 依据: core-viewer T3 `mmap_lab` 五场景实测 (truncate 被拒, rename/delete/append/overwrite 可);
  `is_stale` (FileStat len+mtime 快照比对) 已单测覆盖 append/delete/create-轮转。
- 首块哈希 = 过度设计: 唯一漏网场景是「轮转后新文件 len 与 mtime 都恰好等于旧快照」,
  概率可忽略 (mtime 秒级已够; 且 create 流派新文件几乎必然更小)。
- 落地: `poll_growth` 用 `FileStat::of` 比对快照; len 增 → append, 其余 → `rebuild_file`
  (书签越界丢弃 + 过滤/搜索/展开态清零 + 状态栏「文件已截断/轮转」)。
