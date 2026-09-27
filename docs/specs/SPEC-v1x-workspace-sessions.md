# SPEC-v1x-workspace-sessions: 命名工作台会话（腿四·付费层）

- @author 十四叔
- @date 2026/09/24
- 状态: **五段全闭**（2026-09-24 一日: 「go」→plan→/build auto T1–T4（407 绿）
  → 双路评审并账 Critical×3+Required×5 全修 +6 锁（413 绿）→ code-simplify
  3 项行为零变化收口（413 测试零修改）; 含账本改名 `state.json` 一次到位）;
  **人工验收（五条）记账**（总清单 F 组）
- 所属: 能力地图 `SPEC-v1x-map.md` 模块 `workspace-sessions`（构建顺序**末棒**;
  依赖: `licensing`（付费门, 枚举已预埋）+ `table-column-config`（列配置模型与
  写穿接缝）; 免费层欠账三连已全部建成）

## Objective

ROADMAP §腿三/腿四 字面项：**命名会话（过滤器组合 + 搜索历史 + 列配置 + 展开态）
跨会话保存/切换**。付费层叙事「批量 / 留存 / 交付」的**留存**面——「把自己搭起来的
工作台留下来」（intent: 会话模板, 对标 LogViewPlus Templates）。成功长什么样：
把一个排障现场调好（过滤组合点好、搜索备好、列摆好、子行展开）→ 命名保存 →
关应用再开、或换到别的现场后，一键切回这个工作台；免费层点入口见统一升级提示。

## 范围（2026-09-24 用户「go」裁定，全按推荐项）

1. **载荷四样 = 过滤组合 + 搜索查询 + 列配置 + 展开态**（推荐①）:
   - **过滤组合** = `filter_applied` 真相串（多子句 AND; 级别直方图点选走
     `apply_level_filter` → `Msg::ApplyFilter`, 已折叠进同一串, 无第二态）
   - **搜索查询** = `search_query` 串 —— ROADMAP「搜索历史」按「搜索查询真相串」
     兑现; **多条搜索历史列表 = Out**（无基建, 账面仅此一处提及; 见 Open Q2）
   - **列配置** = `ColumnConfig`（order/hidden/widths, **复用模型**——
     table-column-config 接缝定案「不再另造第二套」）
   - **展开态** = 已展开的**文件行号**表（`ExpandMap` 的键集; 子行数应用时
     现算——内容可能已变, 不存快照）
2. **作用域 = per-路径命名会话**（推荐②）: 一个文件路径下多套命名工作台,
   「跨会话保存」= 跨应用启停, 「切换」= 命名快照互切。展开态行锚 / 列名 schema /
   过滤字段名全是文件语境, 跨文件应用是伪需求; 与「会话应用 = 写穿 per-file
   条目」的接缝一致。
3. **载体 = 并入状态账本 + 改名 `state.json` 一次到位**（推荐③, 非新裁——
   bookmark-persist 已裁定「腿四扩多套会话时再统一改名 `state.json` 一次到位」）:
   `columns.json`（零真实用户, v1.x 未发布）改名 `state.json`, 顶层加 `sessions`
   段; `files` 段 = 现 per-路径记忆（LRU 64 不动）, `sessions` 段 = 命名会话
   （用户自管, 不吃 LRU）。temp+rename 原子落盘 / 损坏备份守卫 /
   `columns_path` panic 封死家法**整套随迁**（封死 fn 随名改 `state_path`）。
4. **门控 = `Feature::WorkspaceSessions` 全动作**（推荐④, 枚举与 label「工作台
   会话」licensing 已预埋）: 保存/应用/删除全在门后（**勘误 M16**: 「重命名」
   不设独立动作 = 同名覆盖语义, 评审 FYI 定案）; **门控点位 = 入口**
   （export D7 先例: 免费态点「会话」入口 → 统一升级提示, 在弹层/对话框**之前**）;
   **数据永在**——降级不变砖 = 锁动作不毁数据（试用过期者再购即回）。
