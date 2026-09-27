# SPEC-v1x-table-column-config: 列配置三件套（列宽拖拽 / 列显隐 / 列重排）

- @author 十四叔
- @date 2026/09/23
- 状态: **五段全闭**（2026-09-23 一日: spec→plan→build T1–T6→双路评审并账
  Critical×1+Required×6 全修→code-simplify Nit×5 清零, 365 测试绿,
  零框架/引擎改动）—— **人工验收记账延后**（2026-09-23 用户裁定「全部记账」,
  总清单 `tasks/acceptance-pending.md` A 组六条）
- 所属: 能力地图 `SPEC-v1x-map.md` 模块 `table-column-config`（构建顺序第 4 位，接 `export`
  之后；~~免费层欠账三连第一件~~ **2026-09-27 翻案改判付费层**（地图翻案块②；
  功能本体零返工，接门待做与腿一同窗口）；依赖: **无**——~~不接 licensing 门控，
  免费层白送~~ 接门后接 licensing）

## Objective

把「看得清」交还给用户的手——JSONL 表格的列宽现在是**发现期采样建议值**（`Column.width_chars`，
clamp [4,32] 字符），长消息列挤、短 id 列宽，用户却动不了它；列多了看不全（溢出右缘整列
不画），也没法把关心的列挪到眼前。ROADMAP §一欠账表字面三件套：**列宽拖拽 / 列显隐 /
列重排**（出处 `SPEC-jsonl-table.md:30` 的 Out，v1.x 归位免费层）。

**成功长什么样**：表头列间手柄拖宽窄（双击恢复采样宽）；表头按住拖动换列序（插入位
有指示，Esc 放弃）；表头右端「列…」弹层勾显隐；重启后这套摆法还在——免费用户拿到的
是「LogViewPlus 有而我们没有」的那半截基本功。

## 范围（2026-09-23 用户三项裁定，全按推荐项）

1. **显隐与列管理入口 = 表头「列管理」弹层**（裁定①）：表头右端「列…」按钮 +
   表头右键，弹层复用 export 格式菜单范式；离列最近，发现性最好。
2. **列重排 = 表头拖拽换位**（裁定②）：表头本体按住拖动换位（资源管理器/VS Code
   同款手势），插入位指示；与列宽拖拽共用按住-拖动-抬起基建（滚动条拖拽先例已在）。
3. **自带轻持久化·免费层**（裁定③）：列宽/显隐/列序跨重启记住，照深浅主题/侧栏开关
   先例落盘；「与腿四共用状态持久化基建」按搭车读法——本模块建的状态模型与存取即
   腿四 `workspace-sessions` 将来复用的基建，腿四仍加值（命名多套会话保存/切换）。

**In**:

1. **列宽拖拽**：表头列间手柄按住拖动改该列宽；双击手柄恢复该列采样宽
2. **列显隐**：「列管理」弹层逐列开关；**至少保留一列可见**（最后一可见行不可关）
3. **列重排**：表头本体拖拽换位（手柄热区外），插入位指示，Esc 放弃手势
4. **轻持久化**：per-文件路径记住三件套摆法，换文件/重启恢复（D4）
5. **「列管理」弹层**：显隐开关行 + 「恢复默认」行（清该文件全部手动配置）

**Out**:

- **点击表头按列排序**——不在欠账表三件套；且显示行序=文件行序是行锚定数学的根基，
  排序直接破虚拟化模型，属另一维度另议
- 显示配置影响**导出/分析**的列序列集（export D5 口径不动，见 D5）
- 行多选（Shift/Ctrl 点选）——前置=框架 `Event::MouseInput` 加修饰键，M5 级另排
  （ROADMAP §一；**本模块不需要它**，别误当前置）
- 时间戳列类型系统（ROADMAP 列「打磨」项，别混）
- 明文（Raw）模式的任何列概念——Raw 是单列连续文本，级别徽章/时间戳只是着色的原文
  片段，Ctrl+T 切 Raw 后表头整块消失（`view.rs:901-908`）
- 列宽自适应算法改进、横向滚动条改动、快捷键（表头按钮够用，SHORTCUT_KEYS 不动）
- per-视图多套列配置（那是腿四命名会话的职责）

