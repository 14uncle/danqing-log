# plan-v1x-workspace-sessions: 命名工作台会话任务拆解

- @author 十四叔
- @date 2026/09/24
- Spec: `docs/specs/SPEC-v1x-workspace-sessions.md`（2026-09-24「go」批准:
  范围①-⑥ + Open Q①②③④全按推荐）
- 状态: **待过目** —— 批准后进 build（`/build auto` 惯例: 零 commit、TDD 循环不减）
- 测试基线: **393**（2026-09-24 field-picker-ui simplify 收口实测）

## 组件表

| 文件 | 职责 | 改动面 |
|---|---|---|
| `src/columns.rs` | 账本改名 `state.json` + `SessionEntry` + `sessions` 段（容错/恒写/迁移/按路径替换） | 模型扩展 + 改名（序列化面**无** `bookmarks` 键 —— Open Q1 strip） |
| `src/main.rs` | `sessions`/`session_selected`/`session_menu_open` 状态 + save/apply/delete 链 + 5 个 Msg + 门控两道闸 + 弹层族扩员（Esc/互斥/模态） | 状态 + 臂 |
| `src/settings.rs` | `sessions_card` 弹层（**RowList 第三消费者**）+ 命名输入持有者（`PickerInput` 泛化） | 新卡 + 既有小件零行为变化泛化 |
| `src/view.rs` | 状态栏「会话」按钮（`导出` 同排同规; 无文件不出） | 按钮 |
| `src/license.rs` 等 | **零改动**（`Feature::WorkspaceSessions` 已预埋） | — |

## 关键实现事实（2026-09-24 开工核实）

1. **付费门已预埋**: `license::Feature::WorkspaceSessions` + label「工作台会话」;
   `allows` + `ShowUpgradePrompt` 统一升级提示 + 两道闸纪律（入口判 + update
   兜底）全在 —— 本模块只接线不造门。
2. **`save_state` 是读改写**（load 账本 → `put` 当前条目 → `save_to`）: `sessions`
   段只要 `from_json`/`entry_to_value` 恒写就自动跨保存保真; 写入侧再补
   `put_sessions_for_path`（**只替换当前路径的切片**, 其他路径会话照留）——
   摘恒写 = roundtrip 红（A/B）。
3. **改名一次到位**（bookmark-persist 既定裁定「腿四扩多套会话时统一改名
   `state.json`」）: `default_path` 换名; `main::columns_path` → `state_path`
   （**panic 封死家法随迁**）; 载入走**读旧写新迁移**（旧 `columns.json` 存在
   且新名无 → 读旧; 落盘只写新名）—— 零真实用户, 但迁移写上防手工建过的人。
4. **过滤组合 = 单串**: `apply_level_filter` 走 `Msg::ApplyFilter`（级别点选已
   折叠进 `filter_applied`, 无第二态）; 会话存 `filter_applied`/`search_query`
   **查询串**（D2: 应用重跑即真相, 不存行集 —— AsyncJob 代次拒旧先例护在途）。
5. **展开态现算**: `ExpandMap` 键 = 文件行号, `sub_rows` 惰性 parse（`toggle_expand`
   填）。会话存**行号表**（升序去重）; 应用时逐行现算子行 —— 越界/无子行**静默
   剔除**（书签越界剔除先例）, `expand_rev += 1` 作废旧选区。
6. **RowList 五闭包契约照用不改**（`pick_list.rs`）: rows_fn/highlight_fn/on_pick/
   more_fn/max_rows 已够本场景（点行=应用+记选中, highlight=选中名）——评审
   留档「第三消费者」在此落地, **契约零变化**（若 build 中发现缺口, 先零变化
   演进再消费）。
7. **命名输入 = `PickerInput` 持有者同构**（R3 教训钉死: **不许** `on_change`
   镜像; TextInput 无 set/insert API 只有 rev 清空 —— 删除指针**不能**靠输入框
   预填, 见衍生设计）。落地形态 = 把 `PickerInput` **零行为变化泛化**为
   「提交钮 + TextInput」持有者（构造收 `submit_fn: Fn(String) -> Msg`）, picker
   传 `Msg::PickerSubmit`、会话传 `Msg::SaveSession` —— 单一事实源, picker 锁
   零回退。
