# SPEC-v1x-merge-timeline: 多文件时间戳合并 (腿一)

> 作者: 十四叔 · 日期: 2026-09-27 · 状态: **口径已批准 (2026-09-27 用户「go」八项全按推荐) · 待 plan**
> 流水线: spec → plan → build → review → code-simplify (逐段推进)
> 范围裁决: 单能力**六腿** (Phase 0 判定不拆多模块 —— 验收时刻唯一:
> 「实机 N 个源合成一条可滚动、可跟随的时间线」; 引擎/视图/源管理共享同一合并索引, 拆缝都不自然)
> 所属: 能力地图 `SPEC-v1x-map.md` 模块 `merge-timeline` (**v1.x 发布前置**, 09-27 翻案块①);
> 依赖: `licensing` (付费门) + live-tail 已有增量索引 (`append_from`/`lines_from`/`run_filter_from`)

## 0. 背景 (为什么做 / 为什么现在)

- **2026-09-27 用户裁决**: 腿一从「第二波单独起 spec」提前为 **v1.x 发布前置**, 发布延期等它。
  证据 (四路调研, 全文 `../research-v1x-paid-tier-2026-09-27.md`):
  合并 = 品类付费**入场券** (LogViewPlus 买家评价 3/10 点名 / 官方论坛 52 条建议中 7 条 = 最大
  功能簇 / Dadroit 把 Union 钉 $198/年顶档); 现三腿 (export/sessions/analytics) 买家实锤≈零;
  **腿一是其余三腿的承重梁** —— 单文件会话 = 重做一遍就行, 合并工作区的会话 / 合并时间线的
  导出 / 跨文件统计才是真「批量·留存·交付」。
- **反面约束 (同样写死)**: 合并的免费等价物最多 (lnav 核心特性 / OtrosLogViewer GUI 按钮 /
  微软 logmerge / VS Code 插件) —— 「没有 merge 卖不动, 只有 merge 也卖不动」。
  差异化 = **大文件性能 × JSONL 列化 × GUI 零语法**组合, 缺一项就退化成又一个 logmerge。
- 本模块是 intent「三大技术风险」之一 (多文件合并), 风险本体在腿 B —— plan 阶段原型先行,
  实测校准后再钉验收数字 (见 D5)。

## 1. Objective

**用户故事**: 事故复盘时, 我把 auth / worker / db 三个服务的日志合成一条时间线, 一眼看到
「db 的 ERROR 发生在 worker 超时之后 200ms」; 点 worker 那行追 req_id, 三个源里同一个请求的
踪迹一次过滤出来; 各源颜色分明, 可以把噪音大的源临时藏起来; 测试机的钟比生产快 3 秒,
给它拨 -3s 对齐; 文件还在写, 新行自动进时间线。存成会话, 明天打开接着看。

**成功的样子**: 3×1GB 合并打开线性不爆炸, 滚动/过滤/跟随体感与单文件一致, 全程不写一行语法。

## 2. 范围 (六腿)

### 腿 A: 引擎时间戳解析 (danqing-logfile 联动, 新 `src/timestamp.rs`)

- **JSONL**: 时间戳字段自动发现 —— 常见名优先序 `ts`/`timestamp`/`time`/`@timestamp` +
  值形态探测 (ISO 串 / epoch 数); 复用腿二收口的 `scan.rs` (`scan_field`/`FieldValue`) 通路,
  不新开提取岔路。
- **.log**: 行首时间戳格式探测, 首版格式集 = **ISO-8601 家族** +
  **`YYYY-MM-DD HH:MM:SS[,|.]mmm`** (log4j 系) + **epoch 秒/毫秒**。
- 每源**自动探测 + 可手改** (源管理弹层里改格式/字段); 采样探测 (对齐列发现 512 行先例),
  探测失败的源**拒绝加入合并并明示原因** (不静默猜 —— 猜错的时间线比没有更糟, D2)。
- 产出 per-line 时间戳提取 API + 每源格式结论; 提取成本线性有上限, **零全量 serde parse**
  (对齐 memmem 粗筛哲学; 手写解析, **无新依赖** —— 引擎现无 chrono, 不引)。