5. **UI = 状态栏「会话」按钮 + 弹层族四件套**（推荐⑤; 导出/设置同排先例）:
   卡体 = 命名会话行列表（**RowList 第三消费者**——评审留档「五闭包等第三
   消费者」在此了结）+ 命名输入 + 「保存当前」/「应用」/「删除」。弹层范式
   全套: 四件套（Overlay + bind_open + on_scrim_click + 行按钮）、Esc 插层、
   模态守卫、与三弹层互斥（`close_popovers`/`popover_open` 收口直扩一员）。
   同名保存 = **覆盖** + 底栏「已更新」（会话是工作台快照, 覆盖是主用法）。
6. **上限与失效**（推荐⑥; **勘误 M3**: 上限 32 → **12**, 评审定案——与弹层
   可视封顶取齐, 可达集恒等于允许集, 超出可视的上限是静默不可达的假闸）:
   每路径命名会话上限 **12** 条, 满员拒绝 + 提示; 展示序 = 保存时刻**降序**
   （新存在顶）; 展开态**行号越界剔除 + 无子行剔除**（书签越界剔除先例,
   「安静消失」; 应用时对当前内容现算子行, 解析不出子行的行号丢弃）。

**In**:

1. 会话四件套载荷的保存/应用/删除/覆盖（per-路径命名）
2. 状态栏「会话」入口 + 命名会话弹层（RowList 行列表 + 命名输入 + 三动作）
3. 账本扩展: `state.json`（改名）+ 顶层 `sessions` 段（结构化容错: 坏条丢条
   不丢账, 书签载入侧同粒度先例）
4. 应用 = 写穿 per-file 条目（列配置落 `files` 段即存, 接缝定案）+ 过滤/搜索
   走既有 `apply_filter`/`apply_search` 全链 + 展开态换入
5. 门控四动作 + 免费态升级提示（两道闸: 入口判 + update 兜底, licensing 先例）
6. 回归锁: 会话 roundtrip / 应用写穿 / 展开态剔除 / 门控两态 / 互斥·Esc·模态
   接线 / 上限拒绝

**Out**:

- **书签入会话**——Open Q1（推荐不入: 应用/保存均不触碰书签; 书签已有免费
  per-路径持久化, 换工作台毁书签是惊吓; `columns.rs` 腿四接缝注释「导出命名
  会话须显式 strip `bookmarks`」同向）
- **多条搜索历史列表**（历史基建/回填 UI——独立诉求, 实机喊累另起; Open Q2）
- 跨文件/全局会话模板（LogViewPlus Templates 的跨文件面——账面无此诉求,
  不发明; Open Q2 时代可复议）
- 视图模式（Table/Raw）入载荷——跟随文件检出 + Ctrl+T 自由, 不锁
- 书签/顶行/选中位置入载荷——书签见上; 换工作台回顶 + 选中 0（重看语义）
- 会话导入/导出（跨机器搬家）——「交付」是腿三导出的叙事, 会话是本机留存
- 自动会话（崩溃恢复/自动保存草稿）——命名快照是明确动作, 不做隐式

## 开工前事实盘点要点（2026-09-24 调研）

- **付费门已预埋**: `license::Feature::WorkspaceSessions` + label「工作台会话」
  （licensing T7 建枚举时即含）; `allows(Feature)` + 统一升级提示
  `ShowUpgradePrompt` + 两道闸先例全在（export D7 / field-analytics D5 同款）。
- **持久化基建已就位**（table-column-config D4 + bookmark-persist）:
  `ColumnFiles`/`FileEntry`（`{ path, updated, order, hidden, widths, bookmarks }`）
  + `save_to` temp+rename 原子落盘 + `backup_if_corrupt` 损坏备份守卫 +
  `put` LRU 64（无书签先挤淘汰保护）+ 载入侧「坏字段丢字段不丢条」。
  **接缝两处定案**: ①col-config「workspace-sessions 复用此模型（会话应用 =
  写穿 per-file 条目）, 不再另造第二套」②bookmark-persist「腿四扩多套会话时
  统一改名 `state.json` 一次到位」。