## 开工前事实盘点要点（2026-09-23 调研，6 节全文见会话记录）

- **列模型在引擎**：`danqing-logfile/src/jsonl.rs:25-34` `Column { name, width_chars }`——
  只有名字和采样字符宽，**零用户态**。宽度语义=采样最大值 clamp [4,32]（`:23,:55-62`），
  `MAX_COLUMNS=16` 首见序（`:41,:144-180`）。引擎字段**不宜**被 UI 拖拽改写
  （`discover_schema` 重跑/换文件会互踩）——用户配置是产品层新状态。
- **列几何铁律**：`event` 无 `TextBatch`，列宽重算不可能（`view.rs:827-830` `col_spans`
  注释「D2」）——任何新几何必须 **paint 缓存 / 与 paint 同源**，event 只读。
  列宽现值 = `"8"×width_chars` 实测 + `COL_PAD=16` 左→右累加（`view.rs:1348-1371`），
  溢出右缘整列不画；横向偏移 `x_offset` 是视图局部态**不经应用层**（`view.rs:1988-1993`）。
- **表头现状零交互**：`HEADER_H=28`（`view.rs:123`）有绘制无 hit rect/hover/事件分支，
  点表头现落「此处无行」notice（`view.rs:2196-2206`）。命中矩形双先例：纯函数
  （`settings_hit_rect`/`export_hit_rect`，`view.rs:92,101`）+ paint 写 `Cell<Rect>` event 读。
- **拖拽基建全备、零框架改动**：`CursorMoved` 全树广播 + `MouseInput{button,pressed,position}`
  + 框架指针捕获（拖出区域抬起仍路由回来，`handler.rs:117-119,958-977`）；两条完整
  先例可抄——滚动条拇指拖拽（`view.rs:758-761,1957-1994,2214-2219`）与文本框选
  「按下潜伏→超 `CLICK_DIST=4.0` 升级→move 跟手→抬起落定/Esc 连潜伏一起清」
  （`view.rs:831-835,2008-2018,2266-2286`）；双击产品侧自判（`DOUBLE_CLICK_MS=300`，
  `view.rs:279-281,1089-1098`）。`CursorIcon` 无 `ColResize` 变体（小缺口，见 D8）。
- **弹层范式**：export 菜单四件套 `Overlay::themed + Center + bind_open 谓词 + on_scrim_click`
  （`settings.rs:401-465`），Esc 次序 `upgrade > settings > export_menu > 栏` 与模态守卫
  在 `app_key_filter`（`main.rs:2059-2118`，**必须在 ctrl+字符筛选之后**——踩过坑）；
  换文件关菜单先例 `apply_fresh_closes_export_menu`（`main.rs:2837-2849`）。
  `Overlay` 是全屏 scrim 模态；框架另有弹层通道（`Dropdown` 走它，`settings.rs:618-623`）。
- **持久化现状**：`config.toml` 仅两键（主题+侧栏显隐）**朴素行解析 + 整文件覆盖写**，
  红线「两个键必须同源写」（`src/config.rs:56-101`）；塞结构化列配置会撑爆它——
  `license.key` 已是「同目录独立文件」先例（`license.rs:249`）。无 per-file 状态；书签
  纯内存。`save_config` 测试构建无注入路径直接 `panic!`（`main.rs:606-613`）——
  「测试不得写真实配置」家法的封法。
- **测试范式**：合成几何注入（`cell_fixture`，`view.rs:5541-5575`）+ temp 路径硬 panic
  封死真实桌面副作用 + 面板几何守卫（`panel_contents_fit_fixed_height`，`settings.rs:1030`）
  + 一致性锁（`shortcut_card_*_matches_dispatch` 族）+ **真 paint 接线锁**
  （`log_view_paint_wires_the_update_dot`；v1.0.2 教训：绘制锁的 paint 原点必须含
  非零平移态）。**不许调会弹真对话框的入口**（export 评审事故有案）。

## 设计决策

### D1: 状态真身 = 产品层 `ColumnConfig`，引擎采样宽降级为「初始建议宽」

