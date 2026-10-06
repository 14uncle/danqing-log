# SPEC-notice-visibility: 操作提示可见性 (Warn 浮层 + Info 底栏强化)

> 作者: 十四叔 · 日期: 2026-09-28 · 状态: **待批**
> 流水线: spec → plan → build → review → code-simplify (逐段推进, spec 写完不立即编码)
> 范围裁决: 单能力**两腿** (Phase 0 判定不拆多模块 —— 验收时刻唯一:
> 「操作被拒时, 视线不离开操作区也能注意到反馈」; 两腿共享同一 notice 通道与同一
> 分派判据, 拆缝不自然)
> 意图: `docs/intent/notice-visibility.md` (2026-09-28 interview-me 四轮裁定 + 显式 yes)
> 所属: v1.x 地图外**体验模块** (全授权态受益, 非付费腿; 命名无前缀同
> `SPEC-selection-copy` / `SPEC-update-badge` 先例); 依赖: 无引擎/框架前置

## 0. 背景 (为什么做 / 为什么现在)

- **用户原话 (2026-09-28)**: 「用户交互体验提升, 状态栏操作提示文本, 不注意的话
  用户都看不到」—— 整个 danqing-log 的操作提示体验, 不是某一个场景。
- **主因 (interview 轮 2 裁定)**: **位置 + 样式**。样式这条路 M3 (2026-09-14) 已治过
  一轮 (Warn 红 / Info 正文色分通道, `view.rs:2519`) 仍「看不到」→ 剩下的主变量是
  **位置**: 眼睛在操作点, 提示在屏幕底栏, 余光扫不到。时机 (4 秒消退/单槽顶掉)
  裁定**不是病根**, 生命周期模型不动。
- **Why now**: 腿一带进一大批新提示面 (合并门控/偏移格式/源管理), G 组实机验收
  当场暴露; 收银台开门前补这块基础体验。

## 1. Objective

**用户故事**: 我在合并源卡里输错偏移格式、在没选中源时点步进钮、在过滤计算中点导出
—— 这些「你按的那下没生效」的时刻, 反馈**弹到我视野里**, 不是在屏幕最底边悄悄
换一行字; 「已复制 3 行」这类成功确认仍在底栏, 但一眼能辨出那是**给我的反馈**,
不是状态信息。

**成功的样子**: 实机故意触发 Warn, 视线不离开操作区也能注意到浮层; Info 在底栏
一眼可辨; 深浅两主题都成立。

## 2. 范围 (两腿)

### 腿 A: Warn toast 浮层 + 呈现分派

- **分派在 view 层按 kind 走** (D4): notice 状态模型零改动 (单槽 + `notice_until` +
  `tick_notice` 全留), `set_notice` **一个字不动** —— 绕开家族病史⑤
  (set_notice 内含 refresh_status 次序陷阱) 的整个面。**Warn 时底栏 notice 位
  不重复画** (方案二无留痕; 方案三「底栏留痕」已否)。
- **新组件 `src/toast.rs`** (D5): 非模态浮层 —— **无 scrim、不抢焦点、不进 Esc
  次序表、不进 `close_popovers`/`popover_open` 互斥清单**; 事件只拦落在自身矩形内
  的点击 (= 点掉, `Msg::DismissNotice` → 清 notice), 其余放行 (`Ignored`)。
- **挂 `Stack` 末位** (main.rs:3478 建树处): 后画 = 最上层, 满足「画在模态弹层
  **之上**」约束 —— 合并源卡里输错偏移的 Warn 正发生在弹层开着时, toast 被弹层
  盖住 = 最该看到的场景反而看不见。
- **几何**: 底部水平居中, 底缘贴状态栏顶 (intent 轮 4 裁定); 自然宽随文案 +
  上限截断 (Open Q1)。
- **样式** (D6): 不透明 `background()` 底 + 圆角 (card_shell 同款论据: `surface`
  系半透明 token 不能当浮层底) + `danger()` 左侧色条 (Warn 语义色) + 正文色文字;
  深浅两主题各验。