- 时间表示: i64 毫秒 (内部统一); 无 tz 的时间戳按**源时区**解释 (腿 D 接)。

### 腿 B: 合并索引 + 虚拟滚动视图 (danqing-logfile 新 `src/merge.rs` + danqing-log 新 `src/merge_view.rs`)

- **有序性假设 (D1, 地基)**: 文件内追加序 ≈ 时间序, 容忍**有限乱序窗口** (归并缓冲;
  乱序超窗的行钉住不重排 —— lnav/logmerge 同款假设); **全量排序不做** (GB 级不可行)。
- **无时间戳行继承上一行时间戳** (stack trace continuation 语义; 文件首行无 ts 且第二行有
  则首行继承第二行 —— 边界情形 plan 钉死)。
- 等时间戳 tie-break 规则写死: 同源保持文件行序, 异源按源序号 (稳定可复现, 测试可断言)。
- 合并行表物化: 行 = (源序号, 文件行号), 内存预算 **≤ 合并总行数 ×16B** (D5);
  视图为 `expand.rs` `Lines` 抽象的第四种实现 (全量/过滤/展开之后) —— 行锚定虚拟化数学复用。
- **合并视图的列模型 (D9)**: 基座 = 「时间 | 源 | 消息」三栏; JSONL 表格模式 = 各源 schema
  **并集列** (异源缺列留空, 消息列兜 .log 源原文); 混合源 (.log + JSONL) 允许。
- 过滤/搜索/直方图在合并视图 = per-source 执行 (`run_filter_from` 复用) + 重归并;
  交互面 (过滤栏/搜索栏/侧栏) 零新控件。

### 腿 C: 源管理 (加/减源, 按源着色, 按源隐藏)

- 入口: 底栏「合并…」钮 + 快捷键 (实现裁定, 验收可裁); 当前文件为主源, 弹层追加/移除源,
  源列表显示: 色块 + 文件名 + 格式结论 + 偏移 (腿 D) + 显隐开关。
- **按源着色**: 行首源色条 + 时间列着色 (D11 色板); **按源隐藏**: 关即重归并 (索引重建),
  开即恢复; 源上限 **8** (第 9 个明示拒绝, D7)。
- 合并视图与单文件视图 = 同一窗口两种模式, 切换互不丢状态。

### 腿 D: 时钟偏移/时区校准 + 无时间戳行策略

- 每源偏移量 (±ms 粒度, 弹层手输 + 常用快捷档) 与**源时区** (无 tz 格式必填, 默认本地);
  偏移在**解析边界统一施加** —— 排序与显示同源, 不允许两处各算一遍 (update-badge 几何锁教训)。
- 无时间戳行 = 腿 B 继承规则, 不在本腿另设策略 (单一语义源)。

### 腿 E: req_id 跳转 (最小形态, D3)

- 选中行 → 右键/快捷键「追踪此字段值」→ 以该值为过滤串的**跨源过滤** + 命中导航
  (复用现有过滤/搜索链, 零新机制); .log 源对选区文本同款可用 (追踪子串)。
- **明言不建**: Span 树 / 瀑布图 / 任何追踪语义模型。

### 腿 F: live-tail 合流

- 各源独立跟随 (现有 250ms 节流/轮转重建机制 per-source 复用, `append_from`);
  新行按 D1 规则进时间线 (尾部追加为主, 乱序走窗口); 合并视图跟随 = 钉时间线尾部。

### 不做 (明言)

- tab 系统 / 多窗口 (D4: 单窗口双模式); Span 树/瀑布; 自定义时间格式串 (Open Q1);
  全量排序; 远程源 (腿五候选, 另案); AI 功能 (调研 §十 停放)。
- **三连接门不进本 spec** (D8): 另起轻 todo `tasks/todo-gate-trio.md` (无 spec,
  照 export/sessions 先例纯接线), 本模块 build 窗口内顺手完成。

## 3. 决策

**2026-09-27 用户八项口径 (「go」全按推荐)**:

| # | 决策 | 内容 |
|---|---|---|
| D1 | 有序性假设 | 追加序≈时间序 + 有限乱序窗口; 超窗钉住; 无 ts 行继承上一行; 全量排序不做 |
| D2 | 时间戳来源 | JSONL 字段自动发现 + .log 行首格式探测 (ISO 家族/log4j/epoch); 每源自动探测+可手改; **探测失败拒绝加入并明示**; 自定义格式串不进首版 |
| D3 | req_id 形态 | 最小形态 = 选中值 → 跨源过滤 + 命中导航 (复用过滤链); Span 树/瀑布不建 |
| D4 | 入口与应用模型 | 不引入 tab; 「合并…」入口 + 弹层管源; 单窗口双模式互不丢状态; **merge group 进 sessions 载荷 = 首版目标** (承重梁兑现), build 超支可裁为后补 (Open Q2, 不烂尾) |
| D5 | 性能目标 | ~~3 源×1GB 合并打开 ≤ 单文件打开 ×3~~ **CP0 校准 (2026-09-27 T0c 实测)**: 红线 = 3×1GB 合并就绪 ≤ **1.6s** (串行实测 1525ms +5% 余量)、8×200MB ≤ 1.0s (实测 896ms); 目标 (非红线) = T1 源级并行后 ≤ 0.8s。滚动/过滤体感与单文件一致; 索引内存 = 总行数×16B **实测精确成立** (17.0M 行 = 259.2 MiB, D10 物化无需分块); logbench --merge 出数 |
| D6 | 门控 | `Feature::MergeTimeline` 新枚举, 门控点 = 入口 (export 先例: 免费态弹升级对话框; 两道闸, 数据不丢) |
| D7 | 规格上限 | 源上限 8 (明示拒绝第 9 个); 单源 ≤ 已验证 10GB; 总行数不设软上限 (实测记录内存曲线) |
| D8 | 三连接门 | 不进本 spec; 轻 todo 另案, 同窗口完成 |

**spec 新增实现裁定 (随本 spec 一并呈批)**:

| # | 决策 | 内容 |
|---|---|---|
| D9 | 合并列模型 | 基座「时间\|源\|消息」三栏 + JSONL 并集列 (异源缺列留空); 混合源允许 |
| D10 | 索引物化 | 合并行表物化 (源序号+文件行号), 内存红线 = 总行数×16B; plan 原型实测校准, 触红线先回 spec 再上分块 |
| D11 | 源色板 | 8 源色需 Theme 新 token 族 (框架小联动, **plan 阶段提请用户批准**); 不批则退路 = 单 accent 色 chip + 行首源名标签; **不自定义色** |

## 4. Commands / Structure / Style (增量, 其余沿用两仓 CLAUDE.md)

- 命令不变: `cargo test` / `cargo clippy --all-targets -- -D warnings` / `cargo fmt` (两仓各自)。
- 改动文件:
  - **danqing-logfile** (先落地先 push): 新 `src/timestamp.rs` + 新 `src/merge.rs`
    (**新 .rs 文件头规则触发**: `//! @author 十四叔` + `//! @date`) + `lib.rs` re-export。
  - **danqing-log**: 新 `src/merge_view.rs` (Lines 第四实现 + 视图模型) / `view.rs` (双模式切换+
    三栏绘制) / `main.rs` (Msg 链+模式状态) / `settings.rs` 或新弹层 (源管理, 弹层族第五员先例在案)
    / `columns.rs` (sessions 载荷加 merge group 段 —— 账本可辨谓词 `is_recognizable` 须同步认新段,
    M1 教训) / `license.rs` (Feature 加枚举+label) / `src/bin/logbench.rs` (--merge 场景)
    / `src/bin/genlog.rs` (多源 fixture: 可控时间戳/乱序/无 ts 行)。
- 联动链: danqing-logfile 三件套 → commit+push → danqing-log **关 patch** cargo check 重解复钉 →
  两仓分别提交, message 注明关联; **patch 开着时 lock 是 path 态, 不许提交**。
- 可能的框架联动 (仅 D11 色板, 经用户批准后): danqing `Theme` 加 source palette token →
  同链复钉。**目标零框架改动**, D11 是唯一候选。
- 隐私: 合并纯本地操作, 零联网, `docs/privacy-policy.md` **零变化** (验收时核一遍)。

