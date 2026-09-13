# Spec: UI 视觉重构 (ui-redesign)

> @author 十四叔 · @date 2026/09/13
> 状态: **模块地图已批**（2026/09/13）；模块 1 执行计划见
> `tasks/plan-color-pipeline.md`。模块 2–5 待模块 1 验收后另立 plan。
> 前置意图: `docs/intent/ui-redesign.md`（interview-me 收敛，用户显式 yes）
> 用户已裁定: **方案 B**（保留 sRGB 目标 + 补 sRGB→linear 转换，2026-09-13）
> 跨仓: 模块 1–3 落在 `../danqing`，模块 4–5 落在本仓。**分仓分别提交**。

---

## 0. 定案：为什么现在「不好看」（两病根，已查实）

### 甲｜双重 gamma 编码（框架层）

渲染目标被强制选为 sRGB（`../danqing/src/render/mod.rs:160-165` `find(is_srgb)`），
而 `rect.wgsl:102` / `text.wgsl:68` 把作者态颜色**原样输出**，硬件再编码一次。

`Color::from_srgb8(r,g,b)` 就是 `r as f32 / 255.0`（`../danqing/src/layout.rs:42-44`），
**只做除法、不做 sRGB→linear**。于是每个颜色都被 `srgb_encode` 抬亮一档：

| 颜色 | 作者值 | 实际显示 | 后果 |
|---|---|---|---|
| 暗色背景 `#191920` | 0.098 | **(88, 88, 99)** | 近黑 → 中灰（用户照片里那片灰） |
| 暗色斑马纹 白10% | 叠后 0.188 | (120, 120, 120) | 灰上更浅的灰条 |
| 浅色背景 `#F0F8F6` | 0.941 | (249, 252, 251) | 偏 9/255，**看不出来** |
| 浅色正文 `#0F172A` | 0.059/0.090/0.165 | **(69, 85, 113)** | 近黑 → 灰蓝（「发虚」） |
| ERROR 红 `#EF4444` | 0.937/0.267/0.267 | (248, 141, 141) | 鲜红 → **粉** |

**这一条解释了用户的全部观感**：暗色被毁（近零值抬得最狠），浅色几乎无损
（近 1.0 的值不怎么动）——「浅色还行、暗色不对」同根同源。

**为什么护栏没拦住**：框架对 `Color` 持有**两套互相矛盾的契约** ——
`layout.rs:10` 写「线性空间，提交 GPU 前不做伽马转换」，`theme.rs:61` 写
「输入视为 sRGB 编码」。GPU 通路信了前者，WCAG 对比度护栏信了后者
（`relative_luminance` 是全仓唯一做解码的地方）。**两条路各信一句**，于是
`SPEC-dark-theme.md` 的对比度标准按 token 算全部达标（14:1 / AAA），
屏幕上实际是 6.4:1 的灰叠灰。**护栏量的不是渲染过的数。**

### 乙｜暗色主题根本没接完（产品层，见模块 4）

`docs/SPEC-dark-theme.md` 的 12 条清单里 **4 条还在**，另有 4 条它没列。
要害两条：

- `src/view.rs:101-167` —— `level_color` / `status_color` / `cell_color` 的
  **兜底正文色写死 `0.12,0.12,0.12`**（浅色主题的近黑），却用在暗色背景（0.098）上。
  **文字与背景本来就是同一档暗**；再经双重编码，双双落到 ≈0.38 中灰
  —— **差 9/255，文字和背景同一个灰**。这就是「灰泥」的字面成因。
- `src/view.rs:1232-1240` —— 过滤/搜索输入框 `TextInput::themed(&LightTheme)`
  **写死浅色主题** + 三个写死色。

---

## 1. 能力地图（**待批**）

| # | 模块 id | 仓库 | 职责 | 依赖 | 状态 |
|---|---------|------|------|------|------|
| 1 | `danqing:color-pipeline` | danqing | 消除双重编码 + 收敛 `Color` 契约到单点 | — | **已可开工** |
| 2 | `danqing:theme-recalibrate` | danqing | Light/Dark token 重校；对比度护栏改为量**渲染后**的值 | 1 | 待批 |
| 3 | `danqing:component-polish` | danqing | TitleBar / Tabs / Dropdown / Overlay / TextInput 默认观感对齐 token | 2 | 待批 |
| 4 | `log:token-completion` | danqing-log | 12 条残留 + 新发现 4 条，暗色全链路接完 | 2 | 待批 |
| 5 | `log:layout-rhythm` | danqing-log | 层次 / 对齐 / 间距节奏 / 分区方式（**密度不变**） | 4 | **待设计提案** |

