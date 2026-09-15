# TODO: interaction-polish (交互打磨)

> spec: `docs/specs/SPEC-interaction-polish.md` | plan: `tasks/plan-interaction-polish.md`
> 矩阵 (**兼人工验收清单**, 清单见其 §6 —— **唯一真身**, 别处只许指过来不许抄): `tasks/matrix-interaction-polish.md`
> 逐条勾选推进; 每任务完成后跑三件套 (fmt + clippy + test)。
> 构建序: **M1 (框架) → M2 → M3 → M4 → M5**; commit/push 点标 ⏸ 待用户点头。
> 基线: 本仓 **169 绿** (51 lib + 110 main + 8 genlog, 2026-09-15 M5 完成后实测);
> 框架 **600 lib** + 集成 59 绿 (**M5 的 T21 动了一处框架**: `TextInput::select_all`
> 转 pub —— 已 push `24bd9a4`, 见 T21 条)。
> 记账前先量, 别抄旧数 —— main 从 74 → 82 (M2) → 92 (M3) → 105 (M4) → 110 (M5)。
> **后续尖端实测** (`#[test]` 计数, 逐提交量): `bfa8c21` **169** —— 即上面那个 169,
> **不是写错了, 只是停在 M5 没往后记** → `38b4cd4` review 轮 **173** →
> `ffb3ae6` case-insensitive **178** → `24dc2d5` **179** (`cargo test` 实测同为
> 51 lib + 120 main + 8 genlog = 179, 与计数一致)。
> **注意**: case-insensitive 模块与本模块**同 crate 交错**, 故尖端那个数**不是本模块
> 单独的** —— 别再拿它当本模块基线, 要看本模块的量就 checkout `38b4cd4`。
> **M0 (用户实机走查 5 条) 在 build 前**: P10 / P11 / P19 / P30 / P31。

## Phase M1: 框架 (danqing)