- **`columns.rs:218` 腿四接缝注释**: 导出命名会话须显式 strip `bookmarks`
  （review Optional 落笔）; 与 bookmark-persist Out 条「书签随会话导出/切换
  （腿四增值面）」**字面冲突**（Out 读作归属转移, 评论读作定死不随）——
  故提 Open Q1 显式了结, 不在两可间静默取舍。
- **状态面**（`main.rs` LogApp）: 过滤 = `filter_applied`/`filter_landed`
  （在途窗口脱钩, 会话存**查询串**不存行集, 应用重跑即真相）; 搜索 =
  `search_query` + `search_pattern`/`SearchNav`（导航态随应用重建）;
  级别点选 = `apply_level_filter` → `Msg::ApplyFilter`（**已折叠**, 无第二态）;
  列 = `columns: ColumnConfig`; 展开 = `expanded: ExpandMap`（文件行号 →
  子行数, `expand_rev` 修订号作废旧选区）+ `sub_rows`（惰性 parse 缓存）。
  **无搜索历史基建**（grep 实证零命中）。
- **弹层族现况**: 三弹层（col_menu/export_menu/picker）互斥「开一关二」+
  `close_popovers`/`popover_open` 单点收口（field-picker-ui simplify 落成,
  本模块直扩一员即可）; Esc 次序 `upgrade > settings > picker > col_menu >
  export_menu > 栏`; 模态清单在 `App::event` 键盘/滚轮门禁与 `app_key_filter`
  Ctrl 守卫**同源**。会话弹层插入即全套复用。
- **RowList**（`pick_list.rs`）: 自绘行列表件, 五闭包契约（rows_fn/highlight_fn/
  on_pick/more_fn/max_rows）——评审留档「等第三消费者再收」, 本模块即第三
  消费者; 若契约要收（如命名行双动作）, simplify 惯例先行为零变化再演进。
- **框架生命周期铁律**（field-picker-ui 事实盘点定谳）: `view()` 建树一次
  不再重建, 动态行 = RowList 自绘 / 结构固定值可动 = 绑定闭包——会话行列表
  必走 RowList, 命名输入 = `PickerInput` 持有者同构（R3 教训: **不许**再走
  `on_change` 镜像）。
- **测试文化**: 逐测唯一临时文件 / `new_empty_at` 注入 / `state_path`（随名）
  panic 封死家法 / A/B 摘除验红 / 真 paint 锁（非零平移原点）/ 表驱动容错 /
  测试严禁真实桌面副作用。

## 设计决策

### D1: per-路径命名会话 + 应用写穿 per-file 条目

会话挂在文件路径下（`sessions` 条目含 `path`）; 应用会话 = 列配置换入并
**写穿** `files` 段该路径条目（既定接缝——用户手工摆法与会话带来的摆法同一
份记忆）, 过滤/搜索走既有链重跑, 展开态换入。

### D2: 载荷 = 查询串 + 列配置 + 展开行号表, 全部「应用时重建真相」

不存行集/不存子行内容/不存导航表——过滤与搜索存**查询串**（应用重跑,
AsyncJob 代次拒旧先例护在途）, 展开存**行号表**（应用现算子行, 剔除失效项）。
文件长变/内容改写后应用会话 = 尽力还原**布局意图**, 不承诺行位原样
（与书签行锚同一哲学, 已知局限同款表述）。

### D3: 账本 = `state.json` 顶层 `sessions` 段

```json
{ "files": [ …既有… ],
  "sessions": [ { "path", "name", "filter", "search",
                  "order", "hidden", "widths", "expands", "updated" } ] }
```