- 新文件 `src/columns.rs`（纯逻辑 + 存取 + 对账，`export.rs` 同款组织）：
  `ColumnConfig { order: Vec<String>, hidden: Vec<String>（或 set）, widths: HashMap<String, f32> }`
  ——三件套 = 三个映射，列名 exact 匹配（JSON 键名大小写敏感）。
- 有效列宽 = `widths.get(name).copied().unwrap_or_else(|| 采样宽实测)`；
  **不改写引擎 `Column.width_chars`**，`discover_schema` 重跑/换文件零互踩。
- 显示列序列 = `order` 过滤 `hidden`；`order` 缺省 = schema 首见序（与表格/CSV 同口径底）。

### D2: 三机制手势口径（裁定②落点）

| 机制 | 手势 | 细节 |
|---|---|---|
| 列宽 | 表头列间**手柄**按住拖动 | 手柄热区 = 列边界 ±`HANDLE_HALF=4.0`px，**手柄优先于表头体**；**双击手柄**恢复该列采样宽（`is_double_click` 先例） |
| 重排 | **表头体**按住拖动换位 | 手柄热区外按下潜伏，超 `CLICK_DIST=4.0` 升级为换位拖拽（文本框选同款升级）；拖拽中画**插入位指示**（accent 细线）；落下改 `order`；**Esc 放弃**（连潜伏按下一起清，先例同款） |
| 显隐 | 「列管理」弹层开关行 | 见 D3 |

- 表头体**单击无动作**（不排序，见 Out）；未超阈值抬起=无事发生。
- 手柄 hover 高亮（paint 侧细线加粗/变色）提供发现性，照 `settings_hover`/`export_hover`
  先例由 `CursorMoved` 写 hover 态。
- 拖拽期间视图局部态跟手预览（`h_drag` 写 `x_offset` 不经应用层的同款先例），
  **抬起才提交**进 `ColumnConfig` 并落盘；Esc/放弃 = 回拖前值。

### D3: 「列管理」弹层 = export 菜单范式（裁定①），ASCII 勾选行

- 入口两个：表头右端「列…」文字按钮（**固定角落，不随 `x_offset`**，hit rect 常算
  吞并先例）+ 表头右键（右键已是独立输入，「右键不再冒充左键」已修）——同一 Msg。
- 形制照抄 export 四件套（`Overlay + bind_open + on_scrim_click + format_btn` 行式），
  新增 6 处接线照调研清单走（开关字段 / Msg / 卡体 / `Stack` 尾 child / Esc 次序插入 /
  换文件关菜单）。
- 卡体内容：每列一行开关（`[x] 列名` / `[ ] 列名`——**ASCII 勾选框**，danqing 字体是
  GB2312 子集无几何符号，`+`/`-`/`[ ]` 先例）+ 底行「恢复默认」（清该文件手动宽/显隐/序，
  回采样宽+全显+首见序）。
- **至少一列可见**：关最后一可见列的开关行不可用/点击拒绝并提示（D6）。
- Esc 次序插入：`upgrade_prompt > settings_open > col_menu_open > export_menu_open > 栏`；
  模态守卫条件补 `col_menu_open`（位置维持 ctrl+字符筛选**之后**）；`col_menu_open` 与
  `export_menu_open` **互斥**（开一个先关另一个，防双 scrim 叠）。

### D4: 轻持久化 = 独立 `columns.json`，per-路径 key，列名对账，LRU 上限（裁定③落点）

- **载体**：`config_path()` 同目录 `columns.json`（`license.key` 独立文件先例），
  `serde_json` 读写（既有依赖，**零新依赖**；`config.toml` 朴素解析器不碰）。
- **粒度**：per-**文件路径** key（「这个日志我调好了列」的用户心智；全局单套会跨文件
  互相污染）。结构：`{ "files": [ { "path", "order", "hidden", "widths", "updated" } ] }`，
  **LRU 上限 64 条**（`updated` 排序淘汰，条目极小但不无限长）。
- **对账 merge**（schema 是采样产物，同文件重开列集合可能变）：以**当前 schema 首见序**
  为底，remembered `order` 里的列名依次取出重排，新列按首见序补尾；`hidden`/`widths`
  按列名命中，**失配列静默剔除**（不报错不猜）；live-tail/重开增列 → 新列默认值
  （采样宽+可见+补尾）。
