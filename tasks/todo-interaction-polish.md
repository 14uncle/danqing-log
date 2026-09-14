# TODO: interaction-polish (交互打磨)

> spec: `docs/specs/SPEC-interaction-polish.md` | plan: `tasks/plan-interaction-polish.md`
> 矩阵 (兼实机核对单): `tasks/matrix-interaction-polish.md`
> 逐条勾选推进; 每任务完成后跑三件套 (fmt + clippy + test)。
> 构建序: **M1 (框架) → M2 → M3 → M4 → M5**; commit/push 点标 ⏸ 待用户点头。
> 基线: 本仓 **133 绿** (51 lib + 74 main + 8 genlog); 框架 **600 lib** + 集成 59 绿
> (2026-09-14 M1 完成后实测。**原记 115 / 583 都是旧数字** —— 115 是 selection-copy
> 之前的, 那之后 main 从 56 涨到 74; 记账前先量, 别抄旧数)。
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

- [ ] **T6: 三态共存 (P10)** 【已复核】
  - 说明: `src/view.rs:1084-1095` —— 去掉 `!has_text_sel` 压制; hover 从
    `else if` 里独立出来; 选中底 / 选区带 / hover **各自 token 同时画**
  - Acceptance: ① 拖框选全过程, 选中行底与左 accent 竖条**仍在**;
    ② 悬停选中行 hover 可见; ③ 三态同屏时两两可辨 (ΔL\* ≥3, 或至少一笔描边分隔)
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: M

- [ ] **T7: 命中通道 (P11 P14 P15)** 【P11 已复核】
  - 说明: `view.rs:1094` 与 `:1163` —— 命中行底**换 token**, 且**不再靠画序分胜负**;
    `:1227` 命中区间与文本选区带分通道; 「当前命中」与一般命中分通道
  - Acceptance: ① 悬停搜索命中行, hover **可见**; ② 多命中时**当前命中一眼可指认**
    (不再只靠 3px 竖条); ③ 文本选区带与命中区间同行交叠时**可分**
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: M

- [ ] **T8: 单元格底换通道 (P16)**
  - 说明: `view.rs:1109` 的 `cell_highlight_colors` —— 底笔不再与行选中底同 token;
    `accent` 描边**保留**为「具体哪一格」的唯一指示
  - Acceptance: ① 单元格底 ≠ 行选中底 (同屏可辨); ② 描边仍在;
    ③ 浅色下单元格高亮与 hover 可辨
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: S

- [ ] **T9: 超限拖选即时视觉 (P28)**
  - 说明: `view.rs:675` (`selection_over_limit`) 与 `:1591-1595` ——
    超限**发生即**改变选区外观 (不再等 Ctrl+C); 出声部分在 T12 接入 M3 通道
  - Acceptance: 拖选进行中越界时外观可区分, 且 T12 完成后伴随 notice
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: S

- [ ] **P6 (由 M1 转入, 2026-09-14 归位)**: 过滤/搜索栏的**焦点不可见**
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

- [ ] **T10: 命绘同源收口 (S1 S2 S3)**
  - 说明: S1 行 y 映射 (paint 循环式 `view.rs:1045,1058` ↔ `row_at` `:695-697`);
    S2 展开 glyph 命中区 (同常量两处 `:1248` ↔ `:1531`); S3 侧栏桶行高亮 y
    (`histogram.rs:306,311-320` ↔ `row_rect` `:85-92`)。**T17 (滚动条拖拽) 依赖
    S1 的单点** (D7)
  - Acceptance: ① S1 两式**对拍逐行相等**; ② S2 抽成单函数; ③ S3 走 `row_rect`;
    ④ 既有几何守卫全绿 (含 2026-09-14 的 `text_x`/`row_at` 那批)
  - Verify: `cargo test`
  - Files: `src/view.rs`, `src/histogram.rs`
  - Scope: M

## Phase M3: 拒绝要说清 (本仓, 依赖 M1)

- [ ] **T11: notice 通道 + 错误态独立视觉 (P27)** 【已复核】—— **地基**
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

- [ ] **T12: 九条沉默接入 (P20 P21 P22 P23 P24 P25 P26 P33 P34)**
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

- [ ] **T13: 「被吞掉的输入必有原因」守卫**
  - 说明: 把本仓孤例 (「正则无效」进底栏 `main.rs:1006-1008`) **升格为规则**
  - Acceptance: 守卫覆盖 T12 的 9 个触发点; 新增「被吞输入」时有处可挂 (不是逐条靠人记)
  - Verify: `cargo test`
  - Files: `src/main.rs` (或 `src/view.rs`)
  - Scope: S

## Phase M4: 前提与归属 (本仓, 依赖 M1)

- [ ] **T14: Esc 一并清行选中 (P19)** ★ —— **不变量优先**
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

- [ ] **T15: 右键 / 中键只认 Left (P29)**
  - 说明: `view.rs:1516` 处理 MouseInput **不筛 button** → 右键 / 中键与左键同效
    (行选中 / 开设置卡)。改: 只认 `MouseButton::Left`。理由: 缺陷不在「右键没有菜单」,
    而在**左键的语义被一个没有 affordance 承诺的手势触发了**
  - Acceptance: 右键 / 中键在列表区与底栏 ⚙ 上**不产生**选中或开卡; 左键各行为逐项不变
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: S
  - 注意: 面板弹出层的 scrim 关闭 (`overlay.rs:250-258` 仅 Left) 与标题栏按钮**已是**只认
    Left, 保持不动

