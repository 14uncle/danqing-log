# TODO: token-completion (模块 4 —— 暗色全链路接完)

> spec: `docs/specs/SPEC-ui-redesign.md` §1 模块 4 / §3 | plan: `tasks/plan-token-completion.md`
> 逐条勾选推进; 每任务完成后跑三件套 (fmt + clippy + test)。
> **Phase 2 起要动 `../danqing`** —— 三件套**两仓都要跑**, 落地链按农场约定走。
> 状态: **待批** —— 未获批准不进 build。
> 基线: 本仓 **99 绿** (51 lib + 40 main + 8 genlog), 框架 **577 绿** (2026-09-13 实测)。

## 范围前提 (别照 `SPEC-dark-theme.md` 的清单做)

那份「已知遗漏清单」12 条里 **8 条已完成**（逐条核过, 见表）。
本模块实际只做 **R1–R5** 五项, 其中 R1 是清单里根本没有的那条。
**读 plan §0 的表, 不要读旧清单。**

## Phase 1: 白板 (纯产品侧, 零框架依赖)

- [x] **T1: `clear_color` 跟随主题 (R1)** ✅ 2026-09-13
  - 说明: `WindowConfig.clear_color` (`main.rs:1441`) 写死浅色, 且**全仓无一处
    `set_clear_color`** —— 启动时配置里的暗色主题不被采纳（`main.rs:297` 已读出主题,
    `:1438` 构造配置时却没问它）, 运行时 `Msg::SelectTheme` (`:1115-1118`) 也不发通知。
    标题栏那条亮带的颜色**就是** `clear_color`（`TitleBar` 背景是有意的 `TRANSPARENT`,
    `danqing/src/widget/title_bar.rs:254` + 测试 `:1410` 锁死）, 内容区反被
    `th.background()` 盖住故看不见。
  - Acceptance: ① 新增单点 `fn window_clear_color(t: AppTheme) -> Color` 返回
    `t.theme().background()`; ② 启动路径与切换路径**都**经它 (AD1: 不许两处各算一份);
    ③ `Msg::SelectTheme` 里发 `window_sender.set_clear_color(...)`;
    ④ 单测: 浅/暗两主题各断言 = `LightTheme.background()` / `DarkTheme.background()`
  - Verify: 单测 + 真机两条 —— (a) 存暗色配置启动, 窗口底色**一开始就是暗的**;
    (b) 运行中浅↔暗互切, 底色**即时跟随**
  - Files: `src/main.rs`
  - Scope: XS
  - 实测: 新增 `window_clear_color` (置于 `title_theme` 旁, 同为「主题→派生值」类);
    `run()` 的 `WindowConfig.clear_color` 改用 `app.theme`（`app` 上一行已从配置
    读出主题, 此前没人问它）; `Msg::SelectTheme` 里对 `window_sender` 发
    `set_clear_color`。**RED 先行**: 测试先红 (`cannot find function
    window_clear_color`, E0425 ×3), 实现后绿。全量 **100 绿** (51+41+8, 较基线 +1),
    fmt / clippy 零警告。
    **本仓此处零框架改动** —— 框架侧 `set_clear_color` 通路本就齐备
    (`window/event.rs:90` → `Handler`), 产品从未调用而已。
    **Checkpoint A 像素实测确认诊断成立**: 修后 y=2 全宽 `(25,25,32)`,
    标题栏左侧（x<173, logo + 标题）在 y=20 有 63 个底色像素 —— 白板消失。
    *（留档: 我中途自纠过一次说「不是 clear_color」, 那是**错的** —— 三个采样点
    x=300/900/1700 全落在过滤框内（173..1796）, 把框的灰看成了标题栏的灰。
    采样必须跨控件边界取。）*
    **顺带照出的**: 过滤框/表头/斑马三处的 `(93,93,94)` 是 `surface_variant()`
    白 10% 经 linear 混合的产物, 归模块 2 的 alpha 重校。

### Checkpoint A: 真机截图过审 (用户)

- [ ] 暗色截图: 标题栏那条亮带消失、整窗统一; 浅色截图: 无意外变化
- [ ] **用户点头后才进 Phase 2** —— 这是「小批次、过一批留一批」的落点

## Phase 2: 框架开口 + 输入栏