**构建顺序**: 1 → 2 → （3 ∥ 4）→ 5

### 设计提案门（硬约束，来自意图）

`log:layout-rhythm` 之前必须有一道门：**先出视觉方案（真机对照截图）给用户过，
过了才进 build**。理由已经发生变化 —— 现在屏幕上这套颜色是**错的**，
在错的颜色上做配色设计＝照着 bug 调。所以顺序必须是：
**修管线 → 用户真机看一眼 → 再谈设计**。方案未过**不得进 build**。

---

## 2. 模块 1: `danqing:color-pipeline`

### Objective

消除 UI 颜色的双重 gamma 编码；把 `Color` 的色彩空间契约**收敛到一处**，
使 GPU 通路与 WCAG 护栏不再各信一句。

### 方案（用户已裁定 B）

**保留 sRGB 渲染目标**，在**颜色进入 GPU 的单点**做 sRGB→linear。
不改 `Color` 的存储语义（存储 = sRGB 编码，与既有全部调用点一致）。

### 转换点清单（全框架共 4 处，均为单点）

| # | 位置 | 说明 |
|---|------|------|
| 1 | `src/render/rect.rs:100-108` | `push_rounded_rect` 写 `RectInstance.color`（值在 :103） |
| 2 | `src/render/text.rs:162-174` | `push_text` 写 `GlyphInstance.color`（值在 :173） |
| 3 | `src/render/rect.rs:723-729` | `draw_span` 的 `LoadOp::Clear` |
| 4 | `src/render/background.rs:843-848` | background pass 的 `LoadOp::Clear` |

### 明确不动

- `image.wgsl:64` / `background.wgsl` 的纹理采样路径 —— 采样 `Rgba8UnormSrgb`
  时硬件自动 decode，**本来就正确**。这正是选 B 而非 A 的理由：
  A（改非 sRGB 目标）会把这两条对的管线一起弄坏。
- `R8Unorm` 字形图集（`render/text.rs:304`）—— 只存 alpha 覆盖率，不含颜色。
- `background.wgsl` 内建颜色常量与场景调色板 —— 该管线在 linear 空间自洽。

### 契约收敛（防止复发的唯一手段）

把 `src/layout.rs:10` 的 doc 从「线性空间，提交 GPU 前不做伽马转换」**改为事实**：
存储 = sRGB 编码；`sRGB→linear` 只发生在 GPU 边界（上面 4 个点）。
同步核对 `src/theme.rs:61` 的措辞，两处必须**互相引用、说法一致**。

> 这条不是文档洁癖：bug 的温床就是这两句互相矛盾的 doc —— 谁读哪句，谁就写出
> 一半错的代码。

### 成功标准

- [ ] 四个转换点全部落地，且**全框架 grep 不到第二处**转换（单点可审计）
- [ ] 单元测试：转换函数已知值 —— `0.0→0.0`、`1.0→1.0`、`0.5→0.2140`、
      `0.04045` 分段边界两侧连续
- [ ] 真机验收：暗色下背景**取色器实测** `#191920`（±2/255）
      —— 判据是屏幕上量出来的像素，不是算出来的对比度
- [ ] 浅色主题变化符合预测（各通道偏 ≤ 9/255），无「看起来不一样了」的意外
- [ ] 框架全部测试绿；本仓 98 测试绿不受影响
- [ ] `cargo fmt` + `cargo clippy --all-targets -- -D warnings` 零警告

### 已知代价（接受）

半透明叠加的混合从 sRGB 空间变为 **linear 空间** —— 玻璃感、斑马纹、hover 的
观感会变。这是**正确**行为，但 alpha 需要模块 2 重新校准。

### 边界

- **Always**: 转换只在 GPU 边界做一次；改完同步改两处 doc 契约
- **Ask first**: 动 `Color` 的存储语义（会波及全部调用点，本轮不做）
- **Never**: 为了「顺手好看」在这轮调主题配色 —— 配色是模块 2 的事，
  本轮只修管线，否则无法判断观感变化是修复带来的还是调色带来的

---

## 3. 模块 2–5（骨架，待地图批准后细化）

