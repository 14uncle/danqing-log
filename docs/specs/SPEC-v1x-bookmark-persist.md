# SPEC-v1x-bookmark-persist: 书签持久化（免费层欠账）

- @author 十四叔
- @date 2026/09/23
- 状态: **五段全闭**（2026-09-23 一日: 「go」三项全按推荐 → plan → /build auto →
  双路评审并账 Critical×1+Required×6 全修 → code-simplify, 376 测试绿, 零 commit,
  零框架/引擎改动）—— **人工验收（五条）记账**（总清单 D 组）
- 所属: 能力地图 `SPEC-v1x-map.md` 模块 `bookmark-persist`（构建顺序第 5 位，接
  `table-column-config` 之后；免费层欠账三连第二件；依赖: **无**——不接 licensing
  门控，免费层白送，复用 `table-column-config` 建的 per-路径状态持久化基建）

## Objective

书签是用户在大文件里「夹的书签」——夹好了、关掉应用就没了。ROADMAP §一欠账表
字面项：**书签持久化**（出处 `SPEC-core-viewer.md:29` 的「会话内」限定，2026-09-12
由 v1.x **改判回免费层**：「它是 v1 欠账不是新价值，锁进付费层会招骂」）。
现状 `bookmarks: BTreeSet<u64>` 纯内存（`main.rs:241` 注释自认「会话内有效，
持久化归 v1.x 会话功能」——那是改判前的旧话，本模块就是来还这笔账的）。

**成功长什么样**：`b` 夹好书签 → 关应用再开还在；换第二个文件各记各的；日志轮转
后越界的书签安静消失、其余照旧——与列配置同一套「这个日志我调好了」的直觉。

## 范围（2026-09-23 用户「go」裁定，全按推荐项）

1. **载体 = 并入 `columns.json` per-路径条目**（推荐①）：`FileEntry` 加
   `bookmarks` 字段，LRU/路径 key/容错/panic 封死基建整套复用（地图裁决④「与腿四
   共用状态持久化基建」的直接落法）。文件名不改（今天才落地零用户；命名历史包袱
   记已知局限，腿四扩多套会话时再统一改名 `state.json` 一次到位）。备选：独立
   `bookmarks.json`（代价 = LRU/容错基建抄第二遍，否）。
2. **上限每文件 256 条**（推荐②）：满则拒绝新增并提示（「至少保留一列可见」同
   守卫风格，说清为什么）；数值可调，语义是「正常使用永远碰不到，刷屏有闸」。
3. **失效语义 = 行号越界剔除**（推荐③）：书签存**文件行号**（现状语义不动，
   过滤模式下不漂移是既有锁）；轮转/截断/载入时按当前 `line_count` 越界剔除
   （`apply_rebuild` 现行为照搬进载入侧）。文件被原地改写而行数不变——行号会指错
   行，**不做内容哈希校验**（已知局限，与 `FileStat` 首块哈希区分轮转是两回事）。

**In**:

1. per-路径书签持久化：增/删（`b`/Ctrl+B）即写盘，重启恢复，换文件各记各的
2. 载入侧归一与容错：坏 `bookmarks` 字段**丢字段不丢条**（非数值/重复/乱序 →
   剔重排序收编，整条其余部分照留——评审 C6 同粒度）；越界行号剔除
3. 上限拒绝 + 底栏提示（推荐②）
4. rebuild/轮转既有越界剔除**保持**；追长（append）书签不动（行号仍真）

**Out**:

- 书签**命名/注释**、书签列表面板、书签搜索——账面无此诉求，不造
- 跨文件/全局书签（per-路径是「这个日志我夹了书」的直觉，与列配置同粒度）
- 「恢复默认」连书签一起清——那是**列配置**的恢复默认（D3），书签归书签，
  两个动作不混
- 书签随「命名工作台会话」导出/切换（腿四 `workspace-sessions` 的增值面）
- 内容哈希防错行（见推荐③）
- `b`/`'`/Ctrl+B/Ctrl+G 键位与交互的任何变化（纯持久化，零交互改动）

## 开工前事实盘点要点（2026-09-23 调研）

- **模型现状**：`main.rs:241-242` `bookmarks: BTreeSet<u64>`（文件行号集合，
  自动有序去重）；view 镜像 `view.rs:832-833` 同型，`sync` 克隆（`view.rs:1476`，
  用户量级逐帧克隆无感——注释自记）。纯逻辑 `next_bookmark`（`main.rs:1785`，
  严格大于+环绕+空集 None）**已有测试**（`main.rs:3534+`，MAX 不溢出/环绕/严格性）。