- **写盘时机**：配置变更即写（`save_config` 先例同款）；换文件 `apply_fresh` = 载入
  该路径条目（无则默认）；空文件态无配置概念。
- **容错**：文件缺失/解析失败 → 空配置起步**不炸不覆盖**（下次保存再写好）；
  `columns_path` 测试构建无注入路径直接 `panic!`（与 `cfg_path`/`license_path` 同规，
  「测试不得写真实配置」）。
- 与腿四的接缝：`ColumnConfig` 模型 + 读写函数即「共用状态持久化基建」本体，
  `workspace-sessions` 将来存命名会话时**复用此模型**（会话应用=写穿 per-file 条目），
  不再另造第二套。**已兑现（2026-09-24 workspace-sessions T1/T2）**: `SessionEntry.config`
  就是 `ColumnConfig`, 应用 = 换入 + `merge_columns` + `save_state` 写穿。

### D5: 作用面 = 表格渲染；显示配置不影响交付物

- 生效面：**主行单元格渲染**（列宽/列序/显隐）；展开子行的 (路径, 值) 渲染**不**受列配置
  影响（子行不是列）。
- 入口与手势都在表头 → **仅 `table_mode()`（`mode==Table && schema.is_some()`）可交互**；
  Ctrl+T 切 Raw 时配置**原样保留**（无表头无入口），切回 Table 即生效——模型存在与否
  跟随文件，不跟随 ViewMode（与 export 的 `schema.is_some()` 判据同哲学：视图切换不改
  文件的属性）。
- **显示配置不影响导出/分析**：CSV 列序列集维持 export D5「schema 首见序全列」口径，
  分析面板字段列表同理——显示归显示，交付归交付，别人拿到的交付物可预期。
  补回归锁防将来「顺手跟随」。

### D6: 约束与兜底

- **≥1 可见列**：最后一可见列不可隐藏（弹层开关行拒绝 + 零状态变更）。
- **宽 clamp 像素常量**：`MIN_COL_W=40.0` / `MAX_COL_W=512.0`（event 无 `TextBatch` 量不了
  字符宽，clamp 只能常量；数值 plan 阶段可调，语义是「别拖没/别拖爆」）。
- 隐藏列**仍占 `MAX_COLUMNS=16` 发现名额**（隐藏≠删除，不重跑发现）。
- 「恢复默认」= 该文件条目整体回默认并落盘（D3 底行）。

### D7: 免费层白送，不接 `Feature` 门控

地图定死（模块表依赖列为空，欠账表「凡属 v1 欠账者一律进免费层」）。显式写死防止后人
「顺手」接升级提示——列配置接付费墙 = 「扣走我本来就该有的东西」的教科书形态。

### D8: 光标与发现性 = 零框架改动首版

- 手柄 hover 高亮（自绘）+ 常规 `Pointer` 光标；**首版不加 `CursorIcon::ColResize`**
  （滚动条拖拽先例也没有特殊光标，一致性优先），列进打磨/后补联动（Open Q①）。
- 本模块**零 danqing / danqing-logfile 改动**（拖拽/弹层/双击全是既有能力）——
  与 export 的「一处引擎访问器」不同，本模块纯产品仓。

## 成功判据

**机器可验**：

1. 状态模型纯逻辑：三件套变更/恢复默认/最少一列可见守卫/宽 clamp，边角（空 order、
   全隐藏拒绝、重复列名容错）全锁
2. 对账 merge：增列补尾 / 减列剔除 / 重开取回 / 失配静默 / 列名 exact 匹配，五态各一锁
3. 持久化 roundtrip 全等 + LRU 淘汰 + 损坏文件容错不炸不覆盖 + `columns_path` 测试
   panic 封死
4. 几何：手柄热区命中与表头体分离（±4px 边界值）/「列…」hit rect / 拖宽 clamp /
   全隐藏拒——均 paint 缓存读，断言与 paint 同源
