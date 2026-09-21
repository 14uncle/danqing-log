# SPEC-v1x-field-analytics: 字段分析（腿二）

- @author 十四叔
- @date 2026/09/19
- 状态: review 完成（2026-09-19, 6 条 Required 全修, 250 测试绿）, 待 code-simplify
- 所属: 能力地图 `SPEC-v1x-map.md` 模块 `field-analytics`（构建顺序第 2 位，依赖 `licensing` 的门控机制）

## Objective

把「483 万行 235 ms 的字段提取能力」从「找那一行」升级成「回答分布问题」——
运维问「昨天 p99 多少」「ERROR 都是什么 status」，现在得写脚本或导出到别处才能答。
本模块让这类问题在查看器里一次点击出结果。

这是付费层第一条功能腿（收银台 `licensing` 已落地），也是地图上唯一
「没被变现的资产」的变现（ROADMAP §二）。竞品处境：LogViewPlus 全量解析
1 GB 冷启动实测 60 秒、klogg 无结构化——缝隙仍开着。

**成功长什么样**：表格模式下，侧栏选字段 → 点「分析」→ 后台扫一遍 →
数值列出 count/min/max/mean/p50/p95/p99，枚举列出 Top 20 取值分布；
1 GiB JSONL 单列 ≤ 1.5 s（热缓存）；免费用户点「分析」弹统一升级提示。

## 范围

**In**：

1. **引擎前置**（`danqing-logfile` 联动，本模块第一任务）：顶层字段扫描器
   —— 单遍状态机（in-string / escape / 花括号深度），产出**类型化** token
   （数值 / 字符串 / bool / null），零 `Value` 树。取代 memmem 切取在分析
   路径的地位：聚合对每行取值、**没有过滤那种「粗筛+serde 验证」两段可省**，
   边界错误（POC 已知的 `,"key":"` 内嵌误判）会被逐行放大——ROADMAP 标注的
   前置就是这条。正确性用**与 serde_json 全行 parse 的差分对拍**锁住
2. **分析器**（本仓 lib 纯逻辑）：单列聚合
   - 数值列：count / min / max / mean 流式精确 + p50/p95/p99（reservoir 采样，
     上限 100 万值 ≈ 8 MB；超限在结果上标注「采样估计」）
   - 枚举列：取值计数 Top 20 +「其他」桶（distinct 上限 1 万，超出并入其他并标注）；
     bool / null 入枚举
   - 混合类型列：采样投票定多数类型，按多数类型分析，注明跳过 N 行
3. **UI（侧栏扩展，2026-09-19 用户裁定）**：直方图侧栏下方「字段分析」区——
   字段下拉（候选 = schema 的列）+「分析」按钮 + 结果区（数值 = 统计表；
   枚举 = 计数条，复用对数横条画法）；结果显示其作用域（行数 + 生效过滤串），
   过滤变更后**不自动重跑**，结果旁标「基于旧过滤 · 重跑」
4. **付费门控**：点位 = 「分析」按钮——免费态弹统一升级提示
   （`Feature::FieldAnalytics`，走 licensing 模块的 `ShowUpgradePrompt`）
5. **测量通路**：`logbench --analyze <field>`（性能数字的弹药来源，与既有
   实测表同口径）

**Out**：

- 嵌套字段分析（子行不是列；schema 只管顶层扁平列）
- 多列联合 / 时间序列分布（「随时间的错误率」是另一个价值点，另行立项）
- 分析结果导出（归腿三 `export` spec 议）
- 时间戳列类型系统（ROADMAP §一欠账表另项，别混进来）
- 分析结果缓存（每次点重算——够便宜，不造缓存正确性负担）

## 设计决策

### D1: 引擎前置独立成第一任务，先收边界再谈聚合

`scan_field`（`danqing-logfile`）：单遍、零分配、行内状态机。判据 =
**与 serde_json 逐行 parse 的差分全等**（采样行集 + 对抗样本：`,"level":"`
内嵌于字符串值、嵌套对象同名 key、转义引号、`\uXXXX`、科学计数法、负数、
行尾缺逗号）。聚合器只消费 `scan_field`，不许再碰 memmem 切取。

### D2: 分位数用 reservoir 采样，标注诚实

count/min/max/mean 流式精确（无内存增长）；p50/p95/p99 用 Vitter reservoir
（上限 100 万值）。超限标注「采样估计（基于前 N/总 M 行）」——不假装精确。
不用 t-digest：不引新依赖，reservoir 几十行自实现即可，采样语义还更好解释。

### D3: 作用域跟随当前过滤（2026-09-19 用户裁定）