- [x] **T0: 前置 —— 开本地 patch 联动** ✅ 2026-09-13
  - 说明: 在本仓根 `cp tools/local-patch.toml .cargo/config.toml`。
    **不开则 `../danqing` 的本地改动静默不生效**（patch 默认关）。
    收尾 (`T6`) 时 `rm` 回默认态; 该文件已 gitignore, 不进提交。
  - Acceptance: 改一行框架代码后本仓 `cargo build` 能看见变化
  - Verify: `cargo tree` 输出指向本地路径。**不要拿 `cargo metadata` 当验证手段**
    （2026-09-13 教训: metadata 不改写 lock, test 会 —— 拿 metadata 验证会得出相反结论）
  - Files: `.cargo/config.toml`（本地新建, 不提交）
  - Scope: XS

- [x] **T2: `TextInput::bind_theme` + 过滤/搜索栏接入 (R2)** ✅ 2026-09-13
  - 说明: 框架 `TextInput` 在 `themed()` 里把主题**摊平成已解析的 `Color` 字段**
    (`danqing/src/widget/form/text_input.rs:21-73`), **无 setter** ——
    跨帧持有编辑状态的控件没有换主题的路径。`Dropdown` 同款但不受影响
    （设置卡每帧重建）。**受害的只有过滤栏/搜索栏**（重建会丢输入文本与焦点）。
    产品侧还额外把四个可见色全写死 (`view.rs:1226/1233/1241-1243`)。
  - Acceptance: ① 框架新增 `TextInput::set_theme(&mut self, theme: &impl Theme)`,
    **重放 `themed()` 的全部赋值**（含 `placeholder_color`）; 语义 doc 写明是
    「**换主题**」会覆盖此前显式设置（AD2: 只加不破, 既有调用点零波及）;
    ② 框架 RED 测试: Light 建的输入框 `set_theme(&DarkTheme)` 后,
    `color`/`caret_color`/`selection_color`/`placeholder_color`/`border_color`/
    `focus_border_color`/`background` **逐项 = DarkTheme 对应 token**;
    ③ `Bar::sync` (`view.rs:1323`) 里对两个输入框各调一次;
    ④ 删掉四处写死色, 占位色改 `th.text_secondary()`;
    ⑤ 产品侧 grep `TextInput::themed(&LightTheme)` **零命中**
  - Verify: 两仓三件套 + 真机（暗色: 过滤/搜索栏文字、占位、光标、选区全部可读;
    浅色: 无意外变化）
  - Files: `danqing/src/widget/form/text_input.rs` (+ `form/mod.rs` 如需 re-export),
    `src/view.rs`
  - Scope: M
  - 备注: **本轮第一次给框架加公开 API**。按农场约定走联动链 ——
    框架三件套 → **push 框架** → 本仓 `cargo update -p danqing` → 提交 lock。

## Phase 3: 收尾三小项

- [x] **T3: `header_line()` 随主题 (R3)** ✅ 2026-09-13
  - 说明: `view.rs:76-78` 恒用 `LightTheme.divider()`。调用点两处 (`:563`、`:890`)。
  - Acceptance: 改 `fn header_line<T: Theme>(th: &T) -> Color { th.divider() }`,
    两处调用点传 `th`
  - Verify: 两仓三件套 + 真机（暗色表头线不再用浅色主题的色）
  - Files: `src/view.rs`
  - Scope: XS
  - 备注: 动前先确认两处 `th` 在作用域内; 若 `:563` 没有则从 `self.theme.theme()` 取

- [x] **T4: 书签行号金按主题取色 (R4)** ✅ 2026-09-13 (用户裁定: 两支金放产品侧, 不套 accent)
  - 说明: `view.rs:678-679` 写死 `0.75,0.60,0.10`。旧清单建议套 `Theme.accent`,
    但那会把「书签」与「选中/强调」混成一个通道 —— **建议不套**
    （亮色金 + 暗色更亮的金, 两支都放产品侧, 不为此扩框架 trait）
  - Acceptance: 按用户裁定实现; 回归锁断言两种主题下书签色**互不相同**且
    **都不等于** `text_secondary()`（即确实换了通道）
  - Verify: 两仓三件套 + 真机（暗色下书签行号一眼可辨）
  - Files: `src/view.rs`
  - Scope: XS

