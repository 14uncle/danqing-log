# SPEC-checkbox-widget: 框架复选框组件 + `[x]` 文本勾选换挂

- @author 十四叔
- @date 2026/09/28
- 状态: **五段全闭**（2026-09-28 一日: 「go」→ plan → `/build auto` T1–T5 →
  双路评审并账全修（§10）→ code-simplify（§11））; 人工验收 **I 组**三条记账待实机
  （需付费态，与 G/H 同窗口）
- 模块 id: `checkbox-widget`（不属 `SPEC-v1x-map` 七模块；UI 打磨件，级别同 `SPEC-selection-copy`）
- 触发: 2026-09-28 用户实机看合并源卡发问「勾选状态使用中括号+x 表示吗？」→ 裁定方向: **框架新增复选框 widget**，产品两处文本勾选换挂。
- Phase 0 范围检查: 单一能力（复选框观感），不起能力地图。

## 0. 背景（为什么做 / 为什么现在）

- `[x]`/`[ ]` ASCII 勾选是**字体约束的权宜**：内嵌 Sarasa 子集**没有 ✓/✕ 字形**
  （`settings.rs:480` 记「GB2312 字体约束」；2026-09-14 「清除筛选」前缀 `✕` 渲染成 0×0
  空字形的事故同根）。矢量绘制**绕开字体**——框架 `CloseButton` 纯矢量（`push_diagonal`
  对角线算法）已证明这条路。
- 现状两处**同义手势**都是文本勾选（全仓 `[x]`/`[ ]` 零遗漏盘点）：
  ① 合并源卡源行（`settings.rs:893`，显隐切换 + 按源色块）；
  ② 「显示列」弹层列行（`settings.rs:522`，列显隐切换）。
  两处都挂在产品侧自绘件 `RowList`（`src/pick_list.rs`）上——建树冻结铁律下动态行列表
  的正解，行 = （文案， 载荷），勾选态**烤在文案串里**。
- 框架 form 族现状：Switch / Dropdown / TextInput / TextArea / IconInput——**无复选框**。
  Switch 覆盖「即时开关设置」语义；列表成员显隐语义（这两个弹层）没有对应控件。

## 1. Objective

- 框架（danqing）拥有复选框**视觉与交互的单真源**；产品两处 `[x]` 文本勾选换成矢量盒。
- 成功的样子：两个弹层在明暗两主题下勾选态一眼可辨、不再读成「日志正文里的 ASCII」；
  RowList 第三消费者（字段查询弹层，无勾选语义）**零变化**；框架 `Checkbox` 行为由单测锁定
  并登入 showcase（框架家法：以用代测）。

## 2. 范围（两腿）

### 腿 A: danqing 新增 `src/widget/form/checkbox.rs`（框架，先行 push）

- `pub struct Checkbox`：**与 Switch 同构**——`bind<S>(bool)` / `on_toggle<M>(Msg)` /
  `bind_theme`（每帧刷新随主题流动的颜色，`SwitchColors` 同法）/ 焦点环（`push_rounded_border`）/
  Space·Enter 持焦切换 / pressed 0.85 缩放 / `focusable() == true`。
  （hover 只消费事件、无独立视觉 —— hover 底色由所在行/容器负责; 评审勘误:
  原稿写「hover·pressed 态」措辞多写了一点点。）
- **公开静态画法** `Checkbox::paint_box(rects, rect, checked, colors)`——RowList 逐行复用
  的**同一算法**（widget 本体 paint 也调它，杜绝两份画法漂）。
- 配色收口 `CheckboxColors::from_theme`：勾中 = `accent()` 实心 + 白勾；未选 = `border()`
  边框空盒；勾恒白（Switch knob「恒白与主题无关」先例）。
- 几何常量：`BOX_SIZE = 14`（逻辑像素）、边框 1.5、圆角 3；勾 = 两笔对角线，
  复用 `widget::push_diagonal`（CloseButton × 符同算法）。
