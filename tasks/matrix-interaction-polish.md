# 矩阵: 手势 × 反馈 (interaction-polish)

> @author 十四叔 · @date 2026/09/14
> 前置意图: `docs/intent/interaction-polish.md` (interview-me 裁定 + 显式 yes)
> 用途: ① spec 的需求输入 ② 用户实机走查的**核对单** (双侧走查的「实机侧」)
> 状态: **待用户逐条实机核对** —— 下列每条都标了代码位置, 核到哪条不对就划掉

---

## 0. 方法

四路并行普查, 全部落到 `file:line`; 标注 `【已复核】` 的几条是 agent 报完我又
**读源码本身**对过的 (**本仓规矩: 结论要有对照组, 不转述**)。

**框架侧路径口径**: 普查报告里 `focus.rs` / `flow.rs` / `text_input.rs` 等简写,
已按真身展开为 `danqing/src/widget/...` (`focus.rs` 在 `src/widget/` 下, **不在** `src/`;
`flow.rs` 在 `src/widget/layout/`; `text_input.rs` 在 `src/widget/form/`)
—— 2026-09-14 核 spec 时按简写找不到文件, 遂逐条改正。

| 路 | 范围 | 产出 |
|----|------|------|
| 框架 | `danqing/src/window/handler.rs` 等 14 文件 | 向下派发的事件全集 + 反馈原语有无 |
| 鼠标 | 本仓 `view.rs`/`main.rs`/`histogram.rs`/`settings.rs`/`tray.rs` | 手势 → 反馈 |
| 键盘 | 同上 + `config.rs` | 键 → 行为 → 反馈 → 可发现性 |
| paint | 同上 (方向相反) | **状态 → 视觉** 映射 + token 共用 + 命绘同源 |

判定口径 (三问, 见意图文档): **违1** = 按下无即时变化 / **违2** = 结果留不住或
指认不出对象 / **违3** = 不可用或失败时无说明。

---

## 1. 粗糙点清单 (本次要清零的东西)

### 1.1 违第 3 问 —— 按了没反应, 也不说为什么 (最集中的一类)

| # | 症状 | 位置 | 判定 |
|---|------|------|------|
| P19 ★ | **Esc 清焦点 → Ctrl+C 静默失效, 而行高亮仍在屏上**。触发链: 点一行 → Esc → Ctrl+C, 什么都不发生。Esc 分支只看 `selection/selected_cell/press/dragging`, **不碰 `selected`**, 全空则 `Ignored` → 框架清焦点; 而 Ctrl+C 无焦点时不进组件、app 的 ctrl 分支只认 b/g/t | `view.rs:1609-1613` + `danqing/src/window/handler.rs:446-450` + `main.rs:1247-1263` | 违2+违3 |
| P20 | 列表区**末行下方空白**单击/双击完全沉默: 不取消行选中、不清文本选区以外的任何东西、不说为什么 | `view.rs:1527-1568` | 违1+违3 |
| P21 | 侧栏**不可点**桶行点击被吞 (`.log` 整栏只读有顶部提示兜底; JSONL 的个别桶没有) | `histogram.rs:456-458` | 违3 |
| P22 | 表格**无 glyph 的行**, 点行首展开区照样发 `ToggleExpand` → 零反应 (`in_glyph` 不校验本行可展开) | `view.rs:1531` | 违1+违3 |
| P23 | 纯 `.log` 按 **Ctrl+T** 毫无反应且不解释 | `main.rs:1349-1351` | 违3 |
| P24 | **→/←** 在明文行/无嵌套行/parse 失败时提前 return, 无反应无解释 | `main.rs:476-482` | 违3 |
| P25 | **Ctrl+G** 无书签时按了毫无反应 | `main.rs:1066-1075` | 违3 |
| P26 | **空态**下 Ctrl+F / T / B / G / 方向键 全静默 | `main.rs:1242-1245` | 违3 |
| P27 | **错误态在视觉上不存在** —— `notice` (打开失败/轮转/选区超限) 与「正则无效」都拼进同一个 `self.status` 字符串, 用 `text_secondary()` 画, 与打开耗时/过滤统计**同色同字号** | `view.rs:1408-1414` 【已复核】 + `main.rs:927-929` / `main.rs:1007` | 违3 |
| P28 | **选区超复制上限**: 拖选期间无任何视觉, 只在按 Ctrl+C 时才知道 | `view.rs:675`, `view.rs:1591-1595` | 违3 |
| P29 | **右键 = 左键**: `view.rs:1516` 不筛 button, 右键被复用为行选中; 没有右键菜单, 也无说明 | `view.rs:1516/1523` | 违3 |
| P30 | **滚轮在侧栏/过滤栏上不回落**: 指针必须移回内容区才能滚列表 (框架按点子命中分发, 不向父级回落) | `histogram.rs:442-450` + `danqing/src/widget/layout/flow.rs:288-296` | 违1+违3 |
| P31 | **Tab 能走出设置卡模态**: `collect()` 不看 `modal_barrier`, 焦点落到卡后被盖住的 LogView / Bar 上, 按 Tab 看不到任何事发生 | `danqing/src/widget/focus.rs:147-168` | 违1+违3 |
| P32 | **设置卡模态被 `app_key_filter` 穿透**: Ctrl+O 在卡片之上弹系统文件对话框; Ctrl+F 把焦点按 id 送到卡后**看不见的**输入框, 此后打字进的是它 | `danqing/src/window/handler.rs:418-425` + `main.rs:1235` 自认 | 违3 |
| P33 | **同一键两义无提示**: 栏持焦时 `Space`=输入空格、`↑↓`=滚列表、`Home/End`=移光标, 与未持焦时语义相反 | `danqing/src/widget/form/text_input.rs:596-600` + `view.rs:1982-1989` | 违3 |
| P34 | **Ctrl+A** 在分栏持焦以外被 `main.rs:1247-1263` 的 `if *ctrl { … return }` 静默吞掉 | `main.rs:1247-1263` | 违3 |

