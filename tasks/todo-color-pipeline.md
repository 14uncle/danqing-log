# TODO: color-pipeline (消除双重 gamma 编码)

> spec: `docs/specs/SPEC-ui-redesign.md` §2 | plan: `tasks/plan-color-pipeline.md`
> 逐条勾选推进; 每任务完成后跑三件套 (fmt + clippy + test)。
> **改动全部落在 `../danqing`** —— 本仓只改文档与验收记录, 但三件套**两仓都要跑**。
> 构建序: 开 patch → 原语+契约 → 接入 (rect+text 同批) → 清屏色 → 审计 → 真机验收。
> 状态: **机器部分全绿 (2026-09-13); 真机目视通过 (暗色全列可读); 待用户判收口口径**。

## 真机验收记录 (2026-09-13, 用户实机两次反馈)

- **第一轮**: 暗色「只剩 level 列, 其它列整列消失」→ 定案为产品侧写死近黑被照出来,
  当场插入 T5 修复 (见文末追加节)。
- **第二轮**: 暗色**全列可读、背景近黑** ✓ 核心缺陷闭环。
  用户同时提出三项设计改动, 裁定**归模块 5**(已记入 `SPEC-ui-redesign.md` §3)。
- **仍未做**: 取色器实测 `#191920`(±2/255)。用户以目视通过 —— 收口口径待其裁定。
- **新发现(归模块 4, 非模块 5)**: 暗色下**标题栏+过滤栏那条仍是亮的**,
  根因 `TitleBar` 背景 `TRANSPARENT` 透出写死的浅色 `clear_color` + 过滤栏
  `TextInput::themed(&LightTheme)`。**它是「变显眼」不是「新坏」** —— 内容区诚实
  变暗后这条亮带才刺出来。**推论: 模块 4 剩下的活比原估更要紧**。

## 执行偏离记录 (2026-09-13)

1. **T1 与 T2 合并成一个原子单元**。plan 原打算「先落原语、行为零变化」, 但
   `LinearRgba` 是没有使用者就编译不过的类型标记 (clippy `dead_code` 三处报错)。
   要么挂 `#[allow(dead_code)]`, 要么提前塞进公开 API —— 两条都不好。
   **合并后原语与使用者同批落地**, T1 的「行为零变化」中间态因此不存在。
2. **波及面比 plan 写的大**: plan 说 T2 动 2 个文件, 实际动了 **8 个**。
   多出的 6 个是 widget 测试模块 (`dropdown` / `icon_input` / `switch` /
   `text_area` / `text_input` / `title_bar`)。它们用本地 helper `rgba_of(token)`
   与 `RectBatch::instance_colors()` 比对, 断言「画出来的就是主题 token 色」——
   而实例里现在存的是**线性空间**值, 这个断言必然失败 (实测 9 条红)。
   修法是让 helper 同样解码, **测试反而变强了**: 它现在断言的是
   「widget 用了正确 token」这条真实不变量在 GPU 边界解码后的形态。
   *这条波及是 AD1(CPU 侧转换)+ AD2(字段换类型) 的必然代价, 不是实现失误。*

## Phase 0: 前置

## Phase 0: 前置

- [x] **T0: 打开本地 patch 联动** ✅ 2026-09-13
  - 说明: 在本仓根执行 `cp tools/local-patch.toml .cargo/config.toml`。
    **不开则 `../danqing` 的本地改动静默不生效** (patch 默认关, 2026-09-13 定的模型)。
    用完 `rm .cargo/config.toml` 回默认态; 该文件已 gitignore, 不进提交。
  - Acceptance: 改一行框架代码后本仓 `cargo build` 能看见变化;
    `Cargo.lock` 被改写回 path 记录属**预期** (故只在联动期开, 收尾前关掉)
  - Verify: `cargo metadata` 看 danqing 是否指向 `../danqing`; **不要拿 metadata 当
    验证手段本身** (2026-09-13 教训: metadata 不改写 lock, test 会)
  - Files: `.cargo/config.toml` (本地新建, 不提交)
  - Scope: XS
  - 实测: 已写入; `cargo tree` 输出 `Adding danqing v0.1.0 (F:\github\farm01\danqing)`
    证明解析到本地路径; `Cargo.lock` 少两行 `source = "git+…"` (path 记录, **收尾须还原**)

## Phase 1: 地基

