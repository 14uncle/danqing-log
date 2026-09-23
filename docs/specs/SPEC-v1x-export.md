# SPEC-v1x-export: 导出（腿三）

- @author 十四叔
- @date 2026/09/23
- 状态: **build + 双路评审修复收口**（2026-09-23 一日: 批准 → plan → T1–T7 → 双路
  review REQUEST CHANGES → 全部修复, 312 测试绿）—— 待人工验收 → code-simplify
- 所属: 能力地图 `SPEC-v1x-map.md` 模块 `export`（构建顺序第 3 位，接 `field-analytics` 之后；依赖 `licensing` 的门控机制）

## Objective

把「看懂」的结果**交出去**——用户过滤出一批行、答完「昨天 p99 多少」之后，
总得能把结果交付给别人或别的工具（工单附件 / 周报 / Excel 透视）。
ROADMAP §二腿三的三格式字面：**过滤结果导出、JSON 美化导出、CSV**。
这是付费层「交付」半边的第一件，与腿四「留存」合成收尾（单独看不构成购买理由，
是腿一/腿二的收尾件——ROADMAP 定位如此，本模块不拔高）。

**成功长什么样**：过滤/搜索出结果 → 状态栏「导出…」→ 选格式 → 选路径 →
后台流式写盘（UI 可响应、可取消）→ 交付文件在 Excel / 编辑器 / 脚本里直接可用；
免费用户点导出弹统一升级提示。

## 范围（2026-09-23 用户四项裁定，全按推荐项）

1. **行集口径 = 当前结果全集**（裁定①）：导出「当前生效过滤下的全部命中行」，
   与侧栏桶计数、状态栏命中数同一口径。展开子行不单独出（导出对象是文件行）。
2. **明文只出原始行**（裁定②）：原始行导出两模式都有（字节原样）；
   JSON 美化与 CSV 只在 JSONL 表格模式可用。明文按时间戳/级别/消息启发式切列
   **不做**（切错列 = 脏数据）。
3. **流式 + 可取消 + 实测定档**（裁定③）：行集逐段扫写出，零文件内容整读；
   进度反馈 + 可取消；性能线写目标值，build 后用 logbench 实测回填。
4. **导出整体付费**（裁定④）：免费态点任何导出 → 统一升级提示
   （`Feature::Export`，licensing 模块已备好该枚举）。小结果复制已有三级复制链兜底，
   免费层不留导出缺口。

**In**：

1. **三种格式**（可用性按模式收口，见 D1）：
   - **原始行**：结果行集的文件行**字节原样**（含原行尾，不转码不解析）
   - **JSON 美化**（仅 JSONL）：单行 JSON → 缩进 pretty-print
   - **CSV**（仅 JSONL）：schema 列 → 表头 + 数据行，RFC4180 方言（D5）
2. **导出作业**：后台流式写盘 + 进度反馈 + 可取消（取消删半成品），单作业
3. **入口**：状态栏「导出…」按钮 + `Ctrl+E`（快捷键页登记）；格式小菜单
   只列当前模式可用格式；保存对话框复用 `rfd`（Ctrl+O 同款依赖）
4. **付费门控**：点位 = 导出入口（保存对话框之前），免费态弹统一升级提示
5. **测量通路**：`logbench --export <fmt>`（性能数字的弹药来源，与既有实测表同口径）

**Out**：

- 明文 CSV 切列（裁定②：启发式切列会出脏数据）
- 展开子行进导出（渲染产物不是文件行，原文保真优先）
- 分析结果导出（field-analytics 曾记「归腿三议」——本 spec 初裁 **不做进首版**，
  见 Open Questions ①）
- `.xlsx` 真 Excel 格式（CSV 就是给 Excel 的，xlsx = 一个新依赖 + 一整套格式工程）
- 导出到剪贴板（三级复制链已覆盖）
- 增量/定时/监听导出、多作业并行导出
- 时间戳列类型系统、列配置（另项，别混进来）