5. 手势：按下超 `CLICK_DIST` 升级换位 / 落点重排正确 / Esc 放弃（含潜伏按下）/
   拖宽跟手、抬起提交、Esc 回滚 / 双击手柄恢复采样宽（300ms/4px 判据同款）
6. 弹层接线：开闭 / Esc 次序（含 `col_menu` vs `export_menu` 互斥）/ 模态守卫补项 /
   `apply_fresh` 关菜单（先例锁同款）
7. **真 paint 接线锁**：手动宽生效后字形 x 位移正确，**paint 原点含非零平移态**
   （v1.0.2 平移不变锁教训）；隐藏列零字形产出、列序变化字形序跟随
8. 显示配置不影响导出回归锁：摆列后 CSV 仍 schema 首见序全列（export D5 口径不动）
9. 三件套全绿（fmt / clippy 0 / 测试全绿），**基线 314 不破**；关键锁 A/B 精确红
   （对账 / 持久化 / 接线三处必录）

**人工验收**（用户实机）：

1. 列间手柄拖宽窄跟手、双击恢复采样宽；手柄 hover 有反馈
2. 表头按住拖动换位，插入位指示清楚，Esc 放弃，落下后列序正确
3. 「列…」（按钮与右键）弹层勾显隐，最后一可见列关不掉；「恢复默认」一键回初始
4. 摆好后重启记住；换一个文件各记各的；损坏/删除 `columns.json` 后 app 如常
5. 深浅两套主题下手柄/插入位指示/弹层可辨；Ctrl+T 切 Raw 再切回配置还在

## 已知局限

- **列管理弹层是全屏 scrim 形制**（裁定①=复用 export 范式，范式零风险）——比
  「贴表头的轻量 popover」重；实机若嫌重，可换框架弹层通道（`Dropdown` 同通道），
  属形制微调不动口径
- 宽 clamp 是像素常量，不随字体度量自适应缩放（`FONT_SIZE` 是常量，实际无此风险面）
- 隐藏列仍占 `MAX_COLUMNS=16` 名额；schema 采样 512 行后新字段不进列集是既有边界，
  对账只处理「列集变化」不重跑发现
- `widths` 存像素值——若将来主题字号可变，旧像素值语义漂移（现字体恒定，记录在案）
- 同一文件用不同路径写法打开（大小写/8.3 短名/符号链接）会算两个 key（路径 exact）；
  实机撞到再归一化
- 表头拖拽换位的拖拽中 ghost/整列预览只做**插入位指示**（最小可辨），不做整列浮起
  （自绘成本高、收益边际）
- **滚出视口的列只能排进前导槽/尾槽**——换位缝隙按「在视列中点」计数，横滚中间
  态下无法把在视列插到左滚出列之前/右滚出列之后（指示线与落点一致，属能力边界；
  评审 B3。要支持需把缝隙升为全 order 槽位并处理滚出区不可见指示）
- 「列…」按钮固定右角、与列区几何重叠：重叠带命中**手柄胜**（评审 B1 裁决，
  与公共边界「左列手柄胜」同款文档化），按钮主体仍可点；宽表下按钮文字与列分隔线
  可能视觉交叠（按钮无底色，形制微调后补）

## Boundaries

- 注释/文档中文；新 `.rs` 文件头 `//! @author 十四叔` + `//! @date 2026/09/23`
- **零新依赖、零框架/引擎改动**（serde_json / rfd / danqing 既有；拖拽弹层双击全有先例）
- 五段流水线：spec 批准 → plan → build → review → code-simplify，spec 后不立即编码
- 未获用户指示不 commit/push
- 不改写引擎 `Column.width_chars`；不动 `config.toml` 两键朴素解析器；不接 `Feature` 门控

## Open Questions

1. ~~**`CursorIcon::ColResize` 光标要不要顺手联动**~~ **已裁（2026-09-23 plan）：
   首版零联动**——`CursorIcon` 仅 `Default/Pointer/Text`（`event.rs:40-48`），滚动条/
   拖选两条拖拽先例均无特殊光标（一致性），手柄 hover 高亮已给发现性；`ColResize`
   列**后补联动**，实机反馈驱动
