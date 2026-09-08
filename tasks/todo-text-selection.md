# TODO: 文本选区与复制 (text-selection-copy)

> Plan: `plan-text-selection.md`; Spec: `../docs/specs/SPEC-text-selection-copy.md`
> 顺序即依赖序: T1 → T2 → [检查点] → T3 → T4 → [检查点] → T5

## Phase 1: 引擎 + 纯逻辑

- [x] **T1: danqing opt-in 键回退** — `App` trait 加 `propagate_unhandled_keys() -> bool` (默认 `false`); `handler.rs` dispatch_focused_event 的 Key 分支: 焦点路径 event_at_path 返回 Ignored 且开关开启时, 回退 `app.event(event)`。Escape/剪贴板既有分支不动。
  - Acceptance: 开关开 = 焦点组件忽略的键到达 app.event; 开关关 (默认) = 现状丢弃; TextInput 消费的键不重复到达 app.event
  - Verify: danqing `cargo test --lib --tests` (新增开/关两态分发测试) + `cargo clippy -- -D warnings`
  - Files: `danqing/src/app.rs`, `danqing/src/window/handler.rs`, `danqing/tests/` (分发测试) | S
  - 备注: 属引擎改动 (Ask first 已经 plan 门裁决); lib.rs 无需动 (trait 方法, 非新类型)

- [x] **T2: `src/selection.rs` 纯逻辑** — lib 侧新模块: `TextSelection { anchor, caret }` (显示行, 解码行字节偏移); `token_at(line: &str, off: usize) -> (usize, usize)` (空白分隔取词, UTF-8 字符边界安全); 规范化 `ordered()`; 复制文本拼装 `copy_text(sel, lines: &dyn Fn(u64) -> String) -> String` (首行后缀/中间整行/末行前缀, `\n` 拼接)。
  - Acceptance: token 边界 (行首/行尾/连续空白/全空白/多字节不劈字符); 反向选区规范化; 单行/跨行/三行以上拼装逐字节正确
  - Verify: `cargo test selection`
  - Files: `src/selection.rs` (文件头 `//! @author 十四叔` + `//! @date 2026/09/08`), `src/lib.rs` | S

### Checkpoint: 地基
- [ ] danqing `cargo test --lib --tests` + clippy 零警告
- [ ] danqing-log `cargo test` 全绿 (selection 单测)

## Phase 2: 交互与复制

- [x] **T3: view.rs 框选/双击事件 + 选区渲染 (原始模式)** — 布局计算提取共享 (paint/event 同一 gutter/text_x 来源, 参照 settings_btn_rect Cell 先例); 按下记锚点 (行文本区内) → 拖动超 4px 升级框选 → CursorMoved 跟手 (measure 前缀命中测试) → 抬起定稿; 双击 (300ms/4px) 置 token 选区; 选区区间矩形渲染 (复用命中高亮模式, 选区后画 = 视觉优先); 有选区时行选中底色让位; 表格模式/sub_row/行号槽区不产选区。
  - Acceptance: 坐标→(显示行, 解码偏移) 命中映射单测 (含 gutter/x_offset 扣除); 框选/反拖/跨行渲染区间正确; 单击不产生空选区
  - Verify: `cargo test` + 实机拖选高亮肉眼验收
  - Files: `src/view.rs` | M

- [x] **T4: 焦点接入 + 复制闭环** — LogView `focusable()=true` + `focus_id("log-view")`; 消费 `Event::Copy` (有文本选区时; 空选区忽略); `selected_text()` 返回规范化拼文本 (原始) / `selected` 行完整解码原文 (表格); main.rs LogApp 实现 `propagate_unhandled_keys()=true`; Esc 有选区时 LogView 消费清选区。
  - Acceptance: 点击日志区后 Ctrl+C 贴进记事本逐字节一致; 焦点在过滤/搜索栏时 Ctrl+C 仍是 TextInput 原语义; 持焦后 j/k/箭头/PageUp/`/`/b 导航不失灵 (T1 回退生效); 表格模式 Ctrl+C = 完整 JSONL 原文行 (含被截断列)
  - Verify: `cargo test` + 实机 Ctrl+C → 记事本比对
  - Files: `src/view.rs`, `src/main.rs` | M

### Checkpoint: 功能闭环
- [ ] 三件套绿 (fmt + clippy + test)
- [ ] 实机五种姿势人工验收: 双击选词 / 框选 / 跨行 / 表格行复制 / 焦点切换 (用户上手)

- [x] **T5: 验收回归** — logbench 基线复核 (数字不劣于 SPEC.md 实测基线); 边界姿势: GBK 行选区、空选区 Ctrl+C、Esc 清选区、选区与搜索命中交叠、水平滚动后命中测试。
  - Acceptance: logbench 无回归; 边界姿势逐项过; 三件套绿
  - Verify: `cargo run --release --bin logbench` + 人工
  - Files: — | S
  - 2026-09-08 自动化部分已落地: logbench 全项远优于 SPEC 闸门 (索引 98ms/搜索 82ms/随机 0.64µs 行); Esc 单测; GBK=解码同路径 (多字节 token 单测); 余下交叠/实机姿势归用户验收门

## Review 修复 (2026-09-08, 评审 Request changes → 全数落地)
- [x] **R1 指针捕获 (danqing 引擎, 用户批准)**: Handler 捕获按下坐标 + 抬起重定向; `pointer_capture` 纯函数 + 4 单测; 一并治愈 TextInput 同类隐患
- [x] **R2 Esc 关卡恢复**: `app_key_filter` 前置 `settings_open && Esc → CloseSettings`
- [x] **R3 复制上限 10 万行 (用户拍板)**: `COPY_MAX_LINES` + 超限 `selected_text=None` + `Msg::Notice` 底栏提示
- [x] **hit_area 断路 (自查)**: LogView 补 `area: Cell<Rect>` + `hit_area()` —— 无它 focusable 形同虚设
- [x] **O1 左键门控** (右键不清选区/不污染双击, 含测试钉死)
- [x] **O2 测试补洞**: floor_char_boundary 多字节 + copy 中劈钳制 + 框选状态机 (含 R1 粘滞回归); danqing 端到端接线测试豁免 (Handler 需事件循环, 真值表+人工验收覆盖)
- [x] **O3/Nit**: hit_text 注释修正、press 元组瘦身、expect 根除、full_w/scroll_trim 测量去重、子行复制语义 doc

## Code-simplify (2026-09-08, 行为保持, 全套件守护)

- [x] ① `selection::row_slice` 唯一权威: paint 选区矩形与 copy_text 两处钳制逻辑合一 (+1 测试)
- [x] ② `measure_row_geom` 抽出: paint 主线 -20 行闭包块
- [x] ③ `handle_text_press` 抽出: pressed 臂 6 层嵌套压平 (语义等价已核对: 表格/空文件态清除本就是 no-op)
- [x] ④ danqing 抬起重定向直白化 (枚举绑定不能用功能更新语法, 显式重构)

## 备注

- 提交获授权时的联动顺序: danqing (T1) 提交并 push → danqing-log `cargo update -p danqing` → 提交 lock, commit message 注明关联
- 构建经本仓 `[patch]` 段用本地 `../danqing`, T1 落地后本机即生效, 无需等 push