- **生命周期**: 沿用 `NOTICE_TTL` 4 秒消退 + 可点掉 (intent 锁定假设);
  多条连发 = 单槽顶掉 (现状语义, intent 明言不动)。

### 腿 B: Info 底栏样式强化

- 现状: Info 与常态状态信息同排同号 (`view.rs:2536`), 唯一差别是文字色
  (`text_primary` vs `text_secondary`) —— 「扫到了也读不出是给我的反馈」。
- **强化形态 (D7)**: notice 文本加**圆角色块衬底** (`surface_variant` 级 token 打底 +
  `text_primary` 文字, 横向 padding 收进色块) —— 与 Warn 浮层形成功位差,
  与常态信息形成样式差。**不用字体图标** (U+2715 空字形陷阱在前: Sarasa 子集外
  字符画 0×0, 09-14 实机撞过一次)。
- 位置不动 (status 右侧 16px 处), 常态信息排版零变化。

### 不做 (明言)

- 通知中心 / 历史列表 / Windows 系统通知; Info 上浮层; 动 4 秒消退模型;
  改各调用点文案与触发逻辑 (通道升级, 不是文案改革); toast 排队栈 (单槽语义不动);
  淡入淡出动画 (框架无动画基建, 不为它开); 常驻红错误通道 (正则无效类) 维持原样
  —— 它是独立第三通道, 不进本 spec。

## 3. 决策

**intent 裁定 (2026-09-28 interview-me, 逐轮记录在 intent 文档)**:

| # | 决策 | 内容 |
|---|---|---|
| D1 | 分级切法 | **Warn 走浮层** (没生效必须看到); **Info 留底栏**强化样式 (成功冗余确认, 全量上浮层会连环弹); 方案一/三已否 |
| D2 | 浮层落点 | **底部中央, 状态栏正上方** (语义连贯 + 不遮过滤栏/表格/滚动条高交互区) |
| D3 | 生命周期 | `NOTICE_TTL` 4 秒沿用 + 可点掉; 单槽顶掉不动; 不进 Esc/互斥清单 |

**spec 新增实现裁定 (随本 spec 一并呈批)**:

| # | 决策 | 内容 |
|---|---|---|
| D4 | 分派点 | view 层按 kind 分派呈现; notice 状态模型与 `set_notice` 零改动; Warn 时底栏不重复画 |
| D5 | 组件归属 | 产品侧新 `src/toast.rs` 自绘 Widget (框架 Overlay 是模态的, 不可用作 toast); 成熟后 pomodoro 需要再下沉框架 (产品先验证惯例) |
| D6 | toast 样式 | 不透明 `background()` 底 + 圆角 + `danger()` 左色条 + 正文色文字; 双主题各验 |
| D7 | Info 强化形态 | 圆角色块衬底 (token 化), **不用字体图标** (字形子集陷阱) |
| D8 | toast 宽度 | 自然宽随文案 + 上限 `min(窗口宽×0.6, 520px)`, 超出省略号截断 (Text 不换行, 列宽截断先例); notice 文案实测均短, 截断是兜底不是常态 |

## 4. Commands / Structure / Style (增量, 其余沿用两仓 CLAUDE.md)

- 命令不变: `cargo test` / `cargo clippy --all-targets -- -D warnings` / `cargo fmt`。
- 改动文件:
  - **新 `src/toast.rs`** (**新 .rs 文件头规则触发**: `//! @author 十四叔` + `//! @date`)
    —— Toast Widget (sync 读 notice/主题, layout 几何, paint, 命中→DismissNotice)。
  - `src/main.rs` —— Stack 末位挂 toast; `Msg::DismissNotice` (清 notice,
    `tick_notice` 同途收口); toast **不进** `close_popovers`/`popover_open`/Esc 表。
  - `src/view.rs` —— 分派: Warn 时底栏 notice 位不画; Info 色块衬底。
  - `src/lib.rs` 不动 (toast 是 bin 侧组件, 与 settings.rs 同层)。