- [x] **T1: 契约收敛 + 转换原语 `LinearRgba`** ✅ 2026-09-13 (与 T2 合并落地)
  - 说明: 新增仅渲染层可见的 `LinearRgba` newtype (建议 `danqing/src/render/linear.rs`),
    `impl From<Color> for LinearRgba` 做**全仓唯一**的 sRGB→linear decode。
    同时把两句互相矛盾的 doc 改成事实并互相引用 ——
    `layout.rs:10` 现写「线性空间, 提交 GPU 前不做伽马转换」(**错的**),
    `theme.rs:61` 现写「输入视为 sRGB 编码」(对的), 二者必须说法一致。
  - Acceptance: `0.0→0.0`; `1.0→1.0`; `0.5→0.2140` (编码值近似, 容差按 f32);
    `0.04045` 分段边界两侧**连续** (低段 `c/12.92` 与高段 `powf(2.4)` 衔接);
    alpha **原样透传**不参与转换;
    两处 doc 已互相引用且与实现一致
  - Verify: `cargo test -p danqing` (框架内单测)
  - Files: `danqing/src/render/linear.rs` (新增), `danqing/src/render/mod.rs`,
    `danqing/src/layout.rs` (doc), `danqing/src/theme.rs` (doc)
  - Scope: S
  - 备注: ~~**此任务完成后行为零变化**~~ —— **未成立**, 见「执行偏离记录」1。
    原语与使用者同批落地, 无中间态
  - 实测: 转换数学落在 `layout.rs::srgb_to_linear` (**全仓唯一实现**, 非自由函数而是
    公开标量 —— 与 `theme.rs::relative_luminance` 共用同一份, 后者原来自带一份
    私有 `decode`, 已改调共享实现)。RED 先行: 4 条断言先红 (`todo!()`),
    实现后绿。已知值全对: `0.5→0.21404`、分段点两侧连续、
    `25/255→0.00972`、`32/255→0.01445`。
    `LinearRgba` 带 `#[repr(C)] + Pod + Zeroable` (16 字节, 与 `[f32;4]` 逐字节一致)
  - Files (实际 6, 比预估多 `lib.rs`): `render/linear.rs` (新), `render/mod.rs`,
    `layout.rs`, `theme.rs`, `lib.rs`

## Checkpoint: 地基

- [x] `../danqing` 测试全绿; 本仓 98 测试全绿 ✅ 2026-09-13
      (~~此时渲染无任何变化~~ 不成立 —— T1/T2 已合并, 见「执行偏离记录」1)
- [x] `layout.rs:10` 与 `theme.rs:61` 两处 doc 已交叉引用、说法一致 ✅ 2026-09-13
      (`layout.rs:10` 原写「线性空间, 提交 GPU 前不做伽马转换」是**错的**, 已改)

## Phase 2: 接入

- [x] **T2: 实例构建点接入 (rect + text 同批)** ✅ 2026-09-13
  - 说明: `RectInstance.color` (`rect.rs:26`) 与 `GlyphInstance.color` (`text.rs:32`)
    类型改 `LinearRgba`; 在**写实例的单点**转换 ——
    `push_rounded_rect` (`rect.rs:100-108`, 值在 :103)、
    `push_text` (`text.rs:162-174`, 值在 :173)。
    **两条必须同批**: 分开改会让中间态渲染不一致 (矩形 linear、文字不是), 无法验收
  - Acceptance: 实例 buffer 里的颜色已是 linear 值
    (单测: `from_srgb8(25,25,32)` 经路径后 ≈ `(0.0097, 0.0097, 0.0144)`);
    **类型系统守住** —— 传 `Color` 直给实例字段**编译不过**;
    四条管线的 blend 配置不变 (仍 `ALPHA_BLENDING`)
  - Verify: `cargo test` 两仓 + 真机目视 (暗色背景应明显变深、正文对比恢复)
  - Files: `danqing/src/render/rect.rs`, `danqing/src/render/text.rs`
    (+ 6 个 widget 测试模块的 `rgba_of` helper, 见「执行偏离记录」2)
  - Scope: S → 实际 M
  - 实测: `RectInstance.color` / `GlyphInstance.color` 类型改 `LinearRgba`,
    两个构造点各一行 `LinearRgba::from(color)`; 顶点属性仍 `Float32x4`。
    **布局不变由实测断言守住** (不是推理): `instance_layout_is_unchanged_by_linear_color`
    断言 `size_of::<RectInstance>() == 68`、glyph 侧 `64`, 均绿。
    **先跑后改**: 编译过后实测 **9 条红**, 全在 widget 测试的 accent/border 断言;
    改完 6 个 helper 后 **575 绿 / 0 失败**, 本仓 **98 绿**。
    **新增守卫 12 条**: 标量解码 4 (`layout`) + `LinearRgba` 4 + 实例解码 2
    (rect/text 各一) + 布局实测 2。基线 passed **563** → **575** (1 ignored 不动)