- **交互现状（本模块零改动面）**：`b`/Ctrl+B 切换选中行书签（`toggle_bookmark`，
  `main.rs:1752-1767`，按文件行号、过滤语义不漂移，status 绰「已添加/已去掉书签」）;
  `'`/Ctrl+G 跳下一书签（`goto_next_bookmark`，`main.rs:1769-1783`，报位次
  `书签 i/N`，空集提示「尚无书签 (b 添加)」）; 底栏常驻「· 书签 N」
  （`main.rs:1578-1579`）; 绘制 = 行号槽左缘 3px 金色竖条 + 金色行号
  （`view.rs:1874-1890`），书签色产品侧独立通道（`bookmark_color`，
  `view.rs:528-534`，回归锁 `bookmark_color_is_its_own_channel_per_theme`）。
- **生命周期现状**：换文件 `apply_fresh` **清空**（`main.rs:1273`）; 轮转
  `apply_rebuild` 越界剔除（`main.rs:1179` `retain(|&l| l < new_count)`）;
  追长不动。显示行→文件行号映射 `file_line_of`（`main.rs:1047`，子行归父行）
  ——书签锚文件行号，展开/过滤不影响锚点（既有语义，不动）。
- **持久化基建（直接复用，D4 全套现成）**：`src/columns.rs` 的 `ColumnFiles`
  = per-**路径** key + LRU64（`FILE_CAP`）+ `columns.json` 独立文件（`license.key`
  先例）+ serde_json Value 手拼（零新依赖）+ 载入侧归一（去重/⊆ order/有限值，
  评审 C1/C3/C6 收口）+ 损坏/缺失→空集合**零写回** + `columns_path` 测试
  `panic!` 封死（「测试不得写真实配置」家法）。写盘时机 = 变更即写
  （`save_columns`，`main.rs:702-720`）。**本模块就是给 `FileEntry` 加一个字段 +
  载入点接线**，不造第二套。
- **测试范式**：逐测唯一临时文件（`temp_columns_path`/`temp_log` 夹具族）/
  `new_empty_at` 注入（panic 封死，lvlcnt/freshfocus/freshinv 三教训在案）/
  A/B 精确红必录。

## 设计决策

### D1: 状态真身 = `FileEntry.bookmarks: Vec<u64>`（有序去重），内存态仍是 `BTreeSet<u64>`

- 磁盘形状（`entry_to_value`/`entry_from_value` 扩展）：条目加 `"bookmarks": [行号]`，
  **可缺省**（空 = 无书签，语义自洽）; 载入归一 = 剔除非数值/负数（JSON 无负整数
  进 u64 会拒收）/去重/升序收编为 `Vec`，再进 `BTreeSet`（C6 同粒度：坏键丢键
  不丢条）。
- `toggle_bookmark` 增删后 `BTreeSet → sorted Vec` 写回条目并落盘（变更即写）。
- 引擎零接触、view 零改动（镜像走既有 sync 克隆）。

### D2: 载入/失效语义 = per-路径恢复 + 越界剔除

- `load_columns_for_current_file` 泛化为 **`load_state_for_current_file`**（一次
  读盘同取列摆法 + 书签——同一文件读两遍是浪费，也是两次漂移机会）：
  `apply_fresh` 调用点不变（schema 就位后）。
- 载入书签按**当前 `file.line_count()`** 越界剔除（`apply_rebuild:1179` 同式），
  剔除后**不主动写回**（损坏零写回同哲学——下次变更才写好）。
- `apply_rebuild` 现有 `retain` 保持（语义已对）; `apply_appended` 不动。

### D3: 上限与守卫（D6 风格）

- `MAX_BOOKMARKS: usize = 256`：`toggle_bookmark` 新增时满员**拒绝并提示**
  （「书签已达上限 256, 先去掉一些」——「至少保留一列可见」同款：拒绝 + 说清
  为什么 + 零变更）。删除/跳转不受限。
- 存量超限的手造数据：载入**截断保升序前 256**（容错收编，不炸不报）。

### D4: 免费层白送，不接 `Feature` 门控

地图定死（模块表依赖列为空）。与列配置同理显式写死：书签持久化接付费墙 =
「扣走我本来就该有的东西」。