- **showcase 登记**（`examples/showcase.rs`，框架家法：新增组件必须出现）。
- 导出链：`form/mod.rs` `pub use` → `widget/mod.rs` `pub use form::{… Checkbox}`。
  （`lib.rs` 的 `pub mod widget` 已公开，无需动。）

### 腿 B: danqing-log RowList 加法 + 两处换挂（产品，复钉后落地）

- `RowList::with_checkbox(checked_fn)`：第五闭包
  `Box<dyn Fn(&LogApp, &str) -> Option<bool>>`（`with_swatch` 第四闭包先例，**加法不改契约**）。
  返回 `None` = 该行不画盒；**不装 = 零变化**（字段查询行）。
- paint 行内次序：**色块（可缺）→ 复选框（可缺）→ 文案**（现状次序不动，`[x]` 原位换盒）。
  x 偏移同源收口：`SWATCH_W` 先例，新增 `CHECK_W`（盒 14 + 间隔 6 = 20）；盒垂直居中
  `(ROW_H - 14) / 2`。
- 两处 `rows_fn` 删 `{mark} ` 前缀，状态移入 `checked_fn`：
  ① 显示列弹层：`Some(!app.columns.is_hidden(payload))`；
  ② 合并源卡：按载荷（路径）查 `merge.sources[].hidden`（`with_swatch` 同款查法）。
- **事件路径零改动**：整行仍是点击目标（含盒区域），复选框只是视觉 affordance；
  「按下抬起同载荷才触发」等既有锁不破。

### 不做（明言）

- 半选（indeterminate）/ 禁用态 / **动画** / 内建 label（Switch 同——文案由外侧拼）。
- 字段查询弹层行**不加盒**（无勾选语义）。
- 设置页 Switch **不换** Checkbox（语义不同：Switch = 即时开关；Checkbox = 列表成员显隐）。
- RowList 事件模型不动（不设「只有点盒才触发」——整行点击是既有手势，收窄热区无理由）。
- 字体子集不扩（矢量绕开，不为 ✓ 加字形）。

## 3. 决策

- **D1 两处都换**（2026-09-28 用户裁定）：合并源卡 + 显示列弹层同批换挂，不留「一半矢量
  一半 ASCII」拼接；字段查询行不动。
- **D2 完整 widget + 静态画法**（用户裁定）：框架落真 `Checkbox`（事件/focus/键盘齐备），
  画法核心公开静态化供 RowList 逐行调。**widget 本体本次无产品树内消费者**——RowList 消费的
  是 `paint_box`；这是有意裁决（组件族完整性 + showcase 以用代测），**不是死代码**，
  模块注释写明，免得评审当死代码砍。
- **D3 视觉**（用户裁定）：勾中 = accent 实心填充 + 白色矢量勾；未选 = `border()` 1.5px
  边框空盒；盒 14px。**补充 D3b（高亮行反白）**：RowList 高亮行（合并源卡的「移除」选中行）
  铺 accent 底，accent 填充在其上不可见 → 高亮行上的盒反白：勾中 = 白填充 + accent 勾，
  未选 = 白边框空盒——与该行文字反白同规（主题无 on-accent token，白即既定呈现）。
  显示列弹层无高亮行，不受 D3b 影响。
- **D4 加法闭包**：`with_checkbox` 不装零变化（字段查询行），装了逐行取态——
  `with_swatch` 的判罪锁同族（行数据每帧重取，防启动快照类冻结）。
- **D5 点击目标 = 整行不变**：复选框无独立热区，事件路径一行不改。
- **D6 无动画**（与 Switch 150ms tween **有意不同**）：RowList 逐行静态画法无实例状态，
  tween 不可得；standalone widget 亦不做（族内一致性让位于画法单真源——若 widget 有动画而
  行内没有，同一勾选两副面孔更糟）。明示偏离，写进模块注释。