有过滤 → 分析过滤后的行集（「ERROR 请求的 duration p99」这种真实问法）；
无过滤 → 全文件顺序扫。行集本就是内存中的 `Vec<u64>`，按行号取行
（随机访问 0.58 µs/行），比全扫还快。结果区必须显示作用域行数与过滤串——
分布数字不说作用域就是耍流氓。

### D4: UI 落侧栏（2026-09-19 用户裁定）

直方图侧栏下方加「字段分析」区，不加新窗口/模态概念。侧栏现有
`Ctrl+L` 显隐、`仅统计·不可点选` 只读提示的模式沿用；字段分析区在
原始模式（.log 无 schema）不显示——没有列可分析，不是灰掉。

### D5: 门控点位 = 「分析」按钮

免费态点「分析」→ `ShowUpgradePrompt(Feature::FieldAnalytics)`（licensing
已建的统一提示）；下拉选字段本身不拦（看得见、摸不着 = 免费层的橱窗，
与 LogViewPlus 的 trial 橱窗同策略）。付费态直跑。

### D6: 类型判定 = 目标行集前 100 行采样投票

全部可解析为数值 → 数值列；否则枚举列；混合列按多数类型分析并在结果
注明「跳过 N 行非数值」。判定采样只跑一遍，不二次扫文件。

### D7: 性能目标与测量

1 GiB JSONL、单列、热缓存：**≤ 1.5 s**（参考锚：行口径计数 94 ms、
memmem 字段口径 24 ms——状态机比 memmem 慢一个量级是可预期代价；
serde 全行 parse 同文件要秒级到十几秒，列发现事故在档）。
测量走 `logbench --analyze <field>`，数字进 `PERFORMANCE_REPORT.md`。
若状态机实测超目标，备选优化 = memmem 先定位候选位置再局部验证
（粗筛思路的倒置），build 阶段凭数据定，不预支。

### D8: 结果不缓存，过期要标注

每次点「分析」重算；过滤/文件变更后旧结果留着但标「基于旧过滤 · 重跑」，
不静默作废（用户可能正在读它）。换文件 = 整个区清空（文件语境没了）。

## 成功判据

机器可测：

- 差分对拍：`scan_field` 与 serde_json 在采样行集 + 对抗样本上**逐值全等**
  （含类型判定）；嵌套同名 key 取的是**顶层**那个
- 数值统计小文件手算全等；reservoir 超限 → 结果带「采样估计」标注；
  枚举 distinct 超限 → 「其他」桶 + 标注
- 作用域：有过滤时结果行数 == 过滤行集大小；无过滤 == 文件行数
- 门控：免费态点分析 → 升级提示出现且无扫描发生；付费态出结果
- 性能：`logbench --analyze duration_ms` 1 GiB JSONL ≤ 1.5 s（热缓存）
- 现有 231 测试全绿不破

人工验收（用户实机）：demo-1gb.jsonl 上对 `duration_ms` / `status` 各跑一次，
看速度体感与结果可读性；免费态弹窗文案过一遍。

## 已知局限

- 只对顶层扁平列；混合类型列按多数类型（少数派行被跳过，注明）
- reservoir 分位数是采样估计（超限才标注，未超 = 精确）
- 枚举取值显示截断（32 字符，同列宽惯例）
- 分析中单列单行成本行宽有界但非零——宽行文件（563 KiB/行那种形状）
  会比普通日志慢，目标值按 demo 文件形状定

## Boundaries

- Always：三件套（fmt + clippy -D warnings + test 全绿）；中文注释；新文件头
  `@author/@date`；测试无真实桌面副作用；报错回喂只贴失败项
- Ask first：`danqing-logfile` 的引擎改动（本次预期内：scan_field 是**新增**，
  不改既有提取/过滤行为——联动链路：兄弟仓先 push → 本仓复钉 lock）
- Never：memmem 切取进分析路径（D1）；聚合结果不写盘；为分析新建缓存层

## Open Questions

- [ ] reservoir 上限 100 万值——build 时若实测内存/速度宽裕可上调，标注格式随定
- [ ] 侧栏结果区高度与直方图的空间分配（量了再写，09-13 教训：别估算留余量）
- [ ] 枚举 Top 20 的 20——build 时按侧栏宽度实测定（窄侧栏可能 12 更读得动）

## 实现记（2026-09-19 build 收口, 与上文分叉处回本）

- **「字段下拉」改「字段行逐行可点」**（D4 措辞分叉）：下拉是建树时冻结的框架控件,
  而 schema 开文件后才有; 逐行可点与直方图桶行同构 (hover/命中同一套手法)。
  点字段行即分析 —— 字段行就是「分析」按钮 (D5 门控点位语义不变)
