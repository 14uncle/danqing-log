# Plan: 模块 4 `log:token-completion` (暗色全链路接完)

> spec: `docs/specs/SPEC-ui-redesign.md` §1 模块 4 / §3
> 上游清单: `docs/SPEC-dark-theme.md` §模块 3「已知遗漏清单」（**该清单已过时**, 见下）
> 模块 1 执行记录: `tasks/todo-color-pipeline.md`
> 状态: **待批** —— 未获批准不进 build

---

## 0. 摸底结论: 范围比 spec 写的小得多

SPEC §3 说「12 条残留 + 新发现 4 条」。**逐条核过代码后, 12 条里 8 条其实已经做完**
（清单停在上一轮结束时的状态, 此后 T5 与本轮其它改动又清掉一批）。重列如下 ——
**这张表是本 plan 的唯一范围依据**, 不是那份清单。

| # | 清单项 | 实际状态 | 依据 |
|---|--------|---------|------|
| 1–4 | `level_color` / `level_cell_color` / `status_color` / `cell_color` 兜底近黑 | ✅ **已做** | `view.rs:103-117` 等四处已 `th.text_primary()` (今日 T5) |
| 5 | `Bar::base_input()` 输入色/光标色 | ❌ **残** | `view.rs:1237-1245` 写死 `&LightTheme` + 三色 |
| 6 | `Bar` 占位色 `0.45,0.45,0.48` | ❌ **残** | `view.rs:1226` / `:1233` |
| 7 | 书签行号金 `0.75,0.60,0.10` | ❌ **残** | `view.rs:678-679` |
| 8 | 展开 `[+]/[-]` 图标色 | ✅ **已做** | `view.rs:738` 已是 `th.text_primary()` |
| 9 | `title_theme()` 标题栏调色板 | ⚠️ **半做** | `main.rs:84-107` 已按主题分支, 但调色板是**手抄的**, 与 spec 定稿不符 |
| 10 | `settings.rs` 卡片硬编码 | ✅ **已做** | 走 `sync()` 取 `app.theme.theme()` (如 `settings.rs:435-439`) |
| 11 | 主题切换 UI 换 Dropdown | ✅ **已做** | 设置卡「常规」页 |
| 12 | 删除自建 `src/theme.rs` | ✅ **已做** | 仓内已无该文件 |

**加上清单里根本没有的 1 条（本轮最大的那条）**: 见 R1。

### R1 —— `clear_color` 从不跟随主题（「暗色那条白板」的真因）

`WindowConfig.clear_color` 在 `main.rs:1441` **写死浅色** `Color::rgb(0.98,0.98,0.98)`,
而**全仓没有任何一处调用 `set_clear_color`**（grep 过, 零命中）。

- 启动: `LogApp::new_empty()` 已从配置读出主题（`main.rs:297` `theme: cfg.theme`）,
  但紧接着 `run()` 构造 `WindowConfig` 时**没问它** —— 存了暗色也照样开在白底上。
- 运行中: `Msg::SelectTheme`（`main.rs:1115-1118`）只改 `self.theme` + 存盘,
  **不发 `SetClearColor`** —— 切主题后窗口底色不动。

**为什么它表现成「标题栏那条亮带」**: 框架的 `TitleBar` 背景是 `Color::TRANSPARENT`
（`danqing/src/widget/title_bar.rs:254`, 且有测试 `assert_eq!(bar.bg, Color::TRANSPARENT)`
把它锁死 —— 那是**有意**设计, 让窗口底色/场景层透出）。
所以标题栏那条的颜色**就是 `clear_color`**。内容区反而看不见: `view.rs` 在内容区
画了不透明的 `th.background()` 盖住了。

> **这不是「新坏」, 是「变显眼」**: 模块 1 把内容区从假灰改成诚实的近黑之后,
> 旁边那条真·浅色才刺出来。与 T5 那次同款 —— 修好一处, 照出另一处。