## 5. Testing Strategy

注入惯例照旧 (构造注入状态, 不碰全局静态, 不触网不触商店不触真实桌面)。

**danqing-logfile (基线 68 不许破 —— plan 复核实测值)**:
- `ts_detect_*` 格式矩阵: ISO 家族 / log4j 逗号毫秒 / epoch s / epoch ms / **失败拒绝** (不猜)。
- `ts_jsonl_field_discovery`: 常见名优先序 + 值形态 + 无 ts 字段的 JSONL 明示拒绝。
- `merge_monotonic_two_sources` / `merge_out_of_order_within_window` /
  `merge_out_of_order_beyond_window_pinned` (D1 三态)。
- `merge_missing_ts_inherits_previous` (continuation 锁, 含文件首行边界)。
- `merge_equal_ts_stable`: 同源保行序 + 异源按源序号 (tie-break 锁)。
- `merge_incremental_append`: live-tail 新行尾部合流 + 乱序走窗口。
- 全部**先红后绿**, A/B 留痕 (摘归并/摘继承必精确红)。

**danqing-log (基线 413 不许破)** —— **2026-09-28 家法核对后按实际锁名勘误** (原表三条
  名字与落地分叉, 见下注):
- 显示行二态契约: `merge_state_filtered_row_at_maps_positions` + `window_decodes_visible_rows_only`
  (行位置 ↔ 合并行双向映射; 与 `position_of_and_next_bookmark_pos` 合起来覆盖原
  「len/get 契约」的意图)。**勘误注**: 原写「`merge_view_lines_contract`: Lines 第四实现
  契约」—— 落地时合并态**没有**扩展 `expand::Lines` 枚举, 而是 `MergeState.filtered:
  Option<Vec<u32>>` + 自己的 `row_at`/`window` (见 §9 实现记 9)。
- 隐藏三态: `rebuild_masked_hides_source_and_keeps_ids` (引擎, 行数 + 源序号不漂) +
  `merge_source_hide_rebuilds_and_restores_order` (产品, 藏→行数减且余序不乱→恢复全序逐位相同)。
  **勘误**: 原名 `source_hide_rebuilds_index` 从未落地 —— 产品级那条 2026-09-28 补上。
- `offset_applied_at_parse_boundary`: 偏移单源施加 (排序与显示同源锁, 两处各算必红)。
- 门控两态: `merge_gate_blocks_free_tier_and_never_prompts_paid` (免费态入口 → `upgrade_prompt
  == Some(MergeTimeline)` 且弹层不开; 付费态放行且全程不误弹)。
  **勘误**: 原名 `gate_free_state_opens_upgrade_dialog` 未落地, 覆盖在同名不同措辞的锁里。
- `session_merge_group_roundtrip`: 载荷 (源列表+偏移+隐藏) roundtrip + 坏条丢条 +
  `is_recognizable` 认新段 (M1 复发守卫)。
- `trace_field_value_builds_filter`: 选中值 → 过滤串生成 + 跨源过滤链。
- 颜色按 token 断言 (D11 批准后), 不钉色号。

**实测定档**: logbench `--merge` 场景 (3×1GB 混合源: JSONL+.log, 含乱序与无 ts 行) ——
打开/重归并/隐藏重建/跟随追加四组数字, 进 PERFORMANCE_REPORT。

## 6. Boundaries

- **Always**: 提交前三件套两仓全绿; 中文注释; 基线不破 (68 / 413); 联动两仓分别提交;
  新锁先红后绿 A/B 留痕; 文档宣称的测试名写完 grep 确认存在; 等时间戳 tie-break 稳定可复现。
- **Ask first**: 框架 (danqing) 任何改动 (D11 色板是唯一候选); 新依赖 (目标零新增,
  时间解析手写); 合并索引分块方案 (触 D10 红线时先回 spec); 会话载荷裁剪 (Q4 授权内的
  唯一退路, 裁了必须 Open Q2 记录 + spec 回写); 发布链动作。
- **Never**: 全量排序; 静默猜时间格式/时区; tab 系统; Span 树/瀑布; 自定义格式串 (首版);
  自定义色绕开 Theme; 测试触网/触商店/真实桌面副作用; patch 开着提交 lock;
  过滤/搜索/直方图为合并视图新造并行实现 (一律 per-source 复用 + 重归并)。

