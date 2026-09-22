# SPEC: 更新提示 UI 打磨 (update-hint-ui) — 页签角标 + link 按钮 + 布局重整

> 作者: 十四叔 · 日期: 2026-09-22 · 状态: **草稿待批准**
> 流水线: spec → plan → build → review → code-simplify (逐段推进)
> 前序: `SPEC-update-badge.md` (同日五段收口, 人工验收通过) —— 本 spec 是验收后的两条 UI 反馈 + 一张实机截图的直接产物
> 范围裁决: 单能力**两腿** (页签角标与版本行重排共享同一验收时刻「打开设置卡」, 不拆 map)

## 0. 背景 (为什么做)

update-badge 人工验收通过, 用户实机给出两条改进 + 一张截图 (关于页):

1. **底栏「⚙ 设置」有角标了, 设置卡里的「关于」页签没有** —— 打开设置卡后更新提示藏在某个页签里, 没有导航信号。
2. **「新版本号后面的前往下载按钮」要改成 link 按钮, 直达 GitHub release; 微软商店版本走应用内更新。参考 danqing-pomodoro。**
3. **「现在布局有点乱」** —— 截图实证的乱源有二:
   - **对齐体系打架**: 关于页标题区 (应用名/版本/描述) 是**居中**构图, 而更新提示行「有新版本 v1.0.2　前往下载」**左对齐**插入其间, 破坏构图;
   - **风格断裂**: 「前往下载」是无形态黑字 (仅 hover 变色), 而下方「问题反馈」是 accent link 形制 —— 同页两种可点物的视觉语言不一致。

参照系已核实 (`danqing-pomodoro/src/main.rs`): 更新提示行 = `Center::new(Row[Text status, update_button])`,
按钮 = 框架 `Button` **幽灵样式** (透明底 / hover `surface` 底 / focus `accent` / 文案 accent 色, `version_action_button`);
双轨文案「前往下载/更新」与商店轨 StoreContext 应用内更新它也早已齐备 (`update.rs`)。

**实现面核实 (plan 前, D3 据此翻案)**: 「link 按钮」的准确指涉 = 本仓 `settings.rs` 私有组件
**`Link`** —— 就是截图里「问题反馈」那个: 常显下划线 + accent 文案 + hover 圆角底 + 32px 命中
+ `Msg::OpenUrl` 消息链 + `sync` 刷 token。非框架件、非 pomodoro 幽灵 Button; pomodoro 参照的
是**双轨行为** (已实现)。同页两种可点物语言不一致正是乱源二, 修法 = 版本按钮并入 `Link` 的语言。

## 1. Objective

**用户故事**: 作为用户, 打开设置卡时我在「关于」页签上看到小圆点; 进去后更新提示行**融入居中构图**,
「前往下载」是一个有 hover 反馈的 link 形制按钮 (accent 文案), 点击直达 GitHub release;
商店版点「更新」拉起系统商店更新对话框。

**成功的样子**: 实机截图里那行红框内容居中排布、按钮成形; 「关于」页签右上角有与底栏同款的小圆点。

## 2. 范围

### 腿 A: 「关于」页签角标 (danqing `tabs.rs` + danqing-log `settings.rs`)

- **形态与 D1 完全同款**: 6px `th.accent()` 小圆点, 锚「关于」页签文字**右上角** (Y 与文字顶齐平,
  X 文字右缘外 2px) —— 与底栏设置按钮角标同 token 同几何, 不发明第二套角标语言。
- **显隐同源**: `crate::app_update::hint().is_some()` 才画, `None` 零痕迹; 只标「关于」一个页签
  (提示内容所在页), 其余页签永不出角标。
- **框架缺口**: `Tabs` (`danqing/src/widget/view/tabs.rs`) 无 per-tab 角标机制, 页签文字由框架绘制,
  应用侧挂不上 —— 框架 `Tabs` 增加 per-tab 圆点角标能力 (API 形状 plan 阶段定, 语义契约以本节为准:
  绘制圆点、token 同 D1、不改变页签命中与切换行为、无角标时零痕迹零布局位移)。
- 页签角标**非点击目标**: 点击行为照旧切页签; 无 tooltip。

### 腿 B: 版本行重排 + link 按钮 (danqing-log `settings.rs`)

- **布局 (对齐归一)**: 更新行整体**居中** (pomodoro `Center::new(version_status_widget)` 同款);
  行内 `Row` 横排 `[「有新版本 v1.0.2」(text_secondary), token 间距, link 按钮]`, 版本号仍在按钮之前
  (用户定位「新版本号后面的按钮」, 相对顺序不变)。
- **link 按钮 (形制 = `settings.rs` 现成 `Link` 的语言)**: 常显下划线 + accent 文案 + hover
  圆角底反馈 + token 取色; 点击按轨分派 (`perform_action` 语义: GitHub 开编译期常量 URL /
  商店拉起系统更新)。实现上**泛化复用 `Link`、不复制形制** (点击消息可定制, 「问题反馈」
  调用点零改动), 并出**行内形** (hover/下划线随文字块) 适配「版本号后面」的行内语义 (D5);
  替换现在手绘文字 + `Cell<Rect>` 命中的土法实现 (硬编码 RGB 初值一并消灭 —— 颜色一律 token)。
