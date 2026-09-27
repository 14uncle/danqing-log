# plan: v1.x 腿一 merge-timeline（多文件时间戳合并）

- @author 十四叔
- @date 2026/09/27
- 状态: **待过目** —— spec `docs/specs/SPEC-v1x-merge-timeline.md` 已批准（2026-09-27「go」，D1–D8 口径 + D9–D11 实现裁定）
- 任务清单: `todo-merge-timeline.md`（本文件的勾选镜像）; 另案同窗口: `todo-gate-trio.md`（三连接门，D8）

## 一、事实盘点结论（plan 前置，全为代码实证）

**引擎（danqing-logfile，基线 68 测试）**:
- `LogFile`: `line(i)` 随机访问（步进索引）+ `lines_from(start)` 顺序迭代器（logfile.rs:434）+
  `append_from` 增量索引（logfile.rs:538）+ `bytes()` + `FileStat`（len/mtime/head 哈希，轮转判别）。
- `scan.rs::scan_field`（腿二收口产物）: 顶层字段单遍状态机扫描，零 Value 树，带 serde 差分对拍 ——
  **JSONL 时间戳字段取值直接复用**，不新开提取岔路。
- `jsonl.rs`: `detect()` JSONL 判定（64 行采样+字节预算）/ `discover_schema()`（512 行+4MiB 预算，
  MAX_COLUMNS 16）—— ts 字段发现的采样纪律照此。
- **无 chrono 无时间库**（Cargo.toml 实证）—— 时间戳手写解析，零新依赖。

**产品（danqing-log，基线 413 测试）**:
- `LogApp`（main.rs:163）= **单文件核心**: `file: Arc<LogFile>` / `mode: ViewMode{Raw|Table}`（**显示
  模式**，非工作区模式）/ `schema: Option<Arc<Schema>>` / `filtered: Option<Arc<Vec<u64>>>` /
  后台作业族 `AsyncJob<T>`（levels/filter/export/open 四先例）/ 弹层族（settings/升级/导出/列管理/
  字段查询/会话，六先例，互斥+Esc 插层规矩在案）。
- 门控先例（main.rs:1149）: `entitlement.allows(Feature::X)` → 否则 `ShowUpgradePrompt(Feature::X)`，
  **门控点 = 入口**，两道闸数据不丢；`Feature` 三枚举 + `label()`（license.rs:186）。
- 会话载荷（columns.rs:259 `SessionEntry`: path/name/filter/search/config/expands/updated）
  + `is_recognizable`（columns.rs:376）+ `put_sessions_for_path` 切片替换。
- 打开管道（open.rs）: `OpenJob::launch(kind, path)` / `launch_append`（live-tail 增量）/ 分相进度。
- 显示行模型（expand.rs）: `Lines = All{total} | Filtered(&[u64])` 二态 + `ExpandMap`（BTreeMap
  行号→子行数）—— **合并视图照此形状复制到「合并行空间」**，不改造原二态（单文件回归靠 413 基线保）。

**架构裁决（plan 级，随本文件呈批）**: 合并 = **工作区级双模式**。App 加 `Workspace::Single | Merge`
并列状态 bundle（Merge 持 `sources: Vec<MergeSource>` + 合并索引 + per-source 过滤态），
`ViewMode::Raw|Table` 在合并视图内继续有效（并集列 = Table 的合并形态）。**Single 模式现有字段
零触碰**——单文件全部行为靠 413 基线 + 家族病史对照清单保回归。

## 二、依赖图与切刀

```
T0 原型拆雷 (genlog 多源 + timestamp/merge 雏形 + logbench --merge)
 └─ CP0 检查点: 实测校准 D5/D10 → 回填 spec 验收线 → 用户确认数字
T1 timestamp.rs 完整 ─┐
T2 merge.rs 完整     ─┴─ CP1 联动: logfile push → danqing-log 关 patch 复钉 → 两仓提交
T3 merge_view 视图模型 + 双模式 + 三栏绘制
 └─ CP2 检查点: D11 源色板提请用户批准 (框架 token or 退路单色 chip)
T4 源管理弹层 + 入口 + 门控 + 并集列
T5 时钟偏移/时区 ┐
T6 req_id 追踪   ┴─ (可并行, 同依赖 T4)
T7 live-tail 合流 (依赖 T2 增量 + T3)
T8 sessions 载荷 merge group (依赖 T4; D4 授权退路在案)
T9 实测四组数字 + PERFORMANCE_REPORT + spec 实现记 + 人工验收记账
```

切刀原则: 每条都是竖切（引擎条带测试锁 / 产品条带 UI 可见行为），不横切「先全引擎后全产品」
之外的层 —— 引擎两段必须先闭（CP1 复钉是产品的编译前置）。

## 三、任务分解（验收标准随条）

### Phase 0 — 原型拆雷（danqing-logfile + 两个 bin，不动产品）