2. ~~**列管理弹层内要不要重排微调行**~~ **已裁（2026-09-23 plan）：不进首版**——
   裁定②已定拖拽为主手势；实机喊累再补上移/下移两枚小按钮（低成本后补）

## 实现记（2026-09-23 build 收口 T1–T6, 348 测试绿）

- **状态归属**: `ColumnConfig` 真身在 `LogApp`（D1）, `Widget::sync` 拷入 `LogView`
  并对账（每帧幂等 merge, ≤16 列可忽略）; 拖宽预览 = view 局部态（`ColWDrag`,
  `x_offset`「不经应用层」同款）, **抬起才发 Msg 提交** —— Esc/放弃零半提交（D2,
  锁 `width_drag_escape_discards_preview_without_msg`）。
- **几何**: `header_geom: RefCell<Vec<HeaderSpan{x0,x1,schema_idx,display_idx}>>` +
  `cols_btn_rect` paint 写 event 读（D2 铁律: event 无 TextBatch）; **空配置容错** =
  schema 首见序全显采样宽（未 sync 夹具/首帧前）。`display_idx` 记显示序全集位,
  换位缝隙 = 前导槽（滚出左缘列）+ 在视列中点计数。
- **换位落点**: `move_name_before(name, before)`——`before==name` = 原位 no-op,
  before 失配 = 排尾; 缝隙语义与在位几何一致（拖到「原位缝隙」=不动）。
- **持久化**: `columns.json` 独立文件（`license.key` 先例）, serde_json **Value 手拼**
  （serde derive 不在依赖 —— plan 核实②定案, 零新依赖）, per-路径 **LRU64**,
  损坏/缺失容错**零写回**; `columns_path` 无注入 `panic!`（`save_config`/`license_path`
  同规封法）。写盘时机 = 变更即写（`save_config` 同哲学）。
- **弹层**: export 四件套照抄; `format_btn` 泛化收 `impl Fn() -> Msg`（动态列行要
  捕获列名; 框架 `Button::on_click` 本就收 `impl Fn`, 原签名是自设的窄门）;
  弹层数据**取值传入**（引用被 'static widget 捕获 —— `export_menu_card` 同款教训）;
  Esc 次序插层 `upgrade > settings > col_menu > export_menu`; 模态守卫与滚轮
  不穿透补 `col_menu_open`; 与 `export_menu_open` **双向互斥**; `apply_fresh` 关菜单
  + schema 就位后载入 per-路径记忆。
- **panic 封死当场抓 3 条既有测试**（`new_empty()` 触 apply_fresh 读 columns.json →
  lvlcnt/freshfocus/freshinv 补 `new_empty_at` 注入）—— save_config 先例的
  「让它写不出去」守卫按预期起效, 评审记一笔。
- **两把回归锁**: 真 paint 平移不变字形锁 `column_config_moves_glyphs_under_nonzero_
  origin`（原点 (137,89), v1.0.2 教训: 绘制锁原点必须含非零平移态）+ CSV 不跟随锁
  `csv_columns_ignore_column_config`（export D5 口径不动, `export_csv_columns` 收口）。
- **机器判据 1–9 全过**（348 绿; A/B 三红见 `tasks/todo-v1x-table-column-config.md`
  Checkpoint A/B: 对账 / 持久化 / paint 预览）; Open Q①② 维持 plan 裁定未动。

## 评审记（2026-09-23 review 阶段：双路独立评审 + 并账修复闭环）

**双路互不知情**（export 先例同款）：①五轴全量路（正确性/可读/架构/安全/性能）
②三区深潜路（手势状态机互斥与打断 / paint-event 几何同源 / 持久化对账生命周期）。
两路均 Request changes；并账去重后 **Critical×1 + Required×6**，全部修复、每修一锁。

### 修复清单（并账去重）

