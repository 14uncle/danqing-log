# PLAN: update-hint-ui (两腿)
> Verify 栏数字 = 2026-09-22 收口实测终值。基线: danqing-log 258→**262** (+4 锁), danqing 617→**621** / 646→**650**+1ignored (各 +4 框架锁 = 3 + 实机 bug 补的平移锁)。

> 日期: 2026-09-22 · spec: `docs/specs/SPEC-update-hint-ui.md` (D3 已按 `Link` 事实翻案回写) · 零 commit 推进, T5 留用户闸门

## 组件与依赖

| 腿 | 组件 | 仓库 | 依赖 |
|----|------|------|------|
| A框架 | Tabs per-tab 角标 | danqing `widget/view/tabs.rs` | — |
| A应用 | 「关于」页签角标接线 | danqing-log `settings.rs` | A框架 |
| B | `Link` 泛化 + 版本行重排 | danqing-log `settings.rs` | — (与 A应用 同文件, 串行) |

实施顺序: **T1(A框架) → T2(A应用) → T3(腿B) → T4(收口) → T5(联动, 用户闸门)**。
T3 与 T1/T2 逻辑独立但同文件; T1 高风险先行 (Tabs 内部几何), fail fast。

## 关键实现事实 (开工前已核实, 不靠猜)

1. **Tabs 绘制挂点** (`tabs.rs` `paint()`): 每 tab 按 `measure_tabs` 产出的 `tab_areas[i]`
   (相对矩形) 定位文字 —— `text_x = area.x + text_info.origin.x + icon 偏移`, `text_y = area.y + text_info.origin.y`;
   选中指示线同源自 `text_info.size.width`。角标挂点 = 文字右上 (与底栏 D1 同参):
   x = 文字右缘 + 2, y = 文字顶, 6px accent。**T1 第一件事**: 读 `measure_tabs` 确认
   `text_info.size` 是否含 icon 偏移 (影响「文字右缘」取值), 几何锁 A/B 验证。
2. **`Link` 是 `settings.rs` 私有组件** (~728 行): 整行宽 layout (约束宽 × `LINK_ROW_H`=32)、
   文本行内居中、**常显下划线** (accent 1px, 注释明言「裸小字链接发现性太差」)、hover 圆角 6
   `surface_variant` 底、命中整行、点击 `msgs.push(Msg::OpenUrl(url))`; `sync` 刷 token
   (`th.accent()`/`th.surface_variant()`)。**泛化面**: ① 点击消息可定制 ② 行内形 layout (自然宽);
   `Link::new(text, url)` 旧调用点 (「问题反馈」`feedback_row`) **零改动**。
3. **消息链现成**: `Msg::OpenUrl` (`main.rs:383`) → `main.rs:1661` open。新增 `Msg::PerformUpdateAction`
   → `app_update::perform_action()` —— 双轨语义已收敛在 `perform_action` (GitHub 开 URL / 商店拉更新)。
4. **手绘土法现状** (T3 消灭): `VersionRow` paint 手推 baseline + `Cell<Rect>` 命中 + 硬编码 RGB
   初值 (`Color::rgb(0.40, …)`) + event 直调 `perform_action()`。
5. **角标先例**: 底栏 `update_dot_rect`/`paint_update_dot` (`view.rs`, `UPDATE_DOT_D=6.0`) + 三锁
   (显隐两态对拍 / 几何 / 接线) —— 腿 A 语义同参 (6px / accent / 文字顶齐平 / 右缘外 2px),
   实现落 `tabs.rs` 内 (框架不反依赖应用)。
6. **布局组件现成**: `Center`/`Row` 已在 `settings.rs` import (14 行), 零新增依赖; pomodoro 形态
   = `Center::new(Row[Text status, action])`。

## D5 衍生设计 (行内 vs 整行, spec D5)

`Link` 整行形 hover 底吞整行 —— 塞进 Row 会拉满剩余宽。T3 泛化出**行内形**: layout 自然宽
(文字宽 + padding), hover 底/下划线随文字块。**两行式替代案** (status 一行 + 整行 Link 一行,
与「问题反馈」完全同构) 记录在 spec D5 —— 实机观感验收 (b) 当场可换, 不预设翻案。