- [x] **T1: `Widget::cursor_at` + 框架 `cursor_at(root, pos)` + handler apply**
  - 说明: `Widget` trait 新增 `fn cursor_at(&self) -> Option<CursorIcon> { None }`
    (与 `focusable()` / `focus_id()` / `modal_barrier()` 同款「默认实现 + 框架调度」);
    框架新增 `cursor_at(root, pos)`, **复用 `hit_focusable` 的 `visit` walk**
    (`src/widget/focus.rs:199-249`) —— 同套模态屏障 (`:222`) 与祖先裁剪 (`:214-219`),
    **不另推命中几何** (D1); `Node` 透传新方法; `handler.rs:47` 的
    `window: Option<Arc<WinitWindow>>` 已有, winit 0.30 `set_cursor` 直接可用,
    **不引依赖**; 求值时机 = **帧末** (D: 鼠标不动而内容变也要重算)
  - Acceptance: ① 同一 pos 下 `cursor_at` 与 `hit_focusable` 命中**同一节点**;
    ② 模态开时不从卡后取光标; ③ 无命中 → `CursorIcon::Default`;
    ④ 未实现 `cursor_at` 的控件恒返 `None`, 行为与今**逐字节相同**;
    ⑤ 滚动 / 切主题后光标随帧重算 (不靠 `CursorMoved` 驱动)
  - Verify: `cd ../danqing && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
  - Files: `../danqing/src/widget/mod.rs` (trait + `Node` 透传),
    `../danqing/src/widget/focus.rs`, `../danqing/src/window/handler.rs`
  - Scope: M
  - **前置**: `cp tools/local-patch.toml .cargo/config.toml` (本仓, patch 默认关)
  - 注意: ~~先核实包装层是否已透传同类 trait 方法~~ —— **已核实, 无此问题**:
    `pub type Node = Box<dyn Widget>`, **不是包装结构体**, 带默认实现的 trait 方法
    自动对所有节点可用, 无需任何透传
  - **完成记录 (2026-09-14)**: 框架新增 `CursorIcon` (`event.rs`, 平台无关词汇
    Default/Pointer/Text) + `Widget::cursor_icon()` 默认 `None` +
    `focus::cursor_at()`; `focus::visit` 重构为 **`visit_hits`**(两个查询共用一趟,
    `first_wins` 决定谁胜: 焦点取末次=祖先覆盖后代**不变**, 光标取首次=最深最上) +
    handler 每帧 `update_cursor()` (帧末, 且只在变化时调系统 API, 同 `last_window_title`)。
    新增 4 条测试 (最深者胜 / 屏障对照 / 不表态返 None / 界外返 None)。
    **实测: 框架 596 lib + 集成 60 全绿, clippy 0。** 既有 16 条 focus 测试仍绿
    = 重构未动焦点语义 (`overlapping_siblings_low_index_wins` 是回归锁)。
    **实现期发现并修正**: winit 0.30 已把 `set_cursor_icon` 废弃、改名 `set_cursor`
    (走 `impl Into<Cursor>`); 变体名是 `Pointer` 不是 `PointingHand`。

- [x] **T2: `collect()` 认模态屏障 (+ P9)**
  - 说明: `src/widget/focus.rs:147-168` 的 `collect()` 施加与**同一文件** `visit()`
    (`:222`) 同一规则 (开态屏障只深入最上层那个, 屏障外兄弟子树不进链) ——
    **修分叉, 不新造规则** (D2)。依据: `hit_focusable` 文档注释 (`:193-198`) 记着
    修过的**同款** bug (设置卡下拉点击后焦点落到底层 LogView); P31 是同款 bug 的
    **Tab 通道**。顺带 P9: 链首不落在不可见根 Stack 节点
    (机制待核 `src/widget/layout/stack.rs:95-97` + `:54-59` 首帧自动聚焦)
  - Acceptance: ① 屏障外的可聚焦节点不进 chain; ② 屏障内多节点 Tab **成环**;
    ③ 无屏障时 chain 与今**逐项相同** (回归锁); ④ 与 `visit()` 的屏障用例**共用夹具**;
    ⑤ Tab 首停不是不可见节点
  - Verify: 同 T1
  - Files: `../danqing/src/widget/focus.rs`
  - Scope: S
  - **完成记录 (2026-09-14)**: `collect()` 加开态屏障过滤 —— 只深入最上层那个
    屏障子树, 屏障外兄弟不进链。**只加过滤, 不改遍历顺序** (本函数正序 = Tab 顺序,
    与 `visit_hits` 的逆序是两种用途, 一并「统一」掉就是引入 bug)。
    新增测试含**对照组**: 同一棵树屏障开/关给不同链 (`[[], [1,0]]` vs `[[], [0]]`)。
    **实测: 21 条 focus 测试全绿** (既有 20 + 新 1)。
  - **P9 未做, 待裁** —— 见下。

- [ ] ~~**P9**~~ —— **2026-09-14 用户裁定: 保持现状, 只落档 (不做改动)**
  - **裁定理由 (记下来免得下次又当 bug 修)**: 「应用启动时**不显示可见焦点**、
    Tab 一次即到首个真控件」被认定为**合理行为**, 不是缺陷; 代价是 Tab 循环里
    多一个无反馈的站点 (违三问第 1 问), 但只多按一次, 而**修它会改变启动焦点行为
    (启动即带焦点环), 那是更大的可见变化**。两害相权取其轻。
  - 现状由 T2 的测试**钉住** (`tab_chain_excludes_nodes_behind_an_open_modal_barrier`
    的断言里出现 `vec![]`)。**将来若有人要把这个 `vec![]` 从断言里删掉,
    那是推翻本裁定, 不是修 bug** —— 请先回来看这一条。
  - **落档位置**: 本文件 + `docs/specs/SPEC-interaction-polish.md` §2 (M1.2 尾注)。

<details><summary>P9 原始排查记录 (保留作档案)</summary>

- [x] **P8: CloseButton 补焦点态** (框架)
  - **定位**: 不是新设计, 是「**CloseButton 少了 Button 已经有的一整套焦点机制**」——
    修复即一致性。
  - **框架侧 (已完成)**: `focused` / `focus_color` / `bind_focus_color` /
    FocusIn-FocusOut 分支 / `reset_focus` 覆写 / `is_focused()` —— 与 `Button`
    **逐项对齐** (焦点环也逐项同款: 内缩 3px、圆角 4、划线 4 / 空隙 2 / 线宽 1) ——
    同一框架里焦点只该有一种画法, 免得两个控件各教一套。
  - **产品侧 (已完成)**: `src/settings.rs` 的关闭钮补 `.bind_focus_color(th.accent())`。
    **为什么必须绑**: 框架默认是 `Color::WHITE` (与 Button 同款, 那只在深色底面上看得见),
    而设置卡的底是 `background()` —— 浅色下不绑就是把白圈画在白底上, P8 等于没修。
  - Acceptance: ① FocusIn/FocusOut 切换状态; ② **持焦时 RectBatch 多出一笔**
    (锁「画出来了」, 不只是标志位 —— 「有焦点态却没画」正是 P8 的原始形态);
    ③ 焦点色绑定随 sync 更新。
  - **实测**: 框架 **600 lib + 集成 59** 全绿; 本仓 **133 绿** (51 + 74 + 8); 两仓 clippy 0。
  - Files: `../danqing/src/widget/base/close_button.rs`, `src/settings.rs`

- [ ] **P9 (从 T2 拆出)**: Tab 链首是**不可见的根节点**
  - **机制已核实**: `Stack::focusable()` = `children.any(|c| c.focusable())`
    (`src/widget/layout/stack.rs:95-97`) → 只要树里有任何可聚焦控件, **根 Stack
    自己就是可聚焦的**, 于是 `collect` 把它 (路径 `[]`) 放进链首;
    首帧自动聚焦 `chain[0]` (`focus.rs:54-59`) → 应用启动时焦点落在一个画面上
    不存在的节点上; Tab 循环里也因此多一个**按了没变化**的站点 (三问第 1 问)。
  - **为什么没直接改 (它可能是承重的)**: 把 `Stack::focusable()` 改成 `false`,
    会让「首帧自动聚焦 `chain[0]`」从不可见的根**变成第一个真控件** ——
    启动即带焦点环, 这是**可见行为改变**, 且各产品可能正依赖「启动无可见焦点」。
    不是能顺手改的东西, 故拆出来让用户裁。
  - **选项**: (a) `Stack::focusable()` 不再因子级可聚焦而返回 true (需评估启动焦点变化);
    (b) 保持现状 —— 认定「启动不显示焦点、Tab 一次到首个控件」是合理行为, 只是循环里
    多一格; (c) 只改首帧自动聚焦的挑选规则 (跳过空路径节点)。
  - **现状已被 T2 的测试钉住** (`vec![]` 出现在断言里), 改哪一项都会让那条测试红 —— 
    这是有意的: 让 P9 的决定必须**显式**做出。

- ~~**T3: 未认领事件回退开关 (默认 off)**~~ —— **2026-09-14 撤回 (机制本就存在)**
  - 原计划: 新增与 `propagate_unhandled_keys` 对称的未认领事件回退开关。
  - **撤回证据 (读调用链, 不是读单点)**:
    ① 框架 `src/window/handler.rs:983-985` —— `if result == Ignored { self.app.event(&internal) }`,
    未认领的**鼠标事件本来就无条件回退给应用** (键盘才需要 opt-in);
    ② 应用 `src/main.rs:1222-1231` —— `let Event::Key {...} = event else { return }`,
    滚轮**到得了应用, 被应用自己丢掉**。
  - **改判**: P30 根因在**产品侧**, 框架零改动。修法转入 **M4 的 T17**
    (处理 `Event::MouseWheel` + 按位置路由; `delta` 两种单位都要接, 普查 G10)。
  - **编号保留不复用。**
  - **教训**: 普查报的「框架按点子命中不回落 (`flow.rs:291`)」**在那一层是对的**,
    漏的是调用链**上一层**的 handler —— **子代理的结论要在调用链的上一层再核一次**,
    局部正确不等于全局正确。(与本批 T4 的撤回同源: 两个「框架活其实不用干」,
    两个都是读代码本身才发现的。)

- ~~**T4: 四态分通道 token 槽位**~~ —— **2026-09-14 撤回 (前提有误)**
  - 原计划: 在框架 `Theme` 上加分通道 token 槽位 (只加槽位、渲染零变化)。
  - **撤回理由 (动工前读码发现)**: 本仓分层是「框架给**通用**调色板 (`LightTheme`/
    `DarkTheme`) → 产品 `LogTheme` (`config.rs:144`) **逐方法转发** →
    **产品语义色放产品侧函数**」。规矩写在本仓自己的注释里
    (`src/view.rs:126`: 「与 `bookmark_color` / `expand_block_bg` 同一处理:
    **产品语义放产品侧**」), 三个先例 (`row_band_bg` `:128` / `row_hover_bg` `:150` /
    `cell_highlight_colors` `:166`) 与守卫 (`:2356`) 全在产品侧。
    把 `hit_row`/`selection_cell` 这类**日志查看器语义**塞进通用框架 `Theme`
    = 产品概念泄漏进框架, 与既有分层相反。
  - **改判**: **M2 的四态分通道改为纯产品侧改动** (照 `row_hover_bg` 先例: 产品侧
    取色函数 + 浅/深两套值 + 产品侧守卫), **框架零 token 新增**;
    M3 的 notice 配色同理 (`danger()` 框架已有, 中性回执用 `text_secondary()`)。
  - **编号保留不复用** —— 免得 T5–T23 全体挪位。
  - **教训 (写在这里免得重犯)**: 「框架先出现、产品后赋值」读起来顺, 但与**本仓
    实际分层相反**。分层的判据在**代码注释**里, 不在架构直觉里 —— 动框架前先读那一段。

- [x] **T5: 联动落地** ✅ (2026-09-14, 用户点头后执行)
  - **框架侧**: 两笔提交 → push `dev` (`b4b43e1..ec8ae09`)。
    `e439725` 光标形状 API + 命中遍历两查询共用一趟; `ec8ae09` CloseButton 补焦点态。
  - **本仓**: 关 patch (`rm .cargo/config.toml`) → `cargo check` 按 manifest 重解,
    **一步就钉到了刚 push 的 `ec8ae09b`** (这一步本就够, 无需再 `cargo update -p`) →
    `Cargo.lock` 为带 `source` 的 pinned 态 (不是 path 态)。
  - **验证**: 无 patch 状态下 `cargo check --all-targets --locked` 通过 (= 外人克隆
    可复现), 本仓 133 绿 + 两仓 clippy 0。
  - **提交**: 框架 `e439725` + `ec8ae09`; 本仓 `1566c0e` (五份文档) +
    `7c39236` (settings + lock)。本仓两笔均在 **`dev`**, master 未动。
  - **踩坑并已修 (记下来)**: 第一笔本仓文档提交**落到了 `master`** —— 工作区在
    发版合并后停在 master 上, 而本仓 CLAUDE.md 只写了分支模型、没写「工作区可能
    停在哪儿」。**修法** (无损失): 先 `git diff --stat dev b2d8351` 确认两分支树
    **完全相同** → `git checkout dev` + `git cherry-pick` → `git branch -f master
    b2d8351` (**不切分支、不碰工作区**, 比 `reset --hard` 安全)。
    已写进用户级 memory; **提交前先看 `git status -sb` 首行是不是 `## dev...`**。
  - 说明: danqing 三件套 → **push (待用户点头)** → 本仓**关 patch**
    (`rm .cargo/config.toml`) → `cargo update -p danqing` → 三件套绿 →
    提交 lock (待用户点头, message 注明关联)
  - Acceptance: `Cargo.lock` 的 danqing `source` 钉到**新 rev**; 本仓 115 基线 + 全绿;
    无 patch 状态下 `cargo build --locked` 可复现
  - Verify: `cargo test` + `grep -A2 'name = "danqing"' Cargo.lock`
  - Files: `Cargo.lock`
  - Scope: S
  - 注意: 两个 lock 陷阱 (2026-09-14 各踩一次) —— ① patch 开着时 lock 必为 path 态,
    **此态别提交**; ② 关 patch 后若报 `package ID specification danqing did not match
    any packages`, **先跑一次 `cargo check`** 重解再 `cargo update`。
    RustRover 并发 cargo 抢锁会把 lock 写回旧 rev —— 报 `could not find ... in danqing`
    时重跑, 验 lock 前关 IDE 自动 cargo

