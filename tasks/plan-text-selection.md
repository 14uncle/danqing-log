# Implementation Plan: 文本选区与复制 (text-selection-copy)

> 模块 spec: `docs/specs/SPEC-text-selection-copy.md` (2026-09-08 用户裁决四项决策)
> Task list: `tasks/todo-text-selection.md` (本仓惯例 per-feature 文件, 不动 live-tail 的 plan.md/todo.md)

## Overview

给 LogView 加字符级文本选区: 原始模式双击选词(空白分隔 token)/拖动框选(可跨行)/Ctrl+C 复制纯文本; 表格模式 Ctrl+C 复制选中行完整原文。选区与现有「选中行」并存, 选区出现时行选中视觉让位, Esc/新单击清除。

## Architecture Decisions

### ① Ctrl+C 接入 = LogView 持焦 + 引擎 opt-in 键回退 (Ask first, 待用户裁决)

框架链路已查明: Ctrl+C/X 在 `danqing/src/window/handler.rs:417` 拦截 → 焦点路径发 `Event::Copy` → `selected_text_at_path` 读回 → arboard 写入 (TextInput 同路径); 点击时 `set_by_click` 自动聚焦命中组件 (handler.rs:850)。**但**焦点组件未消费的 Key 事件被丢弃、不回退 `app.event` (handler.rs:426) —— LogView 持焦后现有键盘导航 (j/k/箭头/PageUp/`/`/b/`'`/f/Home/End/Space) 全灭。

**决策: danqing `App` trait 加 opt-in 开关** (工作名 `propagate_unhandled_keys() -> bool`, 默认 `false`), 开启时焦点路径未消费的按下事件回退 `app.event`。默认关 = pomodoro/clipboard 零行为变化, 免回归审计; danqing-log 开启。弃选: 应用层复制 (需引擎加 WriteClipboard + 选区状态上移, 偏离框架焦点剪贴板设计); LogView 复制键位映射 (~15 键双源真相)。

联动顺序 (提交获授权时): danqing 提交并 push → danqing-log `cargo update -p danqing` → 提交 lock。**顺序不能反**。

### ② 选区状态在 LogView 内, 偏移 = 解码后字符串的字节偏移

- `TextSelection { anchor: (u64, usize), caret: (u64, usize) }` — (显示行, 解码行内字节偏移); 规范化只在读取/渲染/复制时做, 事件热路径不排序
- 为什么用解码偏移而非文件字节: 搜索命中高亮走文件字节是因为 regex 跑在 raw 上; 选区两端都是人机交互产生, 渲染/复制/词边界全在解码文本上, GBK 双字节无映射歧义
- 渲染复用搜索命中高亮的先例 (view.rs:623-654, measure 前缀→区间矩形), 选区矩形**后画** = 与命中高亮交叠时选区视觉优先 (spec Open Question 按默认建议落地)
- 复制文本 = 解码行切片, 多行 `\n` 拼接; 表格模式 = `selected` 行 `file.line()` 全量解码 (不受单元格截断影响)

### ③ 纯逻辑下沉 `src/selection.rs` (lib 侧)

view.rs 已 1384 行; token 边界/规范化/拼文本是纯逻辑, 按 logfile.rs 先例进 lib 可单测 (TextBatch::measure 无 GPU 依赖)。事件/渲染留在 view.rs。

### ④ 交互语义 (spec 决策的落地细则)

- 双击检测组件内自判, 沿用 `danqing/src/widget/title_bar.rs:597` 先例 (300ms / 4px)
- 单击 = 现状 `Msg::Select(row)` 不变; 按下即潜在新选区锚点, 拖动超阈值 (4px) 才升级为框选 —— 单击不产生空选区
- 起点在行号槽/展开符区 = 现状行为, 不进选区
- Esc: LogView 有选区时消费 (清选区), 无选区时忽略 (框架清焦, 现状)
- 表格模式不产文本选区 (拖/双击 = 行选择现状); 展开子行 (sub_row) 不参与选区命中
- 有文本选区时行选中底色不画 (视觉让位), `selected` 状态本身保留 (表格复制要用)
- 原始模式下只有行选中、无文本选区时, Ctrl+C 不复制任何东西 (spec: Ctrl+C 只认文本选区)

## Task List

详见 `tasks/todo-text-selection.md`。顺序: T1 (引擎, 风险前置) → T2 (纯逻辑) → 检查点 → T3 (交互渲染) → T4 (焦点复制闭环) → T5 (验收回归)。

依赖: T2 无依赖; T3 依赖 T2; T4 依赖 T1+T3 (无 T1 的键回退, LogView 持焦 = 导航全灭); T5 依赖 T4。

### ⑤ 评审修复 (2026-09-08 code-review, 3 Required 均经用户裁决)

- **R1 指针捕获 (danqing 引擎, 用户批准)**: MouseInput 按位置命中分发, 抬起落在其他区域时按下组件永远收不到配对抬起 (按下态泄漏 = 框选粘滞)。修复 = Handler 捕获「消费按下的坐标」, 配对抬起重定向回该坐标 (同坐标 = 同命中路径; 布局剧变属可接受近似)。一并治愈 TextInput 同类隐患。纯函数 `pointer_capture` + 4 单测
- **R2 Esc 关卡恢复 (产品侧)**: LogView 持焦后框架对未消费 Escape 只清焦不回退, 设置卡需按两次 Esc (S3 回归)。修复 = `app_key_filter` 前置: `settings_open && Esc → CloseSettings`
- **R3 复制上限 10 万行 (用户拍板)**: 滚轮甩底 + Ctrl+C = 全文件选区逐行解码, UI 冻结分钟级。超限 `selected_text` 返回 None + `Msg::Notice` 底栏提示
- **自查加餐**: LogView 只加 `focusable` 没加 `hit_area` → hit_focusable 只认 hit_area, 点击永不聚焦 (Ctrl+C 断路)。补 `area: Cell<Rect>` 缓存 + `hit_area()`。教训: danqing 可聚焦 = focusable + hit_area 两件套
- Optional/Nit 全收: 左键门控 (O1)、floor/drag 状态机测试 (O2)、注释修正 (O3)、press 元组瘦身/expect 根除/测量去重 (Nit)。danqing 端到端接线测试豁免: Handler 构造需事件循环, 真值表 + 人工验收覆盖

## Risks and Mitigations

| 风险 | 影响 | 缓解 |
|---|---|---|
| 键回退开关改变 danqing 既有语义 | 中 | opt-in 默认 false, siblings 零变化; danqing 侧单测覆盖开/关两态 |
| 命中测试与 paint 布局漂移 (gutter 宽两处算) | 中 | 提取共享布局计算 (paint/event 同一函数), 参照 settings_btn_rect Cell 先例 |
| GBK/Latin-1 行解码偏移与显示错位 | 中 | 选区偏移一律落在解码字符串, 渲染/命中/复制同一路径, 天然一致 |
| 拖动期 CursorMoved 逐次前缀测量 | 低 | 单行 O(前缀) measure, 每帧一次; 不进 paint 热路径 |
| 双击与单击选中行视觉打架 | 低 | 第一击选中行 (现状), 第二击转词选区并让位, 用户预期内 |

## Open Questions

- 无阻塞项。spec 三个 Open Question 均按默认建议落地: 不做拖出视口自动滚动 / 选区优先于命中高亮 / 不做 Shift+Click