列三字段复用 `ColumnConfig` 的序列化形状（**勘误 M13**: 容错面不全同 ——
会话身份字段严（缺 name/path/updated = 条废）, 列三字段宽（容缺省, 应用时
merge 兜底））; 坏条**丢条不丢账**; `sessions` 不吃 files 的 LRU（用户自管,
上限拒绝制）。
改名一次到位: `default_path`/`columns_path`→`state_path`/注释/测试封死随迁,
旧 `columns.json` 读入即迁移（存在旧名→读旧写新, 一次性; 零真实用户但迁移
写上防手工建过文件的人）。

### D4: 全动作门控 + 数据永在

入口判（免费态 → `ShowUpgradePrompt(Feature::WorkspaceSessions)`, 弹层不
开）+ `update` 兜底（两道闸, licensing 既有纪律）; `sessions` 段读写**不受
门控**（数据永在: 试用过期/降级者的会话原样躺在账本里, 再购即回）。

### D5: 弹层族第四员 + RowList 第三消费者

状态栏「会话」钮（`导出` 按钮同排同规）开弹层; 行 = 命名会话（文案 = 名字,
载荷 = 名字）, 点行 = **应用**（RowList on_pick 语义天然贴合）; 卡底 = 命名
输入（`PickerInput` 同构持有者: Enter/「保存当前」同路收口）+ 「删除」
（删除选中行）。互斥/Esc/模态并入既有收口（`close_popovers` 扩员后名实
微调为「弹层族」, 一行注释的事）。

### D6: 同名覆盖 / 上限拒绝 / 失效剔除

同名保存 = 覆盖 + 「已更新」提示; 满 32 条新增拒绝 + 说清; 应用时展开行号
越界或无子行 = 静默剔除（书签先例: 「安静消失」, 损坏零写回同哲学）。

## 成功判据

**机器可验**（2026-09-24 build 收口已全过）:

1. ✅ 会话 roundtrip: 四样载荷写出→载入全等; 坏 `sessions` 条（缺字段/错型/
   超上限行号）丢条不丢账, 好条照留
2. ✅ 应用写穿: 应用会话 → `files` 段该路径条目列三字段 = 会话值（落盘实证）
3. ✅ 展开态剔除: 越界行号 / 无子行行号丢弃, 合法行号展开且子行数现算正确
4. ✅ 门控两态: 免费态四动作全被拦（升级提示 + 弹层不开 + 账本零变化）;
   付费态放行; **付费态永不触发升级提示**（两道闸锁）
5. ✅ 弹层接线: 开闭 / 互斥（与三弹层双向）/ Esc 次序 / 模态守卫 / 换文件关
   并草稿复位（picker 同规）; 保存/应用/删除三动作 Msg 链
6. ✅ 上限: 第 13 条拒绝 + 提示; 删除后可再存（上限随勘误 M3 = 12）
7. ✅ 三件套全绿, **基线 393 不破**（收口 407）; A/B 精确红在案（摘会话写出 /
   摘写穿 / 摘剔除各一）

**人工验收**（记账 → `tasks/acceptance-pending.md` 续 F 组）:

1. 调好工作台（过滤组合 + 搜索 + 摆列 + 展开子行）→ 命名保存 → 重启应用
   开同文件 → 应用会话 → 四样全部回来
2. 同文件存两会话互切, 各自完整; 同名保存覆盖生效
3. 免费态点「会话」入口出升级提示且不丢数据; 激活后原会话可用
4. 日志轮转/截断后应用旧会话: 布局回来、越界展开安静消失、不崩
5. 换文件开「会话」列表 = 该路径自己的会话; 应用另一文件的会话不出现（或
   列表只显本路径——按 Open Q2 裁定口径）

## 已知局限