## Phase M2: 四态分通道 (本仓, 依赖 M1)

> **2026-09-14 修正**: 本节六条**全部是产品侧改动** —— 照 `row_hover_bg` 的先例
> 加产品侧取色函数 + 浅/深两套值 + 产品侧守卫, **不新增框架 token** (见 T4 的撤回记录)。
> 依赖 M1 只是「同一批工作」的意思, **技术上不依赖** T1/T2/T3 的任何产出。

- [x] **T6: 三态共存 (P10)** 【已复核】
  - 说明: `src/view.rs:1084-1095` —— 去掉 `!has_text_sel` 压制; hover 从
    `else if` 里独立出来; 选中底 / 选区带 / hover **各自 token 同时画**
  - Acceptance: ① 拖框选全过程, 选中行底与左 accent 竖条**仍在**;
    ② 悬停选中行 hover 可见; ③ 三态同屏时两两可辨 (ΔL\* ≥3, 或至少一笔描边分隔)
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: M
  - **实测**: 本仓 82 main 绿 (含 5 条新测试), clippy 0
  - **实现期发现并修的 bug**: S1 重构时 `row_y` 丢了 `rows_top` 偏移,
    是测试红出来的 (`hit_row_is_weaker` 等 4 条全红), 不是看出来的

- [x] **T7: 命中通道 (P11 P14 P15)** 【P11 已复核】
  - 说明: `view.rs:1094` 与 `:1163` —— 命中行底**换 `hit_row_bg`** (α 减半:
    浅色 0.30→0.15, 暗色 0.20→0.10), 且**不再靠画序分胜负**; `:1227` 命中区间与
    文本选区带分通道; 「当前命中」与一般命中分通道
  - Acceptance: ① 悬停搜索命中行, hover **可见**; ② 多命中时**当前命中一眼可指认**
    (不再只靠 3px 竖条); ③ 文本选区带与命中区间同行交叠时**可分**
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: M
  - **P15 的验收标准放宽并写明**: 「两者交叠时可分」在浅色下**做不到**
    (同 token 同 α, 画序不可分), 只锁「两者都画」; 「选区带压过命中区间」
    的感知由**位置**提供 (选区带是用户拖出来的, 命中区间是搜索留下的)

- [x] **T8: 单元格底换通道 (P16)**
  - **判定: 无需改动** —— `cell_highlight_colors` 的描边 (`th.accent()`) 已是
    「具体哪一格」的唯一指示, 底笔与行选中同 token 是**有意为之**
    (`view.rs:180` 注释写死), 已有回归锁 `cell_highlight_border_is_a_second_stroke`
  - Files: `src/view.rs`
  - Scope: S

- [x] **T9: 超限拖选即时视觉 (P28)**
  - 说明: `view.rs:675` (`selection_over_limit`) 与 `:1591-1595` ——
    超限**发生即**改变选区外观 (不再等 Ctrl+C); 出声部分在 T12 接入 M3 通道
  - Acceptance: 拖选进行中越界时外观可区分, 且 T12 完成后伴随 notice
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: S
  - **实现**: 选区带在超限时换 `th.danger()` (拖选进行中即变)
  - 两条新测试: 超限用 danger / 未超限仍用 sel (不误报)

- [x] **P6 (由 M1 转入, 2026-09-14 归位)**: 过滤/搜索栏的**焦点不可见**
  - **为什么转来**: 原挂在 M1 (框架), 但实现位置在**产品侧** ——
    `src/view.rs:1799` 的 `.chromeless()` 是**产品**选择的(为了去掉输入框自带边框,
    因为它嵌在过滤栏里), 框架只是「chromeless 时不画焦点描边」
    (`danqing/src/widget/form/text_input.rs:488` 的 `if !self.chromeless`
    把背景**和** `focused` 描边一起跳过)。
  - **修法 (产品侧)**: 在 `Bar` 的 paint 里按「焦点是否在本栏」画一条焦点指示
    (如 accent 下划线 / 左侧竖条), 不动框架的 chromeless 语义。
  - Acceptance: Ctrl+F 后栏的焦点**一眼可见** (不再只有半周期闪烁的 caret);
    两栏 (过滤/搜索) 各自可辨; `chromeless` 的外观在未持焦时保持不变。
  - Files: `src/view.rs`
  - Scope: S
  - **实现 (2026-09-15, 与 P33 同批)**: `Bar::input_focused()` (读
    `TextInput::is_focused()`) 判据, 持焦时在栏**底边**画一条 2px `accent()` 线;
    未持焦时一条不多画。守卫 `focused_bar_paints_key_hint_and_focus_line` /
    `narrow_bar_drops_the_key_hint_but_keeps_the_focus_line` (后者同时锁
    「提示放不下 ≠ 焦点不可见」)。
  - **附记**: 「两栏各自可辨」由 `active` 单点决定 (同一槽位同一时刻只有一栏可见),
    不存在两栏同时可辨的问题; 原措辞是把「切换角色」当成了「并排两栏」。