## 设计决策

### D1: 行集口径 = 「当前结果行集」统一抽象（2026-09-23 用户裁定）

导出对象是**一个行号集**（升序去重），三格式共享同一取行管道：

| 模式 | 搜索/过滤状态 | 行集 |
|---|---|---|
| JSONL 表格 | 过滤生效 | 过滤命中集（`run_filter` 同源，与侧栏桶计数口径一致） |
| JSONL 表格 | 无过滤 | 全部文件行 |
| 明文原始 | 搜索生效 | **含命中的行**（一行多命中只出一次）——与状态栏命中数同源 |
| 明文原始 | 无搜索 | 全部文件行 |

- JSONL 模式下搜索**不改行集**（搜索是高亮+导航，视口行集由过滤决定）——导出同此。
- live-tail：**作业启动时冻结**行集（as-of 快照）。导出中文件再增长不进本次结果
  ——交付物要可复述（「这是 X 点 Y 分的结果」），活口径交付出去没法引用。

### D2: 流式导出作业，取消 = 删半成品（2026-09-23 用户裁定）

- 复用 `search.rs` 的 AsyncJob 范式（worker + tick 拾取）新建 `export_job`：
  mmap 读 + 分段写（固定缓冲循环），**内存 O(缓冲)**，行号集外不驻留内容。
- 进度 = 已写行数/总行数（状态栏显示，与 Opening 阶段感知同款位置语义）。
- **取消语义 = 删半成品文件**——留一个内容不完整的「结果文件」比没有文件更坏
  （交付物被误引用无法挽回）。删除失败降级为提示路径。
- **单作业**：导出进行中再点入口 → 拒绝并提示，不排队不并行（一次交付一个作业，
  并行写盘抢 IO 没有收益）。
- 导出期间 UI 保持可响应（异步打开管道的既有承诺同级）。

### D3: 原始行导出 = 字节保真（含原行尾）

- 写出内容 = 文件行**原始字节**，行尾保留文件原样（CRLF 文件导出仍 CRLF）。
  不解码、不转码、不规范化——GBK / UTF-16 文件导出后与源文件对应行**逐字节相同**。
- 这是「交付」的信任基础：别人拿去 diff / 喂脚本不丢信息。
- 两种模式都可用（明文模式**唯一**可用格式）。

### D4: JSON 美化 = 逐行 parse 后 pretty，失败行原样

- 逐行 `serde_json::from_slice` → `to_string_pretty`（indent 2），对象之间**空一行**
  分隔（连续 pretty 对象不空行会粘连成不可读块）；输出 UTF-8 + `\n`。
- **解析失败行原样写出**（保持字节），完成后提示「N 行非 JSON，已原样导出」
  ——不丢行、不猜、不中断（与引擎「失败语义明确」的家法一致）。
- 仅 JSONL 模式可用（裁定②）。

### D5: CSV 方言 = UTF-8 BOM + CRLF + RFC4180，列 = schema 列

- **BOM**：文件头写 UTF-8 BOM（`EF BB BF`）——Excel 中文不乱码的唯一可靠手段，
  目标用户就是拿去给 Excel 的。BOM 只在 CSV 写，另两格式不写。
- **引号规则**：RFC4180——字段含逗号/引号/换行则整体加引号，内部 `"` 转 `""`；
  行尾 CRLF。手写转义，**不引 csv crate**（一个转义函数 + 对拍测试就够）。
- **列 = schema 首见序**（与表格显示同口径）：表头 = 列名行；行内缺字段 = 空串；
  schema 外字段**忽略**（表格也不显示它，口径一致）；嵌套/复合值 = 紧凑
  单行 JSON（`{"a":1}`，与单元格显示同口径）。
- **源非 UTF-8**（GBK JSONL 罕见但存在）：解码走 `danqing-encoding`，输出一律
  UTF-8——CSV 是交付文本，统一编码；原始行格式负责字节保真那条路。
