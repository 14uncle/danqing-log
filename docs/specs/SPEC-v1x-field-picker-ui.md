# SPEC-v1x-field-picker-ui: 免语法字段查询 UI（免费层欠账）

- @author 十四叔
- @date 2026/09/23
- 状态: **五段全闭**（2026-09-23 一日: 「go」→plan→/build auto→
  双路评审并账 Critical×1+Required×6 全修, 393 测试绿, 含 T0 col_menu 启动快照
  修复; 2026-09-24 code-simplify 行为零变化收口, 393 测试零修改; 零框架/引擎改动）;
  **人工验收（五条）记账**（总清单 E 组）
- 所属: 能力地图 `SPEC-v1x-map.md` 模块 `field-picker-ui`（构建顺序第 6 位，接
  `bookmark-persist` 之后；~~免费层欠账三连**末件**~~ **2026-09-27 翻案改判付费层**
  （地图翻案块②；本功能只存在于 JSONL 语境——.log 无此按钮，免费层手输迷你语法过滤不动；
  功能本体零返工，接门待做与腿一同窗口）；依赖: **无**——~~不接 licensing
  门控，免费层白送~~ 接门后接 licensing）

## Objective

过滤迷你语法（`level=ERROR`、`user.id=42*`、`status>=500`）对没读过文档的人是
一堵墙——字段名要打对、算符要记住、嵌套点路径易打错。ROADMAP §一欠账表字面项：
**免语法字段查询 UI（列发现采样结果做成下拉点选）**。成功长什么样：不记得语法也
能查——点字段、点算符、只打值，点「过滤」直接出结果；免费层白送。

**附带 T0 前置修复**（开工前事实盘点揪出，见 D2）：`table-column-config` 的
「显示列」弹层行集被**启动快照冻结**（框架 `view()` 一次性建树不再重建，建树时
schema 为 None）——结构上永远只有「恢复默认」一行，人工验收 A3 面。本模块需要的
「动态行列表」正是它的修法，T0 先修 col_menu 并共享组件。

## 范围（2026-09-23 用户「go」裁定，全按推荐项）

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

**机器可验**（2026-09-23 build 收口已全过）：

1. ✅ 拼子句纯函数：字段+算符+值 → `字段<op>值`（6 算符各一测, 前缀 = 值尾 `*`）;
   空值/空字段/含空白值拒绝
2. ✅ 追加语义：空查询直提 / 有查询空格连接（`parse_query` roundtrip 锁）;
   提交走 `ApplyFilter` 全链（既有过滤锁零回退）
3. ✅ **T0 回归锁（真 paint）**：建树后 schema 就位/换文件 → col_menu 行**跟随**
   （摘 sync 缓存重建 = 精确红）; 点行发 `ToggleColumn`（合成几何注入）
4. ✅ 字段行同构锁：schema 变化行跟随 / 点行发拼接请求 / ≤16 封顶 +「还有 N 列」
   （尾行不可点）
5. ✅ 弹层接线：开闭 / Esc 次序（含与 col_menu/export_menu 互斥）/ 模态守卫 /
   scrim 关闭 / Enter 与「过滤」钮同路; 空值拒绝 + notice; 关闭清草稿
6. ✅ `.log`（schema None）不出「字段…」按钮（真 paint 显示判据锁）
7. ✅ 三件套全绿，**基线 376 不破**（收口 388）; A/B 精确红两处在案（见实现记）

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

1. ~~**算符行常显 vs 点字段后才显**~~ **已裁（2026-09-23 「go」）：常显** ——
   一行小按钮，一眼看全语法面
2. ~~**T0 修复的组件归属**~~ **已裁（2026-09-23 「go」→ plan 落形）：独立小件
   `src/pick_list.rs`** —— 供 col_menu 与本模块字段行两处复用
3. 弹层卡宽/形制照 col_menu（`CARD_WIDTH`）即可, 不另设——非问题, 记档

## 实现记（2026-09-23 build 收口 T0–T2, 388 测试绿）