- **`danqing:theme-recalibrate`** — Light/Dark token 重校；把对比度护栏从
  「量 token」改成「量渲染后」。**含 alpha 重校**（模块 1 改了混合空间）。
- **`danqing:component-polish`** — TitleBar / Tabs / Dropdown / Overlay / TextInput
  的默认观感与 token 对齐。
- **`log:token-completion`** — 清 `view.rs` 的 `level_color`/`status_color`/`cell_color`
  兜底近黑、`*_fg` 写死色板、`header_line()` 恒用 `LightTheme.divider()`、
  `Bar::base_input()` 写死浅色主题、书签金、`title_theme()` 写死调色板（含浅色分支
  accent 是蓝色 `0.18,0.35,0.60`、与框架玉色 `#0F766E` 不是一套）、
  `histogram.rs` 初值、`main.rs:1441` clear_color。
- **`log:layout-rhythm`** — 层次 / 对齐 / 间距节奏 / 分区方式。**密度不变**
  （用户原话「不允许」）。**须先过设计提案门**。

  **2026/09/13 用户实机看暗色时亲口提的三项，归本模块**（用户裁定「放模块 5」）：
  1. **去掉斑马行** —— 现走 `surface_variant`（白 10%）。它现在偏重**不是设计选择，
     是线性混合的副作用**（模块 1 的已知代价）；去掉对齐 klogg/LogViewPlus
     （`view.rs:612` 用户自己写过「klogg 基准无斑马」）。
  2. **展开块给底色** —— 现在展开的子行与真实行**长得一模一样**（只差没有行号），
     用户分不清「这坨是第 1 行展开的」还是「又是几行日志」。给它一块底色，语义就分开。
  3. **整个内容区给底色** —— 让表格区读作「一块面板」而非整窗。

  **硬约束（动手前必须知道）**: 上面 ②③ 要**两个互不相同、且都不透明**的底色。
  而框架主题里**两个主题都不透明的只有 `background()` 一个** ——
  Light 的 `surface_variant`(#EEF6F2) 与 `background`(#F0F8F6) 差 2/255（等于没有），
  Dark 的 `surface`/`surface_variant` 全是半透明。**所以本模块必须给框架 `Theme` 加 token。**
  **加法必须带默认实现**（如 `fn canvas(&self) -> Color { self.background() }`），
  否则 danqing-pomodoro 等既有实现者会被编译打断。这是本轮**第一次真正动公开 API**。

---

## 4. Open Questions

> 2026/09/13 用户授权「按你推荐」后，1 / 3 已定。

1. ~~清屏色归属~~ → **定：沿用 `WindowConfig.clear_color` + 切主题时发 `SetClearColor`。**
   不引入 `background_frame()` —— 后者会拖进背景场景机制（息壤的地基），
   这里只需要一个纯色底。
2. **蒙版纹理格式不一致**（盘点发现，非本轮病灶）：`sky_mask`/`glow_mask` 用
   `Rgba8UnormSrgb`，未配置时的 1×1 黑占位却用 `Rgba8Unorm`（`background.rs:1065`）。
   **建议记 v1.x** —— 非本轮病灶，且蒙版阈值语义（sRGB decode 后才得阈值）
   一改就会动画面观感。
3. ~~设计提案形式~~ → **定：改代码出真机截图，但做成可整体回退的小批次。**
   静态效果稿做不出自绘框架的真实字形渲染与像素对齐，而判据本就是「看真机」；
   「方案未过不进 build」约束的是「别在看之前垒一大坨」，不是「不许有中间代码」。
   **执行约束**: 每批出真机截图过审，过一批留一批，**未过的当场回退**。

---

## 6. 变更记录

- **2026/09/13**: 用户批准模块地图（1→2→(3∥4)→5）；批准 `color-pipeline` plan
  及 AD1（CPU 侧转换）/ AD2（`LinearRgba` newtype）。Open Q1/Q3 定案（见上）。
  执行计划落 `tasks/plan-color-pipeline.md` + `tasks/todo-color-pipeline.md`。

---

## 5. 参考

- 意图与裁决: `docs/intent/ui-redesign.md`
- 上一轮（产出当前暗色配色，**其成功标准已被证伪**）: `docs/SPEC-dark-theme.md`
- 农场约定: `../CLAUDE.md`；框架规则: `../danqing/CLAUDE.md`