- 「搜索历史」只存当前搜索查询串（多条历史列表 Out; Open Q2 口径）
- 展开态行锚: 内容原地改写而行数不变会指错行（书签同一哲学, 不做内容哈希）
- 会话名自由文本无校验（GB2312 字形集外字符显示空白——与查询串同暴露面）
- 模式（Table/Raw）不入载荷; 应用会话不回顶之外的视口还原（回顶 + 选中 0）

## Boundaries

- 注释/文档中文; **零新依赖**; 框架/引擎改动 = **零**
- 五段流水线: spec 批准 → plan → build → review → code-simplify; spec 写完
  不立即编码
- 未获用户指示不 commit/push
- 不动过滤语法/`parse_filter` 唯一入口; 不动 `files` 段 LRU 语义; 书签状态
  零触碰（除非 Open Q1 改裁）
- 测试严禁真实桌面副作用; `state_path` 测试构建 panic 封死家法随迁不松

## Open Questions（2026-09-24 「go」全按推荐裁定）

1. ~~**书签是否随会话（保存/应用）**~~ **已裁: 不含**——应用/保存均不触碰
   书签。bookmark-persist Out 条「书签随会话导出/切换（腿四增值面）」按
   **归属说法**读（腿四自己定）, 与 review 落笔「导出命名会话须显式 strip」
   的定死读法在此统一为不随: 换工作台毁书签是惊吓, 书签已有免费 per-路径
   持久化, ROADMAP 内容清单无书签。
2. ~~**「搜索历史」口径**~~ **已裁: 只存当前搜索查询串**——多条历史列表 Out
   另起（无基建; 账面仅 ROADMAP 一处提及, 不同车造基建）。
3. ~~**换文件后「会话」列表显谁**~~ **已裁: 只显当前路径的会话**（per-路径
   语义直觉; 无文件空态不出入口）。
4. ~~**删除是否有确认步**~~ **已裁: 无确认**——底栏说清即走, 不可撤销
   （会话是快照不是唯一记忆, 重存即可; 书签删除同样无确认）。

## 实现记（2026-09-24 build 收口 T1–T3, 407 测试绿 = 393 + 14）

- **T1**（`columns.rs` + `main.rs` 路径侧）: 账本改名 `state.json` 一次到位 ——
  `default_path` 换名 + `columns_path`→`state_path`（**panic 封死家法随迁**,
  should_panic 锁在案）+ **读旧写新迁移**（`load_state_account`: 新名优先/
  仅旧名读旧/只写新名）。**修程一笔**: 迁移派生踩到双方案陷阱 —— 测试注入是
  `with_extension` 邻居派生（`x.state.json`↔`x.columns.json`）而生产是整名兄弟
  （`state.json`↔`columns.json`）, 无统一表达式 → `legacy_state_path`
  **分支配对**是唯一不歪的写法。`SessionEntry`（四样载荷 + **结构无
  `bookmarks` 键**, Open Q1 strip 落到类型面）+ `sessions` 段恒写/坏条丢条
  不丢账/收集硬顶 `SESSIONS_HARD_CAP`/`put_sessions_for_path` 只动本路径切片
  （bookmark-persist Critical「读改写覆盖抹全记忆」同族面提前封）。A/B:
  **摘恒写 = roundtrip 精确红**。
- **T2**（`main.rs` + `expand.rs`）: `save_session`（快照四样 = 查询串×2 +
  `ColumnConfig` + `ExpandMap::lines()`, 同名覆盖/空名/满 32 拒绝说清）+
  `apply_session`（①列换入写穿 `save_state` ②过滤/搜索走既有全链重跑 ③
  `rebuild_expands` 越界/不可展开**静默剔除** ④回顶+记选中）+ `delete_session`
  （无确认）+ `save_state` 扩写 `put_sessions_for_path` + 换路径切片替换清选中。
  **plan 偏差一笔**: `Msg::DeleteSession(String)` → **`DeleteSelectedSession`**
  （无载荷）——「删除」钮是无状态 `Fn() -> Msg` 读不到选中名, 指针语义收进臂
  （TextInput 无 set API 无法预填名字 —— plan 衍生设计已预告此矛盾, 不绕镜像）。
  A/B: **摘写穿 = 落盘断言红**; **摘剔除 = 断言红**。
