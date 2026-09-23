# SPEC-v1x-field-picker-ui: 免语法字段查询 UI（免费层欠账）

- @author 十四叔
- @date 2026/09/23
- 状态: **spec 草案待批准**（2026-09-23 起草; 范围三项推荐 + T0 前置修复随「go」
  一并裁定）—— 批准后进 plan
- 所属: 能力地图 `SPEC-v1x-map.md` 模块 `field-picker-ui`（构建顺序第 6 位，接
  `bookmark-persist` 之后；免费层欠账三连**末件**；依赖: **无**——不接 licensing
  门控，免费层白送）

## Objective

过滤迷你语法（`level=ERROR`、`user.id=42*`、`status>=500`）对没读过文档的人是
一堵墙——字段名要打对、算符要记住、嵌套点路径易打错。ROADMAP §一欠账表字面项：
**免语法字段查询 UI（列发现采样结果做成下拉点选）**。成功长什么样：不记得语法也
能查——点字段、点算符、只打值，点「过滤」直接出结果；免费层白送。

**附带 T0 前置修复**（开工前事实盘点揪出，见 D2）：`table-column-config` 的
「显示列」弹层行集被**启动快照冻结**（框架 `view()` 一次性建树不再重建，建树时
schema 为 None）——结构上永远只有「恢复默认」一行，人工验收 A3 面。本模块需要的
「动态行列表」正是它的修法，T0 先修 col_menu 并共享组件。

## 范围（三项推荐 + T0，随「go」一并裁定）

1. **形态 = 过滤栏「字段…」按钮 + 弹层表单**（推荐①）：弹层 = 字段行逐个可点
   + 算符行可点（`=` / 前缀 `*` / `>=` / `<=` / `>` / `<`）+ **值输入框**（弹层内
   TextInput，唯一要打的）+ 「过滤」按钮 → 拼 `字段<op>值` 提交。免掉字段名、
   算符、连接符的全部语法记忆。备选「点选往主过滤栏插文本」被否——框架
   `TextInput` 无编程插入 API（只有 rev 清空），要走联动不值。
2. **追加语义 = 空格连接（AND）+ 立即应用**（推荐②）：新子句追加到当前已应用
   查询之后（空查询 = 就是它），走 `Msg::ApplyFilter` 全链（异步不冻界面、
   Esc 清、状态栏计数全复用）；与手输 Enter 同一条真相路（`parse_filter` 唯一
   解析入口）。空值拒绝 + 说清（`字段=` 空匹配非用户本意）。
3. **免费层白送**（推荐③ = D7 同款）：不接 `Feature` 门控，地图定死。
4. **T0 前置修复搭车**：自绘**动态行列表**组件（框架树冻结的正解，AnalysisPanel
   先例）——先修 col_menu 行集（回归锁伺候），本模块复用同一组件画字段行。

**In**:

1. 过滤栏右端「字段…」按钮（表格模式 + `schema.is_some()` 才显示; 「列…」按钮
   先例）
2. 弹层表单：字段行（schema 列名逐行可点，当前选中高亮）+ 算符行（6 选一，
   默认 `=`）+ 值输入框（Enter 或「过滤」提交）+ 提交即 `ApplyFilter`
3. 追加 AND 语义; 空值拒绝 + notice
4. T0: col_menu 行集动态化（启动快照 bug 修复）+ 共享行列表组件 + 回归锁
5. 弹层范式全套：四件套（`Overlay + bind_open + on_scrim_click + 行按钮`）、
   Esc 次序插入（`upgrade > settings > 本弹层 > col_menu > export_menu > 栏`）、
   模态守卫、与 col_menu/export_menu 双向互斥

**Out**:

- **值列表点选**——无值发现基建（`discover_schema` 只出列名），值域是
  field-analytics 顶值的付费面，不免费搭车; 实机喊累另起
- **主过滤栏文本注入**——`TextInput` 无插入 API，联动不值（推荐①的备选已否）
- 新语法/新算符（`!=`、正则、OR）——语法面不动，只做既有 6 算符的点选壳
- Bare（全文）子句的点选生成——弹层只产 Field 子句，全文搜索仍走主栏手输
- 表头右键入口——已被列管理占（table-column-config D3）
- SHORTCUT_KEYS 变更（按钮够用）