8. **弹层族第四员**: 三弹层互斥收口 `close_popovers`/`popover_open`（field-picker
   simplify 落成）直接扩员; Esc 插层 `upgrade > settings > session_menu > picker >
   col_menu > export_menu > 栏`; 模态门禁（`App::event` 键盘/滚轮 + `app_key_filter`
   Ctrl 守卫）清单同步; 换文件关弹层清草稿（picker 同规, `session_clear_rev`）。
9. **状态栏钮先例**: `导出`/`设置` 按钮在 `view.rs` 底栏 paint/event（hit rect
   同源缓存）;「会话」同排同规, **无文件不出**（Open Q3: 空态不出入口）。
10. **测试文化**: 逐测唯一临时文件 / `new_empty_at` 注入 / `state_path` panic
    封死 / A/B 摘除验红 / 真 paint 锁（非零平移原点）/ 表驱动容错 / 测试严禁
    真实桌面副作用。

## 衍生设计

- **`SessionEntry` 契约**（`columns.rs`）:
  `{ name: String, filter: String, search: String, order: Vec<String>,
  hidden: Vec<String>, widths: Map<String, f32>, expands: Vec<u64>, updated: u64 }`
  —— 列三字段**沿 `ColumnConfig` 的序列化形状**（同容错同模型, D1「不再另造」）;
  序列化面**无 `bookmarks` 键**（Open Q1: strip 是结构保证不是运行时剥离）。
  载入容错 = 坏条**丢条不丢账**（`name` 缺/空/重复名去重 = 保首见; `expands`
  剔非数值并升序去重; 收集硬顶 `MAX_SESSIONS*4` 防天文数组, 书签先例）。
- **account 形状**: `{ "files": [ …既有… ], "sessions": [ { "path", …SessionEntry
  字段…, "updated" } ] }`（平铺含 `path`, 与 `files` 数组同风格）; `sessions`
  恒写（空 = 空数组）; 不吃 `files` 段 LRU。
- **弹层交互（D5 精化）**: 点行 = **应用并记为选中**（应用是热路径, 一击直达;
  高亮 = 最近点选名 —— `highlight_fn` 现成）; 「删除」作用于**选中行**（无选中
  → 提示「先点选会话」; 无确认, Open Q4）; 命名输入 + Enter/「保存当前」同路
  = 保存/覆盖（输入即名字, 不依赖预填 —— TextInput 无 set API 的正解, 不绕）。
  应用成功即关弹层清草稿（picker 提交先例）。
- **apply 顺序**（写穿 + 异步链）: ①列换入 `merge_columns` + `save_state`（写穿
  per-file 条目, 接缝定案）②`apply_filter(filter)` ③`apply_search(search)`
  ④展开重建（剔除 + `sub_rows` 现算 + `expand_rev+1`）⑤`top_row=0`/`selected=0`
  ⑥关弹层清草稿。**书签零触碰**（Open Q1, 前后全等锁）。
- **save 快照源**: `filter_applied` + `search_query` + `columns.clone()` +
  `expanded` 键集（升序去重）; 空名拒绝 + 提示; 同名覆盖 + 「已更新」
  （D6）; 满 32 拒绝（新名才计数, 覆盖不算新增）。
- **`Msg` 面**: `OpenSessionMenu` / `CloseSessionMenu` / `SaveSession(String)` /
  `ApplySession(String)` / `DeleteSession(String)`（前二 = 弹层开合互斥;
  后三 = 动作, 门控两道闸）。

## 任务

### T1: 账本改名 state.json + sessions 段（`columns.rs` + `main.rs` 路径侧）

改名（`default_path`/`state_path` panic 封死随迁/读旧写新迁移）+ `SessionEntry` +
`ColumnFiles.sessions`（恒写/坏条丢条不丢账/收集硬顶）+ `sessions_for_path` /
`put_sessions_for_path`（只替换当前路径切片）。

Acceptance: ①四样载荷 roundtrip 全等（含 `expands` 乱序重复收编）②坏条丢条
不丢账（表驱动: 缺 name/错型/天文数组）③迁移（旧名读入 → 新名写出零丢失,
双名并存时新名优先）④`state_path` 测试构建 panic 封死（家法锁）⑤摘 `sessions`
恒写 = roundtrip 精确红（A/B）⑥`put_sessions_for_path` 只动本路径切片。
Verify: `cargo test` + clippy 0。

### T2: 会话 save/apply/delete 状态链（`main.rs`）

