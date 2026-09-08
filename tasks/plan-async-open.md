# Plan: async-open 异步打开管道

> 模块 spec: `docs/specs/SPEC-async-open.md` (2026-09-08 评审门通过)。
> 任务清单: `tasks/todo-async-open.md`。本文 = 架构决策 + 依赖图 + 风险。

## Overview

四条同步打开路径 (启动/Ctrl+O·拖拽/轮转重建/tail 巨量追平) 全部迁入
worker 线程: 新模块 `src/open.rs` 提供 OpenJob (AsyncJob 消费方 + 进度/取消
通道), 引擎 `logfile.rs` 加进度/取消检查点, `main.rs` 增 Loading 态与
pickup 分派, `view.rs` 呈现占位与底栏进度。窗口先出, 冻结变常数。

## Architecture Decisions

### D1. OpenJob = AsyncJob 消费方, 不改 AsyncJob 本体

新模块 `src/open.rs`:

```rust
pub(crate) enum OpenKind { Fresh, Rebuild, Append }  // pickup 分派用
pub(crate) struct OpenOutcome { file: LogFile, schema: Option<Schema> }
pub(crate) struct OpenJob {
    job: AsyncJob<anyhow::Result<OpenOutcome>>,
    progress: Arc<AtomicU64>,   // 已扫字节
    total: u64,                 // 文件字节
    cancel: Arc<AtomicBool>,
    kind: OpenKind,
    path: PathBuf,
}
```

- **取消 = drop**: `LogApp.open_job: Option<OpenJob>`; 取消/被取代 = 置 None
  (worker 持有的 cancel Arc 仍活 → 扫描循环早退; 晚到结果无人 poll, 随
  done Arc 回收)。不新增 Esc 绑定: 取消通道 = Ctrl+O 再开 / 关窗, 天然满足
  spec 成功判据, 不与既有 Esc 链冲突
- AsyncJob 的代次防乱序原样保留 (双保险; 其测试不动)
- 发起口: `OpenJob::launch(kind, path, old: Option<Arc<LogFile>>)` —
  Append 时 old 进 worker 闭包走 `LogFile::append_from`, 其余走
  `LogFile::open_with_hooks`; Fresh/Rebuild 在 worker 内完成
  open → jsonl::detect → discover_schema 全链 (schema 采样毫秒级, 随绑)

### D2. 引擎钩子: 可选、零开销、语义不动

`logfile.rs` 加 `IndexHooks { progress: Option<Arc<AtomicU64>>,
cancel: Option<Arc<AtomicBool>> }` (Default = 全 None):

- `LogFile::open(path)` → `open_with_hooks(path, &IndexHooks::default())`,
  同步 API 签名不变 (logbench/单测零改动)
- `scan_chunk` 加 hooks 参数: 循环内每累计 ≥8MB 做一次 `progress.fetch_add`
  + cancel 检查 (每换行检查 = 12M 次/GB 原子税, 必须节流); 取消 → 早退
- build 完成处复查 cancel, 已取消则 `Err("索引已取消")` —— 半成品索引
  永不进入应用层 (双保险于 drop 语义)