- **行为按轨道不变 (仅形态升级)**:
  - GitHub 轨: 文案「前往下载」→ `open::that("https://github.com/14uncle/danqing-log/releases/latest")`
    —— **直达语义已成立**: `/releases/latest` 由 GitHub 302 到最新 release **详情页**; URL 保持
    **编译期常量**, 远端 tag **不拼进 URL** (D2 不变量「远端 JSON 永不作定位符」原样成立,
    中间人至多伪造/压制提示, 无法改写跳转目的地)。
  - 商店轨: 文案「**更新**」→ `store::request_update()` 拉起系统商店更新对话框 (上一批腿 C 已实现,
    **行为一字不动** —— 用户「商店版走微软那套应用内更新」与现状一致, 本腿只统一按钮形态)。
- **无 hint 零痕迹**: 整行不占位 (现状 `layout` 高度 0 语义保留)。

### 不做 (裁决在案)

- 不动底栏「⚙ 设置」角标 (D1 已验收) / 不改 hint 语义 / 不加 tooltip / 不动托盘。
- **不拼 tag 直达具体 release 页** (见 D3: 破 D2 不变量的收益仅少一次 302)。若日后要做, 须先立
  tag 字符白名单 + host 锁定的扩展设计, 另起 spec。
- 「问题反馈」link 本体不动 (重排后与新按钮自然同族观感)。
- 含发布链 (打包/Release/商店提交) 不在本范围。

## 3. 决策 (2026-09-22)

| # | 决策 | 内容 |
|---|------|------|
| D1 | 角标语言 | 页签角标与底栏角标**同款同参**: 6px accent 圆点, 文字顶齐平 + 右缘外 2px。全应用只有这一种角标 |
| D2 | 「直达」语义 | 保持 `releases/latest` 编译期常量 (GitHub 302 到最新 release 详情页 = 直达)。**远端 JSON 永不作定位符**不变量不动 —— 这是上一批 D2 安全权衡的地基 |
| D3 | link 形制 | **`settings.rs` 现成 `Link` 的语言** (常显下划线 / accent / hover 圆角底 / token 取色) —— 用户原话「link 按钮」即「问题反馈」同款。**翻案本 spec 初稿拟案「pomodoro 幽灵 Button」** (plan 前核实: pomodoro 参照的是双轨行为而非按钮形制; 同页可点物语言统一才是乱源二的修法) |
| D4 | 布局 | 版本行整行居中 + 行内横排 [status, gap, 按钮]; 对齐体系与关于页标题区一致 (乱源一的修法) |
| D5 | 行内 vs 整行 | `Link` 现形是**整行**组件 (hover 底吞整行), 塞进 Row 会拉满剩余宽 —— 泛化出**行内形**变体 (自然宽, hover 随文字块)。**两行式替代案** (status 一行 + 整行 Link 一行, 与「问题反馈」完全同构) 记录在案, 实机观感验收 (b) 当场可换。**2026-09-22 验收裁决: 行内形通过, 两行式不启用** |

## 4. Commands / Structure / Style (增量, 其余沿用两仓 CLAUDE.md)

- 命令不变: `cargo test` / `cargo clippy --all-targets -- -D warnings` / `cargo fmt`。
- 改动文件:
  - 腿 A 框架: `danqing/src/widget/view/tabs.rs` (+ `lib.rs` re-export 若有新公开 API)
  - 腿 A 应用 + 腿 B: `danqing-log/src/settings.rs` (页签角标接线 + VersionRow 重排组件化)
- 无新 `.rs` 文件 (文件头规则不触发)。注释一律中文, 决策处写「为什么」。
- 联动链 (框架腿): danqing 三件套 → commit+push → danqing-log 关 patch 复钉 → 两仓分别提交注明关联。
  **patch 开着时 lock 是 path 态, 不许提交**; 全程零 commit 推进, 联动收口一步留用户闸门 (update-badge T6 先例)。

## 5. Testing Strategy

**注入惯例沿家族**: 构造注入状态, 不碰全局 `publish`, 不触网不触商店; 断言走 `RectBatch`/`TextBatch`
内省 (`#[doc(hidden)]` 三件套先例), A/B 摘掉修复须**精确红**。

框架新增 (danqing lib 测试族):
- tabs 角标**两态对拍**: 有角标恰画一个 accent 圆点实例 (几何: 文字右上、顶齐平), 无角标零痕迹;
  摘掉 push 须精确红。角标不改变 tab 栏高度 (零布局位移) 与命中区域。

腿 A 新增 (danqing-log main 测试族):
- `about_tab_badge_follows_update_hint` —— 接线锁: hint 有/无两态驱动页签角标; 把谓词写死 false 须红
  (锁不停在框架层, 应用侧 bind 也在锁内)。