### 1.2 违第 1 问 —— 按下没有即时变化

| # | 症状 | 位置 | 判定 |
|---|------|------|------|
| P10 ★ | **拖框选时选中行指示消失** —— `if i == self.selected && !has_text_sel`, 有文本选区时行底色**和**左侧 accent 竖条一起不画; 且 1091 是 `else if`, hover 也一并熄灭 | `view.rs:1084-1095` 【已复核】 | 违1 |
| P11 ★ | **hover 在搜索命中行上失效** —— 命中行底在 hover 底之后画 (`1163` after `1094`), 且同用 `th.selection()`, 直接盖掉 | `view.rs:1094` vs `1163` 【已复核】 | 违1 |
| P6 | **搜索栏焦点无描边** —— `.chromeless()` 让 `TextInput` 跳过焦点描边, 唯一证据是闪烁 caret (半周期熄灭时零指示); Ctrl+F 的即时变化只剩它 | `view.rs:1799` + `danqing/src/widget/form/text_input.rs:488` | 违1 |
| P7 | **`LogView` 有 `focusable=true` 有 `focus_id`, paint 里零焦点态** —— 焦点在日志区还是搜索框, 界面上看不出来 | `view.rs:1630` | 违1 |
| P8 | **CloseButton 无焦点态** (只有 hover 变色) | `danqing/src/widget/base/close_button.rs:129-141` | 违1 |
| P9 | **Tab 首停是根 Stack 节点** (不可见节点), 因为首帧自动聚焦 `chain[0]` | `danqing/src/widget/focus.rs:54-59` + `danqing/src/widget/layout/stack.rs:95-97` | 违1 |
| P2 | 滚动条 track/thumb 无 hover、无按下态 (见 §2 边界题) | `view.rs:1354-1398` | 违1 |
| P3 | 表格行首展开标识 `+/−` **无 hover** —— 可点却无指示 | `view.rs:1240-1253` | 违1 |
| P4 | 过滤/搜索栏 TextInput **无 hover** | `danqing/src/widget/form/text_input.rs` 无 `CursorMoved` 分支 | 违1 |
| P5 | 设置卡 Dropdown 控件**展开前无 hover** (仅点击后转 `focused` 边框) | `danqing/src/widget/form/dropdown.rs:523-534` | 违1 |
| P1 | 表头无 hover (见 §2 边界题) | `view.rs:969-999` paint 有 / event 无 | 违1 |

### 1.3 违第 2 问 —— 结果留不住 / 指认不出对象

| # | 症状 | 位置 | 判定 |
|---|------|------|------|
| P14 | **多命中时「哪个是当前」只剩一条 3px 竖线** —— 搜索命中行底 == 选中行底 (同 `th.selection()`) | `view.rs:1163` vs `1085` | 违2 |
| P15 | **文本选区带与搜索命中区间同 token** → 同一行内两者交叠时不可分 | `view.rs:1139/1332` vs `1227` | 违2 |
| P16 | **单元格底与行选中底同 token** (2026-09-14 补 `accent` 描边后才可分); 浅色下 `selection` 与 `hover` 只差 ΔL\* 0.79 | `view.rs:1109` + `view.rs:108-112` | 违2(弱) |
| P17 | **Ctrl+C 无成功回执** —— 复制了哪条、复制了几行, 无任何反馈 (只在超限时报错) | `view.rs:1585-1600` | 违2(弱) |
| P18 | **侧栏双击 = 两次单击自相抵消** (套用筛选后立即清除, 闪一下) —— 无双击区分 | `histogram.rs:442` | 违2 |