## 7. Success Criteria

机器部分 (build 收口 + 评审修复后复测):
1. [x] 两仓 `cargo test` 全绿: danqing-logfile **99** / danqing-log **494** (186+294+11+3) —— 09-28 实测。
2. [x] 两仓 `cargo fmt` + `cargo clippy --all-targets -- -D warnings` 零警告。
3. [x] logbench `--merge` 四组数字实测定档, 进 PERFORMANCE_REPORT §合并 (09-28 T9)。
4. [x] `tasks/todo-gate-trio.md` (三连接门) 同窗口完成 (G1–G5 全勾, 09-28)。

人工验收 (用户实机, **需付费态 key** 激活 MergeTimeline; 条目为草案, plan 后钉死):
- [ ] a) 3 源 (JSONL×2 + .log×1, 各 ≥500MB) 合并打开: 时间线序正确, 滚动/跟随体感与单文件一致。
- [ ] b) req_id 追踪: 选中 worker 行追踪其 req_id, 三源同请求行一次滤出, 命中导航可走。
- [ ] c) 按源着色可辨 + 按源隐藏: 藏 db 源后时间线重建正确, 恢复后序不乱。
- [ ] d) 时钟偏移: 给「快 3 秒」的源拨 -3s, 交叉事件序对齐 (构造已知序 fixture)。
- [ ] e) 无时间戳行 (stack trace) 跟随上一行时间戳, 不炸序不失踪。
- [ ] f) 免费态点「合并…」弹升级对话框, 无合并入口泄漏; 激活后可用。
- [ ] g) merge group (源+偏移+隐藏) 存成会话, 重启恢复 (D4 若裁则本条转 Open Q2)。
- [ ] h) 探测失败的源 (无 ts 格式的 .log) 拒绝加入, 原因明示可读。
- [ ] i) 性能实机 (口径 = **CP0 校准后的 D5 红线**): 3×1GB 混合源合并就绪 ≤ 1.6s
  (T0c 串行实测 1525ms; logbench `--merge` 数字 + 体感双证); 8×200MB ≤ 1.0s (实测 896ms)。
  参照: LogViewPlus 1GB 结构化冷启实测 60s —— 合并 3GB 比它单开 1GB 快约 40 倍。

## 8. Open Questions

- **Q1 (defer)**: 自定义时间格式串 —— 首版不做; 触发条件 = 真用户探测失败反馈, 届时另案。
- **Q2 (授权内退路)**: merge group 会话载荷若 build 超支 → 裁为腿一后补, 本文件回写 + 验收 (g) 转注。
- **Q3 (plan 裁)**: 并集列数爆炸 (多源 schema 差异大) 的列上限/截断策略。
- **Q4 (验收裁)**: 源上限 8 的实机体感 (弹层拥挤度), 偏小偏大当场裁。
- **Q5 (plan 裁)**: ~~乱序窗口的具体尺寸 (时间窗 or 行数窗), 原型实测定~~ **已裁 (2026-09-27
  CP0)**: **显式窗口退役** —— min-head 归并的「文件内行序严格保持」天然就是钉住语义
  (乱序行只在成为游标头时按当时最小值找位, 不回头重排), 无需窗口参数; 增量合流的插入
  = 尾端回找 (WALK_CAP 兜底, 见 merge.rs 注释)。T0 六组归并锁已按此语义钉死。
  **2026-09-28 (T9) 回写**: 增量插入的**回找帽退役** —— 批插入改整批单遍后深回找只付
  一次 (2M 行 ≈ 数 ms), 帽的「近似位」没有存在理由了, 插入位改**真值位** (锁
  `insert_rows_deep_walkback_is_exact_not_capped`); 帽仍在 `remove_row` (摘除目标天然
  近尾, 超帽 = 找不到 → 调用方兜底重建)。

## 9. 实现记 (2026-09-28, T9 收口)

**口径分叉与实测回填** (数字全文 `PERFORMANCE_REPORT.md` 合并节):