腿 B 新增 (danqing-log main 测试族):
- `version_row_is_centered_with_link_action` —— 布局几何: 行内 [status, gap, 按钮] 整体居中,
  按钮 = Link 同款形制 (常显下划线 + `th.accent()` token 断言不钉色号); 无 hint 整行零高度。
- `link_open_url_behavior_is_preserved` —— Link 泛化回归锁: `Link::new(text, url)` 旧调用点
  (「问题反馈」) 点击仍发 `Msg::OpenUrl`, 泛化不许改它的行为。
- `version_row_action_button_emits_perform_update_action` —— 点击产出 `PerformUpdateAction`
  (原名 `…dispatches_per_track`, 评审 Required #2 名实对齐); **双臂分派**锁在 app_update
  模型层 `action_target_dual_arm_never_splices_remote_tag` (GitHub 臂 → 编译期常量 URL /
  商店臂 → 应用内更新, 与 `dispatch_polarity_is_never_inverted` 同族) —— review 轮补锁兑现。
- 既有 258 基线不破; 旧 `VersionRow` 手绘几何锁随组件化重写 (断言语义保留: 有提示才占位)。

基线纪律: danqing-log **258** 不许破 (新增只加不减, 并锁除外须写明); danqing 默认 617 / `--features update`
646+1ignored 不许破, 新增按实测回填 plan。

## 6. Boundaries

- **Always**: 提交前三件套全绿; 中文注释; 颜色/间距走 token; 基线不破; 联动两仓分别提交;
  A/B 先红后绿留痕; 文档宣称的测试名 grep 确认真存在。
- **Ask first**: 框架 `Tabs` API 最终形状 (语义以 §2 为准, 命名面 plan 定); 拼 tag URL (默认不做, D2);
  改 hint 语义; 改「问题反馈」; 超出 §4 文件清单。
- **Never**: 远端 JSON 拼进 URL (D2 不变量); 无 hint 时留任何角标/行残留; 自定义色/魔法值绕开 Theme;
  测试触网/触商店/真实桌面副作用; patch 开着时提交 lock。

## 7. Success Criteria

机器部分 (2026-09-22 build 收口 + 实机 bug 返修 + review 轮补锁后终值):
1. [x] 两仓 `cargo test` 全绿: danqing-log **265** (基线 258 + 7 新锁: 88 lib + 166 main +
   8 genlog + 3 keygen) / danqing **623** (默认) 与 **652**+1ignored (update) (基线 617/646
   各 +6 = 3 + 平移锁 + review 轮预约/空 label 边界锁)。
   A/B 先红后绿留痕 **7** 条: `恰一圆点 left:0 right:1` ×2 (框架) / `有提示恰添一个角标圆点
   (on=0 off=0)` (接线·写死 false) / 居中回退左对齐红 + 消息换 OpenUrl 红 (腿 B 双锁) /
   `y left:166.125 right:86.125` (角标 y 双加 area.origin.y, ZERO 原点测试盲区 ——
   布局/绘制锁的 paint 原点必须含非零平移态) / `dot.x=191 expected=245`
   (review R1 几何锚: 错绑页签 A/B 精确红)。
2. [x] 两仓 `cargo fmt` + `cargo clippy` (`--features update` 与默认双模式) 零警告。
3. [x] 手绘调色路径清零: `VersionRow` 手绘按钮 RGB / `text_primary` 初值随组件化消灭;
   `text_secondary`/`Link::accent` 构造初值保留 (sync 先于 paint 每帧刷 token, 与 `Link`
   既有模式同款 —— 首帧前占位, 不是绘制路径)。

人工验收 (用户实机, 沿 update-badge 植缓存手法出提示态) —— **2026-09-22 用户实机验收通过**
(含角标 y 双加返修后复验):
- [x] a) 「关于」页签右上小圆点与底栏「⚙ 设置」角标**同显同灭** (植缓存有/无 hint 两态)。
- [x] b) 版本行**居中**, 行内顺序「有新版本 vX → 按钮」, 与标题区构图协调 (乱源一消除) ——
   观感当场裁: **行内形通过, 不换两行式** (D5 收口)。
- [x] c) 按钮 link 形制 (accent 文案 + hover surface 底): 「前往下载」点击直达 GitHub release 页;
   商店侧载「更新」拉起系统更新对话框 (侧载无待装更新时空返回不炸)。
- [x] d) 无 hint 对照: 页签无点、关于页无此行, 与验收前的干净态一致。

## 8. Open Questions

- ~~(D5 观感) 行内形 vs 两行式~~ —— **2026-09-22 验收裁决: 行内形通过**, 已回写 D5。(D3 已按 `Link` 事实翻案定案。)
- (记录) 若将来要求「直达具体 tag 页」(省一次 302): 须先立 tag 白名单 + host 锁定扩展设计, 另起 spec
  (D2 不变量优先于少一次跳转)。