### 1.4 可发现性与一致性 (不属三问本身, 但同源)

| # | 症状 | 位置 |
|---|------|------|
| P36 | `/` `b` `'` `f` `→/←` 与 Esc 多义 **只在仓外 `README.md:73-90`**; 设置卡自陈「只列猜不出来的那几个组合键」(`settings.rs:214`), 但 `/` 与 `f` 不属猜得出来那类 | `settings.rs:216-223` |
| P37 | **Ctrl+L 隐藏侧栏后, 界面上无任何「如何找回」** —— 提示行随侧栏一起消失 | `histogram.rs:412-414` |
| P38 | 托盘菜单两项无快捷键文字 (第 4 参传 `None`) | `tray.rs:19,21` |
| P39 | **Ctrl+F 清掉栏内已输入未应用的草稿** (「聚焦即干净开始」), 而 Ctrl+F 是「回到搜索框」的反射键 | `main.rs:985` |

### 1.5 无缺口但属上述病历的三条 (防「改回」)

- 侧栏不可点行**有意不给 hover** (`histogram.rs:433` 经 `is_row_clickable` 过滤) ——
  给了就是教错; 只读整栏靠顶部提示兜底。**保持**。
- 表格模式**不做单元格词级框选** (D2 划线) —— 但「拖了零反应」仍属 P20 同族沉默。
- 行 hover 与斑马走**独立通道** (`row_hover_bg` / `row_band_bg`) —— 已有回归锁。**保持**。

---

## 2. 边界题 (需用户裁, 属「手势缺失」还是「反馈缺陷」)

| 题 | 现象 | 选项 |
|----|------|------|
| **滚动条** | paint 画了 track/thumb, 拇指是**强烈的可拖 affordance** (所有平台的滚动条都能拖), 但 `view.rs:1453-1624` 整段 event 无滚动条分支 —— 只能滚轮 | (i) 本批**补拖拽** (行锚定数学已有, 不算新功能语义) / (ii) 判为手势缺失, 单独立项, 本批只处理其外观是否在撒谎 / (iii) 本批不管 |
| **表头** | 列名看着可点, 实际无 hover 无点击 (排序/拖宽都没有) | (i) **不补 hover** —— 给一个不存在的手势加反馈等于教错, 排序/拖宽单独立项 / (ii) 补 hover 暗示 + 本批做排序 (新功能, 与「不加功能」冲突) |

---

## 3. 手势缺失清单 (按意图**不进本批**, 单独立项)

均已查证到「event 侧零分支」或 grep 零命中; **不在此列的不补 hover**(补了就是撒谎)。

| # | 缺失 | 证据 |
|---|------|------|
| 1 | 纵向滚动条点击/拖拽 | 仅 paint `view.rs:1376-1398`; event 无分支 |
| 2 | 横向滚动条点击/拖拽 | 仅 paint `view.rs:1354-1372`; 同上 |
| 3 | 表头点击排序 / 拖宽列 | 仅 paint `view.rs:969-999`; grep `resize\|sort` 零命中 |
| 4 | 表头双击 / 悬停 | `CursorMoved` 只设 `hover_row` |
| 5 | 滚轮在侧栏/过滤栏上 (P30) | 见 1.1 (属回落缺陷, 非缺失) |
| 6 | **拖文件到窗口打开** | 本仓 `DroppedFile/HoveredFile` 零命中; 框架 `WindowEvent` 匹配集 (12 个) 无 winit 拖放事件 → `open.rs:6,27` 的「拖拽」是**路径标签遗留**, 本批只改注释 |
| 7 | 右键上下文菜单 (复制/加书签/展开) | 全仓无; 右键当前被复用为左键 (P29) |
| 8 | 托盘图标**左键** (显隐/唤出窗口) | 框架只收 `MenuEvent`; 无 `TrayIconEvent` |
| 9 | 侧键 Back/Forward 导航 | grep 零命中 |
| 10 | 中键专属用途 (自动滚动/粘贴) | grep 零命中; 中键当前等同左键 |
| 11 | 表格单元格拖拽框选 | 有意缺失 (D2) |
| 12 | 过滤栏鼠标「清除」按钮 | 清除仅键盘 Esc; 框架 `TextInput` 无 clear 按钮 |
| 13 | Ctrl+W / Ctrl+Tab / F1 / Ctrl+= / Ctrl+- / Ctrl+A 全局全选 | 全仓零匹配 (字号是 const `view.rs:48`) |

---

## 4. 框架能力缺口 (三问角度, 全部**查证**非推测)