| 级 | 缺陷 | 来源 | 修法 | 锁 |
|---|---|---|---|---|
| Critical | 表头右键开列管理是**死代码**——`button != Left` 提前返回吞掉右键分支，D3 第二入口从未接通 | 五轴 C + 深潜 A2 | 右键表头上移到左键筛选**之前**（仅表头 y 带；表头外维持 P29 不冒充左键） | `header_right_click_opens_col_menu`（含中键不冒充/表头外不触发） |
| Req R① | 手柄零位移单击无条件提交宽，采样宽被冻成手动宽 | 五轴 R① | `preview_w != start_w` 才提交 | `zero_move_handle_click_commits_nothing` |
| Req R② | 换文件/rebuild/Ctrl+T 打断手势不清态，抬起按旧 `schema_idx` 写进**新文件错列** | 五轴 R② + 深潜 A1 | sync 失效守卫清三手势态（换 Arc/换模式同一作废因，覆盖三条路径） | `file_change_discards_header_gestures_before_release` |
| Req R③ | columns.json 外部数据三连：hidden 重复 → usize 下溢破 D6（debug 崩/release 回绕）/ order 重复画两列 / `1e308`→inf 入库 | 五轴 R③ + 深潜 C1/C3 | `set_hidden` 改过滤计数；`entry_from_value` + `merge_with_schema` 归一（去重 / ⊆ order / 有限值） | `dirty_hidden_cannot_break_min_visible_guard` / `merge_dedups_order_and_hidden_drops_stale_names` / `entry_from_json_normalizes_dirty_lists_and_widths` |
| Req C2 | merge 后可 0 列（唯一可见列被 schema 剔除） | 深潜 C2 | merge 末尾 ≥1 可见兜底（取消隐藏 order 首列）——与 R③ 同一处不变量收口 | `merge_restores_min_visible_when_survivors_all_hidden` |
| Req B1 | 「列…」按钮盖末列手柄，宽表主手势点不中 | 深潜 B1（五轴 Opt③ 升级） | 命中优先级改**手柄 > 按钮 > 表头体**（重叠带手柄胜，文档化裁决） | `handle_hits_before_cols_btn_when_overlapping` |
| Req B2 | 右缘截断列手柄热区在屏外（视觉边缘点不中；hover 也画在屏外） | 深潜 B2 | `HeaderSpan.handle_x = x1.min(text_right)`，命中/hover 同源；`x1` 仍为真布局缘（拖宽起算宽不受影响） | `truncated_column_handle_pins_at_visible_edge` |

### Optional 裁决（修 8 + 预防锁 1 / 文档化 1）

- **修**：FocusOut 清手势（深潜 A3，`focus_out_discards_header_gestures`）/ 手势中
  滚轮锁（深潜 A4，`wheel_is_frozen_while_header_gesture_active`——首版测在
  `x_offset=0` **假绿**（clamp 钳回 0），改可动偏移后真红 `0.0 ≠ 30.0`）/ 双击锚点
  只记手柄命中（深潜区一 ColsButton 串扰，`only_handle_press_records_double_click_anchor`，
  顺带消表头体↔手柄 4px 边界串扰）/ `apply_rebuild` 补对账（深潜 C4，
  `apply_rebuild_merges_column_config_with_new_schema`）/ LRU 同秒并列保护本次条目
  （深潜 C5 = 五轴 Opt②，`lru_put_same_second_survives_truncation`）/ 坏 width 键
  **丢键不丢条**（深潜 C6，并入 entry 归一锁）/ 弹层行序跟 order（五轴 Opt⑤，
  `col_menu_rows_follow_column_config_order`）/ ToggleColumn 未知列零提示
  （五轴 Opt⑥，`toggle_unknown_column_reports_nothing`）。
- **预防锁**：分析面板 D5 对等锁（五轴 Opt①，`picker_fields_ignore_column_config`
  ——面板字段列表本就取 schema 首见序，行为已正确；锁防将来「顺手跟随」显示配置，
  A/B 红不适用）。
- **文档化**：滚出列排序边界（深潜 B3）→ 已知局限两条（缝隙槽位 / 按钮重叠视觉）。
- **随 C6 缓解不另修**：`save_columns` 覆盖写丢手写坏条目（五轴 Opt④）——载入归一后
  粒度已对齐 D4「失配静默剔除」，整条不可辨者跳过属设计语义。

### Nit×5（裁决不阻塞，留给 code-simplify 阶段统一处置）