- UTF-16 路径 (read+transcode 两段一次 syscall 级): 不加细粒度钩子,
  read 后与 transcode 后各一次 cancel 检查; 进度显示 = 不确定文案
  「读取/转码中…」(**spec Open Question #3 裁决落档**)
- 分段/归属/查行语义不动, 「并行==串行」「append==全量」对拍网原样为准入

### D3. append 同步/异步阈值 = 32MB

依据: 新追加字节几乎必在页缓存 (写入方刚产生, 写回页), 串行增量索引
~2.4GB/s → 32MB ≈ 13ms ≤ 16ms 帧预算; 超阈值转 `OpenKind::Append` worker,
期间旧快照可见可滚 + 底栏「追平中」。(**spec Open Question #1 裁决落档**)

门禁: `open_job.is_some()` 时 `poll_growth` 直接 return (在途 job 期间
不叠加 tail 动作; pickup 后下轮 250ms poll 自然追平)。

### D4. 状态机与 pickup 分派

`LogApp` 增 `open_job: Option<OpenJob>`; `tick()` 在现有 filter/search
拾取同槽加 open 拾取, 按 kind 分派:

- `Fresh` → 走 reload_file 既有重置链 (mode/schema/top_row/过滤搜索书签全清),
  启动与 Ctrl+O 共用; 启动路径 = 空态骨架 + 立即 run_app + job 在途
- `Rebuild` → 走 rebuild_file 既有重置链 (书签 retain/清过滤搜索/notice
  「文件已截断/轮转」), 但 open 已在 worker 完成, pickup 只换入
- `Append` → 走现有 append 换入链 (append_filter_hits/follow 滚底/refresh_status)

失败 (Err): notice + 留在旧视图 (Fresh 无旧视图 → 空态), 保持 reload
失败现状语义; 「索引已取消」Err 静默 (本来就是主动取消)。

### D5. 视图呈现 (最小加法)

- `has_file=false` 空态分支 → 两分: 无 job = 现有欢迎语; 有 job = 居中
  「正在打开 <name>」+ 进度行 (复用空态居中范式)
- 有旧文件 + job 在途 (reload/rebuild/append): 列表照画 (旧快照可滚),
  进度只上底栏
- 底栏: loading 时 base_status 替换为 「正在索引 name · 42% · 437/1024MB」
  (UTF-16: 「读取/转码中…」; Rebuild: 「重建中 · 42%」; Append: 「追平中
  · 42%」); 随 250ms poll 同槽刷新
- 无进度条组件 (spec Out)

## 依赖图与构建序

```
T1 open.rs 基础设施 (OpenJob/取消drop语义/进度句柄 + 单测)
   │   ∥
   └── T2 logfile.rs IndexHooks (进度/取消检查点 + open_with_hooks + 单测)
       │   (T1∥T2 可并行, 接口仅 Arc<AtomicU64/Bool> 约定)
       └── T3 main.rs+view.rs: Fresh 接入 (启动/Ctrl+O) + Loading 呈现 + pickup
           │   ★ Checkpoint: 端到端可见 (启动秒出+进度+换入), 人工过目
           ├── T4 Rebuild 接入 (轮转/截断后台重建)
           └── T5 Append 阈值分流 (32MB) + 追平中
               │   ★ Checkpoint: 全量人工验收清单
               └── T6 logbench 回归 + 三件套 + spec 成功判据走查
```

## Risks and Mitigations

| 风险 | 影响 | 缓解 |
|---|---|---|
| LogFile 跨线程 Send 不成立 | T1 编译失败 | memmap2 Mmap 本就 Send+Sync; T1 单测 worker 内 open 即证, 失败则 plan 降级为 channel 传字节 |
| 取消竞态: drop 后 worker 写 done | 无 (Arc 语义) | worker 写完无人读, Arc 回收; 单测覆盖「drop 后 poll 无结果」 |
| 引擎钩子扰动索引语义 | 查行错乱 (致命) | 钩子只在循环内加读操作; 2026-09-08 对拍网 (并行==串行/append==全量) 全绿为准入门槛 |
| 在途 job 与 tail 轮询打架 | 状态错乱 | D3 门禁: open_job.is_some() → poll_growth return; 单测覆盖 |
| 10GB 取消后磁盘仍被旧 worker 占几秒 | 体验毛刺 | 可接受 (检查点 ≤8MB 粒度早退); 不做 IO 优先级 (spec Out) |
| 启动空态门禁误伤 Loading 交互 | Ctrl+O 被门 | 现有门禁只门导航/轮询, Ctrl+O 本来就放行 —— 实机验收覆盖 |

## Open Questions (plan 级, 不阻塞)

- 进度文本的精确格式 (MB vs MiB, 百分比小数位) —— 实机验收看感觉定档
- Loading 中窗口标题是否带「(索引中)」— 验收时定, 不加判据

## 评审修订记录 (2026-09-08 review → 修复批次)

review (独立 code-reviewer + 作者自审)  verdict: REQUEST CHANGES
(1 Critical / 5 Required), 全部进本改动集修复:

| 决策 | 内容 | 出处 |
|---|---|---|
| D6 stat 快照自足 | LogFile.stat 描述被索引字节 (len=打开时刻, head=map 期指纹, mtime=打开句柄), 堵增长漏尾/轮转嫁接 TOCTOU | review R1 |
| D7 跨作业失效 | AsyncJob::invalidate() + apply_fresh/apply_rebuild 开头调用; AppendOutcome {Appended, Rebuilt} 分派 + 兜底带 hooks; apply_rebuild 用新 schema | review C1/R3/R4 |
| D8 追平过滤下沉 | launch_append 携带 (子句, 旧行数), worker 顺带 run_filter_from, 落点 merge_filter_hits 只合并 | review R2 |
| D9 worker 健壮性 | run_catched (catch_unwind 转 Err); 进度在途 pct.min(99); INDEX_CANCELLED 共享常量 | review FYI 升档/O1/O2 |

回归钉: search.rs invalidate 单测 ×1, logfile.rs stat 同源/兜底分派 ×2,
open.rs 两连开/过滤下沉/重建臂 ×3, main.rs C1 应用层回归 ×1 (通道闸门控时序)。
Append 在途过滤的覆盖水位线缺口 (同步时代既有) 另起 follow-up, 不进本集。