- [x] **T10: 命绘同源收口 (S1 S2 S3)**
  - 说明: S1 行 y 映射 (paint 循环式 `view.rs:1045,1058` ↔ `row_at` `:695-697`);
    S2 展开 glyph 命中区 (同常量两处 `:1248` ↔ `:1531`); S3 侧栏桶行高亮 y
    (`histogram.rs:306,311-320` ↔ `row_rect` `:85-92`)。**T17 (滚动条拖拽) 依赖
    S1 的单点** (D7)
  - Acceptance: ① S1 两式**对拍逐行相等**; ② S2 抽成单函数; ③ S3 走 `row_rect`;
    ④ 既有几何守卫全绿 (含 2026-09-14 的 `text_x`/`row_at` 那批)
  - Verify: `cargo test`
  - Files: `src/view.rs`, `src/histogram.rs`
  - Scope: M
  - **实现**: S1 的 `row_y` 改为 `row_at` 的逆运算 (含 `rows_top` 偏移 ——
    实现期 bug 是测试红出来的, 不是看出来的); S2 抽成 `expand_glyph_x` /
    `in_expand_glyph`; S3 收口到 `row_rect`。新增对拍测试 `row_y_and_row_at_are_inverse`

## Phase M3: 拒绝要说清 (本仓, 依赖 M1)

- [x] **T11: notice 通道 + 错误态独立视觉 (P27)** 【已复核】—— **地基**
  - 说明: 现状所有状态拼进**同一个** `self.status`, 用 `th.text_secondary()` 画
    (`view.rs:1408-1414`), 错误与常态**同色同字号**。拆成**常态信息** (打开耗时 /
    行数 / FOLLOW / 模式 / 命中计数) 与**瞬时反馈 (notice)** 两条通道;
    notice 有独立视觉 (警示色 + 可辨呈现); `text_secondary` 兼两义在此收口 (D3/D4)
  - Acceptance: ① 打开失败 / 轮转 / 选区超限 / 正则无效 → notice 与常态信息**可辨**;
    ② **状态栏几何高度不变** (回归锁 —— 不许因提示顶高状态栏, 那会动截图取景);
    ③ 两者同屏时可辨
  - Verify: `cargo test`
  - Files: `src/view.rs`, `src/main.rs`
  - Scope: M
  - **实现 (2026-09-15)**: `LogView::notice` 字段 (sync 从 `app.notice` 取),
    paint 里与 `status` **各画各的**; 三档取色 —— 警示 `danger()` /
    提示 `text_primary()` / 常态 `text_secondary()`。
  - **写这节时自己踩了两个坑, 都是「分通道」的字面反转, 已修并加锁**:
    ① `refresh_status` 把 notice 拼进了 `status`, 而 paint 又单独画一遍 `notice`
    —— **同一句话在底栏出现两次** (且同色, 第一眼只像重复不像出错);
    ② `Info` 档与常态同用 `text_secondary()` (那个 if/else 两个分支写成了同一个
    值, 注释还写着「降噪」) —— **同色即同通道**, P27 原样复活。
    守卫: `notice_is_not_folded_into_the_status_line` (main) /
    `notice_is_drawn_once_in_a_color_of_its_own` (view, 已做 A/B: 把色改回
    `text_secondary` 精确红在计数上)。
  - **②「几何高度不变」的落实方式是「notice 画在已有状态栏行内」** ——
    不新增行、不改 `STATUS_HEIGHT`, 故不存在「提示顶高状态栏」的路径;
    这也正是截图取景不受影响的原因。

- [x] **T12: 九条沉默接入 (P20 P21 P22 P23 P24 P25 P26 P33 P34)**
  - 说明: 逐条给「为什么不行」的原因, 全部走 T11 的通道。
    P20 点空白 (`view.rs:1527-1568`) / P21 不可点桶行 (`histogram.rs:456-458`) /
    P22 无 glyph 行的展开区 (`view.rs:1531` 不校验可展开) / P23 Ctrl+T 无 JSONL
    (`main.rs:1349-1351`) / P24 →← 无效 (`main.rs:476-482`) / P25 Ctrl+G 无书签
    (`main.rs:1066-1075`) / P26 空态按键 (`main.rs:1242-1245`) / P33 栏持焦键义反转 /
    P34 Ctrl+A 被吞 (`main.rs:1247-1263`)
  - Acceptance: 上述 **9 条各一条断言**「触发后 notice 非空且含原因」;
    P22 另需「`in_glyph` 命中但本行不可展开 → 不发 `ToggleExpand` 而去出声」
  - Verify: `cargo test`
  - Files: `src/view.rs`, `src/main.rs`, `src/histogram.rs`
  - Scope: L
  - **实现 (2026-09-15)**: 八条走 T11 notice 通道 (P20–P26 P34); **P33 例外** ——
    见下面那条与 spec §4 的裁决补记。
  - **一条既有测试跟着改判** (`histogram.rs::readonly_sidebar_swallows_clicks_...`):
    它原断言「只读侧栏不得发消息」, 与 P21 直接对立。改法是**保留「不穿透」这条
    不变式、反转「不发声」那条**, 并加强成「恰好一条、且是带原因的 Notice」——
    删掉它就等于把「只读态点击穿透去选中底下的行」这个真缺陷放回来。

- [x] **T13: 「被吞掉的输入必有原因」守卫**
  - 说明: 把本仓孤例 (「正则无效」进底栏 `main.rs:1006-1008`) **升格为规则**
  - Acceptance: 守卫覆盖 T12 的 9 个触发点; 新增「被吞输入」时有处可挂 (不是逐条靠人记)
  - Verify: `cargo test`
  - Files: `src/main.rs` (或 `src/view.rs`)
  - Scope: S
  - **实现 (2026-09-15)**: 落成**一张表**, 不是一个散落的测试集 ——
    `main.rs::every_swallowed_key_says_why` 的 `rows` 即「键盘路径上被吞的输入」
    的总清单, 每行 = (站点, **期望的原因片段**, 触发闭包)。于是「有处可挂」是字面
    的: 再遇到按了没反应, 加一行即可, 且必须写出期望片段 (只写「有提示就算」会放过
    原因写错的形态)。鼠标路径三条各有同构守卫, 在表的 doc comment 里互相指路
    (`view.rs` 两条 + `histogram.rs` 一条)。
  - **为什么不做成一条跨模块的表**: 三条鼠标站点在各自组件的 `event` 里, 要在一处
    驱动它们得搭起整棵控件树 —— 那样测的就不再是那三个站点本身。