## 任务 (详见 todo-update-hint-ui.md)

- **T1** A框架: Tabs per-tab 角标 (API 拟 `bind_tab_badge(index, f)` 保留模式家族; 语义契约 = spec §2:
  绘制圆点 / token 同 D1 / 零布局位移 / 不改命中) + 框架两态锁 (恰一圆点·几何·零痕迹·摘掉 push 精确红)。
  验证: danqing 测试 + clippy (基线+N 回填)。
- **T2** A应用: 「关于」页签角标接线 (`hint().is_some()` 驱动) + 接线锁 (谓词写死 false 须红)。
  验证: danqing-log 258+N。
- **T3** 腿 B: `Link` 泛化 (消息面 + 行内形) + `Msg::PerformUpdateAction` + VersionRow 组件化重排
  (`Center::new(Row[status, gap, link 形按钮])`) + 四锁 (布局几何 / 双臂分派 / Link 回归 / 无 hint 零高度)
  + 手绘土法删除。验证: 258+N, A/B 先红后绿留痕。
- **T4** 收口: 两仓三件套 + 基线对账回填 + 测试名 grep 复核 (settings.rs:953 教训)。
- **T5** 联动 (**用户闸门**): danqing commit+push → danqing-log 关 patch 复钉 → 两仓分别提交注明关联。
  patch 开着 lock 是 path 态不许提交 (既有纪律)。验证: 无 patch `cargo check --locked` 过。

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| `measure_tabs` 的 size 含不含 icon 偏移 | T1 第一件事读源 + 几何锁 A/B 验 |
| `Link` 泛化破坏「问题反馈」 | 回归锁 `link_open_url_behavior_is_preserved` (旧调用点行为不变) |
| 行内形 hover 观感不佳 | 两行式替代案在 spec D5, 验收当场换 |
| 框架 API 形状走样 | spec §2 语义契约锁死 (绘制/token/零位移/零痕迹/命中不变), 命名 T1 内定 |
| lock/patch 陷阱 + 假退出码 | 全程零 commit; T5 单列; 验证一律落盘重定向取真退出码 + 读内容 (09-22 再犯实录) |

## 验证检查点 (收口实测)

- T1 后: danqing 620+1ignored (默认) / 649+1ignored (update) + clippy 双模式 0; A/B 精确红
  `恰一圆点 left:0 right:1` ×2。
- 实机 bug 返修 (2026-09-22 人工验收 (a) 抓获): 角标 y 双加 `area.origin.y` —— `title_top`
  取了已含 area.y 的 `text_y` 又在 push 时再加一次, area 原点非零时圆点跌出面板
  (截图孤点 y≈2×tab栏y−ascent 吻合)。ZERO 原点的几何锁测不出 = 测试盲区。
  修复: `title_top` 改取相对量 `text_info.origin.y − ascent` (坐标一律「相对 + area.origin」一次)。
  补平移锁 `tab_badge_dot_translates_with_area_origin` 先红后绿 (红留痕: `y left:166.125
  right:86.125` 偏大 80 = dy, x 正常); 终值 danqing **621** (默认) / **650**+1ignored (update),
  A/B 留痕累计 6 条。
- T2 后: 259 绿; A/B 精确红 `有提示恰添一个角标圆点 (on=0 off=0) left:0 right:1` (谓词写死 false)。
- T3 后: 262 绿 + clippy 0; A/B 双红 (居中回退左对齐 → centered 锁红; 消息换 OpenUrl → dispatch 锁红)。
- T4 后: 两仓 fmt 0 + clippy 0 + 全绿对账 (262 / 620 / 649+1ignored); 宣称测试名 grep 11/11 在案。
- T5 后: `cargo check --locked` (无 patch) 过; 人工验收 (spec §7) 另行排。

## Review 轮 (2026-09-22, code-review-and-quality + 独立 code-reviewer 双视角)

初审 verdict **REQUEST CHANGES** (无 Critical) —— 4 Required 全修, 终值 **265** / **623** /
**652**+1ignored, 两仓 clippy/fmt 0, 复核 **APPROVE**。