- **公式注入中和**（2026-09-23 评审追加）：单元格首字符 `=`/`@`/Tab/CR 恒加 `'`
  前缀，`+`/`-` 仅**非纯数字**时加（负数列保数值语义）——CSV 的目标就是 Excel，
  而日志内容不可信，RFC4180 引号挡不住公式执行。
- 仅 JSONL 模式可用（裁定②）。

### D6: 门控点位 = 导出入口（保存对话框之前）

- 点「导出…」/ `Ctrl+E` 即查 `Feature::Export`（licensing 已备，名称「导出」）：
  免费态 → 统一升级提示（`ShowUpgradePrompt`），**保存对话框都不开**——
  先让人选完路径再告诉他不能存，是最坏的顺序。
- 裁定④（导出整体付费）：三格式同门，不设免费试导。

### D7: 入口 = 状态栏「导出…」+ Ctrl+E，格式菜单按模式收口

- 状态栏右侧「导出…」按钮（hover/按下反馈按既有按钮范式）；`Ctrl+E` 同动作。
- 点开**格式小菜单**（下拉/弹层，复用 dropdown 范式）：JSONL 模式列三项
  （原始行 / JSON 美化 / CSV），明文模式只列原始行——**不可用格式不出现**，
  不做灰置项（灰置要额外解释「为什么灰」，不出现则无需解释）。
- `Ctrl+E` 若与既有快捷键冲突，plan 阶段核快捷键表后换键（Open Questions ②）。

### D8: 保存 = rfd save_file，默认名带语义

- `rfd::FileDialog::save_file()`（与 Ctrl+O 的 `pick_file` 同款依赖，零新增）。
- 默认文件名：`<源文件名 stem>[-filtered|-searched]-<yyyyMMdd-HHmmss>.<ext>`
  （无过滤/搜索则无中缀；`.log` → 原始行保 `.log`，JSON 美化 `.json`，CSV `.csv`）
  ——带时间戳是交付习惯（同一结果多次导出不互覆）。
- 覆盖确认走系统对话框（rfd 语义），不自造。

### D9: 性能测量 = logbench --export，目标值实测后回填

- `logbench --export <raw|pretty|csv>` 与既有实测表同口径（热缓存注明）。
- **目标值**（先写目标，build 后实测定档回填 PERFORMANCE_REPORT）：
  1 GiB 行集（全文件 ~400 万行）原始行导出**热缓存 ≤ 8 s**（SSD 写入下限主导，
  扫描成本应被写盘掩盖）；CSV / 美化含逐行 parse，目标 ≤ 30 s 量级（同条件）。
- 数字进 `PERFORMANCE_REPORT.md` 与对外文案前**必须实测**，禁止估算冒充实测
  （09-21 口径清扫的家法）。

## 成功判据

**机器可验**：

1. CSV 转义对拍：逗号/引号/换行/CJK/空串/嵌套紧凑 JSON 各形态 + BOM 头字节
   (`EF BB BF`) + 行尾 CRLF 断言；与手算期望逐字节相等
2. 美化对拍：合法行 pretty 结果与 `serde_json` 原生 pretty 全等；非法行原样
   计数正确（混合文件对拍）
3. 原始行字节全等：CRLF 源文件导出行尾保真；GBK 源文件不解码、逐字节相同
4. 行集口径（D1 表四行各一测）：过滤开/关 × 搜索开/关 × live-tail 冻结
5. 门控：免费态点导出 → 弹升级提示且**零文件写出**；付费态 `Feature::Export` 放行
6. 取消：半成品文件被删；导出进行中重复入口被拒
7. 流式契约：导出过程无「整文件内容进内存」路径（行号集除外）
8. 三件套全绿（fmt / clippy 0 / 测试全绿），基线 253 不破

**性能（logbench，实测回填）**：数字进 PERFORMANCE_REPORT，标注硬件与冷热口径；
未达标则按数据决定优化或重订目标（field-analytics D7 同款处理）。