- [x] **T5: `title_theme()` 去手抄 (R5)** ✅ 2026-09-13 (用户裁定: 六项取 `LogTheme`, 独立成批)
  - 说明: `main.rs:84-107` 已按主题分支, 但调色板是手抄的, 且与
    `docs/SPEC-dark-theme.md:168-195` 定稿不符（浅色 accent 是**蓝** `0.18,0.35,0.60`,
    框架玉色是 `15,118,110` —— 两套色）。
  - Acceptance: `base`/`accent`/`text_primary`/`text_secondary`/`surface`/
    `surface_input` 六项改取 `LogTheme`（`config.rs:145+` 已有逐 token 分派）;
    `backdrop_light`/`backdrop_dark` 框架无对应 token, **保留手写并注明理由**
  - Verify: 两仓三件套 + **真机截图过审**（浅色标题栏 accent 由蓝变玉色 —— 本模块
    唯一改浅色观感处, 与 T5(旧) 同类, 须用户复看）
  - Files: `src/main.rs`
  - Scope: S
  - 备注: **本项独立成批, 不夹带进其它任务** —— 未过当场回退

## Phase 4: 收口

- [ ] **T6: 三件套 + 真机验收 + 落地链**
  - `cargo fmt` + `clippy --all-targets -- -D warnings` + 测试全绿（**两仓各跑**）
  - 真机验收: 浅/暗各一轮, 逐条对 R1–R5 的 Verify
  - 落地链: `rm .cargo/config.toml` 关 patch → **push 框架** →
    本仓 `cargo update -p danqing` → 提交 lock → 两仓分别提交（message 注明关联）
  - 关 patch 后重跑三件套, 确认编译输出是
    `Compiling danqing v0.1.0 (https://github.com/14uncle/danqing#<新 sha>)`
    —— **从 GitHub 拉的**, 证明改动真进了远端且不再依赖本地 patch

- [x] **T7: 更新 `docs/SPEC-dark-theme.md` 的过时清单** ✅ 2026-09-13
  - 说明: 那份「已知遗漏清单」现在**谎报进度**（称 4 条残, 实际 3 条残 + 1 条半）。
    本仓复发性教训: 「加新决定、不回头清旧文字」。不可信的清单会让下一个人
    重做已完成的活。
  - Acceptance: 12 条逐条标 ✅/⚠️/❌ 并注明依据; 补入清单原本没有的 `clear_color` 一条;
    加一句「本清单已于 <日期> 核对, 此后以 `SPEC-ui-redesign.md` 为准」
  - Files: `docs/SPEC-dark-theme.md`
  - Scope: XS

## 未开 (依赖本模块或属其它模块)

- [ ] 模块 2 `danqing:theme-recalibrate` —— 含 alpha 重校 + **Open Q3 的暗色语义色**
- [ ] 模块 3 `danqing:component-polish` —— T2 已吃掉其中 `TextInput` 那一小块
- [ ] 模块 5 `log:layout-rhythm` —— **须先过设计提案门**

## 偏离记录 (2026-09-13)

1. **T2 的方法名改了**: plan 写 `TextInput::set_theme(&mut self, theme)`,
   **实际实现为 `bind_theme`** —— 与 `TitleBar` / `Tabs` / `Dropdown` 统一形状
   (`bind_theme<S, T: Theme>(f: Fn(&S) -> T)`)。改名的理由: 本批一共给四个组件
   补了同一件事, **形式必须一致**; 而 `set_theme` 那种「传一个主题值进来」的签名
   与另外三个对不上, 会立刻分叉出第二种写法。
   附带好处: 绑定在**构造时**挂上、每帧自动重放, 不像 `set_theme` 那样要求调用方
   每帧手动喂 —— 少一个「忘了调」的坑。
2. **T2 的验收项 ⑤「grep `TextInput::themed(&LightTheme)` 零命中」判据过严**:
   构造总得给一个主题, 它只是首帧兜底值, 真正决定观感的是绑定。保留该调用并注释说明。
3. **T2 的占位色不改** (原 Acceptance 要求改 `th.text_secondary()`)。
   原因: 框架 `TextInput::themed()` 把它定成**与主题无关**的中性灰 `(160,160,160)`,
   并进主题绑定会**顺带改掉所有其它产品的占位色**; 且现有值两个主题都读得动
   (暗色 3.20 / 浅色 3.9)。已同步改 `SPEC-dark-theme.md` 清单第 6 条的结论。
4. **T3 + T4 合成一笔提交**: 两者都只动 `view.rs`, 无法按文件拆成两笔干净的提交。
5. **T5 触发一条既有守卫转红** (`title_bar_colors_follow_theme_switch` —— 它内部钉着
   旧手抄值 `0.12`)。**那条红是真的**, 已改为从 `LightTheme.text_primary()` 取。
   留档理由: 这是「守卫确实在盯颜色」的一次实证, 不是修测试糊过去。