- [x] **T3: 清屏色两条路径接入** ✅ 2026-09-13
  - 说明: 两处 `LoadOp::Clear` 接入转换 —— rect 的 `draw_span` (`rect.rs:723-729`)、
    background pass (`background.rs:843-848`)。
    **不依赖 Open Q1**: 无论清屏色从哪来 (`WindowConfig` / `set_clear_color` /
    每帧 `BackgroundFrame.clear_color`), 转换点都是这两处
  - **偏离 AD2 措辞 (第 3 处偏离)**: plan 原写「`DrawTarget.clear_color` 类型改
    `LinearRgba`」, **实际保持 `Color`**。原因: `BackgroundFrame.clear_color` 是
    **应用侧 API** (app 传 `Color`), 且 `background.rs:797` 的回退要拿
    `DrawTarget.clear_color` 去构造 `BackgroundFrame` —— 若 DrawTarget 存线性值,
    这条回退就得做 linear→sRGB 反向编码, 平白多一次往返。
    **不变量不变**: 两个结构体存作者态, 解码落在两处 Clear (真正的 GPU 边界)
  - Acceptance: 三条来路 (窗口初始化 `window/mod.rs:119/156`、运行时
    `window/event.rs:89-92` → `handler.rs:1310-1316`、每帧 `app.rs:93-99`) 的清屏色
    均被转换; 无 frame 时的回退路径 (`background.rs:795-797`) 一并覆盖
  - Verify: `cargo test` 两仓 + 真机 (切换主题时窗口底色即时跟随且**颜色正确**)
  - Files: `danqing/src/render/rect.rs`, `danqing/src/render/background.rs`
    (`render/mod.rs` **未改** —— 见「偏离 AD2 措辞」)
  - Scope: S
  - 实测: `LinearRgba::to_clear_value()` 承担转换 (RED 先行: 2 条断言先红)。
    已知值对齐: `#191920` 的清屏值 `r=0.0097212`、`b=0.0144524`;
    近白 `0.98→0.9551` (浅色主题几乎不受影响的**已知代价**在测试里写明)。
    一处自查: 我最初把容差写成 `1e-6`, 实测差 `1.2e-6` 而红 —— **是我断言写紧,
    不是实现错**, 已改 `1e-5` 并注明与 `layout` 侧同口径

- [x] **T4: 全框架审计 + 防回归** ✅ 2026-09-13
  - 说明: grep 全仓确认 decode **只有一处实现** (T1 的 `From<Color>`);
    确认无残余的「作者态颜色直写 GPU」路径。
    AD4 明确不动的两条 (image/background 采样路径) 复核**确实未被误改**
  - Acceptance: 全仓 grep `powf(2.4)` / `12.92` 只命中 `render/linear.rs`
    与 `theme.rs` 的 `relative_luminance` (后者是 WCAG 用, 非 GPU 通路, 合法);
    `image.wgsl` / `background.wgsl` **零改动** (`git diff` 佐证)
  - Verify: `git -C ../danqing diff --stat` + grep 结果落档
  - Files: 无 (只审计)
  - Scope: XS
  - 实测 (**结果好于及格线**): `grep -rn "powf(2.4)\|/ 12.92" src/` 全仓
    **只命中 `layout.rs:108-110` 一处** —— plan 原本允许 `theme.rs::relative_luminance`
    作为第二处, 实际它已改为委托, **第二处不存在了**。
    搜残余裸写 (`color: [color.` / `color: [c.r` / `LoadOp::Clear(wgpu`) → **零命中**。
    `git diff --stat -- 'src/render/*.wgsl'` → **空**: 那两条本来正确的管线 (AD4)
    确实未被触碰

## Checkpoint: 管线修完 —— 真机验收

- [x] 三件套绿 (**两仓各跑**) ✅ 2026-09-13
      框架: `fmt` / `clippy --all-targets -D warnings` 零警告 / **577 passed, 0 failed**
      (基线 563 → 净增 **14** 条); 本仓: 同三件套零警告 / **98 绿** (51+39+8)