## 成功判据

**机器可验**（2026-09-23 build 收口已全过）：

1. ✅ `FileEntry` roundtrip 含 `bookmarks`（缺省/空/满三种形态）；载入归一锁
   （非数值丢弃/去重/升序/超限截断）——摘 `entry_to_value` 的 bookmarks 写出 = 精确红
2. ✅ toggle 增删即落盘、重读（`load_state_for_current_file`）恢复全等——摘落盘 = 红
3. ✅ per-路径隔离：同路径换书签覆盖，异路径互不污染（LRU 既有锁不破）
4. ✅ 越界剔除：rebuild/载入两路径各一锁（行号 ≥ line_count 消失，界内保留）
5. ✅ 上限拒绝 + 提示 + 零变更；删除照常
6. ✅ 坏数据容错：`bookmarks` 字段自爆不废整条（列摆法照留）；整文件坏 JSON 不炸
   不覆盖（既有锁不破）
7. ✅ 三件套全绿，**基线 365 不破**（收口 372）；A/B 精确红三处在案（见实现记）

**人工验收**（记账 → `tasks/acceptance-pending.md` 续 A 组编号）：

1. `b` 夹/去书签后重启还在；位次/计数显示如常
2. 换第二个文件各记各的
3. 大文件删行/轮转后越界书签消失、其余照旧
4. 手坏 `columns.json` 后 app 如常（书签回默认，不炸）
5. 满 256 条时新增有提示；去掉后又能加

## 已知局限

- 文件原地改写而行数不变时书签会指错行（行号锚语义; 不做内容哈希校验）
- ~~`columns.json` 名字只提列不提书签——命名历史遗留（今天才落地零用户），腿四扩
  多套会话时统一改名 `state.json` 一次到位~~ **已兑现（2026-09-24 workspace-sessions
  T1）**: 账本改名 `state.json` 落地, 旧名读旧写新迁移（`state_path`/`legacy_state_path`
  分支配对）, panic 封死家法随迁
- 书签无命名/注释/列表面板（Out 定死; 实机喊累另起）
- 越界剔除后不回写磁盘（损坏零写回同哲学）——但**固化触发器比预期宽**：此后
  任意变更落盘（含拖列宽）都会把剔除后集合写盘，轮转间隙窄窗口撞到会永久丢
  高行号书签（评审深潜 O5；已知边界, 实机撞到再收窄触发器）
- 栏聚焦（过滤/搜索栏持焦）时 Ctrl+B/Ctrl+G 进输入框不 toggle 书签（既有键位缝,
  Out「零交互改动」红线未动; 原始模式单键 `b` 只认小写, 与 Ctrl+B 的
  `eq_ignore_ascii_case` 不一致）——实机喊再收
- toggle 即整文件 RMW 落盘（变更即写, D2 明裁）; 满 256 连续夹书签 = 256 次
  全文件读改写, 嫌吵再 debounce（评审 Optional 留实机）

## Boundaries

- 注释/文档中文；**零新依赖、零框架/引擎改动**（纯 `columns.rs` 扩字段 + main 接线）
- 五段流水线：spec 批准 → plan → build → review → code-simplify
- 未获用户指示不 commit/push（今日四笔已获「commit」指示落账, 不含 push）
- 不动键位/交互/绘制；不接 `Feature` 门控；不改 `ColumnConfig` 语义

## Open Questions

1. ~~**载体改名要不要顺手做**（`columns.json` → `state.json`）~~ **已裁（2026-09-23
   「go」）：不做** —— 腿四扩多套会话时统一改名一次到位（已知局限记档）
2. ~~**上限 256 的数值**~~ **已裁（2026-09-23 「go」）：256 起步** —— 正常使用
   碰不到，实机嫌小再调（常量一处可改）

## 实现记（2026-09-23 build 收口 T1–T2, 372 测试绿）

- **磁盘模型**（T1, `columns.rs`）: `FileEntry` 加 `bookmarks: Vec<u64>`
  （**升序去重**收编态）; 归一收口唯一入口 `normalize_bookmarks`（sort+dedup+
  truncate 256）, `put` / `entry_from_value` 同用 —— 收编态进收编态出,
  roundtrip 全等好判。`entry_from_value` 逐元素容错（非 u64 丢弃）, 坏形状
  **丢字段不丢条**（C6 同粒度）; `entry_to_value` 恒写 `bookmarks` 键（空 =
  空数组, 不省略 —— roundtrip 判据的锚）。`get_entry` 新访问器, `get` 降级
  `.map(|e| &e.config)` 简写（测试调用点零改动）; `put` 改收 `FileEntry`
  （8 调用点, 测试构造器 `entry()` 收噪）。