**修法（单点定义, 不两处各写一份）**: 新增 `fn window_clear_color(t: AppTheme) -> Color
{ t.theme().background() }`, 启动与切换**都**走它。
理由: `SceneTheme::background()` 就是 `palette.base`（`danqing/src/theme.rs:601`）,
标题栏那条想要的正是「主题背景色」。框架侧 `set_clear_color` 通路**已经齐备**
（`window/event.rs:90` → `WindowAppEvent::SetClearColor` → `handler.rs:1310-1313`）,
**只是产品从没调用过** —— 本任务纯产品侧, 零框架改动。

### R2 —— 过滤/搜索栏（框架挡路, 需一行框架 API）

`Bar::base_input()`（`view.rs:1237-1245`）建 `TextInput::themed(&LightTheme)`,
**并把四个可见颜色全部显式写死**: `color(0.20)` / `caret_color(0.10,0.10,0.12)` /
`selection_color(...)` / 占位色 `(0.45,0.45,0.48)`。`Bar::sync` 会把 `self.theme`
更新成当前主题（`view.rs:1323`）并用于**自绘**背景, 但**改不动那两个 `TextInput`**。

**根因是框架的一个真实缺口**: `TextInput` 在 `themed()` 里把主题**摊平成已解析的
`Color` 字段**（`danqing/src/widget/form/text_input.rs:21-73`）, 且**没有 setter** ——
建好之后再无换主题的路径。`Dropdown` 同款（`form/dropdown.rs:125-151`）, 但 `Dropdown`
不受影响: 设置卡每帧在 `view()` 里重建, 每次 `themed()` 都拿当前主题。**唯一受害的是
「跨帧持有编辑状态的控件」** —— 恰好就是过滤栏/搜索栏这两个。
（重建也不行: 会丢正在输入的文本与焦点。）

**修法**: 框架加 `TextInput::set_theme(&mut self, theme: &impl Theme)`,
重放 `themed()` 的全部赋值; `Bar::sync` 里对两个输入框各调一次;
产品侧把四处写死颜色**删掉**（改为跟随 token）。

> 这是本轮**第一次给框架加公开 API**。语义定为「**换主题**」而非「微调」——
> 它会覆盖此前的显式设置。这不是副作用而是定义: 若它只改「没被显式设过的字段」,
> 就得记住谁设过谁没设, 复杂度远大于收益。doc 里写明。

### R3 —— `header_line()` 恒用浅色主题

`view.rs:76-78` 直接 `LightTheme.divider()`, 不看当前主题。调用点拿去画表头下划线,
暗色下那条线用的是浅色主题的 divider 色。**小, 但同属「接完」范畴。**

### R4 —— 书签行号金

`view.rs:678-679` 写死 `0.75,0.60,0.10`。清单 #7 的原方案是「从 `Theme.accent` 派生」,
但 **gold 是「书签」语义, 套 accent 会把它和「选中/强调」混成一个通道** —— 建议不套。
见 Open Q1。

### R5 —— `title_theme()` 手抄调色板

`main.rs:84-107` 已经按主题分支（清单 #9 的字面要求已满足）, 但**值是手写的**,
且与 `docs/SPEC-dark-theme.md:168-195` 的定稿**不符**:

| | spec 定稿 | 实际落地 |
|---|---|---|
| Light `accent` | `15,118,110`（框架玉色） | `0.18,0.35,0.60`（**蓝** —— 另一套色） |
| Light `base` | `240,248,246`（= 主题背景） | `0.96`（≈245, 手写近似值） |
| Dark `accent` | `15,118,110` | `from_srgb8(26,158,138)`（≠ 定稿） |
| Dark `surface` | 白 `0.08` | 白 `0.06` |

**修法**: `base` / `accent` / `text_primary` / `text_secondary` / `surface` /
`surface_input` 六项改为**取自 `LogTheme`**（`config.rs:145+` 已有逐 token 的
`Light/Dark` 分派, 与框架 `LightTheme`/`DarkTheme` 同源）, 消灭手抄。
`backdrop_light` / `backdrop_dark` 框架无对应 token, **保留手写**并注明理由。