| # | 缺口 | 命中 | 证据 |
|---|------|------|------|
| G1 | **`MouseInput` 不带修饰键** (`MouseWheel`/`Key` 都带) | 违1/3 | `danqing/src/event.rs:110-117` |
| G2 | **无光标形状 API** (`CursorIcon` 全库零命中) → 所有 hover 缺口只能靠视觉补 | 违1 | grep 零命中 |
| G3 | **无 disabled 态概念** → 「不可用」没有挂载点 | 违3 | grep 只命中 `ImeEvent::Disabled` |
| G4 | **无 tooltip / 无 spinner** | 违3/违1 | grep 零命中 |
| G5 | **不合成 Click / DoubleClick** —— 全框架唯一双击是 TitleBar 本地实现 | 违1/2 | `title_bar.rs:596-609` |
| G6 | **无 hover 真值** —— 广播坐标, 每组件自做 `contains` | 违1 | `danqing/src/widget/layout/flow.rs:236` |
| G7 | **`ModifiersChanged` 不透传** (Handler 私存后 return) | 违3 | `handler.rs:846-849` |
| G8 | **无状态过渡动画** (有 `Tween` 原语, 无「状态变了自动补间」) | 违1 | `anim.rs:34` |
| G9 | **无「离开控件」事件** (只有窗口级 `CursorLeft`) | 违1 | `event.rs:134` |
| G10 | **滚轮 delta 不归一** (`LineDelta` / `PixelDelta` 原样上抛) | 违1 | `window/event.rs:150-154` |

**基础事实**: `WindowEvent` 实际匹配集 = `{CursorMoved, CursorLeft, MouseInput, MouseWheel,
KeyboardInput, Ime, ModifiersChanged, Focused, CloseRequested, Resized, Moved, RedrawRequested}`。

---

## 5. 命绘同源残留 (漂移风险, 非三问缺口)

2026-09-14 抽 `text_x`/`row_at`/`is_double_click` 后, **缓存型几何全部同源**
(`row_geom`/`col_spans`/`settings_btn_rect`/`btn_area`/`label_width`)。**仍未同源三处**:

| # | 两侧 | 位置 |
|---|------|------|
| S1 | 行 y 映射: paint 行循环式 vs `row_at` 式 | `view.rs:1045,1058` ↔ `view.rs:695-697` |
| S2 | 展开 glyph 命中区: 同常量两处各写 | `view.rs:1248` ↔ `view.rs:1531` |
| S3 | 侧栏桶行高亮 y: 内联式 vs `row_rect` | `histogram.rs:306,311-320` ↔ `histogram.rs:85-92` |

---

## 6. 実机核对单 (用户走查用)

按区域走, 每条对应上文 P 编号:

- [ ] **列表区**: 单击行 / 双击选词 / 拖框选 (**P10**: 拖的时候选中行还在吗) / 点空白 (**P20**) /
      滚轮 / 横向滚 / 点滚动条 (**P2**) / 点行首 `+/−` (**P3**)
- [ ] **表格模式**: 点单元格 / 双击单元格 / 点无 glyph 行的展开区 (**P22**) / 悬停命中行 (**P11**) /
      搜索后看多命中 (**P14**)
- [ ] **搜索/过滤**: Ctrl+F 看焦点 (**P6**) / 重复按 Ctrl+F (**P39**) / 输入无效正则 (**P27**) /
      悬停输入框 (**P4**)
- [ ] **键盘**: 点一行 → Esc → Ctrl+C (**P19** ★) / 空态按 Ctrl+F、方向键 (**P26**) /
      纯 .log 按 Ctrl+T (**P23**) / 无书签按 Ctrl+G (**P25**) / 表格里按 → 用无嵌套行 (**P24**) /
      栏内按 Ctrl+A (**P34**) / 栏内按 Space 与 ↑↓ (**P33**)
- [ ] **侧栏**: 点可点行 / 点不可点行 (**P21**) / 双击桶行 (**P18**) / 悬停空白 / 指针在侧栏上滚滚轮 (**P30**)
- [ ] **焦点与模态**: Tab 走一圈 (**P8/P9**) / 打开设置卡后按 Tab (**P31**) / 卡开着按 Ctrl+O、Ctrl+F (**P32**)
- [ ] **右键**: 行上右键 (**P29**)
- [ ] **其它**: 打开失败时的底栏 (**P27**) / Ctrl+L 关侧栏后找不回来 (**P37**) / 托盘菜单 (**P38**)

---

## 7. 未定 / 未覆盖 (如实记)

- **Tab 实际停靠序**: 由 `collect()` DFS 与树结构推导, **未用运行态探针验证**。
- **托盘左键**「应有行为」: 取决于产品定位, 本仓无裁决记录 → 按缺失手势不纳入。
- **`app_update::hint()` 的下载中/失败态**是否有视觉: `settings.rs:330-336` 只读 status/action
  文案, 未见进度分支 —— **未展开 `app_update.rs` 核实**。
- **所有条目均为静态代码判定, 尚无一条经运行态证实** —— 这正是「实机侧走查」要补的另一半。