- **T3**（`settings.rs` + `view.rs` + `main.rs`）: `PickerInput` → **`SubmitInput`
  零行为变化泛化**（submit_label/submit_w/focus_id/clear_rev/submit_fn 构造注入;
  picker 传 `PickerSubmit`+56 预留宽 = 原值, 既有锁零回退）+ `sessions_card`
  （**RowList 第三消费者**, 契约零变化 —— 评审留档在此了结; 点行=应用并记选中,
  「删除」作用选中）+ 状态栏「会话」钮（export 同排同规; **空态不出**, Open Q3）+
  门控两道闸（`session_gate` 入口判 + 三动作臂兜底; **数据永在**账本读写不设门）+
  弹层族第四员（`close_popovers`/`popover_open` 扩员 + Esc 首插
  `upgrade > settings > session_menu > picker > …` + 模态同源）。
- **机器判据 1–7 全过**（407 绿 = 393 + 14: T1 六 + T2 三 + T3 五）; A/B 红
  三处在案; 零框架/引擎改动（`ExpandMap::lines()` 是产品 lib 增方法）。
  **修程纠偏三处**（非设计问题）: 测试夹具漏 `updated` 被容错正判 / 插块孤儿
  行崩括号 / `MouseWheel` 修饰字段构造缺项 —— 均当场纠。

## 评审记（2026-09-24 review 阶段：双路独立评审 + 并账修复闭环）

**双路互不知情**：①五轴全量路 ②三区深潜路（账本容错/迁移面 · save-apply 写穿
与剔除面 · 弹层键路/门控接线面）。两路均 Request changes；并账去重（重叠取高）
后 **Critical×3 + Required×5 + Optional×6 + Consider/FYI×2**, 全部处置、
每修一锁（+6 新锁 + 4 处扩锁）。

### 修复清单（并账去重）

