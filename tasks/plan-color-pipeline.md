# Plan: `danqing:color-pipeline` (消除双重 gamma 编码)

> spec: `docs/specs/SPEC-ui-redesign.md` §2 | todo: `tasks/todo-color-pipeline.md`
> 意图: `docs/intent/ui-redesign.md`
> 仓库: **改动全部落在 `../danqing`**（框架侧）；本仓只改文档与验收记录。
> 用户已裁定: 方案 **B**（保留 sRGB 目标 + 在 GPU 边界补 sRGB→linear）
> 状态: **已批**（2026/09/13）—— plan 整体批准；AD1 / AD2 按推荐采纳
> （CPU 侧转换 / `LinearRgba` newtype）。

---

## Overview

danqing 的 UI 颜色被**双重 gamma 编码**：渲染目标强制为 sRGB
（`render/mod.rs:160-165`），而 `rect`/`text` 管线把作者态 sRGB 值**原样输出**，
硬件再编码一次。后果是暗色主题的近黑背景显示成中灰、正文与背景差 9/255
（详见 spec §0 定案）。

本轮只做一件事：**把 sRGB→linear 转换补到 GPU 边界，并把 `Color` 的色彩空间契约
收敛到单点**。不做任何配色调整 —— 配色是模块 2 的事，混在一起就无法判断
观感变化是修复带来的还是调色带来的。

---

## Architecture Decisions

### AD1｜转换放在 CPU 侧实例构建点，不放着色器

候选：① CPU 侧写 instance 时转换；② `rect.wgsl`/`text.wgsl` 片元里转换。

选 ①。理由：**① 可单测**（本仓文化是「实测不估算」，着色器里的数学没法单测）；
② 是每像素一段 `pow`，成本随分辨率走，而 ① 随实例数走（可见行约数千）。
② 的收益（省 CPU）在这个量级上不存在。

### AD2｜用 newtype 而非自由函数，让类型系统守住契约

本轮 bug 的温床是**契约含糊**：`layout.rs:10` 说 `Color` 是线性、
`theme.rs:61` 说它是 sRGB 编码，两句都写得像权威，于是 GPU 通路与 WCAG 护栏
各信一句（spec §0）。**修完转换但契约仍靠自觉，同一个 bug 会再来一次。**

因此引入 `LinearRgba` newtype（仅渲染层内部可见），
`impl From<Color> for LinearRgba` 做唯一的 decode。`RectInstance.color` /
`GlyphInstance.color` / `DrawTarget.clear_color` 的类型随之改为 `LinearRgba`
—— **传错类型编译不过**，这比 doc comment 硬。

> 备选（更简）：一个自由函数 `fn to_linear(c: Color) -> [f32; 4]`。
> 成本低一半，但没有任何东西阻止下一个人再写一次直传。
> 若用户认为 newtype 过重，退回此方案，但必须在 `layout.rs:10` 把契约写成
> 「作者态 = sRGB 编码；唯一转换点在 `render::linear`」并交叉引用。

### AD3｜不动 `Color` 的存储语义

`Color` 内部值保持 sRGB 编码（现状即如此，与全部调用点一致）。
改存储语义会波及所有 `Color::rgb(...)` 调用点的含义，观感大面积漂移且无法审。

### AD4｜明确不动 image / background 管线

那两条走 `Rgba8UnormSrgb` 纹理采样，硬件自动 decode，**本来就正确**
（`image.wgsl:64` 直接返回采样结果；`background.wgsl:778` 注释所述的线性运算自洽）。
这正是选 B 而非 A 的理由：A（目标改非 sRGB）会把这两条对的管线一起弄坏。

---

## Task List

> 任务文件在 `tasks/todo-color-pipeline.md`。共 4 个任务 + 2 个检查点。

### Phase 0: 前置

- [ ] **T0: 打开本地 patch 联动**
      `cp tools/local-patch.toml .cargo/config.toml`（在本仓根执行）
      **不开则框架改动静默不生效** —— 改兄弟仓前的第一件事。
      用完删掉即回默认态（该文件已 gitignore，不进提交）。

### Phase 1: 地基

- [ ] **T1: 契约收敛 + 转换原语 `LinearRgba`**（S）
      新增 newtype 与 `From<Color>`；改正 `layout.rs:10` 与 `theme.rs:61`
      两句互相矛盾的 doc，使其互相引用、说法一致。

### Checkpoint: 地基

- [ ] 框架测试 + 本仓 98 测试全绿（此时行为**无变化**，只是原语就位）
- [ ] `layout.rs` / `theme.rs` 两处 doc 已交叉引用，说法一致