1. **① 合并就绪 = 源级并行 (用户 2026-09-28 裁「源级并行提取」)**: 红线 1.6 s 出自
   CP0 的**引擎三段**实测 (1525 ms); 产品路径 (`build_merge`) 另含 `detect_route`
   采样与 JSONL **列发现** (1 GiB 单源 ~71 ms), 串行版实测 **1676–1731 ms** (超线
   ~5–8%)。修法 = **逐源流水线并行** (打开→探测→schema→提取, 每源一线程, 结果
   **按源序回填** —— 源序号是 tie-break 依据, 不许按完成序排; 归并仍串行), 实测
   **947–967 ms (1.78×)**, 红线内且把列发现的口径差一并吸收。D5 的**非红线目标
   ≤0.8 s** 尚差 ~17% (3 线程同读 3 GiB 的带宽争用 + 归并 215 ms 串行段), 记档
   不追 (红线已过)。保序锁 `build_merge_keeps_source_order_under_parallel_build`
   (大源慢/小源快 = 完成序与源序相反, 等 ts 行仍按源序; A/B 倒序回填 → 红)。
2. **D10 内存超募**: 建索引多留 `APPEND_SLACK_DIV = 16` (6.25%) 追加位 —— live-tail
   增量插入若不撞容量就不重分配 (否则 `Vec` 翻倍: 一次追加拷 259 MiB + 驻留翻倍)。
   `index_bytes()` 口径仍是行数 ×16 B (实测 259.2 MiB 相符), 超募部分单列。
3. **④ 增量合流的成本模型 (T9 实测揪出并修)**: 首版逐行 `Vec::insert` 的代价 =
   Σ(回找深度), 批量追尾时新行互为障碍 ⇒ **O(k²/2)**: 4 MiB/2.6 万行实测 792–953 ms;
   慢时钟源触帽后每行常数 2.45 ms ⇒ 2 万行 **48.0 s** 且落近似位。三条修:
   引擎**整批单遍归并插入** + **回找去帽** + **容量留位**; 产品侧**单轮增长 > 20,000 行
   交 worker 重归并** (`MERGE_SYNC_MAX_ROWS`, 与既有的 32 MiB 字节闸并列)。修后
   同两例 **50 ms / 46 ms**。**A/B 留痕**: 反转合并方向 → 差分锁红; 摘容量留位 →
   留位锁红; 摘产品闸 → 分流锁红; 摘锚定窗口 → 选中行锁红。
4. **D3 快照释放挪出 UI 线程**: 1 GiB 映射释放实测 50–58 ms (swap 路径曾测 157 ms),
   live-tail 每次换快照都付 —— 现由一次性线程释放 (`dispose_snapshot`)。
   **已知代价**: 旧映射可能短暂仍在 (测试里对同一路径立刻重写/复制会被 Windows
   映射语义拒) —— 基准与测试一律用**独立路径**, 家法「测试不许时序侥幸」。
5. **D4 选中锚定改写**: 追加后的 `(源,行) → 位置` 原走 `position_of` (从头线性扫,
   17M 行实测 158–250 ms/次), 改为**有界窗口重定位** —— 批行只插在原位之后、末行补全
   至多先摘 1 条, 故新位置必落 `[原位−1, 原位+k]` (k = 批行数)。`position_of` 本身
   保留 (追踪落地/书签跳转/掩码重建, 低频各一次)。
6. **T8 会话载荷的两处口径**: ①「格式手改记录」**无物可存** —— D2 的 route 手改首版
   未建, 载荷故无 route 字段 (探测结论恢复时重探); ②合并态 `expands` 仍是**单文件侧**
   行号 (合并展开未建, T3 边界), 若将来落盘, 键形态钉死为 `merge_view::pack_key`
   的 `(src,line)` 打包值 (结构注已写)。③合并追踪串首版不落盘。
7. **T7 留档复核**: 追踪态过滤集重推实测 **66–86 ms/次** (17M 行, O(合并行数)) ——
   与留档估的 30 ms 同量级, 高约 2.5×, 判「有界可接受」维持。
8. **人工验收九条 (a–i)**: 机器半边全绿; **实机待做** (需付费态 key), 记账在
   `tasks/acceptance-pending.md` G 组; (i) 的性能半边已由本节实测回填。
