# TODO: theme-recalibrate (模块 2 —— token 重校, 含 alpha)

> spec: `docs/specs/SPEC-ui-redesign.md` §1 模块 2 / §3 (A1) | plan: `tasks/plan-theme-recalibrate.md`
> 逐条勾选推进; 每任务完成后跑三件套 (fmt + clippy + test)。
> **改动全部落在 `../danqing`** —— 本仓只改文档与验收记录, 但三件套**两仓都要跑**。
> 状态: **Phase 1 待批** —— 未获批准不进 build。
> 基线: 本仓 **100 绿** (51+41+8); 框架 **577 绿** (2026-09-13 实测)。

## 问题一句话

模块 1 把混合空间从 sRGB 改成 linear（**正确**行为）, 但 token 的 alpha 是照
sRGB 空间的手感定的 —— 于是暗色上集体过量。`surface_variant` 实测渲染成
`(93,93,94)`, 而设计意图是 `(48,48,54)`; **更根本的是它与浅色那条差了 40 倍台阶**。

## Phase 1（本轮）: `DarkTheme::surface_variant` 重校

- [ ] **T0: 开本地 patch 联动**
  - 说明: 本仓根 `cp tools/local-patch.toml .cargo/config.toml`。
    **不开则 `../danqing` 的本地改动静默不生效。** 收尾 `rm` 回默认态（已 gitignore）。
  - Acceptance: 改一行框架代码后本仓 `cargo build` 能看见变化
  - Verify: `cargo tree` 指向本地路径。**别拿 `cargo metadata` 当验证**
    （2026-09-13 教训: metadata 不改写 lock, test 会）
  - Files: `.cargo/config.toml`（本地, 不提交）
  - Scope: XS

- [x] **T1: `DarkTheme::surface_variant` α 0.10 → 0.010** ✅ 2026-09-13
  - 说明: `danqing/src/theme.rs:377-380` 现为 `Color::rgba(1.0, 1.0, 1.0, 0.10)`,
    渲染成 `(93,93,94)`（Δ`L*` **+30.48**）—— 一块灰板。改成 `0.010` 后渲染
    `(38,38,43)`（Δ`L*` **+6.30**）, 与浅色那条的台阶回到同一量级。
    实算的 α→Δ`L*` 映射见 plan §0。
  - Acceptance: ① `surface_variant` 的 α 改为 `0.010`; ② 新增回归锁断言**渲染后**的值
    —— 用 `danqing` 现有的 sRGB→linear 原语反算合成结果, 断言与底色 `#191920` 的
    Δ`L*` **在 4..9 之间**（既非「几乎不存在」也非「灰板」）;
    ③ 既有对比度护栏不得因本改动变红
  - Verify: 两仓三件套 + 真机（暗色: 斑马/表头/过滤框从灰板变为淡台阶; 浅色: 不动）
  - Files: `danqing/src/theme.rs`（值 + 测试）
  - Scope: S
  - 备注: **只改值不改签名** —— 既有调用点与其它产品零编译波及
  - 实测: RED 先行 —— 新守卫先红, 报 **对比度 2.648**, 与我在真机截图上用取色器
    独立算出的 2.648 **逐位吻合**（说明测试里的合成模型复现了硬件的真实行为）。
    改 α 后绿。框架 lib 578 绿（基线 577, +1 即该守卫）; 本仓 100 绿。

### 附: 本任务牵出的两个积压问题（2026-09-13 实测）

**① 模块 1 的波及面漏了 `tests/` —— 已修**

模块 1 当时只修了 `src/` 里 6 个 widget 测试模块, **`tests/` 下的集成测试整个漏了**,
留下 2 条红 + 4 条**永真**断言:

- `tests/design_system.rs::button_paints_with_theme_accent` —— 红（比的是
  linear 实例值 vs sRGB token）。
- `tests/hover_debug.rs::deep_hover_changes_paint_color` —— 红（同款）。
- **更要紧的是另外 4 条没红的**: `box` / `scrollable` / `text_input` / `text_area`
  比的全是 **白色** token（`surface` / `surface_input` 都是 `rgba(1,1,1,α)`）,
  而**白色恰好是 sRGB↔linear 变换的不动点** (`srgb_to_linear(1.0) == 1.0`) ——
  于是它们照样绿, 但它们**已经抓不到任何东西了**: 组件就算一个 token 都不读、
  直接画纯白, 也照样通过。修法是解码 + 补一条非白 token 的断言。

**② 框架测试其实一直是红的 —— 未修, 待裁**

`cargo test` **遇到第一个失败的 target 就停**, 所以此前的「框架 577 绿」只统计了
lib, 后面的集成测试从没被计入。用 `--no-fail-fast` 才看全: 基线共 **6 条红**,
其中 2 条是上面①（已修）, 另 **4 条是布局/焦点测试, 与颜色无关**:

- `fit_center_in_column_does_not_push_later_children_off_screen`
- `fit_box_with_child_wraps_content_height`
- `fill_center_with_fill_max_centers_child_across_full_cross`
- `click_empty_card_clears_focus_for_keyboard_fallback`

像是先前 `Center` 布局改动（见 `CLAUDE.md` 2026-09-13「去掉 Center」条）之后
**测试没跟着改**。**要先判「测试陈旧」还是「代码错了」**——不在本模块范围内,
先记录不动。

- [x] **T3: 设置卡去掉边框**（用户 2026-09-13 在 Checkpoint B 的截图复看中直接裁定）
  - 触发: T2 之后用户复看设置卡, 指令「**设置面板 border 去掉**」。
  - 说明: `settings.rs` 的卡片是 `UiBox::new(t.background()).border_color(t.border())`。
    原注释声称「靠 border 与底层区分」—— 但**实测不需要**: 卡外是 scrim 压暗的
    `(18,18,24)`, 卡内是不透明的 `background()` `(25,25,32)`, **scrim 已经在做区分**。
    那圈边是重复的一道, 且 T2 之前是 `(131,131,135)` 的亮框。
  - Acceptance: 删掉 `.border_color(...)`（`UiBox` 不调它就完全不画边 —— 看
    `layout/box_.rs` 的 `if let Some(border) = self.border_color`, 所以是**删**不是
    把 width 置 0）; 回归锁 `settings_card_paints_no_border`
  - Verify: 真机复看（卡片靠 scrim 台阶浮起）
  - Files: `src/settings.rs`
  - Scope: XS
  - 实测: 截图实测边框像素 **(86,86,90)** —— 与 T2 算出的 `border()` 新值
    `(87,87,90)` 逐通道吻合（又一次「算的和屏幕上的对得上」）。
    **守卫验证过有牙**: 把 `.border_color(t.border())` 临时加回去, 测试报
    「设置卡不应画边框」并红, 随后还原。
    本仓 **101 绿**（51 lib + 42 main + 8 genlog）; fmt / clippy 零警告。
  - 备注: **本项是产品侧改动**（与 T1/T2 的框架侧不同仓）, 落地时归本仓那笔。

## 附二: 「切主题后颜色不跟」是一类 bug, 不是一条 (2026-09-13 用户实机报)

**用户报**: 启动暗色切浅色 / 启动浅色切暗色, 标题栏「丹青日志 LogLens」看不清。

**根因（查实, 不是推测）**: `view()` **只在启动时求值一次** ——
`danqing/src/window/mod.rs:207` 的 `let tree = app.view();`, 之后整棵树交给
`Handler`, **不再重建**。所以 `TitleBar::themed(&title_theme(self.theme), ..)`
烘进去的是**启动那一刻**的主题色; 而卡面上其它控件走 `bind_color` 闭包每帧重读。
于是切主题时: 底色 (清屏色) 换了、标题文字没换 → 浅色主题上浅字 / 暗色主题上暗字。
**两个方向都成立**, 与用户描述一致。

- [x] **T4: 标题栏挂 `bind_theme`**
  - 框架**早就为这件事**准备了 API (`TitleBar::bind_theme`, 文档原话
    「每帧从应用状态重取主题, 刷新随场景流动的颜色」), 产品**从没调用过**
    —— 与 R1 的 `set_clear_color` 是同一个形状: **机制齐备, 调用缺失**。
  - 顺带把标题栏构建抽成 `fn title_bar(theme, title) -> impl Widget`:
    **测试必须复用同一份构建代码**才守得住, 否则测的是副本。
    参数取**值**而非 `&LogApp` —— edition 2024 里 `impl Trait` 捕获全部输入生命周期,
    借引用返回就不是 `'static`, `Column::child` 编译不过 (E0521)。
  - 回归锁 `title_bar_colors_follow_theme_switch`: **构造用暗色、sync 用浅色**,
    断言画出来的标题文字色 = 浅色主题的正文色。两边同主题的话, 就算绑定掉了也照样绿。
    **验证过有牙**: 摘掉 `bind_theme` → 报「没命中即 `bind_theme` 缺失」并红。

- [x] **T5: 设置卡底色改绑定**
  - 同一类: `settings_card(theme)` 的 `UiBox::new(t.background())` 是构造值,
    浅色启动切暗色 → 暗色 scrim 上浮着一张**浅色卡**。
  - 先写测试证实: `settings_card_background_follows_theme_switch` **先红**(证明诊断成立),
    改 `bind_color(|app| app.theme.theme().background())` 后绿。
  - Files: `src/main.rs` / `src/settings.rs`
  - 结果: 本仓 **103 绿**（51 + 44 + 8）; fmt / clippy 零警告