### Phase 2: 接入（工作状态在此阶段变可见）

- [ ] **T2: 实例构建点接入（rect + text 同批）**（S）
      `render/rect.rs:100-108`、`render/text.rs:162-174`。
      **两条必须同批**：分开改会让中间态**渲染不一致**（矩形 linear、文字不是），
      无法验收。

- [ ] **T3: 清屏色两条路径接入**（S）
      `render/rect.rs:723-729` 与 `render/background.rs:843-848` 的 `LoadOp::Clear`；
      `Context.clear_color` / `DrawTarget.clear_color` 类型随 AD2 调整。
      **不依赖 Open Q1** —— 无论清屏色从哪来，转换点都是这两处。

- [ ] **T4: 全框架审计 + 防回归**（XS）
      grep 确认全仓**只有一处** decode 实现、无残余的作者态颜色直写 GPU。

### Checkpoint: 管线修完（**真机验收**）

- [ ] 三件套绿：`../danqing` 与本仓各跑 `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`
- [ ] **真机取色器实测**：暗色下背景 = `#191920`（±2/255）
- [ ] 浅色主题对照：各通道偏 ≤ 9/255，无「看起来不一样了」的意外
- [ ] **用户过目** —— 这是「设计提案门」的前置：修完管线先看一眼真机，
      再谈设计（在错的颜色上设计＝照着 bug 调）

### Phase 3: 未开（待地图批准后另立 plan）

- [ ] 模块 2 `danqing:theme-recalibrate`
- [ ] 模块 3 `danqing:component-polish` ∥ 模块 4 `log:token-completion`
- [ ] 模块 5 `log:layout-rhythm`（**须先过设计提案门**）

---

## Risks and Mitigations

| 风险 | 影响 | 缓解 |
|------|------|------|
| 半透明叠加的混合从 sRGB 空间变为 **linear 空间** | **Med** —— 玻璃感/斑马纹/hover 观感会变 | 这是**正确**行为；alpha 在模块 2 重校。本轮真机对照留档，便于模块 2 定位差异来源 |
| 浅色主题出现意外变化 | Low | 数值预测偏 ≤ 9/255（近 1.0 的值不动）；真机对照验证 |
| 每实例每帧一次 `powf` 的性能 | Low | **先实测**（本仓文化：不估算）。可见行约数千实例，预计 <1ms；若实测有影响，按色值缓存 |
| 框架改动外溢到 danqing-pomodoro | Low | 用户已确认**暗色从未发布**；且本轮**不动组件 API**，只动内部色彩通路 |
| 忘了开 patch，改了半天不生效 | **Med** | T0 显式列为首步；三件套在**两个仓库**分别跑 |
| 顺手调色导致无法归因 | **Med** | spec 边界明令：本轮只修管线，配色归模块 2 |

---

## Open Questions

> 2026/09/13 用户「按你推荐」授权后，1/3/4 已定，2 留待裁决。

1. **~~清屏色归属~~ → 定：沿用 `WindowConfig.clear_color` + 切换时发 `SetClearColor`。**
   不引入 `background_frame()` —— 后者会拖进背景场景机制（那是息壤的地基），
   而这里只需要一个纯色底。启动时按已加载主题给初值，切主题时发事件。
   *影响模块 4，不阻塞本 plan。*
2. **蒙版纹理格式不一致**：`sky_mask`/`glow_mask` 用 `Rgba8UnormSrgb`，未配置时的
   1×1 黑占位用 `Rgba8Unorm`（`background.rs:1065`）。**建议记 v1.x** ——
   它不是本轮病灶，且蒙版阈值语义（sRGB decode 后才得阈值）一旦改动会动画面观感。
3. **~~设计提案形式~~ → 定：改代码出真机截图，但做成可整体回退的小批次。**
   理由：静态效果稿做不出自绘框架的真实字形渲染与像素对齐，而用户自己定的判据
   就是「看真机」；「方案未过不进 build」的真意是「别在看之前垒一大坨」，
   不是「不许有中间代码」。**执行约束**：每批给真机截图过审，过一批留一批，
   **未过的批次当场回退**。
   *注：这条是对用户自划边界的一次收窄解释，若用户不同意，退回严格版
   （先静态稿、过审才动代码）。*
4. **~~地图~~ → 已批**：模块 2–5 的切法与顺序按 spec §1
   （`color-pipeline` → `theme-recalibrate` → （`component-polish` ∥
   `token-completion`）→ `layout-rhythm`）。