- **D7 颜色走 token + 线性空间**：主题色经 `bind_theme` 每帧刷新；`RectBatch` 实例存线性值，
  测试断言经 `LinearRgba` 转换再比（Switch 测试 `rgba_of` 先例），不拿 sRGB 分量直比。
- **D8 联动链路**（家法顺序不能反）：`cp tools/local-patch.toml .cargo/config.toml` 开 patch →
  danqing 改 → 框架三件套 → **push danqing** → 本仓**关 patch** 后
  `cargo update -p danqing`（若报 `did not match any packages`：lock 是 path 态，
  先跑一次 `cargo check` 重解再 update——2026-09-14 实录）→ lock 复钉 →
  **两仓分别提交**，message 注明关联。
- **D9 行内次序不动**：色块→盒→文案（合并源卡现状即色块在前），`[x]` 原位换盒。

## 4. Commands / Structure / Style（增量，其余沿用两仓 CLAUDE.md）

```
框架: cd ../danqing && cargo test checkbox / cargo clippy --all-targets -- -D warnings / cargo fmt
      showcase 目击: cargo run --example danqing-showcase
      (评审勘误: example target 真名是 `danqing-showcase` (Cargo.toml [[example]]),
      写 `showcase` 直接报 no example target —— 管道 tail 会吞这个错, 核退出码)
产品: cargo test / cargo clippy --all-targets -- -D warnings / cargo fmt
```

- 新文件头 `//! @author 十四叔` + `//! @date 2026/09/28`；注释中文。
- 风格基准：框架侧照 `switch.rs`（绑定/theme/焦点）与 `close_button.rs`（纯矢量/`push_diagonal`）；
  产品侧照 `pick_list.rs` 既有闭包加法与守卫写法。
- 测试基线（开工前实测复核）：本仓 **495 绿**；框架记档值 603（09-20），以开工实测为准。

## 5. Testing Strategy

**框架（`checkbox.rs` 单测，Switch 测试同构）**：

- layout 固定 14×14；约束收窄时受约束（`layout_constrains_to_smaller` 同构）。
- 点击：原地按下抬起产 Msg / 按下拖出抬起不产 / 右键不冒充（P29 家法）。
- Space/Enter **持焦才产**，无焦不产。
- 焦点环：FocusIn 后多一笔、FocusOut 消失（CloseButton「锁画出来了不只锁标志位」同构）。
- `bind`/`bind_theme` sync 换值换色。
- **paint_box A/B 判罪锁**：`checked=true` 比 `false` 多出勾的实例（摘勾 = 精确红）；
  未选盒内**无 accent 填充**（防「未选也实心」）；颜色断言走线性空间（D7）。

**产品（`pick_list.rs` / `settings.rs`）**：

- `with_checkbox` **装了 = 每可点行恰一盒；不装 = 零盒**（swatch 锁同构的 A/B 面）。
- checked 态**每帧取**：sync 换数据盒态跟随（T0 判罪锁同族——摘重取 = 红）。
- 两处 `rows_fn` 产出 label **不含 `[x]`/`[ ]`**（文本前缀退役锁；载荷不变）。
- **行宽同源守卫**：盒 + 色块 + 文案 x 偏移同源，行内容宽不超卡内容宽 312
  （G-d「步进钮画出卡外」的同族守卫，`merge_time_edit_row_fits_card_width` 先例）。
- D3b 反白锁：高亮行上的盒用白系色（勾中白填充 / 未选白边框）。

**人工验收（记账 `tasks/acceptance-pending.md` ~~新 H 组~~ **I 组** —— 并行会话
notice-visibility 先占 H, 让位; 三条均需付费态 key（三连弹层与合并源卡均在付费层），
与 G 组同窗口实机）—— ✅ 2026-09-29 三条全过**：