### ⚠ 仍未解: 框架层面**只有 TitleBar 有 per-frame 主题绑定**

盘点全框架的 `bind_theme` 覆盖面:

| 组件 | 每帧刷主题 |
|---|---|
| `TitleBar` | ✅ 唯一一个 |
| `Box` / `Button` / `Dropdown` / `TextInput` / `TextArea` / `Overlay` / `Scrollable` / `Switch` / `IconInput` | ❌ 只有构造态 `themed()` |

**后果（已确认, 非推测）**: 设置卡的 `Tabs::new(&t)` 在构造时烘死
`color_active: accent` / `color_inactive: text_secondary` 等 4 个色 (`tabs.rs:108-111`),
**且 `Tabs` 根本没有主题绑定可用**（只有 `bind(active_index)`）。
浅色启动切暗色 → 页签名停在浅色主题的 `text_secondary`(深灰) 压在暗色卡面上, **读不了**。
`Overlay::themed(&t, ..)` 同样烘死, 但两主题的 `scrim()` 同值, 无可见后果。

**这不是本模块能收的**: 要么给 `Tabs` 等组件补 per-frame 主题绑定（框架加法）,
要么让框架支持「主题变化时重建视图树」（但要解决组件内部状态保活）。
**归模块 3 `danqing:component-polish`** —— 它本来就是「组件默认观感对齐 token」,
而这是同一件事的前提: **两个主题都能用**, 才谈得上对齐。

**这一条把模块 3 从「打磨」抬成了「结构性前提」**: 立项意图里写的是
「浅色暗色都做」, 而按现在的框架模型, 除标题栏外**没有任何组件能跟随运行时切换**。

**用户裁定 (2026-09-13)**: 先做「结构性」这部分, 并**一次想清楚形式**,
不要只补 `Tabs` 一个把同一个坑留给下一个组件。已同步改写
`SPEC-ui-redesign.md` §3 的模块 3 条目。

- [x] **T6: `Tabs` / `Dropdown` 补 per-frame 主题绑定** ✅ 2026-09-13
  - **形式 (有意统一, 别再发明第二种)**: 沿用 `TitleBar::bind_theme` 的同一形状
    `bind_theme<S: 'static, T: Theme + 'static>(f: impl Fn(&S) -> T)`。
    颜色子集各抽私有结构 (`TabColors` / `DropdownColors`) + 一个
    `from_theme(&impl Theme)`, **`new`/`themed` 与 `bind_theme` 共用这一份** ——
    两处各写一套口径正是本仓漂过的典型。
    度量 (字号/高度/圆角/间距) **不刷新** —— 两主题这些值本来就相同, 且动它会牵动布局。
  - **`Dropdown` 的另一处**: `Dropdown::new()` **内部硬编码 `LightTheme`**
    (`dropdown.rs`), 暗色下用 `new()` 建出来的下拉框整个是浅色 —— 比「不跟随切换」
    更早一层。产品改成挂绑定。
  - 产品侧接入: 设置卡的 `Tabs` 与主题 `Dropdown` 各挂 `.bind_theme(|app| app.theme.theme())`。
  - 回归锁 `settings_card_tabs_follow_theme_switch`: **先红** —— 报
    「还剩 **5 处**暗色 text_secondary」, 补完两处绑定后归 **0**。
    断言刻意用「**旧主题的色一个不剩**」而非「新主题的色存在」: 卡里另有别的
    `Text` 绑着同一支 token, 用「存在」判会**永真** (本会话已吃过一次这亏)。
  - Files: `danqing/src/widget/view/tabs.rs`、`danqing/src/widget/form/dropdown.rs`
    (+ 本仓 `src/settings.rs`)
  - 结果: 框架 lib 578 绿 / 本仓 **104 绿** (51 + 45 + 8); fmt / clippy 零警告

### 仍未补的组件 (按需, 形式照 T6)

`Box` 有 `bind_color` 可用; 仍只有构造态的是 `Button` / `TextInput` / `TextArea` /
`Overlay` / `Scrollable` / `Switch` / `IconInput`。
**判定「哪些还需要」属模块 3** —— 不要一次全补, 补的是产品真正踩到的。

### 落地链 ✅ 2026-09-13

- 框架两笔: `e84798f` (token 重校 + 补修漏掉的集成测试)、`435ad14` (Tabs/Dropdown 绑定)
  → push `dev` (`e30c00a..435ad14`)