| 级 | 缺陷 | 来源 | 修法 | 锁 |
|---|---|---|---|---|
| Critical M1 | `backup_if_corrupt` 损坏判据只认 `entries` ——「files 坏条 + sessions 完好」是丢段不丢账的合法容错, 却被整账判损 rename `.bak`, 他会话丢出活跃账本（bookmark-persist Critical 守卫在两段账本上留洞） | 深潜①-C1 = 五轴 3 重叠**取高** | 「可辨账本」谓词收口 `is_recognizable`（两段任一有条目）—— 损坏备份与迁移回落**同源共用** | `state_account_recognizes_sessions_and_falls_back_then_retires_legacy`（sessions-only 不备份 + 他会话不丢） |
| Critical M2 | 迁移读/备判据不对称 + 非「一次性」—— 坏/空新名时 save_state 先挪新名、再读旧名、拿空内存覆盖旧名本路径切片（家族 Critical 复发）; 空新名永久挡死迁移 | 五轴 1 ⊕ 深潜①-R1 | `load_state_account` **可辨**才新名优先（不可辨回落旧名, 同谓词）+ save_state 落成后旧名 rename 退役 `.migrated`（一次性; 不删 —— 数据不毁） | 同上（空新名回落 → 旧记忆写回 → 旧名退役断言） |
| Critical M3 | 上限 32 vs 可视 12 脱节 —— 第 13 条起不可见/不可应用/不可删除, 新会话 push 队尾**存完即消失**（静默不可达） | 五轴 2 | 上限 32→**12** 与 `POPOVER_ROWS_MAX` 取齐（可达集恒等于允许集）+ 展示序 updated **降序**（新存在顶） | `session_rows_show_all_up_to_cap_newest_first`（满员全画无尾行死角 + 第一行 = 最新） |
| Req M4 | 导出点击「点穿」: `return Consumed` 被吞落入「此处无行」, 后到 Notice **覆盖**守卫文案（家族⑤次序陷阱复发）; 旧锁只看正向消息 = 假绿 | 五轴 4 | export 分支复位 `return EventResult::Consumed` | export 锁扩「Consumed + **恰一条**消息」 |
| Req M5 | 模态滚轮锁假绿: `wheel(3.0)` 向上滚在 `top_row=0` 被钳回 —— 门禁整段删掉断言也恒真（家族⑥复发） | 五轴 5 | 真锁改可动位（`top_row=5` + `wheel(-1.0)` 向下） | wire 锁改真（摘门禁必红） |
| Req M6 | 互斥「与弹层族双向」半边无锁 —— `session_menu_open` 漏出 `close_popovers` 时 OpenPicker/OpenSettings 路径无锁会红 | 五轴 6 | 补双向锁 + 「三弹层」注释 10 处随名「弹层族」 | `session_menu_mutex_is_bidirectional` |
| Req M7 | `clear_search` 不 `invalidate` 在途 `search_job` —— 清空/空搜索会话应用后旧搜索经 tick 复活（spec「代次拒旧护在途」的搜索侧半边违约） | 深潜②-R1 | 补 `search_job.invalidate()`（`clear_filter` 同规） | `clear_search_kills_pending_search_job` |
| Req M8 | `apply_session` 遇搜索串正则拒收（跨编码/手造）时 3/4 写穿后仍 `true` 关弹层 —— 四样名不副实 | 深潜②-R2 | 搜索串先验正则, 失败 = **显式清空** + 说清（四样须「已应用或已显式清空」才许关） | `apply_session_with_invalid_search_explicitly_clears` |
| Opt M9 | 幽灵名删除不清指针 → 反复「会话不存在」 | 深潜②-O1 | 指针随名清（找到与否都清） | cap 锁扩「幽灵名删除清指针」 |
| Opt M10 | 名归一不一致: 空白名载入存活 vs 保存拒; 控制字符/超长名直通 notice 与行文本 | 深潜①-O1/O2 | `clean_session_name`（trim + 剔控制字符 + 截 64）保存载入同式 | `clean_session_name_trims_strips_controls_and_clamps` |
| Opt M11 | 窄窗位置计数左缘钳制压「会话」钮字（看得见行号、点出会话） | 深潜③-O1 | 「放不下就不画」（Bar hint 家规）: 会撞钮带整条不画 | 记档（几何锁成本高, 触面 = 窄窗边缘档, 实机核对） |
| Opt M12 | `close_popovers` 关会话不清草稿（靠下次开兜）; 换文件草稿复位无显式锁 | 深潜③-O2 ⊕ 五轴锁表 | 关清草稿随 `close_popovers` 关走 | wire 锁扩「互斥关也清草稿」 |
| Opt M13 | D3「同容错同模型」字面与实现分叉（order 会话侧容缺省） | 五轴 7 | spec 勘误: 会话身份字段严、列三字段宽（merge 兜底） | —（文档） |
| Opt M14 | `normalize_sessions` 双线性扫描 O(n²)（硬顶 8192 时） | 五轴 8 | HashMap 去重 + 计数 | —（行为零变化, 既有锁覆盖） |
| Con M15 | 「删除须先应用建指针」实机语义（想删 B 先把 B 砸脸上） | 五轴 9 | **记档**: 实机抱怨再裁独立选中（hover/右键） | —（记档） |
| FYI M16 | spec 范围④「重命名」措辞 vs 三动作 | 五轴 11 | spec 勘误: 重命名 = 同名覆盖语义 | —（文档） |

### 两路排除项（核对一致）