**注意**: 这会让**浅色标题栏的 accent 由蓝变玉色** —— 本模块唯一改浅色观感的地方,
与 T5 那次同类, **须用户复看**。见 Open Q2。

---

## 1. 架构决策

- **AD1 — `clear_color` 单点定义**。`window_clear_color(t) -> Color` 一处, 启动与
  `Msg::SelectTheme` 都调它。**不许两处各算一份** —— 本仓已有先例: 设置卡页签序号
  曾在两个文件各抄一份、双双漂了（见 CLAUDE.md 2026-09-13 条）。
- **AD2 — 框架只加不破**。`TextInput::set_theme` 是新增方法, 不改 `themed()` 的现有
  签名与语义, 既有调用点零波及（`danqing-pomodoro` 等不受影响）。
- **AD3 — 「接完」不等于「调好看」**。本模块只把**写死的值换成 token**, 确定 token 该
  是什么值是模块 2 的事。R5 里「取 `LogTheme` 而非手抄」是**去重**不是选色,
  故仍在范围内; 但由此产生的观感变化要报备。
- **AD4 — 级别色板 `*_fg` 不动**。`view.rs:80-97` 五色是**固定语义色**, 用户已明令
  本轮不碰。**暗色下它们的可读性另记 Open Q3**, 不在本模块解。

---

## 2. 任务列表

> **状态勾选以 `tasks/todo-token-completion.md` 为准**, 本表只是索引 ——
> 两处各维护一份勾选就是本仓栽过的「两处各抄一份、双双漂掉」。

### Phase 1: 白板（纯产品侧, 零框架依赖）

- [ ] **T1: `clear_color` 跟随主题（R1）**
- [ ] **Checkpoint A: 真机截图过审** —— 暗色标题栏那条应消失; 用户看过后再往下走

### Phase 2: 框架开口 + 输入栏

- [ ] **T0: 前置 —— 开本地 patch 联动**（本阶段要动 `../danqing`）
- [ ] **T2: `TextInput::set_theme` + 过滤/搜索栏接入（R2）**

### Phase 3: 收尾三小项

- [ ] **T3: `header_line()` 随主题（R3）**
- [ ] **T4: 书签金按主题取色（R4, 待 Open Q1 定）**
- [ ] **T5: `title_theme()` 去手抄（R5, 待 Open Q2 定）**

### Phase 4: 收口

- [ ] **T6: 三件套 + 真机验收 + 落地链**
- [ ] **T7: 更新 `docs/SPEC-dark-theme.md` 的过时清单**（见下）

> **T7 不是文档洁癖**: 那份清单现在**谎报进度**（说 4 条残, 实际 3 条残 + 1 条半）。
> 本仓的复发性教训就是「加新决定、不回头清旧文字」—— 清单是不可信的, 下一个读它的
> 人会照着做一遍已完成的活。

---

## 3. 风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| `set_theme` 覆盖显式设置让人意外 | 中 | doc 写明语义 + RED 测试锁住「调用后四色 = token 值」 |
| 改 `title_theme` 动到浅色观感 | 中 | 单独批次 + 真机截图过审; 未过当场回退 |
| 漏改第三条 `clear_color` 来路 | 低 | `window_clear_color` 单点定义 (AD1) |
| 暗色下 `*_fg` 语义色可读性 | 中 | **本模块不解**, 记 Open Q3 交模块 2 |

---

## 4. Open Questions

1. **书签金怎么定** —— 建议**不套 `accent`**（会与「选中/强调」混成一个通道）,
   改为「亮色金 + 暗色下换一支更亮的金」, 两支都放**产品侧**（框架无书签 token,
   不需要为此扩框架 trait）。**待用户裁: 采纳 / 直接套 accent / 本轮不做。**
2. **R5 会改浅色标题栏 accent（蓝→玉色）** —— 采纳（顺手对齐框架, 消灭两套色）
   / 推迟到模块 2 一起裁 / 本轮不做。
3. **暗色下级别色板 `*_fg` 的可读性** —— 用户已明确本轮不动。**建议记入模块 2**:
   语义色大概率也该有明暗两套。**仅登记, 不在本模块解。**