- **差分对拍抓到真差异**: serde_json 的浮点解析与 `str::parse` 可差 **1 ulp**
  (它有自己的浮点通路) —— 扫描器的合同是**字节保真**, 对拍收口为整数字面量全等 +
  浮点相对 1e-15 容差
- **实测**（1 GiB 热缓存, release, demo-1gb.jsonl）: `duration_ms` 全文件 **1001ms**
  （≤1.5s 目标过）/ `status` 1017ms / 跟随过滤（36,123 行）**124ms**（分位数精确）;
  402 万行超 reservoir 上限, 「采样估计」标注如实出现。数字已进 PERFORMANCE_REPORT.md
- demo 文件的 `status` 是**数值列**（200/500/502）——枚举通路靠单测覆盖, 人工验收
  看枚举效果请用 `level`
- 引擎坑新形态: patch 态下兄弟仓加了新模块, clippy 报「找不到 scan」——
  `cargo clean -p danqing-logfile` 即解（陈旧 rmeta 指纹; patch/lock 陷阱家族新成员）
- 混合类型测试的 fixtures 第一次写反了（2/5 不是过半）——修的是测试不是逻辑
- 测试规模: 本仓 lib 86 / main 145 / genlog 8 / keygen 3 = **242**;
  danqing-logfile 68（含 scan 10 条, 差分对拍 3000 行 × 5 字段）

## 评审记（2026-09-19, 单路深度评审: REQUEST CHANGES, 无 Critical, 6 Required 全修）

引擎侧（scan.rs / 聚合算法）原样通过——评审逐条构造了对拍语料盲区形态
（转义的转义 / 嵌套数组同名键 / 数字贴 `}` / 行尾 `\r`）全部免疫正确；
reservoir 为教科书 Algorithm R，独立复跑实测数字与实现记吻合。包袱集中在
面板层与两处聚合语义：

1. **R1 数值结果漏算采样标注行**（`result_rows` 报 11 行、paint 画 12 行）——
   「分位数为采样估计」在旗舰路径（超 reservoir 上限）上必被窗口底边裁掉。
   修：`7 + usize::from(s.sampled)` + 行账目守卫测试（数值±采样 / 枚举±其他
   四态钉住）。
2. **R2 字段行无 hover 反馈**（只在按下瞬间置位，模块头宣称「与直方图桶行
   同构」不属实；且按下拖出抬起残留高亮）——09-13 人工验收那条教训的原样
   复发。修：hover 改光标驱动（CursorMoved 仅可点行留痕 / CursorLeft 清空，
   直方图同款），点击判定改用独立的 `pressed` 锚点，可点性单一事实源
   `row_clickable()`。
3. **R3 枚举 distinct 超限行被丢弃而非并入「其他」**——高基数列（request_id
   类）分布数字凭空少掉 99% 的行，且与 spec「超出并入其他并标注」、
   `EnumStats::capped` 自身文档两边都冲突，还没进实现记。修：overflow 计数
   并入 others；连带把 `capped || others > 0` 改回 `capped`（两语义曾被压成
   一位，面板「其他（N 行）」成死代码）。两条测试锁语义分叉（21 distinct
   不 capped / 10037 distinct 行数守恒 top+others==total）。
4. **R4 结果视图文本无截断无裁剪**——spec 已知局限写明的「32 字符截断」
   没实现；112px 侧栏上作用域行（stale+skipped 组合 ≈200px）、长字段名、
   长取值全部溢出盖画到 LogView。修：measure 截断 + 省略号（`fit()` 助手，
   枚举取值先截 32 字符再按宽截）为主防，`push_clip/pop_clip` 对为兜底。
5. **R5 分析快照的过滤串与行集不同源**——`apply_filter` 发起即写
   `filter_applied`，行集要等 job 拾取才换；窗口内点字段行 = 跑旧行集记
   新串，stale 永 false。修：新增 `filter_landed`（产出当前行集的那一串）
   + `filter_pending`（在途闸），分析快照读落账串；同族两洞顺手收——
   clear_filter / 空查询回全量现在 `invalidate()` 在途作业（原先晚到结果
   会复活贴回），live-tail 增量合并在在途窗口禁行。
6. **R6 选择器高度无上限**——30-50 列的宽 schema（可观测性数据常态）把
   直方图挤到零高、底部字段不可达。修：字段行封顶 16 + 「… 还有 N 列」
   指示行（不可点），spec Open Question「侧栏空间分配」就此落地。