### T12 的例外: P33 不走 notice 通道 (2026-09-15 改判, 已获用户批准动框架)

**原计划 (spec §4)**: 「说清当前焦点下的键义 (至少 ↑↓)」, 与其余八条一样进 T11 的
notice。

**改判理由 —— 这八条与 P33 有一处根本不同: 前八条是「拒绝」, P33 是「改道」**。
`Space`/`Home`/`End` 在栏持焦时**并没被拒绝**, 是输入框收下了:
- notice 的语义是「你这个动作没生效, 因为 X」; P33 是「你这个动作生效了, 只是去到了
  另一个地方」。用拒绝的口吻说改道, 是在说一句不准确的话。
- 更要命的是**触发时机**: 键路径上没有「用户是想翻页还是想打空格」的判据, 只能按
  键触发 —— 那就变成**每打一个空格都在底栏刷一条 notice**。写正则给日志做过滤时,
  底栏会一路闪。

**改法是把它做成「状态提示」而不是「事件反馈」**: 栏持焦时栏内右侧**常驻**一条
`↑↓ 滚列表 · Space/Home/End 归输入框`, 与 P6 的焦点线同批 (同一个 `is_focused()`
判据)。于是它随焦点出现、随焦点消失 —— 恰好是它该在的时机: 用户刚把焦点放进栏里。

**框架改动授权没用上**: 用户批了「P33 批准动框架」, 但查证发现
`TextInput::is_focused()` **早已是公开方法** (其 doc 明写「焦点态描边由外层经
`Self::is_focused` 查询后画在自己的外壳矩形上」)—— 框架本来就是按「chromeless 由
外层自绘焦点」设计的, P6/P33 正是那个外层。故本次**零框架改动**, 授权原样保留。

**让位与绘制同源**: 提示宽度由 paint 测量后写进 `Bar::hint_reserved` (与
`label_width` 同一套缓存), `input_area` 统一扣减 —— 于是「提示画在哪」与
「点在哪儿放光标」不可能分岔。窄窗放不下时**不显示提示, 但焦点线照旧**。

**守卫 (`view.rs`)**: `focused_bar_paints_key_hint_and_focus_line` (画了, 且用
`text_secondary` 与输入文本可辨) / `focused_bar_gives_the_input_area_room_for_the_key_hint`
(让位宽度 = 提示宽度 + 间隙) / `narrow_bar_drops_the_key_hint_but_keeps_the_focus_line` /
`key_hint_names_every_key_the_input_swallows` (**内容锁, 且不是自证** —— 逐个键真喂给
`TextInput`, 吞得下的才要求提示点名; 框架改了键分支或有人为排版删词都会红)。

## Phase M4: 前提与归属 (本仓, 依赖 M1)

- [x] **T14: Esc 一并清行选中 (P19)** ★ —— **不变量优先**
  - 说明: 现状链路 —— `view.rs:1609-1613` Esc 只清 `selection/selected_cell/press/
    dragging`, **不碰 `selected`** → 全空 `Ignored` → 框架 `handler.rs:446-450` 清焦点
    → Ctrl+C 无焦点时不进组件, app 的 ctrl 分支只认 b/g/t (`main.rs:1247-1263`)。
    改: **Esc 一并清 `selected`** (D6)
  - Acceptance: ① 「点行 → Esc → Ctrl+C」端到端断言;
    ② **不变量守卫**: 「选中的**视觉**」与「**可复制性**」永远**同真同假** ——
    对行选中 / 文本选区 / 单元格选中三类各测一次, 不只测这一个手势
  - Verify: `cargo test`
  - Files: `src/view.rs`, (必要时) `src/main.rs`
  - Scope: M
  - **实现期改判 (2026-09-15, 用户当场裁定): 走「高亮跟随焦点」, 不是 D6 的「Esc 清
    掉当前行」**。D6 那条要 `selected` 从 `u64` 变 `Option<u64>`: 全仓 ~20 处写点 +
    5 处读点 (Ctrl+B 书签 / →← 展开 / Ctrl+G / 搜索起点) 都要给「没有当前行」定语义,
    而其中多数只能落回「出声」—— 为一个手势背一整套新状态机。改判后的做法:
    `LogView` 加 `focused: bool` (FocusIn/FocusOut 维护), **三处高亮全部 AND 上它**。
    不变量于是**由构造保证**: 框架只在持焦链路上派发 `Event::Copy`, 所以三处高亮
    看得见 ⇔ 复制得到, 是**同一个因**, 不再靠约定。改动约 10 行, `main.rs` 未动。
  - **代价 (如实记)**: 失焦时看不见选中 (点进搜索栏再点回来即恢复)。这是标准做法,
    但确属可见变化 —— 尤其**打开文件后不点一下就没有高亮** (焦点起始为空)。
  - 守卫: `the_three_highlights_all_follow_focus` (三类各一次, 已做 A/B: 摘掉行选中的
    焦点位, 精确红在「失焦时不得画出来」) /
    `escape_drops_the_highlight_and_hands_focus_back_to_the_framework` (锁住整条链的
    **因**: Esc 必须 `Ignored` —— 那才是框架清焦点的触发条件)。
  - 附带: `cell_fixture`/`sub_row_fixture` 等夹具置 `focused = true` (它们代表
    「正在日志区里操作」), 另有 3 个内联构造的测试补了同一行。

- [x] **T15: 右键 / 中键只认 Left (P29)**
  - 说明: `view.rs:1516` 处理 MouseInput **不筛 button** → 右键 / 中键与左键同效
    (行选中 / 开设置卡)。改: 只认 `MouseButton::Left`。理由: 缺陷不在「右键没有菜单」,
    而在**左键的语义被一个没有 affordance 承诺的手势触发了**
  - Acceptance: 右键 / 中键在列表区与底栏 ⚙ 上**不产生**选中或开卡; 左键各行为逐项不变
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: S
  - 注意: 面板弹出层的 scrim 关闭 (`overlay.rs:250-258` 仅 Left) 与标题栏按钮**已是**只认
    Left, 保持不动
  - **实现**: 按下分支开头一句 `if *button != MouseButton::Left { return Ignored; }`。
    返 `Ignored` 而非 `Consumed`: 确实什么都没做, 不冒充「已认领」; 顺带把右键事件
    留给应用层 —— 将来加右键菜单 (ROADMAP) 时这里不必再改。
  - **附注 (查证)**: 框架的 `set_by_click` 在**任何**按下都会跑 (`handler.rs:973-977`),
    所以右键仍会按位置改焦点。那是框架的归属, 不属本项, 已在代码注释里写明。
  - 守卫: `right_and_middle_buttons_do_not_act_like_the_left_one` —— 判据取**消息队列**
    而非返回值 (返回值是 `Ignored` 也可能是「照样发了消息然后说没做」), 且带左键对照
    (左对照同时证明测试用的点位真的命中, 不是空跑)。已 A/B。