- H1 → **I-a** 合并源卡：盒渲染（勾中/未选/高亮行反白）、点行切显隐——明暗两主题。
- H2 → **I-b** 显示列弹层：盒渲染、点行切显隐——明暗两主题。
- H3 → **I-c** 字段查询弹层：**无盒**回归（第三消费者零变化）。

## 6. Boundaries

- 沿用两仓 CLAUDE.md 全部家法。增量：
  - **Always**: 框架改动先 push 再复钉（D8 顺序）；patch 默认关，改框架第一件事 `cp`。
  - **Ask first**: 改动 `ROW_H`/卡内容宽等既有几何常量；给 `paint_box` 加第三态。
  - **Never**: 不为勾形扩字体子集；不动 RowList 事件模型；不把 Switch 换成 Checkbox。

## 7. Success Criteria

- 机器：框架 603+N 绿、本仓 495+N 绿；两仓 clippy 0 / fmt 过；
  showcase 出现 Checkbox；`grep '\[x\]' src/` 行产出零残留（注释同步更新，不留旧措辞）。
- 人工：**I 组**三条实机过（需付费态 key，与 G 组同窗口）—— ✅ 2026-09-29 全过。
- 联动：danqing push 后本仓 `Cargo.lock` 复钉新 rev 并提交。

## 8. Open Questions

1. standalone `Checkbox` 的焦点/键盘码本次**无实机消费路径**（弹层行不持焦、产品树内无
   复选框点位），行为只靠框架单测 + showcase 锁定——**已接受**（2026-09-28「go」，
   Switch 同码族，真有树内消费者时实机再校）。
2. 勾的笔画粗细取 `push_diagonal` 默认比例（CloseButton 同款 0.085 系数）——**已接受**；
   实机观感若嫌细/嫌粗，调系数即可，不回 spec。

## 9. 实现记（2026-09-28，T5 收口）

**链路**: spec「go」（三裁定全按推荐）→ plan/todo → `/build auto` T1–T5。
全程零 commit，唯一例外 = T2 联动（danqing push + 本仓 lock 复钉提交，
spec D8 + plan 批准已授权）。

**产出**:
- **danqing `c5b1fcf`（已 push）**: `src/widget/form/checkbox.rs` 新件 ——
  `Checkbox` 全件（bind/on_toggle/bind_theme/焦点环外扩 2px/Space·Enter/按下 0.85
  缩放）+ **`paint_box` 静态画法单真源**（widget paint 同调，锁
  `widget_paint_delegates_to_paint_box` 全等断言）+ `CheckboxColors::from_theme` /
  `on_accent` 两套；`form/mod.rs`→`widget/mod.rs` 两级导出；showcase 表单页
  「复选框」区（可交互 + 静态勾中/未选各一）。框架测试 **630→643**（+13）。
- **本仓**: lock 复钉 `danqing#c5b1fcfe`（commit `b091040`）；`RowList::with_checkbox`
  第五闭包（加法不改契约，四消费者零改动）+ 几何常量化同源（新 `ROW_PAD_X`，
  `SWATCH_W`/`CHECK_W` 升 pub(crate) 供守卫同读）；`col_menu_rows` /
  `merge_source_rows` 两处换挂（`[x]` 前缀退役，`checked_fn` 接管）；
  `merge_source_label` 抽纯函数（`merge_union_hint` 先例）。产品 **507→516**（+8；
  另含并行会话 notice-visibility +1 同窗口）。

**A/B 精确红在案（两处，均复原转绿）**:
① 摘 `paint_box` 勾两笔 → `paint_box_checked_adds_fill_and_check_strokes` 红
（「勾中应有两笔勾的圆点队列实例」）; ② 摘 RowList sync 勾态每帧重取 →
`checkbox_state_follows_sync` 红在 2≠1。

**实现与 spec 的分叉（如实记）**:
1. **焦点环画在布局 area 外扩 2px** —— spec 只写「焦点环」; 14px 盒画内环会遮住盒,
   取外扩（瞬态视觉，布局/命中不变），代码注释写明。
