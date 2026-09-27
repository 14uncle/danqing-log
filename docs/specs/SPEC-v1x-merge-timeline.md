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

**danqing-log (基线 413 不许破)**:
- `merge_view_lines_contract`: Lines 第四实现契约 (len/get 双向映射, 与展开实现同套断言)。
- `source_hide_rebuilds_index`: 隐藏→重归并→恢复三态 (行数与序双断言)。
- `offset_applied_at_parse_boundary`: 偏移单源施加 (排序与显示同源锁, 两处各算必红)。
- `gate_free_state_opens_upgrade_dialog`: 免费态入口拦截两态 (注入惯例, export/sessions 先例)。
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
1. [ ] 两仓 `cargo test` 全绿: danqing-logfile ≥ 68+新锁 / danqing-log ≥ 413+新锁 (实测值 plan 记录)。
2. [ ] 两仓 `cargo fmt` + `cargo clippy --all-targets -- -D warnings` 零警告。
3. [ ] logbench `--merge` 四组数字实测定档, 进 PERFORMANCE_REPORT (D5 校准后验收线)。
4. [ ] `tasks/todo-gate-trio.md` (三连接门) 同窗口完成 (另案勾账, 不挡本 spec 验收)。

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