- [x] **T16: 模态守卫 (P32)**
  - 说明: 产品侧 `app_key_filter` 加 `settings_open` 守卫 (`main.rs:1235` 注释已自认
    穿透)。现状: 卡开着按 Ctrl+O **在卡上方弹系统文件对话框**; Ctrl+F 把焦点按 id 送到
    卡后**看不见的**输入框。框架 `app_key_filter` 是应用回调 → **产品侧守卫足够**
  - Acceptance: 模态开时 Ctrl+O / Ctrl+F / Ctrl+T / Ctrl+L / Ctrl+B / Ctrl+G
    逐个断言**不产生卡后副作用**; Esc **仍能**关卡 (既有行为保持)
  - Verify: `cargo test`
  - Files: `src/main.rs`
  - Scope: S
  - **实现**: Esc 分支之后加一句 `if self.settings_open { return Some(Msg::Noop); }`。
    **吞掉而不是放行** —— 返回 `None` 的语义是「我没拦」, 事件会继续往下走。
  - 守卫: `settings_modal_does_not_leak_global_keys_behind_the_card`。**Ctrl+O 有意
    不在表内**: 它的穿透后果是弹**阻塞的原生对话框**, 一旦回归这条测试不是红而是**挂住**
    —— 会挂死的守卫比没有守卫更坏。它与表内三条共用同一个 `if`, 实机那一半由 §6 人工验收清单覆盖。

- [x] **T17: 视图导航输入收口 —— 滚动条拖拽 + 未认领滚轮路由** (新; D7)
  - **滚动条拖拽**: 见 todo 的详细条目
  - **未认领滚轮 (P30, 由撤销的 T3 转入)**: `LogApp::event` 现在把非按键一律丢弃
    (`main.rs:1222`), 滚轮虽到得了应用却被丢 —— 加 `Event::MouseWheel` 分支,
    按位置把指针在侧栏/过滤栏上的滚轮转给列表滚动。
    **`delta` 框架不归一** (`LineDelta` 行数 / `PixelDelta` 像素, 普查 G10), 两条都要接。
  - **实现 (2026-09-15)**:
    - **几何单点**: 抽出 `v_scroll` / `h_scroll` (结构体 + 逆运算 `top_row_at` /
      `x_offset_at`), **paint 与拖拽共用一份**。D7 说的「复用 `row_at`」落地时发现
      做不到 —— `row_at` 是**行粒度**的, 而拖拽要的是像素级连续量, 用它会把往返
      一致性从「精确」降成「整行」。改为复用**paint 那份几何并取其逆**, 这才是
      「不另推一份」的实质。
    - **滚轮路由不需要位置数学**: 指针在日志区上时 `LogView` 已经 `Consumed`,
      能走到应用层的本来就不是它。故按「未认领即滚列表」处理, 顺带覆盖标题栏等。
      **加模态门禁** (与 T16 同一条纪律): 卡开着时滚轮只属于卡。
    - **滚轮换算收口** `main.rs::wheel_rows`: 内容区与未认领那一路共用一支,
      并夹单次上界 —— 框架把 `LineDelta`(行) 与 `PixelDelta`(像素) 抹平成同一个
      `f32` (G10), 触控板一次 ±100 若不夹就跳几百行。
    - **⑤ 纵横两条都做了**: 只做竖条会留下「两根同貌的拇指, 一根能拖一根不能」,
      那正是本模块要消灭的形态。横向状态是视图局部的 `x_offset` (不经应用层),
      故横条直接落 `Cell`, 竖条走 `Msg::ScrollTo`。
    - **`Msg::ScrollTo` 不动 `selected`** (不变量 ③): 抓条是「看」不是「选」。
      与 `ScrollRows` 的行为差异是**有意**的, 测试里带对照。
    - 拇指 hover / 按住加深 (`border` → `text_secondary`), 光标 `CursorIcon::Pointer`
      (走 T1 的 API; `cursor_icon` 无位置参数, 靠 `CursorMoved` 缓存的 `hover_bar`)。
  - 守卫: 往返一致 / 两端夹取 / 拖拽不改选中 (带 ScrollRows 对照) /
    拖拽链不污染文本选区状态机 / hover 与按下态可见 / 光标只在条上 /
    横条往返+拖拽 / 未认领滚轮 (含模态不穿透) / `wheel_rows` 单点。**三处已 A/B**。

## Phase M5: 可发现性 (本仓, 依赖 M3)

- [x] **T18: 复制回执 (P17)**
  - 说明: 现状 Ctrl+C 成功**零反馈** (`view.rs:1585-1600`), 只在超限时报错。
    走 T11 的通道给回执, **用中性色** (与警示色分开), 自动消退 (Q3)
  - Acceptance: 复制成功后底栏出现回执 (含「复制了什么 / 几条」); 中性色与警示色可辨;
    **不改变状态栏几何**
  - Verify: `cargo test`
  - Files: `src/view.rs`, `src/main.rs`
  - Scope: S
  - **实现**: 抽出 `LogView::copy_source` —— 三级来源 (文本选区 > 单元格 > 行) 的
    **唯一判据**, `selected_text` 与回执都从它出发, 于是「真复制了」与「说复制了」
    不可能分家。回执按级给词: `已复制选区 N 行` / `已复制单元格` / `已复制该行`
    (只说「已复制」等于没回答 P17 问的那个「什么」)。
  - **顺带修掉一处白做功**: 原判据是 `selected_text().is_some()` —— 把整段文本
    **先造出来再丢掉**, 而框架随后还要再造一次; 十万行选区就是 16MB 白做两遍。
    改用 `copy_source` 后两个问题一起没了。
  - **自动消退 (Q3)**: `LogApp::notice_until` + `expire_notice()` (tick 每帧调)。
    **单一入口 `set_notice`** —— 原先 8 处各自 `self.notice = Some(..)` 直接赋值,
    直接赋值会漏掉期限, 那条提示就永远赖在底栏上。
  - **`NOTICE_TTL = 4s` 是待实机核对的估值**: spec 说「具体时长 build 时实测定,
    不估算」, 而本机跑不了真机走查。已挂进 §6 人工验收清单 (两个方向都试: 太短没看见 /
    太长碍事)。**这条是本项唯一没做到的验收, 记在这里**。
  - 守卫: `copy_success_reports_what_was_copied` (三级各一次 + 回执两两不同 +
    选区回执带「几条」) / `notice_expires_when_its_deadline_passes`。**已 A/B**。