- **零引擎/零框架改动预期**; 零新依赖; 隐私零变化 (纯本地 UI)。
- 若 build 中发现 toast 必须动框架 (如事件分发顺序不满足「弹层之上仍收点击」),
  走联动链 (danqing 先 push → 复钉), **并在实现记留痕**。

## 5. Testing Strategy

- **toast 组件** (合成几何, settings.rs 测试基建先例):
  - notice=None / Info 时 **不画** (零矩形零文本产出断言);
  - Warn 时画出底色矩形 + 左色条 + 文案 (RectBatch/TextBatch 断言, token 色比对);
  - 点击 toast 矩形内 → `Msg::DismissNotice`; 矩形外 → 事件放行 (Ignored);
  - 宽度上限: 超长文案 layout 宽 ≤ 上限且截断 (D8);
  - 双主题底色均不透明 (alpha==1 断言 —— card_shell 同款守卫)。
- **分派** (view 层): Warn → 底栏 notice 位**无**该文本 + toast 画;
  Info → 底栏画 + toast 不画 (双向断言锁分派)。
- **A/B 精确红**: 摘 kind 分派 → Warn 文本出现在底栏 (红); 摘 toast 命中 →
  点击无 DismissNotice (红)。
- **消退**: 复用 `notice_expires_when_its_deadline_passes` 既有锁 (tick 到点清,
  toast 随之消 —— 补一句断言即可, 不另建时钟)。
- **弹层之上**: merge_menu_open=true 状态下注入 Warn → toast 仍画
  (bind 显隐不被弹层状态污染); 事件分发顺序能否真拦在弹层之前 = **plan 核实项**
  (Stack 事件分发次序读框架源码钉死, 若反序分发则末位天然最先, 写锁; 否则调挂载方案)。
- **Info 色块**: paint 断言底栏 notice 区有衬底矩形且几何包住文本 (含 padding)。
- 基线: 现 **495 绿**; 本模块只增不破。

## 6. Boundaries

- **Always**: 三件套 (fmt / clippy 0 / 测试全绿) 逐 task; 中文注释; token 取色
  不自定义 (家法)。
- **Ask first**: 动框架 (预期零); 动 `NOTICE_TTL` 值; 给 toast 加任何键盘焦点行为。
- **Never**: toast 变模态 (加 scrim/抢焦点/进 Esc 表); 改 `set_notice` 内部
  (家族病史⑤); Info 上浮层; 字体图标 (字形子集陷阱); 改调用点文案。
- spec 写完不立即编码; review 用 `/agent-skills:code-review-and-quality`。

## 7. Success Criteria

**机器判据** (全绿即过):
- §5 全部锁成立; 495 基线不破; clippy 0。

**人工验收 (用户实机, 记账 `tasks/acceptance-pending.md` H 组) —— ✅ 2026-09-29 五条全过**:

| # | 验收项 | 步骤 | 预期 |
|---|--------|------|------|
| H-a | Warn 浮层醒目 | 合并源卡**不选源**直接点步进钮 (G-d 同款场景) | 底部中央浮层弹起, 视线不离操作区可注意到 |
| H-b | 浮层在模态之上 | 合并源卡里输错偏移 (如 `abc`) 提交 | 浮层画在弹层**之上**, 不被盖 |
| H-c | 可点掉 + 自动消退 | 点浮层 / 等 4 秒 | 点掉即消; 不点 4 秒自消 |
| H-d | Info 底栏可辨 | 复制几行看底栏 | 「已复制」有色块衬底, 一眼辨出是反馈 |
| H-e | 双主题 | 深浅各触发一次 Warn + Info | 两套都可辨, 浮层底不透 |

## 8. Open Questions

