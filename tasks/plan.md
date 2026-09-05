# Implementation Plan: core-viewer (引擎硬化 + 完整查看器交互)

> 模块 spec: `../docs/specs/SPEC-core-viewer.md`; 地图/共享约定/性能基线: `../SPEC.md`。
> 本 plan 只展开构建序第一棒 core-viewer; live-tail 与 jsonl-table 待本模块验收后
> 递归出各自 plan (skill 的 per-module 机制)。

## Overview

POC 证明了「快」, 本模块把它硬化成「稳 + 好用」: 编码三件套 (UTF-8/UTF-16/GBK)、
截断/轮转生存原语、索引内存压缩 (48MB → ≤16MB/GB)、PageUp/PageDown (danqing 联动)、
正则搜索 UI、书签、水平滚动。任务 7 个, 两阶段: 引擎地基 (T1–T3) → 交互 (T4–T7)。

## Architecture Decisions

1. **索引压缩 = 步进索引 (stride), 非分段 u32**: 分段 u32 每行仍 4B
   (635 万行 = 24MB) 打不到 ≤16MB/GB; 步进索引每 16 行记一个 u64 绝对偏移
   (≈3.2MB/GB), line(i) = 二分步进表 + memchr 前扫 ≤15 行。随机访问预估仍
   亚微秒 (2.7KB 扫描), logbench 对表验证, 超 1µs/行 退 stride=8。
2. **截断风险实测先行 (T3 第一件事)**: POC 边界「mmap 期间外部截断会崩」是
   Linux 知识假设。Windows 上 Rust `File` 默认无 `FILE_SHARE_DELETE`, 且对已映射
   文件缩容/删除/改名大概率被 OS 拒绝 —— 若实测确认, 残余竞态章节改写, 防御
   级别相应降级 (越界 None + stat 快照 + rebuild API 照做, live-tail 需要)。
3. **不做目录重构**: 5 文件小仓, `engine/`/`app/` 分层是 ceremony (spec 标了
   「非承诺」)。新增模块平铺: `src/encoding.rs`、`src/search.rs`。
4. **编码路径分治**: UTF-16 一次性转码 UTF-8 内存副本 (2 字节编码不适合字节索引);
   UTF-8/GBK 原字节索引 + 行级惰性解码; 搜索查询按文件编码转字节再搜。
   GBK 检测零依赖: 采样 64KB, UTF-8 合法性 → GBK 双字节占比阈值 → 降级 Latin-1。
5. **搜索栏与过滤栏同槽位**: `/` 或 Ctrl+F 任意模式开搜索栏, 临时占过滤栏槽位,
   Esc 关闭退回; 双栏并存太挤。搜索 Enter 触发后台全文件搜索 (不实时, 1GB 正则
   实时太贵), worker 架构复用 POC 过滤的 rev+Mutex+tick 拾取 (OnDemand 16ms
   心跳已证, 无需 boost_frames)。
6. **行内高亮不重存**: 命中行区间逐可见行现算 regex (恒定成本), 不存全量区间表。
7. **PageUp/PageDown 修进 danqing** (打磨寄生): NamedKey 加两枚举值 + VK_PRIOR/
   VK_NEXT 映射; 本机 `[patch]` 消费无需 push; 两仓 commit 均待用户指令。
8. **水平滚动偏移用 f32**: 单行像素域 (最坏 64 万字符 × ~7px ≈ 450 万像素 < 2^24
   安全域), 只有行号 y 轴需要 f64 行锚定; 行号槽钉住不随水平滚动。

## Task List

### Phase 1: 引擎地基

- [ ] **T1: 步进索引内存压缩** — LogFile 行索引稠密 Vec\<u64\> → 步进表;
  line()/line_count()/search() 语义不变; logbench 打印索引驻留字节数
  - 验收: 1GB 索引驻留 ≤16MB; 索引 ≤600ms; 随机访问 ≤1µs/行 (stride 16, 超则 8);
    既有 logfile 测试全绿; 新增对拍测试 (小文件逐行内容比对)
  - 验证: `cargo test`; `cargo run --release --bin logbench -- <1GB文件>`
  - 文件: `src/logfile.rs`, `src/bin/logbench.rs` | M

- [ ] **T2: 编码检测 + 解码/转码路径** — `encoding.rs`: BOM → UTF-8 合法性 →
  GBK 双字节统计 → Latin-1 降级; UTF-16 打开时转码副本; GBK 行级解码 +
  搜索查询转码; genlog 加 `--encoding` 生成小 fixture 入库
  - 验收: fixtures (UTF-8 无/BOM, UTF-16LE/BE BOM, GBK 中文) 行数一致且解码内容
    断言正确; 随机二进制不崩 (降级显示); GBK 中文查询搜索命中; UTF-16 副本
    ASCII 关键字搜索命中; 1GB UTF-8 索引性能不退化
  - 验证: `cargo test`; logbench 复跑
  - 文件: `src/encoding.rs`(新), `src/logfile.rs`, `src/bin/genlog.rs`,
    `tests/fixtures/`(新) | L (边界, 不再拆: 检测+解码+转码是一个语义整体)

- [ ] **T3: 截断/轮转生存原语** — 先实测 (小实验: mmap 存活期外部截断/删除/
  改名/覆写各试一次, 结果记录进本文件附录); 然后: line(i) 越界 → None;
  FileStat 快照 (len+mtime); `rebuild()` 全量重建 API
  - 验收: 实测行为表落档; 越界访问单测不崩; rebuild 后行数/内容反映新文件;
    stat 快照单测
  - 验证: `cargo test`; 实验脚本人工跑
  - 文件: `src/logfile.rs`, 实验产物 (本文件附录) | M

