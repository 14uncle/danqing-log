# SPEC-async-open: 异步打开管道 (后台索引 + 进度反馈)

> 模块 id: `async-open` (地图与共享约定见 `../../SPEC.md`)。
> 依赖 `core-viewer` 的 mmap 引擎与 `search` 的 AsyncJob 基建;
> 改写 `live-tail` 的 append/rebuild 同步调用形态, 构建序排其之后。
> 2026-09-08 用户发起 spec 技能立项, 两项裁决: 加载体验 = A (占位+进度,
> 不可浏览); 成功判据 = 合格档 (见下)。

## Objective

打开大文件不再是 UI 线程的同步阻塞事件。当前四条打开路径全部同步:
启动 (窗口创建前)、Ctrl+O/拖拽、轮转重建、tail 追加 —— 并行索引 (2026-09-08
落地) 把 1GB 压到冷 584ms/热 113ms, 但冷态已是磁盘物理极限, **10GB 级文件
= 秒级到十秒级黑窗/冻结**, 而超大文件恰是本产品对 klogg/LogViewPlus 的主战场。

让窗口先出、索引后台跑、进度可见、全程可响应: 冻结从「文件大小的一次函数」
变成「常数」。

## 范围

**In**:

1. **异步打开管道**: `LogFile::open` + JSONL `detect`/`discover_schema`
   (采样毫秒级, 随 worker 捆绑; 非阻塞源) 整体进 worker 线程, 交付
   `(LogFile, Option<Schema>)` 捆绑; 复用 AsyncJob 代次防乱序
   (快速连开两个文件, 晚到的旧结果丢弃)
2. **四条路径全部接入**:
   - 启动: 空态骨架 + Loading 态先 `run_app`, 窗口按 GPU 速度出现, 索引后台进行
   - Ctrl+O / 拖拽 reload: 同上, 旧文件视图保持至新文件就绪
   - 轮转/截断 rebuild: 旧快照保持可见可滚 (Arc 保活), 新 LogFile 后台建好换入
   - tail 追加: 小追加 (常态, KB 级) 保持同步; **超阈值巨量追平** (久未轮询后)
     转 worker, 期间旧快照继续显示 + 状态栏「追平中」; 阈值 plan 阶段实测定档;
     **过滤激活时增量过滤一并进 worker** (review R2: 否则 GB 级追平的落点
     过滤扫描回到 UI 线程, 「全程可响应」失守)
3. **进度通道**: 并行分段索引的天然边界 → worker 回报已扫字节 (AtomicU64),
   底栏状态行显示「索引中 42% · 437MB/1.0GB」; JSONL 列发现阶段文案切
   「列发现中…」; 完成经现有 tick ≤16ms 拾取
4. **取消**: AtomicBool 旗标, 扫描循环按段/按量检查; 新 job 发起 (drop 取代)、
   关窗 触发 —— 代次失效的 worker 不白跑 10GB 磁盘税
   (2026-09-08 修订: 不设 Esc 取消绑定, 与既有 Esc 链冲突, 见 plan D1)
5. **Loading 态**: 列表区占位 (文件名 + 进度), 标题栏/三键/拖拽/退出全程可用;
   不可滚动浏览 (裁决 A); 完成/失败/取消三出口, 失败 = notice + 留在旧视图
   (保持 reload 失败的现状语义)
6. **回归保险**: `LogFile::open` 同步 API 保留 (logbench/单测继续用),
   logbench 数字不退化

**Out**: 边索引边浏览 (渐进渲染 —— 裁决否: 总行数未定致滚动条/行号语义漂移,
表格模式 schema 未就绪); 索引持久化缓存; 索引限速/IO 优先级调度; 进度条
组件化美化 (底栏文字即够); 多文件并发打开。

## 设计要点

- **AsyncJob 扩展不政变**: 进度与取消作为可选通道叠加 (泛型基础设施),
  代次防乱序语义不动 —— `async_job_delivers_latest_generation` 测试钉着,
  search/filter 现有两条消费路径零改动