- **T0**（`src/pick_list.rs` 新件 + `settings.rs` 换挂）: `RowList` 自绘行列表件
  （`rows_fn`/`highlight_fn`/`on_pick` 闭包契约 + `more_fn` 尾行, ≤`max_rows`
  封顶; sync 每帧取态, paint 行/hover/高亮, event 按下抬起同行为触发——
  AnalysisPanel 同款锚点纪律）; `col_menu_rows()` 换挂 RowList, **快照参数整个
  删除**（`col_menu_overlay(theme)` 单参——参数即病灶）; `col_menu_row_names`
  签名改 `Option<&Schema>` 免逐帧 clone。判罪锁
  `col_menu_rows_follow_schema_across_sync`（摘 sync 行缓存重建 = `0.0≠84.0`
  精确红）。
- **T1**（`main.rs`）: `build_clause`（6 算符, 前缀 = `=`+值尾 `*`, 空值/空字段/
  含空白值拒绝）+ picker 三态 + 6 个 Msg 臂; `PickerSubmit` = 组装 → `filter_applied`
  空格追加 → `apply_filter`（唯一真相路）; 互斥开一关二。A/B 红: 摘追加拼接 →
  `"level=ER*"≠"level=ERROR level=ER*"` 精确红。
- **T2**: 「字段…」按钮走 Bar **hint 同款**「先测后存同帧让位」（`fields_reserved`
  进 `input_area` 单点收口, hit rect 同源缓存; `has_schema` 判据——`.log` 不出）;
  查询卡 = 字段行 RowList + 算符六钮（静态结构, 选中色 `bind_color` 每帧取）+
  值输入 + 「过滤」钮; Esc 插层 `upgrade > settings > picker > col_menu >
  export_menu > 栏` + 模态守卫补 `picker_open`（剪辑键放行先例）+ Enter 提交
  （`app_key_filter` 拦截——TextInput 不消费 Enter 已核）+ 换文件关弹层 + 关闭
  清草稿（`picker_clear_rev` + `bind_clear`）。
- **plan 偏差记录**（核实②的修法换代）: 原设计「FilterForm 自绘复合件**持有**
  值 TextInput, Enter/按钮在持有者内收口」——实现时发现 **`TextInput::on_change`
  镜像通道**（许可页 `LicenseKeyInput` 先例）: 值随编辑进 `picker_value`, 「过滤」
  是纯 Button、Enter 走 `app_key_filter`, **零自绘复合件**。比 plan 少一个组件,
  且是仓内既有先例通道。
  **偏差反转（同日 review）**: 镜像被深潜 R3 打穿（`set_text`/`clear` 不回
  `on_change` 的脱钩窗 = 空框提交出脏子句）, 全局 Enter 拦截又被 R7 打穿（劫持
  算符钮的 Enter）—— **镜像退役, 回归 plan 原案**: `PickerInput` 薄复合件持有
  TextInput, 提交值随信 `PickerSubmit(String)`, Enter 只在持有者内收口 +
  `focus_id` 送焦（R8）。三缺陷一次消解; 教训 = 「兄弟节点读不到缓冲」的收口
  原则当时就写在 plan 核实②（「不许绕」）, 绕道镜像省的组件最后还是补回来了。
- **修程纠偏两处**: ①Enter 拦截块一度误嵌 Esc 分支内（死代码——与评审 A2 同族
  形态, 当场抓出挪正）②`type_complexity`/`push_text(&str)`/`crate::columns` 路径
  三处编译纠偏。
- **机器判据 1–6 全过**（388 绿 = 376 + 12 锁: pick_list 4 + settings 2 +
  main 3 + view 1 + T1 2; 基线 376 不破）; A/B 红两处在案（T0 sync 重建 /
  追加拼接）; 零框架/引擎改动（`view::FONT_SIZE` 放 `pub(crate)` 是可见性, 非改动）。

## 评审记（2026-09-23 review 阶段：双路独立评审 + 并账修复闭环）

**双路互不知情**：①五轴全量路 ②三区深潜路（拼子句查询面 / RowList 生命周期面 /
弹层键路接线面）。两路均 Request changes；并账去重后 **Critical×1 + Required×6**，
全部修复、每修一锁（+5 锁 + 对抗面并入扩锁）。

### 修复清单（并账去重）