`move_column` 边界写法 / `HeaderHit::None` 撞名 / `last_is_move` 名不副实 /
HashMap 序列化序无保证 /「排尾」注释措辞。

### 修复验证

三件套全绿：fmt / clippy `-D warnings` 0 / **365 测试**（348→365，+17 锁：
columns 5 + view 8 + main 2 + settings 1 + analysis_panel 1）。**A/B 红记录**：八条
行为锁全部先红后绿（C1 下溢红 = `attempt to subtract with overflow` panic 实锤；
wheel 锁修正假绿后红）。既有锁 `header_geometry_follows_x_offset` 随 B2 语义调整
视口（原断言打在截断列的屏外布局缘——正是 B2 病灶形态；改 800px 视口保「命中跟
平移走」原意）。P29 右键/中键锁、双击/换位/Esc 手势锁、真 paint 字形锁、CSV D5 锁
全数保持绿。

## 简化记（2026-09-23 code-simplify：本模块改动面 + review defer Nit×5，365 绿行为零变化）

- **Nit×5 全清**：①`move_column` 边界 `to > len-1` → `to >= len`（顺带消空表
  下溢隐患）+ 去恒等 `.min()` ②`HeaderHit::None` → `Miss`（与 `Option::None`
  撞名误读）③测试闭包 `last_is_move` → `msg_count`（实为计数，名不副实）
  ④`entry_to_value` 的 `widths` 键**排序后写入**（HashMap 迭代序随进程变，
  不排则同一摆法两次落盘字节面不同）⑤`move_name_before` doc 补「before 失配
  = 排尾」（原只写了 `None` = 排尾）
- **消重复形状**：表头手势三态判据 ×3（CursorMoved 跟手 / 滚轮锁 / 抬起收口）
  收 `header_gesture_active()`；三态清场 ×3（sync 失效守卫 / FocusOut / Esc）
  收 `abandon_header_gesture()`
- **不动清单**：`merge_with_schema`/`entry_from_value` 的 Vec+contains 去重
  （≤16 列常量成本，与 `visible_names` 同风格，换 HashSet 反引入第二风格）/
  测试重复夹具行（DAMP，与既有测试同款摆法）/ `on_header_*` 内部定点 take/置
  `None`（三态各自的定点操作，与「清场」helper 职责不同，不混）
- 验证：三件套全绿 **365**（断言零改动——②③仅标识符重命名），clippy 0，fmt 干净

## 人工验收

**2026-09-23 记账**（用户裁定「人工验收全部记账」）: 延后待实机, 六条（spec 五条
+ 评审验点「截断列边缘拖宽」）汇总在 `tasks/acceptance-pending.md` **A 组**; 机器
可验部分（1–9）已全过（365 绿）。实机后回填结论:

**2026-09-27 用户实机六条全过**（总清单 A1–A6, 逐项过, 无缺陷回填）:
手柄拖宽/双击恢复/hover 反馈; 拖拽换位（插入位指示/Esc 放弃/落下列序）;
弹层显隐（「列…」按钮与表头右键两入口）/末列关不掉/恢复默认; 持久化
（重启记住/换文件各记各的/坏 `columns.json` 如常）; 深浅主题与 Ctrl+T 往返
配置原样; 截断列可见右缘点得中可拖宽（评审 B2 验点）。A3 的勘误验点
（列管理行随文件换）由总清单 **E5** 兼验通过（见
`SPEC-v1x-field-picker-ui.md` 人工验收节）。

**勘误（2026-09-23 晚, field-picker-ui 事实盘点揭发）**: 本模块「显示列」弹层
行集原为 `view()` **建树快照** —— 框架 `app.view()` 一次性建树不再重建（六处
框架文档铁律）, 建树时 schema 为 None, **结构上弹层只剩「恢复默认」一行**
（换文件亦不更新）; 双路评审与机器锁均未触到框架生命周期面（锁了纯函数/Msg/
paint, 无「建树后行跟随」锁）。**已随 `SPEC-v1x-field-picker-ui` T0 修复**
（RowList 自绘行集每帧取态, 判罪锁
`col_menu_rows_follow_schema_across_sync`）—— 人工验收 A3 项由总清单 **E5**
兼验。