- **全链路**（T2, `main.rs`）: `load_state_for_current_file`（原
  `load_columns_for_current_file`, 一次读盘同取两态; 书签按 `line_count` 越界
  剔除**不写回**）/ `save_state`（原 `save_columns`, 6 调用点: 5 个列配置 Msg 臂
  + toggle）/ `toggle_bookmark` 上限守卫（满 [`MAX_BOOKMARKS`] 拒绝 + 说清 +
  零变更）+ 增删成功即落盘。**次序陷阱按 plan 核实⑤拆除**: 删 `apply_fresh`
  载入之后的 `bookmarks.clear()`（替换语义只在 load_state 一处发生）——
  锁 `bookmarks_are_per_path_and_apply_fresh_replaces_with_memory` 伺候。
- **A/B 精确红三处**: ①摘 `entry_to_value` 的 bookmarks 写出 → roundtrip
  `[]≠[3,5,10]` 红 ②摘载入的 bookmarks 读取 → `[0]≠[1]`（A 残留进 B）红
  ③摘 toggle 的 `save_state` → 「增即落盘」红。
- **机器判据 1–6 全过**（372 绿 = 365 + 7 锁: columns 2 + main 5）; 基线不破;
  零 view/框架/引擎改动（书签镜像走既有 sync 克隆, 如设计所期）。

## 评审记（2026-09-23 review 阶段：双路独立评审 + 并账修复闭环）

**双路互不知情**（export / table-column-config 先例同款）：①五轴全量路（正确性/
可读/架构/安全/性能）②三区深潜路（磁盘归一与往返不变量 / 载入·换文件·失效
生命周期 / 上限守卫与行号语义）。两路均 Request changes；并账去重后
**Critical×1 + Required×6**，全部修复、每修一锁。

### 修复清单（并账去重）

| 级 | 缺陷 | 来源 | 修法 | 锁 |
|---|---|---|---|---|
| Critical | 坏 `columns.json` 后 `save_state` 读改写**覆盖成单条**——其余路径记忆（列摆法+书签）永久抹掉；`fs::write` 非原子加重（半截坏 JSON → 触发覆盖） | 五轴 C | ①`save_to` temp+rename **原子落盘** ②损坏备份守卫：非空文件解析为空账 → 先 rename `.bak` 再开新账（备份失败则拒绝覆盖） | `corrupt_state_file_is_backed_up_not_clobbered` |
| Req R1 | `file_line_of` 的 `unwrap_or((0,0))` 把空文件/过滤 0 命中/越界选中塌成**行 0**，幽灵书签落盘 = 跨会话污染 | 深潜 R1 + 五轴 O① 升级 | toggle 直取 `expand::file_line_at` 的 Option（None = 拒绝+说清+零变更）；save 侧按 `line_count` 过滤（与 load 同式） | `toggle_bookmark_rejects_ghost_rows` |
| Req R2 | `FILE_CAP=64` LRU 挤整条**静默丢真书签**（书签是用户内容不是偏好） | 深潜 R2 | 淘汰保护：无书签条目先挤（**即使更新**），全带书签才纯 LRU；被挤者原样返回，main 侧 warn 留痕 | `lru_evicts_bookmarkless_before_bookmarked` |
| Req R① | 落盘失败仍报「已添加/已去掉」——成功判据 1 静默违约 | 五轴 R① + 深潜 O3 升级 | `save_state` 返 bool；toggle 状态缀「(未落盘)」+ Warn notice。**修时再抓一个次序陷阱**：`set_notice` 内含 `refresh_status` 会冲掉后缀 → notice 必须先于 push_str | `toggle_says_truth_when_save_fails` |
| Req R② | cap 测试「删除照常」半边缺锁（守卫顺序若倒置会堵死腾位路径，测不红） | 五轴 R② | 测试改 257 行真夹具全走 toggle 行为（满员删已有书签照常 / 腾位再加）；实测现序正确 = **预防锁**（A/B 不适用） | `toggle_bookmark_refuses_at_cap_with_notice`（重写） |
| Req R③ | 坏 `bookmarks` 测试「列摆法照留」半边缺锁（丢摆法留空壳条目也绿） | 五轴 R③ | 补 `config.order` 断言 | `entry_from_json_normalizes_bookmarks_leniently`（扩） |
| Req R④ | `main.rs:241` 字段注释仍写「会话内有效，持久化归 v1.x」——与本模块语义相反的旧文字 | 五轴 R④ | 改「per-路径持久化 + 越界剔除」 | —（文字） |