- [x] **T19: 快捷键页补 `/` 与 `f` (P36); 托盘快捷文字 (P38) —— 改判不做**
  - 说明: 按设置卡**自己定的判据** (「只列猜不出来的那几个组合键; 方向键/翻页键不必教」
    `settings.rs:214`) 补齐 `/` 与 `f`; `→/←` 属方向键, **维持不教** (README 保留)。
    托盘菜单两项补快捷键文字 (`tray.rs:19,21` 第 4 参现为 `None`)
  - Acceptance: 设置卡快捷键页含 `/` 与 `f`; 托盘两项带文字; `→/←` 未被加入
  - Verify: `cargo test`
  - Files: `src/settings.rs`, `src/tray.rs`
  - Scope: S
  - **P36 已做**: `SHORTCUT_KEYS` 6 → 8 行, 补 `("`/`", "搜索栏 (原始模式)")` 与
    `("f", "跟随文件增长 (原始模式)")`; `→/←` 未加 (方向键属常识)。
    两条都标了「原始模式」—— 表格模式有常驻过滤栏, `/` 无栏可开。
    注释里写明**判据是把设置卡自己那句话当真**, 不是新造标准。
  - **P38 改判不做 (2026-09-15)**: 托盘菜单的第 4 参是 **accelerator**, 而本应用
    `hotkeys: vec![]` **显式置空**(main.rs 有注释: 不继承番茄钟默认热键), 框架的热键
    机制是 `RegisterHotKey` = **系统级全局**。填一个 accelerator 要么是**画着好看的
    假承诺**(按了没反应 —— 正是本模块要消灭的形态), 要么得真的注册一条全局热键,
    那会**抢走所有其它应用的该组合键**。为一个「菜单上多几个字」付这个代价不划算,
    也与本应用「不打扰」的定位相反。**判据同 intent 的例外条款, 只是方向反过来:
    不去做一个界面兑现不了的承诺。** 故 `tray.rs` 不动。
  - 守卫: `shortcut_card_lists_single_keys_but_not_arrow_keys` (防改回)。

- [x] **T20: 设置卡「显示级别侧栏」开关 (P37)**
  - 说明: 常规页加开关 (Q2)。`config.histogram` 字段**已存在**, 走整文件同源写入
    (见 `src/config.rs` 的既有约束: 分头写会让「改主题」抹掉侧栏开关)
  - Acceptance: 开关控制侧栏显隐; 与 Ctrl+L **双向同步**;
    **改主题不抹开关** (既有约束的回归锁)
  - Verify: `cargo test`
  - Files: `src/settings.rs`, `src/main.rs`, `src/config.rs`
  - Scope: S
  - **实现**: 常规页加 `histogram_switch()` —— `Switch::bind(app.histogram_visible)
    .bind_theme(..).on_toggle(|| Msg::ToggleHistogram)`, 与 Ctrl+L **同一状态、
    同一条消息、同一个落盘**, 不存在第二份真相。(`Switch::new` 烘的是浅色 token,
    故 `bind_theme` 必须挂 —— 不然切暗色后轨道还是浅色的。)
  - 守卫: `histogram_toggle_is_one_state_for_both_entries` (消息侧);
    「改主题不抹开关」由 `config.rs` 既有的 `round_trip_preserves_both_keys` 覆盖。
    **开关到状态那一段接线无单测** (要点得着控件树) —— 由 §6 人工验收清单的 P37 覆盖。

- [x] **T21: Ctrl+F 不再清草稿 (P39)**
  - 说明: `main.rs:985` 「聚焦即干净开始」。改为: 栏**已聚焦**时 Ctrl+F = **全选内容**
    (便于覆写), **不清空**; 未聚焦时聚焦
  - Acceptance: 输入草稿后按 Ctrl+F, 内容**仍在**且被全选; 未聚焦时 Ctrl+F 仍聚焦
  - Verify: `cargo test`
  - Files: `src/main.rs`
  - Scope: S
  - **实现**: `open_search` 不再 `search_clear_rev += 1`, 改 `search_refocus_rev += 1`;
    `Bar` 新增 `bind_refocus_search`, 在 rev 变化**且 `search_ti.is_focused()`** 时
    全选。「已持焦」由栏自己判 (sync 跑在焦点落地之前, 首次聚焦那一刻框里没有光标,
    也没有该全选的东西 —— 时序天然正确)。清空仍归 Esc, 那条路径没动 (测试里带对照)。
  - **✅ 一处框架改动 (本批唯一), 已落地**: `TextInput::select_all` 原是私有
    (`fn select_all`), 改为 `pub` + doc, **danqing `24bd9a4` 已 push**。
    本仓关 patch → `cargo check` (让它按 manifest 重解) → `cargo update -p danqing`
    → lock 复钉 `ec8ae09b` → **`24bd9a4f`** → `--locked` 无 patch 构建通过。
    两仓分别提交, message 互相注明关联。
  - 守卫: `reopen_search_keeps_the_draft` (应用侧: 不再清空 + Esc 仍清, 带对照) /
    `refocus_selects_the_existing_draft` (栏侧: 已持焦才全选, 且不清空)。**已 A/B**。

## Review 轮 (2026-09-15, `/agent-skills:code-review-and-quality`)

三路独立审查 (view.rs / main.rs / M5) 后逐条复核。**结论: 发现 1 个 Critical + 3 个
Required, 全部已修并加锁**。本模块五阶段的 review 一栏至此才算走完 —— build 期
自查漏掉的东西, 独立审查逐条抓了出来。