- [x] **真机验收通过** ✅ 2026-09-13 —— **目视口径**（用户裁定「验收通过」）
      暗色：背景近黑、**全列可读**（对比修复前「只剩 level 列」）；浅色：结构完好。
      **口径变更记录**: plan 原定判据是**取色器实测 `#191920`(±2/255)**，
      用户以**目视**判过。两者都保留了 —— 判据从「屏幕上量的像素」放宽为
      「用户实机目视」。**这不是敷衍**：目视同样抓到了真缺陷（第一轮那次
      「只剩 level 列」就是肉眼发现的，取色器反而抓不到），但仍然记下口径不同。
- [x] 浅色主题对照：结构无意外变化 ✅ 2026-09-13（正文色偏冷的副作用见 T5 记录）
- [ ] **用户过目** (设计提案门前置): 修完管线先看一眼真机, 再谈设计
- [x] **收尾 —— 落地链走完** ✅ 2026-09-13
  - `danqing` 提交 `e30c00a` + push `dev` (`3d5e5e1..e30c00a`) —— 用户授权「我 commit + push 全做」
  - `rm .cargo/config.toml` 关 patch
  - **lock 钉到 `danqing#e30c00af9341d644324e463af79b03af09c79d61`**
    (`danqing-logfile#baee0a8b…` 未动)
  - **关键证据**: 关掉 patch 后编译输出
    `Compiling danqing v0.1.0 (https://github.com/14uncle/danqing#e30c00af)`
    —— **从 GitHub 拉的**, 修复确实进了远端历史且**不再依赖本地 patch**
  - 对钉住的 rev 重跑三件套: fmt/clippy 零警告 + 本仓 **99 绿**
  - ~~「还原 Cargo.lock」~~ —— **原措辞已失效**: 当时假设框架改动不进提交,
    现在它进了, lock 的正确终态就是**钉新 rev**, 不是还原旧的
  - **未做**: danqing-log 侧的提交与推送(待用户指示)

## 追加: 模块 4 的第一刀 (用户实机验收时插入, 2026-09-13)

> 不在原 plan 里 —— 用户看过暗色截图后**当场批准**插入的最小补救。

- [x] **T5: 四处写死的正文色换 token** ✅ 2026-09-13
  - 触发: 用户实机发现**暗色只剩 level 列, 其它列整列消失**。
    根因查实 = `view.rs` 的 `level_color` / `level_cell_color` / `status_color` /
    `cell_color` **四处兜底写死 `Color::rgb(0.12,0.12,0.12)`**(浅色主题的近黑)。
    修好双重 gamma 前它被抬到 ≈97/255 与发灰背景(≈88)勉强可分; 修好后背景诚实了
    (≈25), 这个近黑就**真的看不见**。**管线没弄坏它, 是把它照出来了。**
  - Acceptance: 四处兜底改 `th.text_primary()`(泛型 `T: Theme`);
    **级别色板 (`*_fg`) 不碰**(那是固定语义色, 用户明令本轮不动);
    回归锁 `body_color_follows_theme_instead_of_hardcoded_near_black`:
    浅/暗两主题各断言正文色 = `text_primary()`, 并断言暗色正文色亮度 > 0.5
  - Verify: 两仓三件套; 本仓 **99 绿** (51+40+8; +1 即该回归锁)
  - Files: `src/view.rs` (4 个函数签名改泛型 + 2 个调用点 + 测试)
  - Scope: S
  - **已知副作用 (须告知)**: 浅色主题的正文色由中性 `0.12` 变为 token 的
    `#0F172A`(偏冷石板色)。**这是本轮唯一会动到浅色的改动**, 视觉上很轻微,
    但用户说过「浅色还行」—— 得让他复看确认
  - 过程中的两次自纠: ① 我按 `&dyn Theme` 写, 编译报 **E0038 —— `Theme` 不是
    dyn 兼容的**(`LogTheme` 是实现 `Theme` 的 enum), 改泛型; ② 批量正则改写时
    `\1` 吃掉了字节串的 `b` 前缀 (`level_color("...")` 应为 `b"..."`), 当场 grep 发现并修

## Phase 3: 未开

- [ ] 模块 2 `danqing:theme-recalibrate` (含 alpha 重校)
- [ ] 模块 3 `danqing:component-polish` ∥ 模块 4 `log:token-completion`
- [ ] 模块 5 `log:layout-rhythm` (**须先过设计提案门**)

> Phase 3 待 spec §1 模块地图获批后另立 `plan-<module>.md` / `todo-<module>.md`。