书签零触碰（结构无键 + 内存全等锁）/ `put_sessions_for_path` 只动本路径切片 /
读改写保他路径 files+sessions / 展开剔除与 `toggle_expand` 逐点一致（含 0 与
u64::MAX）/ 写穿次序正确 / SubmitInput 泛化逐行为零变化（layout 恒等式 / 持有者
收口无镜像 / focus_id 注入; Enter 归属由框架焦点路由保证）/ 免费四动作 + 入口
全拦且**「弹层开着切免费态」被上游状态机封死**（`adopt_store_entitlement` 只升
不降）/ 两道闸付费永不误弹（`ShowUpgradePrompt` 内再兜一道）/ Esc 首插 / 模态
三处同源接线（假绿在测试不在接线）/ 无选中删除提示 / 换文件清选中 / 
`ExpandMap::lines()` 键序升序 / `state_path` 家法 / 测试无真实桌面副作用。
**`OpenColMenu` 不关 settings 为既有缺口**（非本模块引入, 记档随 M15）。

### 修复验证

三件套全绿: fmt / clippy `-D warnings` 0 / **413 测试**（407 → 413: +6 新锁;
M4/M5/M9/M12 为扩锁）。**红记录**: M1/M2 联合锁摘谓词必红（sessions-only 被
备份 / 空新名挡死迁移）; M3 锁摘降序或上限取齐必红; M5 旧锁对无效门禁恒真
（假绿实证 —— 家族⑥第二例）。修程纠偏三处: 插块孤儿行 ×2（当场清）/
M4 锁一度装错文件（当场改正）。

## 简化记（2026-09-24 code-simplify 收口: 行为零变化, 测试零修改, 413 绿）

| # | 简化 | 面 | 内容 |
|---|---|---|---|
| 1 | widths 双向去重 | `columns.rs` | `widths_to_value`（键排序写入 —— 评审 Nit 先例的排序语义随 helper 归一）/ `widths_from_value`（逐键容错 —— 评审 C1 先例）—— `entry_*`/`session_*` 四处同用, 列三字段「同模型」的真身落地 |
| 2 | `now_secs` 收口 | `main.rs` | `save_state`/`save_session` 双份 SystemTime 舞蹈 → 单点（钟坏按 0 落账的语义随名自明） |
| 3 | 杂项 | `columns.rs` | `normalize_sessions` 冗余内层 `use` 删（顶层已有）; `MAX_SESSION_NAME_LEN` 常量前置到 `clean_session_name` 之前 |

**不动清单**（Chesterton 判过, 合并即损）: hidden 解析双份 —— `entry_from_value`
严（坏形状 `?` 条废）vs `session_from_value` 宽（丢字段不丢条）是 **M13 勘误的
分叉本体**, 合并即行为变化; `picker_input`/`session_name_input` 同构双测试锁 ——
两消费者各自显式, 合并参数化损锁名可读; `view.rs` 状态栏钮与 export 块镜像 ——
锚链布局可读性优先, 抽多参 helper 反遮结构。

**验证**: fmt / clippy `-D warnings` 0 / **413 测试零修改全绿**。

## 人工验收

**2026-09-24 记账**（「人工验收全部记账」惯例续）: 延后待实机, 五条汇总在
`tasks/acceptance-pending.md` **F 组**（**需付费态**——F3 含免费/激活两态）。
实机后回填结论:

**2026-09-27 用户实机五条全过**（总清单 F1–F5, 逐项过, 无缺陷回填）:
四样（过滤组合+搜索+摆列+展开）保存 → 重启 → 应用全部回来、书签不动;
两会话互切各自完整、同名覆盖生效底栏「已更新」; 免费态入口弹升级提示且
数据不丢、激活后（license key 本地激活）原会话可用; 轮转/截断后应用旧会话
布局回来、越界展开安静消失、不崩; 换文件列表只显本路径自己的会话。
**遗留（不属本清单, 挂收银台开业）**: 真付费获取链路未验 —— 商店 add-on
entitlement 与代销 key 真购买 → 激活, 待 add-on 创建/代销商注册后验。