| # | Finding | 处置 |
|---|---------|------|
| R1 | ABOUT 下标错绑全绿 (无 tripwire, wiring 锁只数「恰一之差」) | **修**: `about_tab_index_matches_tab_chain` tripwire + **真一致几何锁** `about_tab_badge_sits_on_the_active_about_title` (active=关于 的指示线做锚, `dot.x == indicator.x + indicator.w − 6` 恒等式; 错绑下标 A/B 精确红 `dot.x=191 expected=245`) |
| R2 | `…dispatches_per_track` 名实偏差, perform_action 双臂未锁 (spec §5 未兑现) | **修**: 改名 `…emits_perform_update_action` + `action_target` 纯模型 (与 `track_of` 同族) + `GITHUB_RELEASES_PAGE` 编译期 const (spec() 与模型同源三等式) + 锁 `action_target_dual_arm_never_splices_remote_tag`; 臂体 `debug_assert` 模型-执行面同源自证; 文案单点接框架 `update_action_text()` |
| R3 | `bind_tab_badge` 天文下标 `while push` 无界分配 / 越界静默 | **修**: `BADGE_BIND_MAX_INDEX=64` 笔误防线 (超限 warn + 忽略, 不 panic 不 OOM) + 边界锁 (预约语义生效 / 天文下标零痕迹) + 空 label 边界锁 |
| R4 | Cargo.lock path 态混工作区 (patch 开着的指纹) | **流程执行**: T5 第一步关 patch; 任何 commit 不得含该 lock diff (既有纪律, T5 复核点) |
| O1 | 「hover/focus」措辞 vs Link 无 focus 面 | **裁决**: D3 形制原文即 hover-only (「问题反馈」同款), spec 措辞改「hover」对齐现实; focus ring 不做 (defer, 非本批回归) |
| O2 | `inline: bool` boolean trap → `LinkMode` enum | defer (code-simplify 段裁) |
| O3 | Link 下沉 danqing 框架 (引擎复用率) | defer (超 D4 范围, 记候选) |
| O4 | 双注入通道 (`has_hint` 手戳 vs override Cell) | defer (触 sync 重构; 现测试注释已警示自噬风险) |
| O5 | `action_label_of` 字面量非单点 | **修** (并入 R2): GitHub 臂接 `danqing::update::update_action_text()`, 测试钉「前往下载」防漂 |
| O6 | 长 status 溢出行宽 | defer (当前文案短, 低风险) |
| O7 | `Link.area` 相对/绝对混态 | defer (pre-existing 模式, 「问题反馈」同款, 非本批引入) |
| N1 | 几何锁内联重复 `badge_dots` 过滤 | **修** (复用 helper) |
| N2 | 陈旧注释「(常规 / 快捷键 / 关于)」漏「许可」 | **修** |
| N3 | 空 label 角标未测 | **修** (并入 R3 边界锁) |
| N4 | has_hint 翻转 hover 粘滞 | defer (理论性, hint 缓存会话内稳定) |

FYI 确认: D2 不变量保持 (`releases_page: &'static str` 类型级防拼接 + 消息无载荷);
角标零命中重叠; token 纪律无硬编码绘制色; Overlay 门控下 hint 查询成本可忽略。

## code-simplify 轮 (2026-09-22, 行为零变化, 测试零改动)

- **改**: ① tabs.rs `badge_dot_count` 收敛为 `badge_dots(rects).len()` (双实现合一);
  ② settings tests 抽 `round_dots` 共享 radius-3 收集 (两测试断言语义原样)。
- **裁决不改**: Review O2 (`inline: bool` → `LinkMode` enum) —— 单 bool 私有核心已被
  `new`/`inline` 两个语义构造器包住, 换 enum 是同概念数换皮 (churn 非简化);
  O4 (双注入通道统一) —— 必然改测试 (简化红线), 属架构改动, 注释已警示自噬风险。
  其余 (O3 Link 下沉框架 / O6 溢出 / O7 `Link.area` 混态 / N4 hover 粘滞) 为行为面或
  pre-existing, 出简化范围。
- 终验: **265** / **623** 全绿 (断言零改动通过), 两仓 clippy/fmt 0。