## 开工前事实盘点要点（2026-09-23 调研）

- **框架生命周期铁律**（本模块第一约束，六处框架文档同文）：`let tree =
  app.view();` 之后整棵交给 Handler **不再重建**（`window/mod.rs:207` 启动一次 +
  `handler.rs:706` 每帧只 `tree.sync(app)`）。动态内容两条活路：①绑定闭包刷值
  （`Text::bind`/`bind_selected`/`bind_clear`——**结构固定值可动**）②**自绘组件**
  （`Widget::sync` 重建数据缓存 + `paint` 动态画行 + `event` 命中——
  `AnalysisPanel`/`LogView`/`VersionRow` 三先例）。`Dropdown::new(options)` 的
  选项表建树冻结（主题这类静态选项才合适）; export 两卡 = **双 overlay 变体**
  先例（结构差异靠建树时全建 + `bind_open` 谓词切换）。
- **T0 bug 由此定谳**：`col_menu_card` 的列行在 `view()` 建树时用
  `schema.as_deref().cloned()` 快照构造（`main.rs:2047-2050`）——启动后开文件的
  schema 永远进不去（最坏: 开箱即缺行; 最好: 换文件后不更新），弹层只剩
  「恢复默认」。table-column-config 双路评审与机器锁都没触到框架生命周期面
  （锁的是纯函数/Msg/paint, 无「建树后 schema 变化行跟随」锁）——T0 补上。
- **过滤语法现状**（`danqing-logfile/src/jsonl.rs`）：`parse_query` =
  `split_whitespace` → 每 token `parse_clause` → `Clause::Field{path,op,value}` /
  `Clause::Bare`; 算符集 `>=`/`<=`/`>`/`<`/`=`（长先匹配），`=` + 值尾 `*` =
  前缀通配; 键名规范化收在 `LogApp::parse_filter`（`main.rs:1678`，全应用唯一
  解析入口）。**多子句 = 空格连接 AND**。
- **提交链**（`main.rs`）：`Msg::ApplyFilter(q)` → `apply_filter`（AsyncJob
  1GB 亚秒）/ 空查询回全量 / Esc 清 / `filter_clear_rev` + `TextInput::bind_clear`
  （`view.rs:2906` `Bar::bind_clear_filter`）= rev 式注入先例——但只有**清空**，
  无 set/insert 文本 API（`text_input.rs` 全 API 清单核过）。
- **字段发现**：`discover_schema`（`jsonl.rs:144`）→ `Column{name, width_chars}`
  ——**只有列名 + 采样宽，无值采样**; `MAX_COLUMNS=16` 首见序。
- **analysis picker 先例**（`analysis_panel.rs`）：schema 列名逐行可点（自绘,
  hover/命中/`MAX_PICKER_FIELDS` 封顶 + 「还有 N 列」行）→ `Msg::AnalyzeField`
  门控在 main——本模块字段行与它同构（语义换成拼查询子句）。
- **弹层/控件面**：四件套（export/col_menu 两轮验证）/ `format_btn`（`impl Fn()
  -> Msg`）/ `Dropdown`（自管展开/键盘/点外收）/ 弹层内 `TextInput` 先例（许可
  键输入, `settings.rs:246`）。测试范式: 合成几何注入 / 真 paint 锁（非零平移
  原点）/ `new_empty_at` 注入 / 逐测唯一临时文件。

## 设计决策

### D1: 免语法交互 = 弹层表单（字段/算符点选 + 值手输），零主栏注入

「字段…」按钮开弹层；点字段行（选中高亮）→ 点算符（默认 `=`）→ 值输入框打值 →
「过滤」（或值输入框 Enter）= 拼 `字段<op>值`（前缀算符 = `字段=值*`）→ 追加到
当前查询（空格连接）→ `Msg::ApplyFilter`。弹层关闭，主栏/状态栏显示已应用查询
（既有真相路）。**空值拒绝** + notice（说清为什么）。Esc/scrim 关弹层不提交。

### D2: 动态行列表 = 自绘组件（T0 共享），框架树冻结的唯一正解