- [ ] **T16: 模态守卫 (P32)**
  - 说明: 产品侧 `app_key_filter` 加 `settings_open` 守卫 (`main.rs:1235` 注释已自认
    穿透)。现状: 卡开着按 Ctrl+O **在卡上方弹系统文件对话框**; Ctrl+F 把焦点按 id 送到
    卡后**看不见的**输入框。框架 `app_key_filter` 是应用回调 → **产品侧守卫足够**
  - Acceptance: 模态开时 Ctrl+O / Ctrl+F / Ctrl+T / Ctrl+L / Ctrl+B / Ctrl+G
    逐个断言**不产生卡后副作用**; Esc **仍能**关卡 (既有行为保持)
  - Verify: `cargo test`
  - Files: `src/main.rs`
  - Scope: S

- [ ] **T17: 滚动条拖拽 (新, 用户裁定 5 越界纳入)**
  - 说明: paint 已有 (`view.rs:1354-1398`), 但 `:1453-1624` 整段 event **无滚动条分支**。
    新增 按下 / 拖动 / 释放 三分支 + **hover 态 + 按下态 + 光标 (PointingHand, 走 T1 的
    API)**; 位置数学**复用 `row_at`** (`view.rs:695-697`, 见 T10/S1), 不另推几何 (D7)
  - Acceptance: ① 拇指位置 ↔ 内容偏移**往返一致** (互为逆运算);
    ② 拖到顶/底**夹取**不越界; ③ 拖拽**不改变选中行**; ④ hover 与按下态可见;
    ⑤ 纵横两条都做 (若成本差大, 纵条优先, 横条可退 ROADMAP —— **须在 todo 里标明**)
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: M

## Phase M5: 可发现性 (本仓, 依赖 M3)

- [ ] **T18: 复制回执 (P17)**
  - 说明: 现状 Ctrl+C 成功**零反馈** (`view.rs:1585-1600`), 只在超限时报错。
    走 T11 的通道给回执, **用中性色** (与警示色分开), 自动消退 (Q3)
  - Acceptance: 复制成功后底栏出现回执 (含「复制了什么 / 几条」); 中性色与警示色可辨;
    **不改变状态栏几何**
  - Verify: `cargo test`
  - Files: `src/view.rs`, `src/main.rs`
  - Scope: S

- [ ] **T19: 快捷键页补 `/` 与 `f` + 托盘菜单文字 (P36 P38)**
  - 说明: 按设置卡**自己定的判据** (「只列猜不出来的那几个组合键; 方向键/翻页键不必教」
    `settings.rs:214`) 补齐 `/` 与 `f`; `→/←` 属方向键, **维持不教** (README 保留)。
    托盘菜单两项补快捷键文字 (`tray.rs:19,21` 第 4 参现为 `None`)
  - Acceptance: 设置卡快捷键页含 `/` 与 `f`; 托盘两项带文字; `→/←` 未被加入
  - Verify: `cargo test`
  - Files: `src/settings.rs`, `src/tray.rs`
  - Scope: S

- [ ] **T20: 设置卡「显示级别侧栏」开关 (P37)**
  - 说明: 常规页加开关 (Q2)。`config.histogram` 字段**已存在**, 走整文件同源写入
    (见 `src/config.rs` 的既有约束: 分头写会让「改主题」抹掉侧栏开关)
  - Acceptance: 开关控制侧栏显隐; 与 Ctrl+L **双向同步**;
    **改主题不抹开关** (既有约束的回归锁)
  - Verify: `cargo test`
  - Files: `src/settings.rs`, `src/main.rs`, `src/config.rs`
  - Scope: S

- [ ] **T21: Ctrl+F 不再清草稿 (P39)**
  - 说明: `main.rs:985` 「聚焦即干净开始」。改为: 栏**已聚焦**时 Ctrl+F = **全选内容**
    (便于覆写), **不清空**; 未聚焦时聚焦
  - Acceptance: 输入草稿后按 Ctrl+F, 内容**仍在**且被全选; 未聚焦时 Ctrl+F 仍聚焦
  - Verify: `cargo test`
  - Files: `src/main.rs`
  - Scope: S

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

- [ ] **M0: 高风险 5 条实机走查** (用户, **build 前**)
  - P10 (三态互斥 + `else if` 连坐) / P11 (纯画序) / P19 (跨三处行为链) /
    P30 (跨框架分发) / P31 (两条焦点路分叉)
  - 逐条标「成立 / 不成立 / 看错了 / 还有别的」; 推翻的条目**回写矩阵**
- [ ] **人工验收**: `tasks/matrix-interaction-polish.md` §6 核对单**全走一遍**
  (兼「剪枝」与「验收」; 8 个区域)
- [ ] **全过后**: 重拍商店截图 → 打 tag → GitHub Release → 商店提交 (v1.0 链路)

## 明确不做 (防「顺手修好」)

以下三条是**有意为之**的既有决策, 各有断言守着 —— M2 期间不许把它们当缺陷修掉:

- 行 hover 与斑马走**独立通道** (`row_hover_bg` / `row_band_bg`);
- 侧栏**不可点行有意不给 hover** (`histogram.rs:433`, 给了就是教错);
- 表格**不做单元格词级框选** (D2 划线, 但「拖了零反应」的沉默由 T12 兜)。

另有 **浅色 `selection`↔`hover` ΔL\* 0.79 的已知例外保留** (浅色养不起六个两两 ≥3 的面):
M2 只保证「同屏出现第三态时可辨」, 不承诺抹平例外 (spec §0)。