- **T0a genlog 多源 fixture**: `genlog --merge <目录> <每源 MiB> <源数>` —— 交错时间戳 +
  每源可控时钟偏移 / 乱序率 / 无 ts 行率 / 格式轮换（ISO|log4j|epoch）/ req_id 跨源相关 /
  JSONL 与 .log 混合；xorshift 确定性（同参数同文件）; 打印**真值表**（每源行数/时间范围/
  已知交叉事件序）——人工验收 (d)「构造已知序」的唯一靶子。
  验收: 两跑同哈希; 真值表与文件内容对拍测试（genlog 族 8 测试基线不破）。
- **T0b 雏形**: `timestamp.rs`（ISO-8601 一种 + log4j 一种 + epoch s/ms，手写）+
  `merge.rs`（k-way 归并，单调假设，物化 `Vec<(u32 源, u32 行)>`）+
  `logbench --merge <文件...>`（报告: 每源打开/归并耗时/总行数/索引驻留字节）。
- **T0c 实测校准**: 3×1GB 混合源 + 8×200MB 两组; 记录 打开墙钟/归并耗时/索引字节/内存增量。
  → **回填 spec §7 验收线 (i) 的实测数字 + D10 红线裁定**（物化 vs 分块）。
  → **CP0: 用户确认数字后 Phase 1 开工**（数字不合预期 = 回 spec 谈分块，不硬推）。
  预测（实测前不写进 spec）: 归并 ≈ 各源行索引之和 + 线性扫描，3×1GB 打开 300ms 量级,
  索引 1450 万行 ×8B ≈ 116MB —— 若逼 16B/行红线则时间戳入行表问题回 spec 裁。

### Phase 1 — 引擎完整（danqing-logfile）

- **T1 timestamp.rs 完整**: 格式矩阵（ISO 家族: T/空格分隔 × Z/±hh:mm/无 tz; log4j 逗号与点毫秒;
  epoch s/ms 判定）+ JSONL 字段发现（`ts/timestamp/time/@timestamp` 优先序 + `scan_field` 值形态）
  + 采样探测（512 行/4MiB 预算对齐 `SCHEMA_SAMPLE` 纪律）+ 每源 `TsFormat` 结论与**探测失败原因
  类型**（明示文案原料）+ 无 tz 按源时区参数解释 + 偏移在解析边界单源施加的 API 形态
  （`extract(line, fmt, tz, offset) -> Option<i64ms>`）。
  验收: spec §5 引擎测试名单 `ts_detect_*`/`ts_jsonl_field_discovery` 全绿, 失败拒绝锁,
  **先红后绿 A/B 留痕**; 68 基线不破。
- **T2 merge.rs 完整**: 乱序窗口（时间窗 or 行数窗 —— **T0 数据裁定**, spec Q5）+ 无 ts 继承
  （含文件首行边界钉死）+ tie-break（同源行序/异源源序号）+ 增量 `append`（live-tail 新行合流,
  尾追加快路 + 乱序走窗口）+ 源掩码重归并（显隐/移除源的重建 API）+ 索引内存报告。
  验收: `merge_monotonic`/`merge_out_of_order_within|beyond`/`merge_missing_ts_inherits`/
  `merge_equal_ts_stable`/`merge_incremental_append` 全绿 A/B 留痕; 内存 ≤ 红线断言。
- **CP1 联动**: logfile 三件套 → commit+push → danqing-log **关 patch** `cargo check` 重解复钉
  （手法 [[danqing-dep-lock-minimal-reresolve]]，禁 cargo update -p 宽边）→ 两仓分别提交,
  message 注明关联。**patch 开着时 lock 不提交**。

### Phase 2 — 产品（danqing-log）

- **T3 merge_view 视图模型**: 新 `src/merge_view.rs` —— `MergeSource { file: Arc<LogFile>, schema,
  ts: TsFormat, offset_ms, tz, hidden, color }` + 合并行空间 Lines 同款二态（`MergeLines::All|
  Filtered(Vec<u32>)`）+ 展开键 (src,line) + `Workspace::Single|Merge` 双模式 + 归并 AsyncJob
  （后台建索引带进度, open.rs 五相先例）+ view.rs 三栏绘制（时间|源|消息）+ 滚动/选中/书签
  在合并空间复刻。**Single 模式字段零触碰**。
  验收: `merge_view_lines_contract`（len/get 双向映射与展开同套断言）+ 注入 fixture 源组的
  静态合并视图锁（先红后绿）; 413 基线不破。
- **CP2 D11 提请**: 向用户呈色板方案（框架 `Theme` 加 8 源色 token 族 = 一次小联动;
  退路 = 单 accent chip + 行首源名标签）。**批准方向才进 T4 着色**; 退路不影响 T4 其余部分。