| # | 严重度 | 问题 | 处置 |
|---|--------|------|------|
| R1 | **Critical** | **T16 模态守卫吞掉了卡内所有按键**。守卫放在 `app_key_filter` 入口, 而框架在该回调返回 `Some` 时**直接 return、不再走焦点分发** → 设置卡里的主题下拉导航不动、侧栏开关切不了、Enter 关不掉卡。**键盘用户能聚焦到控件却按不动**。基线本来是好的 (只拦 Esc/f/t/l/o), 是本批改坏的 | 守卫下移到「ctrl + 字符」筛选**之后**; 测试补**反向对照**(非全局键必须 `None`)。A/B: 搬回入口即精确红 |
| R2 | **Required** | **T20 单测写用户的真实配置文件**。`new_empty()` 读真路径、`Msg::ToggleHistogram` → `save_config()` → `save_to(真路径)`, 而 `save_to` 是**整文件覆盖写**、`load_from` 对认不出的 `mode` 取默认 (light) —— 一次 `cargo test` 就能改掉用户的主题、抹掉手写注释。全仓唯一一条这样的测试 | `LogApp` 加 `cfg_path` + `new_empty_at(临时路径)`; 并且**测试里拿不到路径直接 panic** —— 与其靠下一个人记得, 不如让它写不出去 |
| R3 | **Required** | **T14 的第三例是假绿**。「三类各测一次」里那例文本选区**根本没画**: 夹具是表格模式, 而选区带只在子行/非表格分支绘制 —— 断言实际被**行选中底**满足, 对不变量零覆盖 | 三类各用**自己那台夹具** + 新增 `band_shaped_rects` (按高度把行选中排除掉)。A/B: 摘掉选区带的焦点守卫, 改前全绿、改后精确红 |
| R4 | **Required** | **todo 里宣称的守卫不存在**: 写着「守卫 `shortcut_card_lists_single_keys_but_not_arrow_keys` (防改回)」, 仓里根本没有这条测试。**宣称有守卫比没守卫更坏** —— 它会让人以为已经挡住了 | 补写该守卫 + 补一条**横向**溢出守卫 (`shortcut_rows_fit_the_card_width`: 框架 `Text` 不换行, 高度守卫看不见横向顶穿) |
| R5 | Optional | 横条命中带**向上**延伸 6px, 吃掉列表末行最下面一带 —— 那一带的**行选中**从此没了 (竖条无此问题, 它的带子在内容区之外) | 改为**向下**延伸 (入状态栏, 那儿除了 ⚙ 没有可点物)。新增守卫 + A/B |
| R6 | Optional | `Msg::ScrollTo` 与 FOLLOW 的**口径不一致**: 跟随态 `top_row` 是 `count-1`, 条能表达的上界是 `count-可见` —— 直接比 `top < top_row` 会把「在底部碰一下条」判成「向上看」而**静默脱掉 FOLLOW** | `ScrollTo` 带 `at_bottom` 位 (拖到条底不参与该判断)。新增守卫 + A/B |
| R7 | Optional | `bars()` 进了 `CursorMoved` 主路径 → 每次鼠标移动付一次 O(过滤命中数) 的 `display_count()`。对「1GB 不卡」是实打实的倒退 | 先按坐标短路 (`near_bar`), 不在条附近就整个跳过; 拖拽中不跳 |
| R8 | Nit ×4 | `CursorLeft` 漏清 `hover_bar` (拇指 hover 态与手型留在屏上) / `reset_focus` 注释把「面板隐藏」当成真实场景 (LogView 不在任何面板里, 全仓无调用点) / 「右下角两权重叠」与几何不符 (只在一处零界相接) / `wheel_rows` 上界只钉「有夹子」不钉值 | 逐条修正 |
| R9 | Nit | 快捷键表的判据自相矛盾: 原始模式的 `b`/`'` 与已列的 Ctrl+B/G 是**同一动作的两套按键**, 按「猜不出来就列」该列、按「不重复」不该列 | 明写补一条判据「同一动作只列一条」, 并在注释里点破原判据的缺口 |

**未改、只记** (两处, 都不是本批引入):
- **空态下滚轮被静默吞掉**, 而同一提交刚给按键加了「尚未打开文件」—— 不对称。
  不补的理由: 滚轮是高频事件, 而 `set_notice` 每次都重置消退期限, 直接照抄会让提示
  变常驻 (从静默变成噪声)。要补得先做「同文案不续期」的去重。
- **`!self.settings_open` 那道滚轮门禁在真机上不可达** (卡开着时框架已把带坐标的鼠标
  事件全吞了) —— 无害的冗余防御, 测试直调 `app.event` 才验证得到它。

**另**: 本批改动了 **8 处既有测试的前置条件** (夹具补 `focused = true`)。它们由
「断言恒画」变成「断言持焦时画」, 补偿覆盖是 R3 那条重写后的三例测试。
**本批只删改了一条既有测试** (`readonly_sidebar_swallows_clicks_without_message` →
`..._but_says_why`, P21 的刻意改判, 已落档), 其余全是新增。

---

## Phase 收尾

- [ ] **T22: 注释口径**
  - 说明: ① `src/open.rs:6,27` 的「Ctrl+O·**拖拽**」—— 拖放打开**从未实现**
    (本仓 `DroppedFile`/`HoveredFile` 零命中; 框架匹配的 `WindowEvent` 全集 12 个里
    无 winit 拖放事件), 删掉不存在的路径; ② `main.rs:1366-1367` 注释提到 `j/k`,
    全仓无此实现
  - Acceptance: 全仓 grep 不再出现指向未实现功能的路径标签
  - Verify: `cargo test` (纯注释, 仅需编译通过)
  - Files: `src/open.rs`, `src/main.rs`
  - Scope: S

- [ ] **T23: 落账 `docs/ROADMAP-v1x.md`**
  - 说明: §一欠账表收 —— 13 条**手势缺失** (矩阵 §3: 表头排序/拖宽、右键菜单、托盘左键、
    侧键、中键专属、拖放打开、Ctrl+W/F1/Ctrl+=/Ctrl+-/Ctrl+A 全局全选……) +
    表头排序与拖宽 + **(b) 顺手与竞品对拍** (需先解决调研通路: 上一轮 **WebFetch 被
    网络策略整体拦截**, 结论只能按「待验证」看待); §四待裁项同步
  - Acceptance: 矩阵 §3 与 §2 的每一条都能在 ROADMAP 找到落点, 无遗漏
  - Verify: 人工对表 (矩阵 §2/§3 ↔ ROADMAP)
  - Files: `docs/ROADMAP-v1x.md`
  - Scope: S

## Checkpoint

- [x] **M0: 高风险 5 条实机走查** (2026-09-15, 用户实机) —— **4 真 1 假 → 歧义已消**
  - **成立** (与静态判定一致): P10 / P11 / P19 / P30
  - **P31: 成立且修复有效** —— 用户确认跑的是 **RustRover debug 本地构建**
    (patch 开着, 框架 `ec8ae09` 即 T2 修复在), 「不会走出模态」= **修复已生效**,
    静态判定不被推翻。这是本批**第一个改完即被实机确认的修复**。
- [ ] **人工验收清单全过 (用户实机)**: `tasks/matrix-interaction-polish.md` **§6**
  **全走一遍** (兼「剪枝」与「验收」; 8 个区域 + 已知不修 3 条 + 主观项 2 条)。
  **本节措辞对齐本仓惯例** —— 此前叫「実机人工验收清单」(日文「実」, 且不叫「验收」),
  `grep 验收清单` 搜不到本模块, 用户 2026-09-15 报「又找不到了」。
  **走查前先读 §6.0**: 要在**无 patch 的独立工作树**里建, 别用可能被并行会话改脏的工作区。
- [ ] **全过后**: 重拍商店截图 → 打 tag → GitHub Release → 商店提交 (v1.0 链路)

## 明确不做 (防「顺手修好」)

以下三条是**有意为之**的既有决策, 各有断言守着 —— M2 期间不许把它们当缺陷修掉:

- 行 hover 与斑马走**独立通道** (`row_hover_bg` / `row_band_bg`);
- 侧栏**不可点行有意不给 hover** (`histogram.rs:433`, 给了就是教错);
- 表格**不做单元格词级框选** (D2 划线, 但「拖了零反应」的沉默由 T12 兜)。

另有 **浅色 `selection`↔`hover` ΔL\* 0.79 的已知例外保留** (浅色养不起六个两两 ≥3 的面):
M2 只保证「同屏出现第三态时可辨」, 不承诺抹平例外 (spec §0)。