新自绘小件（建议 `src/pick_list.rs` 或并入 `settings.rs`, plan 定）：`sync` 从
LogApp 缓存行文案与状态（Vec<String> + 选中索引）, `paint` 画行 + hover, `event`
合成几何命中 → 发闭包构造的 Msg。col_menu 行集与本模块字段行共用（行文案/选中
态/点击 Msg 参数化）；**T0 先修 col_menu**（回归锁: 建树后 schema 就位/换文件,
弹层行跟随——正是漏掉的那把锁），本模块复用。≤16 行封顶 + 「还有 N 列」行
（`MAX_PICKER_FIELDS` 先例）。

### D3: 追加 AND + 立即应用 + 空值拒绝

新子句追加到 `filter_applied` 之后空格连接（空查询 = 直接是它）; 与手输同走
`ApplyFilter`（解析/规范化/异步/计数/Esc 全复用, `parse_filter` 唯一入口不破）。
空值 = 拒绝 + notice（`字段=` 空匹配非本意; 也防手滑点「过滤」清掉整个查询——
空查询 = 回全量的既有语义太重）。

### D4: 免费层白送，不接 `Feature` 门控

地图定死（依赖列为空）。与列配置/书签同理显式写死：免语法查询接付费墙 =
「扣走我本来就该有的东西」。

## 成功判据

**机器可验**：

1. 拼子句纯函数：字段+算符+值 → `字段<op>值`（6 算符各一测, 前缀 = 值尾 `*`）;
   空值/空字段拒绝
2. 追加语义：空查询直提 / 有查询空格连接（AND 合成语义经 `parse_query` roundtrip
   锁）; 提交走 `ApplyFilter` 全链（既有过滤锁零回退）
3. **T0 回归锁（真 paint）**：建树后 schema 就位/换文件 → col_menu 行**跟随**
   （启动快照 bug 的精确红: 摘 sync 缓存重建 = 红）; 点行发 `ToggleColumn`
   （合成几何注入）
4. 字段行同构锁：schema 变化行跟随 / 点行发拼接请求 / ≤16 封顶 +「还有 N 列」
5. 弹层接线：开闭 / Esc 次序（含与 col_menu/export_menu 互斥）/ 模态守卫 /
   scrim 关闭; 空值拒绝 + notice
6. `.log`（schema None）不出「字段…」按钮（`schema.is_some()` 判据, export
   同哲学）
7. 三件套全绿，**基线 376 不破**; A/B 精确红（T0 sync 缓存重建 / 拼子句 两处必录）

**人工验收**（记账 → `tasks/acceptance-pending.md` 续 E 组）：

1. JSONL 开「字段…」→ 点字段/算符/打值/过滤 → 命中正确、状态栏计数如常
2. 再点一子句追加（AND）→ 交集正确; Esc 清回全量
3. 空值点「过滤」有提示且查询不动; Esc/scrim 关弹层不提交
4. `.log` 无「字段…」按钮; 免费态直接可用（无门控）
5. **T0**: 「列…」弹层行随文件换（开 A 看 A 的列、换 B 看 B 的列）

## 已知局限

- 值只能手输（无值发现基建, 值域是 field-analytics 付费面——Out 定死）
- 只产 Field 子句; 全文（Bare）子句仍手输
- 语法面不动：无 `!=`/OR/正则（点选壳不造新语法）
- 弹层内值输入框无历史/补全

## Boundaries

- 注释/文档中文；**零新依赖**; 框架/引擎改动 = **零**（自绘组件是产品侧既有手法）
- 五段流水线：spec 批准 → plan → build → review → code-simplify
- 未获用户指示不 commit/push
- 不动过滤语法/`parse_filter` 唯一入口/SHORTCUT_KEYS; 不接 `Feature` 门控

## Open Questions

1. **算符行常显 vs 点字段后才显**：推荐**常显**（一行小按钮, 一眼看全语法面,
   少一次状态机）——随「go」可裁
2. **T0 修复的组件归属**：推荐独立小件（`pick_list` 级）供两处复用, plan 定形
   （并入 `settings.rs` 也可, 视体量）——plan 裁
3. 弹层卡宽/形制照 col_menu（`CARD_WIDTH`）即可, 不另设——非问题, 记档

## 实现记

（build 阶段回填）

## 评审记

（review 阶段回填）

## 简化记

（code-simplify 阶段回填）

## 人工验收

（记账 → `tasks/acceptance-pending.md`，实机后回填）