### Checkpoint: 引擎地基 (T1–T3 后)

- [ ] 三件套绿 (`fmt` + `clippy --all-targets -- -D warnings` + `test`)
- [ ] logbench 全基线达标 (mmap/索引/搜索/随机访问/索引驻留, 数字抄进意图文档)
- [ ] 与用户过一眼引擎数字再继续

### Phase 2: 交互

- [ ] **T4: danqing PageUp/PageDown 联动** — NamedKey += PageUp/PageDown +
  VK_PRIOR/VK_NEXT 转换映射 + danqing 事件测试; danqing-log 接入
  (PageUp/Down = ±PAGE_ROWS, Space 保留)
  - 验收: danqing `cargo test --lib --tests` 绿; danqing-log PageUp/Down 人工生效
  - 验证: 两仓各自三件套; 人工按一遍
  - 文件: `../danqing/src/event.rs`, `../danqing/src/window/event.rs`(映射),
    `src/main.rs` | S (跨两仓)

- [ ] **T5: 正则搜索 UI** — `/`/Ctrl+F 开栏 (与过滤栏同槽位); worker 全文件搜索
  (命中行表); Enter/Shift+Enter 跳下/上命中 (命中行置视口中部); 可见行命中
  区间高亮; 底栏 `搜索 "..." → 第 k/n 命中 (x ms)`; Esc 关栏
  - 验收: 人工验收 (1GB 文件 `/ERROR` 逐命中跳转 + 高亮 + 计数正确);
    搜索状态机单测 (开栏/输入/应用/关闭/翻页边界)
  - 验证: `cargo test` + 人工
  - 文件: `src/search.rs`(新), `src/main.rs`, `src/view.rs` | M

- [ ] **T6: 书签** — BTreeSet\<u64\> 文件行号; `b` 切换选中行; `'` 循环跳下一
  书签 (跳后选中跟随); 行号槽金色圆点标记; 过滤模式下按文件行号判定
  - 验收: 人工验收 + 集合操作/循环跳转单测
  - 验证: `cargo test` + 人工
  - 文件: `src/main.rs`, `src/view.rs` | S

- [ ] **T7: 水平滚动** — x_offset f32; Shift+滚轮 ±3 字符宽; 底部细滚动条
  (6px, 状态栏上方); 行号槽钉住; 原始/表格模式同法 (表格模式滚动整个列区)
  - 验收: 人工验收 (超长行文件 Shift+滚轮 + 滚动条拖拽区行为); 钳制单测
  - 验证: `cargo test` + 人工
  - 文件: `src/view.rs`, `src/main.rs` | M

### Checkpoint: 模块验收 (T4–T7 后)

- [ ] 三件套绿 + 两仓联动改动分别待提交 (注明关联, 等用户指令)
- [ ] spec-core-viewer 成功判据逐条对照过单
- [ ] 人工验收清单全过 (用户上手)
- [ ] 进 review 阶段 (`/agent-skills:code-review-and-quality`, 全模块)

## Risks and Mitigations

| 风险 | 影响 | 缓解 |
|---|---|---|
| 步进索引退化随机访问 | 中 | logbench 门槛 1µs/行卡死; 超则 stride 16→8 复测 |
| GBK 启发式误判 (二进制/其他单字节编码) | 中 | 阈值经真实样本实验定 (T2 内); 兜底 Latin-1 降级显示不崩 |
| UTF-16 巨文件转码内存翻倍 | 低 | 1GB→500MB 可接受; >2GB 文档化为已知边界 |
| Windows 截断实测推翻假设 | 中 | T3 实验先行, 按实测表调防御级别; 两方向都已有预案 |
| danqing 联动改动污染全家 | 低 | 最小新增 (两枚举值+映射), 不动既有行为; 三件套双仓各跑 |

## Open Questions (plan 已答, 备查)

- ~~GBK 启发式阈值~~ → T2 内实验定 (采样 64KB, 双字节对占比阈值 + Latin-1 兜底)
- ~~搜索栏与过滤栏并存形态~~ → 同槽位 (决策 5)
- ~~索引压缩方案选型~~ → 步进索引 (决策 1)

## 附录: T3 Windows mmap 存活期外部修改实测 (2026-09-05, `mmap_lab` 五场景)

| 场景 | 修改结果 | 修改后读映射 | 结论 |
|---|---|---|---|
| truncate (截断+重写, copytruncate 流派) | **OS 拒绝** (ERROR_USER_MAPPED_FILE 1224) | 原内容完好 | POC 最怕的崩溃通道被 OS 封死, 不崩 |
| rename (create 流派轮转第一步) | 成功 | 原内容完好 (视图跟句柄走) | 安全, 但路径已漂移 → 过期检测 |
| delete | 成功 (延迟删除) | 原内容完好 | 安全, 同上 |
| append (tail 常态) | 成功 | 原内容完好 | 安全; 增长走增量索引 (live-tail) |
| overwrite (同长原位覆写) | 成功 | **新内容立即可见**, 行结构可能已变 | 视图与文件 coherent; 过期检测+重建 |

**对 POC 边界的修正**: 「mmap 期间外部截断会崩」是 Linux (SIGBUS) 假设, Windows 不成立。
真正的风险 = 路径漂移/内容被换而视图不知 → 防御 = FileStat 快照 (len+mtime) +
is_stale 轮询 + rebuild 整体换入 (读旧建新, 视图永远只见一致快照)。
残余风险: 无已知崩溃通道; 轮转工具的 copytruncate 在被本工具打开的文件上会
收到 OS 拒绝 (对方报错, 我方无损) —— 文档化为已知交互行为。