9. **显示行抽象的分叉 (评审期记账)**: 本 spec §5 原写「Lines 第四实现」—— 落地时
   合并态**没有**扩 `expand::Lines` 枚举, 而是 `MergeState.filtered: Option<Vec<u32>>`
   + `row_at`/`window` 自成一路。**意图 (二态显示行 + 位置↔行双向映射) 是兑现的**,
   代价是过滤/滚动/选中的位置数学在单文件与合并两条路上各有一份。是否收口成
   一层抽象, 交模块评审/后续波次裁 (收口会动单文件侧, 不是零风险改动)。
10. **评审轮 (2026-09-28, 双路独立) 的发现与修复** —— 代码评审 REQUEST CHANGES
   (Critical ×2 + Required ×4), 安全审计 0 Critical / Required ×1 (与 C1 同源):
   - **C1 (两路同指)**: 在途追踪 + 源集合换过 → `filtered_from_hits` 按 `src` 索引
     越界 (release 档 `panic=abort` = 整进程死); 等长但重排时则**静默把命中贴到别源**。
     两个漏点: `apply_merge_outcome` (换源落地点) 与 `apply_session` 单文件分支
     (离场) 都未 `invalidate`; 另有「换源期间新起的追踪」绕过旧作废。修: 两处作废
     + `apply_trace_outcome` 到点校验 (不符 → 丢弃 + 出声) + `apply_trace` 运行期闸
     (+ `filtered_from_hits` 改 `get` 纵深)。**复现锁**: 摘运行期闸 → 越界 panic (精确红)。
   - **C2**: `append_source` 的选中锚定有界窗口**证明前提写错** —— 锚行**自身**是被
     补全改判的残行时键变了、位置可任意远 (评审给的反例: (0,2)@位3 → 补全后 @位10,
     窗口 [2,5] 搜不到 → `unwrap_or(lo)` 静默跳到 (0,1))。修: 窗口之外回落
     `position_of` 精确定位。**复现锁**: 摘回落 → 断言精确红在「实得位 2 的 (0,1)」。
   - **R1**: worker panic 会让 `merge_job_live` 永卡 → live-tail 静默冻结 (unwind 档);
     修: 源线程内 `catch_unwind` 兜成「该源拒收」 + `Builder` 释放线程 (建线程失败就地 drop)。
   - **R2**: 「旧 mmap 释放挪出 UI 线程」原**没达成** —— `LogView.merge_files` 同持 Arc,
     state 侧只减一个引用, 真正的 munmap 落在同帧 view 的 sync (UI 线程)。修: view 侧
     退役整表也走同一释放通道 (`dispose_snapshots`) + 退出合并即松手。**报告口径已同步改正**。
   - **R3**: `apply_saved_params` 曾在 UI 线程重提时间戳 (3×1GiB ≈1s)。修: `build_merge`
     收会话参数 (**worker 里**提取时就施加偏移/时区, 并在建索引时掩码显隐), 落点零重提零重建。
   - **评审未抓、本轮自查抓到**: `carry_view_state` **不搬 offset/tz** —— 加/减源会把用户
     T5 校准的时钟偏移**静默重置**。R3 的改法 (参数按路径随行) 一并解决, 锁
     `rebuild_merge_keeps_time_params_by_path`。
   - 其余收口: UI 手输偏移/时区与账本同一条钳制 (`set_time_params` 收口) + `saturating_add`;
     会话恢复拒绝 `\\.\` 设备命名空间 (源路径其余按「用户自己的文件」= 已信任假设, UNC
     网络盘合法但**这是本应用唯一会触网的路径**, 隐私政策口径按此理解); `start_trace`
     两处静默 return 改出声 (P24); 补「无文件在手时恢复合并会话」锁。
   - **已核无问题 (评审给判据)**: 引擎 `insert_rows` 整批单遍与旧逐行语义**逐位等价**
     (评审用 90 万随机用例差分对拍独立验证零失配; 反向合并的内存安全不变式成立);
     账本反序列化无 panic/无界分配/钳制到位; `u32` 行号余量 >8×; 轮转/截断竞态有界诚实;
     测试均不触真实桌面/剪贴板/网络。
   - **记档不修 (留痕)**: ①`position_of` 仍是 O(合并行数) 线性扫 (低频入口: 追踪落地/
     书签跳转/掩码重建/C2 回落, 17M 行 ~158–250 ms/次); ②增量追加不重探 route/schema;
     ③门控漏点 (TraceValue/ClearTrace/follow 不过 `merge_gate`, 今日不可达, 属纵深防御);
     ④单帧多源同时增长时追踪态重推累乘 (3 源 ≈200–260 ms/帧); ⑤引擎 `timestamp.rs`
     的 `n.abs()`/`v*scale` 在手工极值下可回绕 (既有代码, 本次未动)。

## 10. code-simplify 收口 (2026-09-28)

**7 项行为零变化** (产品 494 绿 / 引擎 99 绿, 测试**零改动**, 连跑 5 遍稳):
1. **钳制单一真身**: `columns.rs` 里那份 `MAX_ABS_OFFSET_MS`/`MAX_ABS_TZ_MS` 私有常量
   与 `merge_view` 的重复 (T9 评审修 D3 时新造的分家) → 账本侧改调
   `merge_view::clamp_offset_ms/clamp_tz_ms`, 常量只留一处。
2. **释放策略单一 spawn 点**: `dispose_snapshot` / `dispose_snapshots` 两条同构函数
   → 一个私有 `dispose_offthread<T: Send>` (Builder 回退就地 drop 的注释也归一处)。
3. **在途追踪作废收口**: 7 个调用点各带一句「R5 族」注释 → `discard_trace_job()`
   一个方法 (家族规矩写在方法上, 调用点只留一句指路)。
4. **概念命名**: `MergeState::anchor_at(pos)` (行身份 = (源,行), 生产侧 3 处) +
   `MergeState::file_handles()` (4 处)。
5. **合并复制链单一解析点**: `copy_source` 判「有无可复制文本」与 `selected_text`
   取串原先靠注释约定两处一致 (T6 在这条链上漏出过「说有选中却复制错行」) →
   `merge_sel_slice()` 一个解析点, 判据与取串同源由**构造**保证。
   唯一语义收紧: 源句柄缺失这一**不可达**分支由「静默不复制」变为「出声指路」。
6. **过期前向引用修正**: 合并态 Ctrl+C 的指路 notice 原写「整行复制随 T7」,
   而 T7 波并未做 —— 改为「整行复制未接」(spec 边界如实记)。
7. **顺带修一条既有 flaky 测试** (bookmark-persist 模块 2026-09-23 起潜伏, 非本模块):
   `toggle_says_truth_when_save_fails` 往占位目录写 sentinel 后用 `remove_dir` 删
   (非空删不掉, `.ok()` 静默) → 目录泄漏 → **pid 被复用**时下次 `create_dir`
   撞 AlreadyExists → 约 1/5 复现 (本次靠连跑 5 遍抓到)。修: 开头防御性整树清理 +
   结尾 `remove_dir_all`。**教训**: 「连跑多遍」是抓 flaky 的唯一手段, 单跑绿不算数。

**不动清单 (想过但不改)**:
- `apply_saved_params` 现在是**网**不是主通路 (参数已进 `build_merge`; 只有
  「carry 覆盖了显隐」这类差额需要它) —— 留着, 它有独立单测且是 C 类改动的护栏。
- `position_of` 的全表线性扫: 低频入口 (追踪落地/书签跳转/掩码重建/C2 回落),
  收口要给索引建反向结构, 不值得为 O(1) 次/用户动作开新机制。
- 双模式位置数学分叉 (单文件 vs 合并各一套): 收口必然动单文件侧, 属设计决定,
  留模块评审/后续波次 (§9.9)。
- 引擎 `insert_rows` 的注释密度与 `logbench` 的参数个数: 前者是**不变量声明** (值钱),
  后者是开发者工具 (不为它引结构体)。
- 测试里的内联形状 (`row_at(p).map(|r| (r.src, r.line))` 等): 生产侧已收口,
  测试保持现状换**零 churn** (改测试 = 改锚, 不值)。