**人工验收**（用户实机）：

1. 免费态：点导出弹升级提示，无保存对话框；「去激活」跳转正确
2. 付费态三格式各导一次真文件：CSV 在 **Excel** 打开中文不乱码、列对齐；
   美化文件在编辑器里缩进正确；原始行与源文件 diff 对应行一致
3. 大文件（demo-1gb）导出期间 UI 可响应、Esc/取消可中止且半成品消失
4. 明文模式格式菜单只出现原始行；live-tail 开着时导出为启动时刻快照

## 已知局限

- CSV 列 = 512 行采样 schema：采样后新出现的字段不进表头（与表格显示同口径，
  不是本模块新引入的洞）
- 美化对 minified 超大单行 JSON（数百 MB 一行）会有整行 parse 的内存尖峰
  ——与三级复制链「无字节闸」同款边界，实机撞到再裁
- 明文「搜索命中行」导出按行去重（一行多命中出一次），不是「命中次数」口径
- **混合行尾文件**在稀疏 raw 路径按多数口径统一写出（全集整拷不受限, 逐字节相等）
  —— 实机撞到再升级引擎 `line_with_ending(i)`
- **UTF-16 源**（build 中实证: 引擎打开时转码 UTF-8 副本, `logfile.rs`）——导出的是
  **转码后**的行/字节, 不是源文件的 UTF-16 字节; 「逐字节保真」对 UTF-8/GBK 成立
- 默认文件名时间戳 = **UTC**（零依赖手写 civil-from-days; 时间戳只用于唯一名,
  不承担叙事; 要本地时间再议）
- **同路径覆写**会透过 Windows 共享映射窜进在途快照（mmap 与写句柄同页）——
  冻结语义对 live-tail 的 **append** 增长成立（映射定长）, 对「覆写同名文件再导」
  不设防; 不属产品增长形态, 实机撞到再议
- **行尾探测**取前 8 个行尾多数票——「头样板与正文方向相反」的文件会统一错
  （比混排更隐蔽的采样偏差）; 属已知局限, 实机撞到再补引擎 `line_with_ending(i)`
- **0 命中导出**（过滤结果空）= 合法空交付（raw 0 字节 / CSV 仅 BOM+表头）,
  完成提示如实报「0 行」—— 不加确认弹窗
- 离线 key 是君子协定（licensing 既定，不重复展开）

## Boundaries

- 注释/文档中文；新 `.rs` 文件头 `//! @author 十四叔` + `//! @date yyyy/MM/dd`
- 零新依赖（rfd / serde_json / danqing-encoding 全是既有）；CSV 转义手写
- 五段流水线：spec 批准 → plan → build → review → code-simplify，spec 后不立即编码
- 未获用户指示不 commit/push
- 引擎缺口当场修进 `danqing-logfile`（~~预计零引擎改动~~ **实况修正**: 为全集
  整拷补了 `LogFile::bytes()` 访问器, `danqing-logfile@a81dcac` —— `line()` 剥行尾,
  字节保真需要存储字节快照, 8 行零拷贝）

## Open Questions

1. ~~**分析结果导出**~~ **已裁（2026-09-23 用户「go」随 spec 批准）：不做进首版**
   （ROADMAP 腿三三格式无此项；结果是静态小表，截图/手抄成本低）。将来要做另立小项
2. **`Ctrl+E` 冲突核对**：plan 阶段核快捷键表与框架按键路由，冲突则换键
   （候选 `Ctrl+Shift+E`），不单独占用裁定轮次

## 实现记（2026-09-23 build 收口 T1–T7, 与上文分叉处回本）

- **行尾策略分叉 (plan 衍生设计, 维持零引擎改动→实为一处引擎访问器)**:
  `line(i)` 剥行尾 ⇒ 全集 raw 走**引擎新增 `LogFile::bytes()`** 整拷
  (`danqing-logfile`, 联动未 push 待用户闸门) —— 字节保真是构造保证
  (含混合行尾/无终行尾); 稀疏 raw = 行内容 + 文件级行尾探测 (前 8 个行尾多数票)。