- **T4 源管理 + 入口 + 门控 + 并集列**: 源管理弹层（弹层族第七员: 加源 Ctrl+O 式异步开 /
  减源 / 格式结论显示 / 显隐开关 / 上限 8 明示拒绝）+ 底栏「合并…」入口 + 快捷键 +
  `Feature::MergeTimeline` 门控两道闸（免费态弹升级对话框, 注入惯例两态锁）+
  并集列模型（D9: 各源 schema 并集, 异源缺列留空; 列数爆炸策略 = **spec Q3 本步裁定**:
  并集上限 24 + 超出按首见截断, 弹层可见提示）+ 混合源接入。
  验收: 门控两态/上限拒绝/弹层互斥与 Esc 插层（家族规矩）/ 并集列断言锁。
- **T5 时钟偏移/时区**: 弹层每源偏移（±ms + 快捷档 ±1s/±1min/±1h）与时区（无 tz 格式必填,
  默认本地）; 改偏移 = 该源重取时间戳 + 重归并（不重建文件索引）。
  验收: `offset_applied_at_parse_boundary`（排序与显示同源, 两处各算必红）。
- **T6 req_id 追踪**: 选中行 → 快捷键「追踪此字段值」（JSONL 行取字段值/.log 行取选区子串）
  → 跨源过滤（per-source `run_filter` + 重归并）+ 命中导航复用搜索链。
  验收: `trace_field_value_builds_filter` 锁（串生成与过滤链两段）。
- **T7 live-tail 合流**: per-source `launch_append` 复用 + 新行增量进合并索引（T2 append API）
  + 跟随钉时间线尾 + 轮转重建（FileStat head 哈希 per source）+ **断流源降级标记**
  （单源过期/打不开不拖垮全局, 源列表标「断流」）。
  验收: 增量合流锁 + 轮转源重建锁; 实机跟随（人工验收 a）。
- **T8 sessions 载荷 merge group**: `SessionEntry` 加 merge 段（源路径列表+各源偏移/显隐/格式
  手改记录; expands 键 (src,line) 序列化形态钉死）+ `is_recognizable` 认新段（M1 复发守卫）
  + 应用 = 重开源组重建合并（源缺失 = 明示跳过该源不拒全体会话）。
  验收: `session_merge_group_roundtrip` + 坏条丢条 + 认段锁。**D4 授权退路**: 本步超支 →
  裁 Open Q2, spec 回写, 验收 (g) 转注（不烂尾）。
- **T9 实测与文档**: logbench `--merge` 四组数字（打开/重归并/隐藏重建/跟随追加）进
  PERFORMANCE_REPORT + spec 实现记回写（口径分叉随记）+ 人工验收九条 (a–i) 记账待实机
  （**需付费态 key**）。

## 四、风险与退路

| # | 风险 | 拆法 |
|---|---|---|
| R1 | 合并索引内存 (D10 红线) | **T0 原型就是拆这颗雷**; 触红线 → 回 spec 谈分块, 不硬推 |
| R2 | 并集列爆炸 (spec Q3) | T4 裁定: 并集上限 24 + 首见截断 + 提示 |
| R3 | 多源轮转/断流复杂度 | per-source 复用现有机制 + 断流降级标记, 不许单源拖垮全局 |
| R4 | 单/合并双模式状态互踩 | Merge 独立 bundle, Single 零触碰; 413 基线 + 家族病史对照清单（弹层互斥/Esc/滚轮锁/点穿/拒收显式清空）随模块传代 |
| R5 | D11 色板不批 | 退路单色 chip + 源名标签, T4 不阻塞 |
| R6 | 时间戳解析性能 (逐行手写解析 × 1450 万行) | T0 原型实测; 候选拆法 = 步进缓存 (每 16 行一钉 + 段内递推, 与行索引同构) |

## 五、纪律与基线

- 测试基线: **danqing-logfile 68 / danqing-log 413 / genlog 8 不许破**; 新锁先红后绿 A/B 留痕。
- 提交前三件套两仓各自全绿; 中文注释; 新 `.rs` 文件头（`timestamp.rs`/`merge.rs`/`merge_view.rs`）。
- 颜色/间距走 token（D11 批准后）; 不自定义色; 弹层族规矩（互斥/Esc 插层/模态同源）。
- 联动链 CP1 顺序不能反; patch 开着 lock 不提交; 两仓提交 message 注明关联。
- 隐私零变化（纯本地操作）; 发布链动作不在本 plan。
- 未获用户指示不 commit/push（CP1 的提交动作届时单独点头）。

## 六、三连接门（另案同窗口, `todo-gate-trio.md`）

D8: 无 spec 轻 todo。G0 先把每件的门控点与免费态行为钉成表（权威 = 2026-09-27 裁决:
会话内书签与 .log 直用保持免费）→ G1 Feature 三枚举+label → G2 列配置（入口=列管理弹层+
表头拖拽手势起点）→ G3 书签持久化（功能免费, 持久化通路付费; 提示位走设置卡许可页,
**不在 toggle 时弹对话框**）→ G4 字段点选（入口=「字段…」按钮）。T3–T4 窗口内顺手做。