| # | 问题 | 推荐 | 状态 |
|---|------|------|------|
| Q1 | toast 宽度上限值 | `min(窗口宽×0.6, 520px)` 省略号截断 (D8 已按此写) | **已批 (2026-09-28 「go」)** |
| Q2 | Info 色块用哪个 token | `surface_variant` 打底 + `text_primary` 文字; 若浅色下与底栏底色相邻度不足 (面阶梯教训), 备选 `accent()` 10% 透明度 —— **build 时实测两主题对比度回填** | **已批 + 已回填 (build 实测)**: `surface_variant` 采用 —— 浅色对页面底 Δ`L*` **−7.0** (`theme.rs:272` 注释: 面撞车修复后取值), 暗色对暗底 Δ`L*` **+6.42** / WCAG 1.25 淡台阶 (`theme.rs:448`); 两主题均有 ≥6 Δ`L*` 台阶, 备选方案未启用。实机可辨性归 H-d 验收 |
| Q3 | Warn 浮层期间底栏常态信息 (行号/计数) 是否让位 | **不让位** —— 浮层在状态栏上方浮着, 底栏照常; 分通道不动 | **已批 (2026-09-28 「go」)** |

## 9. 实现记 (2026-09-28, T1–T4 build 收口)

1. **T1+T2 编译边界合并**: toast.rs 测试引用 `Msg::DismissNotice` → 变体/臂/
   `dismiss_notice()` 必须同切片落地 (match 穷尽性), 故实际切片 = T1 组件+消息链 /
   T2 Stack 挂载+锁 / T3 view 分派+Info 色块 / T4 端锁+收口。
2. **plan 核实兑现**: Stack 反序分发 (`stack.rs:87` rev) + 真实鼠标分发主链
   `handler.rs:993` = `tree.event` —— 整树行为锁
   `toast_gets_the_click_before_modal_popover` 复刻真实路径, 一次通过
   (点击 toast → DismissNotice 出队, 弹层收不到 → 不出 CloseMergeMenu)。
3. **A/B 精确红验证**: 摘 view 分派判据 (`NoticeKind::Info` → `_`) →
   `warn_notice_leaves_status_bar_info_stays` 红在「Warn 不得再画在底栏」; 已改回。
4. **Q2 回填**: `surface_variant` 两主题均有 ≥6 Δ`L*` 台阶 (浅 −7.0 / 暗 +6.42),
   备选 `accent()` 低透明方案未启用。
5. **并发事故 (值得记)**: build 期间**并行会话在做 SPEC-checkbox-widget**
   (danqing `c5b1fcf` Checkbox + 本仓 pick_list.rs/settings.rs 工作树改动)。
   clippy 曾报 `no Checkbox in widget` —— 根因 = cargo git checkout 目录被并行会话
   的 fetch 重写到一半的**中间态** (rustc 通道拿到新 rev / clippy 通道还是旧产物)。
   教训三条: ①「特性消失」类报错先问是否有并行会话在动依赖链; ② `cargo clean -p`
   删不掉 clippy 的坏 rmeta, **touch 依赖源码强制重编**才解; ③ 管道 `| tail` 的
   退出码骗局**又踩一次** (clippy 失败被 `&&` 放过, 测试照常跑) —— 09-13 已记,
   复发, 这次起命令退出码一律 `${PIPESTATUS[0]}` 单取。
6. **测试**: 495 → **508 绿** (本模块 +13: toast 8 / main 3 / view 2; 另并行会话
   pick_list 侧同窗口 +4 混入 bin 总数, 全盘 312 main 里 4 条是它的, 已核对归属)。
   clippy 0 / fmt 过 (并行会话的 pick_list.rs fmt 欠账归它自己, 未动)。