- **T2 口径修正 (推翻 plan 原案「不逐行 parse」红线)**: CSV 单元格走 `parse_line`
  + `cell_display` —— 与表格显示**同口径构造保证** (字符串反转义裸值, 嵌套紧凑 JSON);
  一次 parse 供全部 K 列; 非 UTF-8 源先 `decode_line` 再 parse。理由: extract token
  保留 JSON 转义 (`say \"hi\"` 进 Excel 是错的)。pretty 本就逐行 parse, 两者共享
  ≤30s 预算 —— 实测 CSV 20.7s / pretty 18.1s (1 GiB 全集, 见 PERFORMANCE_REPORT)。
- **失败行双口径**: CSV = 整行文本进第一列 + 计数 (不丢行); pretty = 原样字节 + 计数。
- **「JSONL 表格模式」判据取 `schema.is_some()`** (文件是 JSONL): Ctrl+T 切原始视图
  不收窄可导格式 —— 服务端同款守门 (idx≠0 且非 JSONL 直接拒)。
- **轮转重建不作废在途导出** (冻结快照自洽: worker 持 `Arc<LogFile>` 旧快照) ——
  与 filter/search 的「贴错新文件」问题不同族; 换文件 (`apply_fresh`) 照常作废。
- **默认文件名时间戳 = UTC** 手写 civil-from-days (零新依赖), 见已知局限。
- **PANEL_CONTENT_H 180→192**: 快捷键页加 `Ctrl+E` 行后 189.5 越界, 面板守卫红
  → 按其指示就地调 (不预留)。
- 机器判据 1–8 全过 (测试 298 绿); 三处 **A/B 精确红**记录在
  `tasks/todo-v1x-export.md` Checkpoint A (摘行尾策略 / 摘 CSV 转义 / 摘 pretty 空行)。

## 评审记（2026-09-23, 双路独立评审: 均 REQUEST CHANGES → 修复闭环, 312 测试绿）

两路（五轴全量 / 并发·保真·行集深潜）互不知情, 独立提出。合并去重后修复清单:

**Critical ×2（全修）**:
1. **覆盖写+取消/失败吃掉用户旧文件** (A) —— `File::create(目标)` 先截断。修: 写
   `<path>.partial` 成功后 rename 原子替换; 取消/失败/panic 只删 partial, 目标原样。
   锁: `cancel_keeps_existing_target_intact` (修前精确红 = 旧文件被删)。
2. **invalidate + 立刻再 launch = 会话级导出报废** (B) —— 跨代次共享 cancel Arc
   (新 launch 重置撤销旧 worker 的取消) + `AsyncJob` 单槽被旧轮覆写 (新结果永久
   丢失 → running 卡死)。修三件套: 每轮新建 Arc 对 / `AsyncJob` 按代次拒旧覆盖
   (search.rs, 全作业族受益) / 语义不变的 poll 链。锁: `invalidate_then_relaunch_
   keeps_new_job_alive` + `late_stale_result_does_not_overwrite_newer` (search.rs)。

**Required ×7（全修）**: ①默认扩展名按格式分派 (R1/B-R4, 锁 `default_export_ext_
follows_format`) ②D1 四态+冻结上锁 (R2, 锁 `export_line_set_covers_d1_four_states`
+ `export_writes_frozen_snapshot_not_appended_growth`) ③CSV 公式注入中和 (R3, 锁
`csv_neutralizes_excel_formulas_but_keeps_numbers`) ④apply_fresh 关格式菜单 (R4,
锁 `apply_fresh_closes_export_menu`) ⑤worker panic → catch_unwind 收 Failed
(R5, 免 in-flight 卡死) ⑥搜索命中 100 万封顶**拒绝导出**不静默截断 (B-R1, 锁
`capped_search_hits_refuse_export`, 闸在入口) ⑦稀疏末行无行尾不补 + BOM 对齐全集
(B-R2, 锁 `sparse_keeps_bare_last_line_without_added_ending` + `sparse_preserves_
utf8_bom_like_full_copy`) ⑧UTF-8 源直 parse 原字节, lossy 不得把失败行洗成合法
JSON (B-R3, 锁 `invalid_utf8_line_stays_raw_bad_not_washed` + `parse_source_...`)。