| 级 | 缺陷 | 来源 | 修法 | 锁 |
|---|---|---|---|---|
| Critical | **拼接面 < parse 破坏面**：值含 `>`/`<` 静默改写查询（`a=List<String>`→`a=List` Lt…）/ `>` 算符+前导 `=` 拼出双字符算符 / Eq 尾 `*` 偷换前缀 / 字段含空白·点·算符逃逸（`user.id` 扁平键查嵌套 0 命中） | 五轴 C + 深潜 C1/R1/R2 | `build_clause` 拒收面盖住破坏面（字段拒空白/`.`/`=<>`；值拒空白/`<>`/前导 `=`；Eq 尾星拒；Prefix 含星拒）+ notice 分文案说清 | `build_clause_rejects_anything_parse_would_rewire`（表驱动 12 拒 + 6 受；path 逐段断言，不 join 假绿） |
| Req R5 | 托盘 `OpenSettings`/`UpgradeGotoActivate` 不关弹层——双开时 Enter 被劫持到提交（原注释「互斥保证」前提不成立） | 五轴 R① = 深潜 R5 | 开设置关三弹层（互斥双向补全） | `open_settings_closes_transient_popovers` |
| Req R② | roundtrip 锁过弱（`path.join` 假绿 / 无对抗输入） | 五轴 R② | 并入 Critical 表驱动锁 | 同上 |
| Req R3 | 值镜像 vs `bind_clear` 脱钩（`set_text`/`clear` 不回 `on_change`）——空框提交出脏子句 | 深潜 R3 | **架构消解：镜像退役** —— `PickerInput` 薄复合件（Bar 同构）持有 TextInput，提交值随信 `PickerSubmit(String)`（plan 原案「持有者收口」回归，见实现记偏差反转） | `picker_input_enter_and_button_carry_value` |
| Req R4 | RowList `pressed` 存行号——press 与 release 之间 sync 换数据把点击送错列（live-tail 轮转窗口）；`apply_rebuild` 不关弹层 | 深潜 R4 | `pressed` 改存**载荷**（全等才触发）；rebuild 关三弹层（apply_fresh 同纪律） | `press_release_survives_row_data_swap_only_for_same_payload` |
| Req R6 | 导航键/滚轮穿透——门禁只查 settings，picker 开着 ↑↓/Space/Page* 滚背后日志（09-14 同族漏洞扩展面） | 深潜 R6 | `App::event` 键盘 + 滚轮门禁与模态清单**同源**（settings/picker/col_menu/export） | `modal_gate_swallows_nav_keys_and_wheel_when_picker_open` |
| Req R7+R8 | 全局 Enter 拦截吃掉算符钮的 Enter 激活；开弹层不送焦，打字进不了值框 | 深潜 R7/R8 | 合并消解于 `PickerInput`：Enter 只在持有者内收口（钮的 Enter 归钮）；`focus_id="picker-value"` + `OpenPicker` 送焦；**全局 Enter 拦截撤销** | `picker_esc_enter_modal_and_draft_clear`（全局 Enter 不劫）+ `picker_input_enter_and_button_carry_value` |

### Optional / Nit 裁决（全清）

- **修 5**：RowList 可点行 accent 文字（可点暗示——免得纯文本行被读成只读列表，
  五轴 Opt①）/ `max_rows` 16→12 两处（小窗适配 + **「还有 N 列」尾行从死代码
  复活**，五轴 Opt②）/ `reset_picker_draft` 三态一处收口（Opt③）/ `fields_hover`
  CursorLeft 不粘（Nit）/ Bar 过期注释更正（深潜 Nit：TextInput 已有内容裁剪）。
- **记档**：许可页 `LicenseKeyInput` 镜像与 R3 同族（`license_clear_rev` 后的
  16ms 脱钩窗——「激活即弃 key」语义下风险极低，实机撞到再收）；RowList 五闭包
  契约等第三消费者再收（Consider 维持）；`view::FONT_SIZE` 耦合方向留 simplify。

### 两路排除项（核对一致）

T0 语义保全（标记行/恢复默认/守卫链）/ 让位几何同源无重叠命中面 / AsyncJob 代次
无串台 / Esc 后过滤复位完整 / 制表符换行已拒 / `.log` 判据一致。**plan 镜像偏差
曾被判「合理」（五轴 FYI）——随后被深潜 R3 的脱钩窗推翻，已随修复撤销**。

### 修复验证

三件套全绿：fmt / clippy `-D warnings` 0 / **393 测试**（388→393：main 3 +
pick_list 1 + settings 1；对抗面并入扩锁）。**红记录**：Critical 拒收面先红
（`须拒收: "a" Eq "List<String>"`）后绿；R5/R6/R7 三锁对旧实现必红（旧行为 =
不关弹层/键穿透/全局 Enter 劫持）；R4 换数据锁与载荷锚定同批落（行号版实现随
修删除，锁为回归钉）。