### Optional / Nit 裁决

- **修**：手造天文数组收集硬顶 `MAX_BOOKMARKS*4`（超顶按收集序放弃，注释已记
  语义近似）/ `FileEntry` 腿四接缝文档（导出命名会话须显式 strip `bookmarks`）/
  Nit 同车（warn 文案「记忆状态落盘失败」/ toggle 收 `remove` 返回值 / `json!`
  空行随 fmt 归位）。
- **文档化**：栏聚焦 Ctrl+B/Ctrl+G 进 TextInput 不 toggle（既有键位缝，Out
  「零交互改动」红线未动）+ Raw 单键 `b` 只认小写 → 已知局限；「越界剔除不
  写回」的**固化触发器**细化（后续任意变更落盘会固化剔除）→ 已知局限。
- **留实机 / simplify**：toggle 即整文件 RMW 写放大（spec 明裁「变更即写」，
  实机嫌吵再 debounce）。
- **不动**：`columns.rs` 拆 `state.rs`（两路一致：腿四再堆会话态时同车改名
  `state.json` 一次到位）。

### 两路明确排除项（核对一致，非缺陷）

normalize 收编态单点性（put/from_json 双入口全过 `normalize_bookmarks`）/ 坏数据
无绕过（`as_u64` 对浮点·字符串·负数·超 u64 全拒）/ load 的 line_count 时机
（apply_fresh 先换 file 再 load）/ 替换语义两分支必达（无 early return 窗口）/
cap 255/256 边界正确且拒绝路径不谎报 / 双开 RMW 单实例顺序无丢更新 / 手造 259
截断丢高行号属 D3 明文。

### 修复验证

三件套全绿：fmt / clippy `-D warnings` 0 / **376 测试**（372→376，+4 锁：
columns 1 + main 3，另 2 锁扩半边）。**A/B 红记录**：四条新锁先红后绿（Critical
备份红 / 幽灵行号红 = 行 0 落盘 / 落盘失败谎报红 / LRU 淘汰保护红）；修程两处
当场纠偏（`split_off` 在 <64 条 panic → 补长度前提；测试「全带书签」场景写岔成
含无书签条目 → 实现本就正确，改测试钉准两阶段）。

## 简化记（2026-09-23 code-simplify：本模块改动面，376 绿行为零变化）

- **S1 抽 `backup_if_corrupt`**（唯一实质简化）：`save_state` 五职责长函数里的
  「损坏备份守卫」策略块收成命名步骤（Extract helper——概念有名字，`save_state`
  主干读作「守卫 → 收编 → 淘汰留痕 → 落盘」四拍）。
- **不动清单**（Chesterton's Fence 过后判「不为简化而简化」）：`entry_from_value`
  三段容错循环（order/hidden/bookmarks 谓词各异——去重 / 去重+⊆ order /
  as_u64+硬顶，合并反遮语义）/ `toggle` 的 `(added, saved)` 四元组后缀（显式即
  文档）/ `put`·`normalize_bookmarks`·`load_state` 均短函数 / merge 的
  Vec+contains 去重（table-column-config 简化轮不动清单维持）/ `get` 简写保留
  （测试在用的 pub API）。
- **留实机项**（非简化）：toggle 即整文件 RMW 写放大——实机嫌吵再 debounce。
- 验证：三件套全绿 **376**（断言零改动），clippy 0，fmt 干净。

## 人工验收

**2026-09-23 记账**（用户裁定「人工验收全部记账」）: 延后待实机, 五条汇总在
`tasks/acceptance-pending.md` **D 组**。实机后回填结论:

**2026-09-27 用户实机五条全过**（总清单 D1–D5, 逐项过, 无缺陷回填）:
`b` 夹/去书签重启还在、位次/计数如常; 换文件各记各的互不串; 删行/轮转后
越界书签安静消失、界内照旧; 手坏 `columns.json` 后 app 如常（书签回默认,
不炸）; 满 256 条新增有提示、去掉又能加。