7. **人工验收 H 组五条** (§7) 记账 `tasks/acceptance-pending.md`, 待实机。
8. **评审轮 (2026-09-28, 双路独立)**: 代码评审 REQUEST CHANGES (1 Required +
   2 Optional + 3 Nit), 安全审计 PASS (0 Critical/Required, 3 Nit 观感级) —— 全修:
   - **R1 (Required)**: `NoticeKind` 枚举 doc 与 `Msg::Notice` doc 仍是分派前旧模型
     (「底栏提示的级别」「Info 用 text_secondary 画」「同屏可辨」) → 两处定义点 doc
     更新为 D4 分派模型。**家法「加新决定顺手清旧文字」的点名复发**, 且 Info 色
     自 M3 起就写错 (旧账)。失败场景: 下一个人按 doc「修」Info 色回 text_secondary
     = M3②「同色即同通道」复活。
   - **O1**: `hidden_toast_intercepts_nothing` 假绿形态加固 (原版从未 paint, 零矩形
     contains 恒 false, 摘 visible 硬闸也绿) → 先 paint 出真矩形再 sync 降级 Info,
     点陈旧矩形中心须 Ignored —— 锁「两帧之间消退时旧位置的点击不得被吞」。
   - **O2**: 补暗主题底色不透明断言 (spec §5 原文写的就是「双主题」)。
   - **N1**: info_or_none 的 None 分支补零字形断言 (对称)。
   - **安全 Nit① (查实成立)**: 自建 `fit_toast_text` 是框架 canonical
     `fit::ellipsize_tail` 的**近重复**, 且外层重复扣省略号宽 ~14px、注释前提与
     `fit_line` 实际行为不符 → **整个删除**, 直接调 ellipsize_tail
     (「复用 canonical helper」条文正中)。
   - **记档不修 (留痕)**: ①Info 色块/文案无截断 —— 长 status + 长 Info (追踪值
     插值) 时色块可延伸到右侧按钮区底下 (文本溢出是既有行为, 色块让它更显眼;
     现网 Info 文案均短); ②toast 可见期间每帧对同一文案 measure 两遍 (4 秒瞬态,
     可忽略); ③极窄窗 (<60px) 下「…」可微画出 toast 右缘 (TextBatch 无矩形裁剪,
     纯观感)。
   - **评审正面确认 (摘要)**: 事件吞噬五层构造收住 (visible 硬闸先于 hit_rect /
     零矩形 default / paint·hit 同源 / 4 秒 TTL / 门控零污染锁); 分派双向完整
     (NoticeKind 仅两变体穷尽, 全仓无第三绘制点); 「零框架改动」属实 (两兄弟仓
     工作树零 src 改动); 指针捕获配对无泄漏 (抬起重定向回落 = 无操作);
     toast 与状态栏按钮排/滚动条命中带零重叠 (y 区间实测)。
   修复后 **516 绿** / clippy 0 / fmt 过 (评审代理亲跑复核; 516 含并行 checkbox
   会话的 8 条锁, 归属已核)。

## 10. code-simplify 收口 (2026-09-29)

- **S1**: `width_cap()` 收口 —— paint 文案截断预算 / `toast_rect` 矩形钳制 /
  测试期望值三处同式 `(w×0.6).min(MAX_W)` → 单点 (D8 上限将来只动一处)。
- **S2**: `BAR_INSET` 常量钉色条垂直内缩 (原 `+8.0` / `−16.0` 字面量)。
- **不动清单**: ①`Rect::center()` —— 框架无此 API, 不为三处测试点加框架 API
  或产品侧 helper; ②`paints_color` 与 settings.rs:2018 / view.rs `lin()` 三份
  线性色比对同逻辑 —— settings.rs 是并行会话 (checkbox-widget) 活跃文件不能碰,
  view.rs `lin` 调用面广超本批范围, 待其收口后另裁; ③浅/暗主题断言段同形 ——
  断言点不同 (浅色另断 danger/字形), 为抽而抽读者跳两层; ④`dismiss_notice` /
  Stack 挂载 / Info 色块段 —— 单处使用本就简。
- **验证**: 测试零修改全绿 (两刀行为零变化); 本模块三文件 (toast/main/view)
  clippy 零警告 —— 全仓 clippy 挂在并行会话 pick_list.rs:47 `type_complexity`
  (其活跃工作中间态, 归它修, 未碰); fmt 过。