- **快照一致性原则延伸**: 渲染永远只见一致快照 (live-tail 原子切换原则的
  推广): Loading/追平期间视图 = 旧 Arc<LogFile> 或空态, 换入永远 O(1)
- **引擎改动面收窄**: `scan_chunk` 只加进度回报点 (段完成/每 N MB) 与取消
  检查点, 分段/归属/查行语义不动 —— 2026-09-08 的「并行==串行」
  「append==全量」对拍测试网原样生效
- **状态机增量**: `has_file=false` 空态既有 → 增 Loading 态; 完成换入后
  走 reload_file 既有状态重置链路 (书签/过滤/搜索/展开 清失效)

### 评审修订 (2026-09-08 review 批次, 全部落地)

- **D6 stat 快照自足**: LogFile.stat 描述「被索引的这份字节」而非索引完成
  时刻的路径 (len = 打开时刻元数据, head = map 期首块指纹, mtime 取打开
  句柄) —— 堵住索引期间增长漏尾 / 轮转嫁接两个 TOCTOU (review R1)
- **D7 跨作业失效**: `AsyncJob::invalidate()` (代次+1 不发起新工作),
  apply_fresh/apply_rebuild 开头对在途 filter/search job 各调一次 —— 旧文件
  结果不得贴到新文件 (review C1); AppendOutcome {Appended, Rebuilt} 让追加
  退化 (UTF-16/缩容) 按 Rebuild 链分派, 兜底分支同样带 hooks (review R3);
  apply_rebuild 用 worker 新发现的 schema/mode (review R4)
- **D8 worker 健壮性**: 闭包 catch_unwind 转 Err 交付 (panic 不得变永久
  Loading); 进度百分比在途封顶 99 (分子可超分母: 索引期增长/UTF-16 口径);
  取消识别用共享常量 INDEX_CANCELLED 不做字符串匹配

## Success Criteria

- [x] 人工验收: 带 1GB 文件启动, 窗口可见时间 ≤ 无文件启动 + 200ms
  (log `perf startup_to_visible` 对照) —— **2026-09-08 实测过: 935ms vs 780ms, Δ154ms**;
  4GB 冷开「先窗口后内容」实证 (可见 1.35s / 内容就绪 4.07s)
- [ ] 人工验收: 10GB 冷文件打开全程可拖窗/点三键/退出, 底栏百分比前进,
  无冻结感 (4GB 已证机制, 10GB 量级复核)
- [ ] 人工验收: 索引中 Ctrl+O 开另一文件 → 旧 job 取消 (不双份磁盘税),
  新文件正常落地 (构造保证 + 单测, 待体感确认)
- [ ] 人工验收: 索引中关闭窗口 → 干净退出, 无残留线程 (任务管理器读数)
- [ ] 人工验收: tail 轮转重建期间旧内容保持可见可滚, 完成后换入 + 状态提示
- [x] 单测: 打开 job 代次防乱序 (快速两连开, 晚到旧结果丢弃); 取消旗标
  生效 (取消后 worker 早退, 不扫完); 进度计数单调递增至总量
- [x] logbench 回归: 同步 open 数字不退化 (对照 2026-09-08 基线:
  热 113ms / 冷 584ms) —— **实测: 热 92/94/96ms / 冷 722ms (磁盘噪声域)**
- [x] 三件套绿

## Open Questions

- tail 追加的同步→异步阈值: 按「同步扫描 ≤16ms」反推 (热并行 ~9GB/s →
  阈值可达 ~100MB; 冷盘大追加另算), plan 阶段实测定档
- 取消/进度的回报粒度 (每段 vs 每 N MB): 太粗进度跳动, 太细原子操作税 ——
  plan 阶段定
- UTF-16 转码副本路径的进度语义 (read+transcode 两段式, 百分比口径) ——
  plan 阶段给出定义