2. **行宽守卫改「量内容」** —— G-d 模式是 layout 自然宽，但 `RowList::layout` 回
   `constraints.max()` 占满，量不出内容宽；改为 measure 最坏 label + 同源常量求和
   （plan 事实 7 已预判）。
3. **showcase 静态勾中经 `bind(|_| true)`** —— Checkbox 不加 const checked setter
   （无消费者）。
4. **测试基线复核更正**: 框架记档 603（09-20）→ 实测 **630**；产品记档 495（09-28
   上午 CLAUDE.md）→ 实测 **507**（差 12 的构成未逐条对账，记档漂移，以实测为准）。
5. **验收组别让位**: 并行会话 notice-visibility 先占 H 组 → 本模块记账 **I 组**
   （spec 正文 H1–H3 已改 I-a–I-c）。

**并发备注**: notice-visibility 会话同窗口 build（`src/toast.rs` / `main.rs` /
`view.rs` 是其改动，本批零触碰；`settings.rs`/`pick_list.rs` 为本批独有）。
教训兑现一条：clippy 管道假退出码踩中一次（`cmd | tail` 的退出码是 tail 的 ——
09-13 记过的同一把耙又踩了），已改 `> file 2>&1; echo $?` 取真码。

**待办**: I 组三条实机（需付费态 key，与 G/H 同窗口）；review + code-simplify 另起。

## 10. 评审记（2026-09-28，双路独立评审 → 并账全修）

**两路均 APPROVE**（零 Critical / 零 Required）：五轴全量路 + 红队深潜路
（六区对抗取证：单真源 / sync 对齐 / D3b 反白 / 守卫真实性 / 事件零改动 /
探针循环论证）。**主声称全部扛住对抗取证**: D2 单真源（paint_box 调用点全框架+全产品
仅两处 + 全等锁）、D5 事件路径零改动（diff 逐 hunk 核）、checked/rows 索引对齐
（构造保证）、on_accent 对比度实测（浅色 5.47 / 暗色 3.33，过 WCAG 非文字 3:1）。
红队总评: 残余风险全是**覆盖缺口**而非被证伪的声称。

**并账修复（Optional 全清 + Nit 全清）**:
1. **widget paint 方形假设溢出**（A-O1）: `layout` 接受受约束更小尺寸，paint 单按
   width 推导 → 10×8 面积画 10×10 溢出 2px。修 = paint 取 `min(w,h)` 双轴居中 +
   锁 `paint_stays_within_constrained_area`（墨迹全在布局面积内）。（Switch 同病，
   族内遗传，不在本批动 Switch。）
2. **showcase 静态盒可点不翻**（A-O2）: 读成「坏了」+ 教坏下游。修 = 对照对改
   **互镜像可点**（`checkbox_pair` 共享 bool, 一枚正绑一枚反绑, 点了同翻）。
3. **paint_box 正方形前提未声明**（B-O1）: 非正方形 rect 勾画出盒外（红队探针实测
   20×10 墨迹 y=15.25 > 盒底 10）。修 = doc 声明前提 + `debug_assert!`（正方形检查）。
4. **暗色主题零锁覆盖**（B-O2 + FYI-1 并修）: 框架新增 `from_theme_maps_tokens_for_
   both_themes`（fill↦accent / border↦border() 明暗双主题按值锁定 —— 09-13 token
   重校先例证明这支会被再校准）; 产品新增 `checkbox_colors_follow_app_theme_dark`
   （暗色 AppTheme 下探针与画法同换暗色套, 且两主题套互不等 —— 防探针自欺）。
5. **勾形无几何锚点**（B-O3）: 原锁全绿时勾画成 ✕ 也发现不了。修 = 
   `check_strokes_form_a_check_not_a_cross`（bbox ⊂ 盒内 + 横向跨度 + **谷底偏左/
   右端上翘**两条极值签名）。修程记一条: 首版「底部带内全在左侧」断言被真勾形
   打脸（上行笔从谷底爬升, 底部带合法越中线）→ 改单点极值签名。