- 本仓: `416bc2c` / `865daa6` / `85eb7b1` / `b074bf6` / `97af5bb` → push `dev`
- `rm .cargo/config.toml` 关 patch → `cargo check` 实测从 GitHub 拉
  `Compiling danqing v0.1.0 (https://github.com/14uncle/danqing#435ad147)`
- `Cargo.lock` 钉 `danqing#435ad147b4f869185616b64a05b2e5cf6eb09b49`
  (`danqing-logfile#baee0a8b…` 未动)
- 对钉住的 rev 重跑三件套: fmt / clippy 零警告 + 本仓 104 绿

### Checkpoint A: 真机截图过审 (用户)

- [ ] 暗色截图: 斑马/表头/过滤框应是**淡台阶**而非灰板; 浅色应**完全不变**
- [ ] **用户点头后**才谈 Phase 2

## Phase 2: 其余暗色半透明 token ✅ 2026-09-13

- [x] **T2: 四条 token 按角色定台阶**
  - 说明: 先量了全部半透明 token 的**渲染后**台阶, 结果**只有 T1 那条是对的**,
    其余全在同一量级的过量上 —— 暗色 `border` 竟渲染成 `(131,131,135)` (Δ`L*` **+45.8**),
    一条 1px 线比整个底色亮近 5 倍; 浅色同类只有 −7.3。
  - Acceptance: 台阶按**角色**定（不是一律同值 —— 大面积填充要淡, 1px 细线要够亮
    才看得见）; **每个上界卡在旧值之下**, 防止按老手感调回去
  - 实测（旧 → 新, 全是渲染后的真实值）:

    | token | 旧 α → 新 α | 旧渲染 | 新渲染 | 旧 Δ`L*` → 新 Δ`L*` |
    |---|---|---|---|---|
    | `surface` | 0.08 → **0.022** | (84,84,86) | (50,50,54) | +26.7 → **+11.9** |
    | `surface_input` | 0.12 → **0.031** | (100,100,102) | (57,57,60) | +33.4 → **+15.0** |
    | `divider` | 0.15 → **0.061** | (99,99,103) | (68,68,72) | +33.0 → **+19.9** |
    | `border` | 0.28 → **0.109** | (131,131,135) | (87,87,90) | +45.8 → **+28.0** |

  - **`selection` 不动**（accent 0.30, Δ`L*` +26.3）: 它是**语义高亮**, 该显眼。
    拿它跟填充一起比会得出误导性的结论 —— 用户那张截图里选中行读得很清楚,
    正说明 +26 对「选中」是对的、对「斑马」是错的。**同一个数字, 角色不同结论相反。**
  - **浅色不动**: 它的 `surface`/`surface_input` 只有 +2.2/+3.0 —— 不是「对」而是
    **没有余量**（底色 `L*` 已 96.95, 头顶只有 3 点空间）。这条约束记给模块 5:
    浅色做层次只能靠**变暗**, 不能靠变亮。
  - 守卫: 原单条 `surface_variant` 测试改为**表驱动五条**, 每条一个对比度区间。
    **验证过它有牙**: 把 `border` 临时调回 0.28, 测试报
    「border 的台阶越界: 对比度 4.619 不在 2.2..2.7 内」并点出是哪条 ——
    本会话已经产出过 4 条**永真**断言, 所以这次专门证明过守卫能失败。
  - Files: `danqing/src/theme.rs`（值 + 测试）
  - Scope: M
  - 结果: 框架 lib 578 绿; 本仓 100 绿; fmt / clippy 零警告

## Phase 3（待批）: 护栏改为量**渲染后**的值

- [ ] 现有 WCAG 护栏量的是 token 而非渲染结果 —— 那正是模块 1 之前
      「护栏全绿、屏幕全灰」的成因。改成对渲染结果断言（取色器手段见 SPEC §3 A1）
- [ ] **主题间一致性守卫**: 同一 token 在两个主题的 Δ`L*` 不得相差一个量级
      —— 本 bug 的直接回归锁
      *（T2 已部分兑现: 五条 token 的守卫量的都是合成后的值; 差的是跨主题那条。
      另注: 浅色 `surface` 的 +2.2 是**余量不足**而非设计选择, 跨主题守卫
      得认这个例外, 否则一上线就误报。）*


## Phase 3（待批）: 护栏改为量**渲染后**的值

- [ ] 现有 WCAG 护栏量的是 token 而非渲染结果 —— 那正是模块 1 之前
      「护栏全绿、屏幕全灰」的成因。改成对渲染结果断言（取色器手段见 SPEC §3 A1）
- [ ] **主题间一致性守卫**: 同一 token 在两个主题的 Δ`L*` 不得相差一个量级
      —— 本 bug 的直接回归锁

## 偏离记录

> 执行中追加。