## 简化记（2026-09-24 code-simplify 收口: 行为零变化, 测试零修改, 393 绿）

| # | 简化 | 面 | 内容 |
|---|---|---|---|
| 1 | 拒收说清单一收口 | `main.rs` | `clause_reject_notice` = 判据+文案**同一函数**（单一事实源）; `build_clause` 委派拒收、只管拼装; `PickerSubmit` 22 行 if-else 分类链收成 4 行 —— 拒收规则改一处即全对, 判据与说理不再两份漂移。修程: `?` 极性一度写反（Some=拒收 却在 None 早退）, 表驱动锁当场红（build_clause 三红 + picker 红）, 正位后绿 |
| 2 | 三弹层开合/判据收口 | `main.rs` | `close_popovers`（关尽列管理/导出/字段查询, 7 个写点收一处: 互斥开一关二 ×3 / 换文件 / 重建 / 开设置 / 升级去激活）+ `popover_open`（滚轮/键盘/Ctrl 三门禁**同源** —— 评审 R6「同源清单」从注释落实为代码） |
| 3 | 五卡壳收口 | `settings.rs` | `card_shell`/`card_column`/`card_title` —— 设置卡/升级提示/导出格式/列管理/字段查询五卡同形壳（不透明底+圆角+内边距+定宽+16 间距居中列+标题）收一处, 约 75 行重复构造 → 3 个 helper; settings_card 的背景/描边/绑定三注释随壳搬家不丢 |
| 4 | 行封顶常量 | `settings.rs` | `POPOVER_ROWS_MAX = 12` 两处引用（数字与「小窗适配」注释不再双抄） |
| 5 | 同值常量钉死 | `settings.rs` | `BODY_SIZE` 收编为 `crate::view::FONT_SIZE` 别名（两个 14 会漂; 弹层/RowList 与行文必须同号） |
| 6 | RowList 冗余字段 | `pick_list.rs` | `text_on_accent` 每帧恒赋 WHITE = 假缓存（主题无 on-accent token）, 删字段 paint 内联; `RefCell` 导入统一 |
| 7 | 过期注释更正 | `view.rs` | 两处「`TextInput::paint` 既不裁剪」—— 框架 2026-09-20 起已裁剪进边框（`overflowing_text_is_clipped_inside_input_area` 锁在案）; 事实改写, P33「空态才画」规矩保留并说清留下理由 |
| 8 | 可见性收紧 | `settings.rs` | `col_menu_rows`/`picker_field_rows` `pub(crate)` → 私有（只被同文件消费） |

**FONT_SIZE 耦合方向裁定**（评审留档项, 本段了结）: `view.rs` 是 bin 布局 token 家
（`ROW_HEIGHT`/`FONT_SIZE` 实机定档注释成对）, pick_list/settings 均为**消费者** ——
方向正确（同 crate、无环）, 不为一个 const 开新家; 真正的害是 `BODY_SIZE` 同值
分家会漂, 已钉别名（#5）。pick_list → `view::FONT_SIZE` 引用保持。

**留档不动**（照评审记）: 许可页 `LicenseKeyInput` 镜像 R3 同族窗实机再收;
RowList 五闭包契约等第三消费者。

**验证**: fmt / clippy `-D warnings` 0 / **393 测试零修改全绿**（每项后跑锁,
行为零变化）。

## 人工验收

**2026-09-23 记账**（用户裁定「人工验收全部记账」）: 延后待实机, 五条汇总在
`tasks/acceptance-pending.md` **E 组**（E5 = T0 验点「列管理行随文件换」, 同时
充作 `table-column-config` 人工验收 A3 的勘误验点）。实机后回填结论:

**2026-09-27 用户实机五条全过**（总清单 E1–E5, 逐项过, 无缺陷回填）:
「字段…」点选全链（点字段/算符/打值/过滤）命中正确、底栏计数如常; 追加
子句 AND 交集正确、Esc 清回全量; 空值点「过滤」有提示且查询不动、Esc/scrim
关弹层不提交; `.log` 无「字段…」按钮、免费态直接可用无门控。**E5（T0 验点
「列管理行随文件换」）通过** —— 兼充 `SPEC-v1x-table-column-config.md` A3
勘误验点收口（开 A 看 A 的列、换 B 看 B 的列）。