6. **守卫名 overclaim + 数字漂移**（B-N1/N2）: 两守卫改名 `*_width_within_card_budget`
   （fixture 是代表性非最坏值, 注释写明「锁常量同源与排版漂移, 不锁任意长文件名」）;
   「净宽 −6px」更正为**实测 −8px**（plan 事实 5 同改）。
7. **文档错命令**（A-N4）: spec §4 / plan T2 的 `--example showcase` → 真名
   `danqing-showcase`; **T2 验收①当时是假绿**（`| tail -1` 把报错尾部当成功输出）,
   已用真命令补验 exit 0。**今日第三次踩管道假退出码** —— 考你眼力的时候到了:
   凡是验证构建/测试, 一律 `> file 2>&1; echo $?`。
8. **RowList 长文案无裁剪溢出**（B-O4）: 既有边界非本批引入 → 挂账
   `ROADMAP-v1x.md` §四 待裁（红队实测 417px > 312, 合并源卡实机必撞面）。

**留档不修**: `hovered` 字段只写不读（Switch 同病, 族内一致优先 —— 删它会与
被照抄的 Switch 模板分叉, 不值）; spec 腿 A「hover·pressed 态」措辞已勘误;
D6 字面不覆盖 pressed 缩放（动画≠交互反馈, A-FYI-9 精神一致, 记此一句）。

**修复后验证（真退出码）**: 框架 **705 全绿**（lib 646 = 643+3 评审锁 + 集成 59）/
产品 **517 全绿**（+1 暗色锁）/ 两仓 clippy 0 / fmt 过 / `danqing-showcase` 构建 exit 0。
框架修复经同一联动链落地（danqing 二笔 push + 本仓 lock 二次复钉）。

## 11. code-simplify 收口（2026-09-28）

**行为零变化，测试零修改**（框架 704（--lib --tests）/ 产品 517 全绿，
两仓 clippy 0 / fmt 过）。

**简化三项**:
1. **框架 checkbox.rs**: 独立 `impl Checkbox { BOX_SIZE }` 小块并入主 impl
   （一个结构体两个 impl 块无理由）。
2. **pick_list.rs**: sync 里 swatches/checked 两个同形 `match` 块 → 抽 `per_row`
   泛型助手（同形第三份出现前收口；判罪注释随助手搬家；修程: clippy
   type_complexity → `PerRowFn<T>` 别名）。
3. **settings.rs**: `with_swatch`/`with_checkbox` 闭包的「按载荷查源」起手式
   （本次换挂引入的重复）→ 抽 `merge_source_idx`。

**不动清单（看过且判不动）**:
- `hovered` 只写字段 —— 评审留档（Switch 同构 parity，删它 = 与被照抄模板分叉）。
- paint 里色块/盒两个行首件块不合并 —— 形似质不同（单色矩形 vs 框架画法 +
  双套色），抽了要传 4 参，得不偿失。
- 测试内探针 helper 的重复 —— 测试零修改家法，测试内重复是允许的显式。
- Switch 的同款受约束溢出 —— 族内遗传（评审 A-O1 注明），另案不动。

**环境备注**: 框架例程重链被**用户正开着的 showcase.exe** 文件锁挡
（os error 5）—— 编译验证由 clippy --all-targets 覆盖（含 example），链接步用
**备用 target-dir**（`--target-dir $TEMP/…`）绕锁全量验证 exit 0，不打扰用户窗口。
**联动**: danqing `7813d1d` push → 本仓单点换钉 `5275a0f`（同 `3f00814` 注记）。

**五段全闭**: spec → plan → build → review → code-simplify；余 I 组三条实机验收
（记账 `tasks/acceptance-pending.md`，需付费态 key，与 G/H 组同窗口）。