`sessions`/`session_selected` 状态（`load_state_for_current_file` 载入该路径）+
`save_session`（快照四样/覆盖/上限）+ `apply_session`（写穿/全链/展开重建/回顶/
书签零触碰）+ `delete_session`（无确认）+ `Msg` 三臂 + `save_state` 扩写
（`put_sessions_for_path`）+ 换文件随路径换/清选中。

Acceptance: ①保存→载入→应用四样回来（过滤/搜索全链命中正确; 列**写穿** files
段落盘实证）②同名覆盖 + 满 32 拒绝 + 删除后再存 ③展开剔除两态（越界/无子行）
+ 合法行号现算正确 ④书签零触碰（apply/save 前后 `bookmarks` 全等锁）⑤摘写穿
= 落盘断言红; 摘展开剔除 = 断言红（A/B）⑥既有过滤/搜索/列锁零回退。
Verify: `cargo test` + clippy 0。

## Checkpoint A（T1–T2）

- [ ] 判据 1–3 全绿; A/B 红三处（摘恒写 / 摘写穿 / 摘剔除）在案; **基线 393
      不破**; fmt / clippy 0。

### T3: 会话弹层 + 状态栏入口 + 门控（`view.rs` + `settings.rs` + `main.rs`）

`PickerInput` 零行为变化泛化（`submit_fn` 注入）+ `sessions_card`（RowList 第三
消费者: 点行=应用+选中 / 命名输入 Enter 与「保存当前」同路 / 「删除」作用选中）
+ 状态栏「会话」钮（无文件不出）+ 入口判 `Feature::WorkspaceSessions`（免费态 →
`ShowUpgradePrompt`, 弹层不开）+ update 臂两道闸（Save/Apply/Delete 兜底）+
弹层族第四员全套（Esc 插层 / `close_popovers`·`popover_open` 扩员 / 模态门禁
清单 / 换文件关清草稿）。

Acceptance: ①真 paint 锁（弹层行/按钮字形产出, 非零平移原点; 无文件不出
「会话」）②门控两态（免费四动作全拦 + 账本零变化 + 升级提示; 付费放行;
**付费态永不触发升级提示**）③点行=应用+高亮选中 / Enter 与「保存当前」同路
（持有者收口锁, picker 泛化后锁零回退）④Esc 次序/互斥/模态/换文件关清草稿
全套锁⑤「删除」无选中提示 / 有选中即删（无确认, 说清）。
Verify: `cargo test` + clippy 0。

## Checkpoint B（T3）(机器部分)

- [ ] 判据 4–5 全绿; 三件套干净; **393 不破**。

### T4: 文档收口

ROADMAP 勾销「工作台会话持久化」/ README 能力行 + 边界 / map 行 / spec 实现记
+ 机器判据回填 / acceptance-pending 续 **F 组五条** / **三处兑现注记**:
bookmark-persist 已知局限「腿四改名 `state.json`」→ 兑现; col-config 接缝
「写穿 per-file」→ 兑现; `columns.rs` 腿四接缝注释（strip）→ 更新为结构保证 /
todo·CLAUDE.md·记忆落账。（无性能面 —— 会话 ≤32 条常量成本。）

## Checkpoint C（T4）

- [ ] 机器判据逐条回填; 人工验收五条**记账**进总清单 F 组; 状态全线落账。

## 风险与对策

- **删除指针 vs TextInput 无 set API**（衍生设计已解）: 点行 = 应用**并记选中**,
  「删除」作用选中 —— 不靠输入框预填（R3 同族: 不许发明镜像/绕持有者）。
- **`save_state` 读改写保 sessions**: 恒写 + `put_sessions_for_path` 只动本路径
  切片 —— 双锁钉（roundtrip + 他路径切片全等）, 免得保存会话把别的路径会话
  抹掉（bookmark-persist Critical「读改写覆盖抹全记忆」同族面, 提前封）。
- **迁移双名**: 读旧写新; 测试断言「新名落盘、旧名不回写」; 旧名残留只读不写
  （一次性迁移不留双账本）。
- **picker 泛化零回退**: `PickerInput` 改构造签名不动行为 —— picker 既有锁
  （`picker_input_enter_and_button_carry_value` 等）全程护航, 红即回退重想。
- **展开重建成本**: 逐行现算子行 = `toggle_expand` 同成本, 会话展开量级 ≤
  用户实际点开量; 无新性能面（说明记档）。