- **五阶段至此全闭** (spec→plan→build→review→code-simplify); 余 = H 组人工
  验收 (待实机) + commit (待用户点头)。

## 11. 分档口径表 (2026-09-29, H-a 实机验收撞出 → 全仓大盘点, 用户「go」全表批准)

**起因**: H-a 验收 (合并源卡不选源点步进钮) —— 提示「先点选源再改时间参数」
**沉在底栏没上浮层**。根因不是 toast 没生效, 是该提示错给了 `NoticeKind::Info`;
分级语义 (D1) 确立前写的几十处提示**从未系统分过档**。

**口径** (D1 的兑现细则): **Warn = 你按的那下没生效** (操作被拒/前置不满足/
对象失效/功能没有/失败/状态剧变) → toast 浮层; **Info = 操作成功的回执 /
进行中 / 状态说明** → 底栏色块。**新加提示先过本表再定档。**

**Info → Warn 改档清单 (23 处, 按族聚类)**:

| 族 | 文案 |
|---|---|
| 指针语义 (先点选再X) | 先点选源再移除 / 先点选源再改时间参数 / 先点选字段 / 先点选会话再删 / 先双击或框选消息里的追踪值 (view Ctrl+R) / 先双击/框选…再按 Ctrl+R (main 键拦) |
| 前置不满足 | 尚未打开文件 (Ctrl+O 打开) ×3 / 当前没有合并 ×3 / 选中源已不在合并中 / **合并至少需要两个源 (当前文件 + 追加)** ② |
| 操作被拒 | 该源已在合并中 / 过滤计算中请稍候再导出 / 本文件非 JSONL 仅可导出原始行 / 本文件非 JSONL 无表格模式 / 合并视图: 复制需先双击/框选消息文本 |
| 功能没有/不可用 | 行多选未实现 (v1.x 待裁) / 合并视图暂无搜索栏 / 购买页即将上线 / **追踪只在合并视图内可用** ② / **本级别不可点选 (仅统计) · 无生效筛选可清除** ② (直方图 P21 吞没出声两条) |
| 操作无变化 | 本行已展开 / 本行未展开 / 本行无嵌套可展 / 追踪值为空 |
| 点击落空/跳转无对象 | 此处无行 / 没有书签 (' 跳转) / 尚无书签 |
| 状态剧变 | 源不足两个, 已退出合并 |

> ② = **复查抓回的三条漏网** (2026-09-29, 用户实机报「打开 A 追加源选 A 提示
> 该换 Warn」触发): 首盘只覆盖 main.rs + view.rs 且多行调用有跳读 —— 漏了
> app_merge.rs 两条与 histogram.rs P21 两条。复查全仓 `NoticeKind::Info`
> 调用点逐条对口径后确认**至此扫净** (留 Info = 回执/进行中清单不变)。
> 补锁: 行为锁加两族样本 (`kind-start` 起并重复源 / 非合并态追踪) +
> `histogram::tests::clicking_unclickable_row_warns_not_info`。

**留 Info 不动 (回执/进行中/状态说明)**: 已复制/已添加·已去掉书签/会话已保存·
已更新·已删除/导出中·导出已取消/合并中·重归并中·追踪中·追踪命中数/已激活
付费层/合并会话参数已套回/恢复合并会话/正在取消导出。

**锁**: 代表样本行为锁 `main::tests::ineffective_action_notices_are_warn_not_info`
(指针语义/跳转无对象/前置不满足三族); P20「此处无行」家法锁只断文案不断 kind,
免疫本批改档; M3 旧锁 `notice_is_drawn_once_in_a_color_of_its_own` 的 Info 样本
由「此处无行」换成真 Info 文案「已复制 3 行」(拿 Warn 文案当 Info 样本会误导)。
**318 绿** (+1 锁), 三件套过。