Optional 未修（记录在案）：AsyncJob done 槽同帧双完成覆写（窗口极窄、自愈）；
`AnalysisBack` 不作废在途作业（结果落地拽回结果视图）；logbench 枚举输出不
显示 capped；D3 字面「结果区显示过滤串」未实现（精神达成：底栏常驻过滤串）；
对拍语料可补数组套对象/非 ASCII 键名/ryu 指数三条；`columns_src` Arc 地址
缓存靠隐式不变式防 ABA（脆弱非错误）。Nit 已顺手修：scan 测试条数 11→10、
todo T4 旧设计文字。

修复后基线: 本仓 lib 88 / main 151 / genlog 8 / keygen 3 = **250**，
clippy -D warnings 零警告，fmt 净。licensing 修复 delta 复核三条全过
（剪辑键放行无新洞 / 购买防重入配对完整 / 长度闸与 Debug 遮蔽到位）。

## 简化记（2026-09-19 code-simplify, 行为零变化, 250 绿不破）

模块本身刚出评审即进简化，可收的不多，四处：

1. `detect_column_type` 删掉写而不读的 `other` 计数器（`seen` 已含），
   顺手修评审 Optional 的注释超 claim：「过半」→「≥ 一半（平票归数值）」
2. `analysis_panel` paint 里 `bar_full_w = avail_w` 别名残留（R4 修复碎屑）合一
3. licensing 侧两处：`map_store_snapshot` 两个同造 `Paid` 的 arm 合并；
   `store_license.rs` 的 COM+StoreContext 起手式提为 `store_context()` 共用
4. 判定**不动**的：scan.rs（对拍锁着的字节合同，越素越好）、keygen.rs、
   settings.rs 许可页、logbench —— 通读后无可简化项，不为动而动

## 人工验收（2026-09-20 用户实机, 三轮过）

验收四条发现, 逐条修完 → 复验通过。**两条是本模块自己的问题, 两条是
「腿二改造把 v1.0 的东西碰坏了」**:

1. **字段行文字在 hover 块内不居中** —— 面板所有行文本顶对齐
   (baseline = 行顶 + ascent), 而 hover 块与行矩形同心 → 读作「按钮内偏下」。
   修: `vcenter_base()` 文本行盒行内居中 (直方图「清除筛选」行同款教训,
   连参照物注释一并写明); 枚举行的文本区再避开底部计数条。
2. **许可页粘贴长 key 渲染溢出框外** —— 框架 `TextInput::paint` 从不裁剪,
   262 字符单行平铺越界 (过滤/搜索栏是同一颗未爆的雷)。修在**框架**
   (danqing 联动): 选区/正文/占位/preedit/光标一律裁进边框内侧, 回归锁
   `overflowing_text_is_clipped_inside_input_area`。附带 `#[doc(hidden)]`
   字形观测面 `TextBatch::glyph_clips` (与 `text_input::text_color` 同处置)。
3. **侧栏直方图整块消失**（最重）—— 根因**不在本模块的预算**, 而在腿二把
   直方图从 `Row` 的 Fit 子项挪进 `Column` 的 fill 位置: `effective_width`
   的折叠判据是「整个 Row 的可用宽 ≥ 640」(窄窗自动收起), 而列的子项拿到的
   是 cross_max = 侧栏自己的 112 → `112 >= 640` 不成立 → **宽度归零、整块不画**。
   实机表现: 侧栏只剩字段分析区, 而枚举结果 (INFO/DEBUG/… 计数与直方图同源)
   看着像直方图却没有颜色与 hover。
   修: 新增 `src/sidebar.rs` 容器 —— 折叠判定必须在**拿得到整个 Row 宽的那一层**
   做一次 (容器是 Row 子项 → 判一次 → 给内部 Column 钉 tight 宽 → 子组件
   一律「拿来即用」)。删掉直方图里已成死状态的 `visible` 字段。回归锁
   `sidebar_keeps_a_real_width_so_the_histogram_actually_paints` **真画一遍**
   并断言直方图产出字形 (此前的测试从没画过侧栏, 正是漏网原因)。
   附: 面板结果态自然高在矮窗上仍会把直方图挤到零高 —— 高度预算
   (`HEIGHT_BUDGET_FRAC = 0.55`, 装不下折叠为「… 还有 K 条取值」) 同期落地。
4. **暗色主题下许可页占位文本不可辨** —— 许可框占位色传的是亮主题的
   `text_secondary` (深灰), 而框架 `bind_theme` **有意不刷新占位色** →
   暗底暗字。修: 改用与过滤/搜索栏同款的中性灰 `rgb(0.45,0.45,0.48)`, 明暗通吃。