**Optional 裁决**: 进度 u128 溢出修 / 取消前先 poll (双态拧巴) 修 / 过滤在途闸修
(锁 `pending_filter_blocks_export_entry`) / pretty 连续失败行**同样**空行分隔
(口径写死进实现记) / 0 命中=合法空交付 (文档记录) / `export_line_set` 等 110 行
迁 export.rs **留给 code-simplify** / 行尾采样偏差与覆写透映射入已知局限 /
settings 两卡壳重复不动 (卡体已共享) / visit_rows 全集不收束 line_count 不动 (契约注)。

**Nits 修**: idx 魔法数 → `ExportPick` 枚举 / pretty `expect` → 失败行兜底 /
`drop(w)` 先于 rename/remove / launch 撞单作业不再静默。

**评审过程事故（自报）**: 修复中把在途闸从 `begin_export` 挪到入口后, `capped_
search_hits` 测试仍直调 `begin_export` —— **穿到真保存对话框** (套件 34s), 违反
「测试严禁真实桌面副作用」家法。当轮改正为走入口消息, 测试注释写明此坑。

**A/B 精确红记录**: 取消保旧文件 (修前红 = 旧文件消失) / search.rs 代次拒覆盖
(修前红 = 新结果丢) / 稀疏末行 (修前红 = 多出 CRLF) / 公式中和 (修前红 = 裸
`=HYPERLINK`) / UTF-8 坏字节 (修前红 = U+FFFD 洗过且 bad=0)。夹具教训: 坏字节
必须藏在编码检测采样窗 (64 KiB) 之后, 否则检出为 GBK 测不到目标路径。

## 简化记（2026-09-23 code-simplify, 行为零变化, 314 绿不破 —— 既有测试零改动）

1. **`outcome_of`** —— raw/pretty/csv 三处同一份「completed→Done/Cancelled」收尾
   判定收成一函数（概念命名, 三处 5 行 if-else 各消失）。
2. **`ExportFormat::write`** —— 格式分派的三臂 match 曾在作业体与 logbench
   **各抄一份**; 收成方法后两边只剩一行调用（消灭第二条构造线, 与 D9「数字与
   真实路径同一份代码」同哲学）。
3. **`export::now_stamp`** —— `SystemTime` 取值移进 export（UTC 决策归 D8 所有者）,
   main 调用点 8 行 → 1 行。
4. **评审 defer 落地: D1/命名件迁 `export.rs`** —— `line_set_of` / `scope_suffix` /
   `ext_for` 自 main 迁入; **原 main 的同形双分支 match（JSONL/明文）折叠成一处**
   「模式取源」（重复条件=缺模型的信号, 折叠后可读性反升）; main 只剩备料三行。
   补纯函数锁 `line_set_of_picks_source_by_mode` + `scope_and_ext_follow_d1_d8_matrix`
   （取源优先级/命名矩阵边角）; main 既有四态/扩展名测试**原样保留**作适配层人证。

**通读后判定不动**（不为动而动）: `ExportJob` 状态机（评审刚加固, 概念数已最低）/
`csv_escape`·`neutralize_formula`·`detect_line_end`·`civil_from_days`（纯函数已对拍）/
`write_full` 的 `projected` 闭包（u128 注释即意图）/ view 底栏绘制块（自绘几何家法）/
settings 两卡壳（卡体已共享, 4 行包装不值得再抽）/ `WriteOutcome` vs `ExportEnd`
两枚举（writer 层与作业层职责真不同）。

## 人工验收

（用户实机后回填）
