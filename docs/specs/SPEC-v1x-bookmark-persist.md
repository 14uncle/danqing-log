# SPEC-v1x-bookmark-persist: 书签持久化（免费层欠账）

- @author 十四叔
- @date 2026/09/23
- 状态: **spec 草案待批准**（2026-09-23 起草; 范围三项推荐随「go」一并裁定）——
  批准后进 plan
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

## 范围（三项推荐，随「go」一并裁定）

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

**机器可验**：

1. `FileEntry` roundtrip 含 `bookmarks`（缺省/空/满三种形态）；载入归一锁
   （非数值丢弃/去重/升序/超限截断）——摘 `entry_to_value` 的 bookmarks 写出 = 精确红
2. toggle 增删即落盘、重读（`load_state_for_current_file`）恢复全等——摘落盘 = 红
3. per-路径隔离：同路径换书签覆盖，异路径互不污染（LRU 既有锁不破）
4. 越界剔除：rebuild/载入两路径各一锁（行号 ≥ line_count 消失，界内保留）
5. 上限拒绝 + 提示 + 零变更；删除照常
6. 坏数据容错：`bookmarks` 字段自爆不废整条（列摆法照留）；整文件坏 JSON 不炸
   不覆盖（既有锁不破）
7. 三件套全绿，**基线 365 不破**；A/B 精确红（落盘/载入两处必录）

**人工验收**（记账 → `tasks/acceptance-pending.md` 续 A 组编号）：

1. `b` 夹/去书签后重启还在；位次/计数显示如常
2. 换第二个文件各记各的
3. 大文件删行/轮转后越界书签消失、其余照旧
4. 手坏 `columns.json` 后 app 如常（书签回默认，不炸）
5. 满 256 条时新增有提示；去掉后又能加

## 已知局限

- 文件原地改写而行数不变时书签会指错行（行号锚语义; 不做内容哈希校验）
- `columns.json` 名字只提列不提书签——命名历史遗留（今天才落地零用户），腿四扩
  多套会话时统一改名 `state.json` 一次到位
- 书签无命名/注释/列表面板（Out 定死; 实机喊累另起）
- 越界剔除后不回写磁盘（下次变更才写好——与损坏零写回同哲学）

## Boundaries

- 注释/文档中文；**零新依赖、零框架/引擎改动**（纯 `columns.rs` 扩字段 + main 接线）
- 五段流水线：spec 批准 → plan → build → review → code-simplify
- 未获用户指示不 commit/push（今日四笔已获「commit」指示落账, 不含 push）
- 不动键位/交互/绘制；不接 `Feature` 门控；不改 `ColumnConfig` 语义

## Open Questions

1. **载体改名要不要顺手做**（`columns.json` → `state.json`）：推荐**不做**
   （腿四一次到位），若用户偏好现在改成本约 5 处——随「go」裁定
2. **上限 256 的数值**：正常使用碰不到，实机嫌小再调——随「go」可改

## 实现记

（build 阶段回填）

## 评审记

（review 阶段回填）

## 简化记

（code-simplify 阶段回填）

## 人工验收

（记账 → `tasks/acceptance-pending.md`，实机后回填）
