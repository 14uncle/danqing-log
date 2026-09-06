# Plan: 窗口标题栏 (app-chrome)

> spec: `docs/specs/SPEC-app-chrome.md` (2026-09-06 修正: 顶部无自绘标题条, 标题是 winit title)。
> 依赖框架 `titlebar-embed`(embed 槽)。目标: 新增框架 TitleBar 置顶, 内嵌过滤/搜索 Bar, 三窗键可用。

## Overview

现 `view()` = `Column[Bar, LogView.fill]`, 顶部只有 Bar 行(双线程过滤/搜索输入), **无标题条/无窗键**。
改为 `Column[TitleBar(embed Bar), LogView.fill]`: TitleBar 提供 logo/标题/三窗键/拖拽, Bar 进槽。
`LogView` 表格列头(`HEADER_H`)与此无关, 保留。

## Architecture Decisions

1. **Bar 从 sibling 挪进 embed 槽**: `view()` 顶层 `Column` 的 Bar child 移除, 改为
   `TitleBar::themed(...).logo_kind(LogoKind::Log).on_close/on_minimize/on_maximize(...).embed(bar)`。
   Bar 自身(双 TextInput/上下文/转发/clear-rev)不动, 只改变宿住的几何来源。
2. **标题串搬进 TitleBar**: 现 `main.rs` `WindowConfig.title` 的 `丹青日志 POC [JSONL] — <file>`
   改由 TitleBar.title 呈现; 窗口标题(`WindowConfig.title`)仍设(任务栏/Alt-Tab), 二者一致。
3. **三窗键接 WindowAction + bind_maximized**: `on_close→Close, on_minimize→Minimize,
   on_maximize→MaximizeOrRestore`; `bind_maximized` 读应用 `maximized`(应用需维护该态)。
4. **键盘/焦点回归**: Bar 的 Enter/Esc/PageUp/PageDown 拦截 + IME/粘贴/命中转发 + `app_key_filter`
   (Ctrl+T) 全保留; 只改 Bar 所在位置, 不改其行为。

## Task List

### A1: view() 换 TitleBar + 接三窗键
- **Acceptance**: `view()` = `Column[TitleBar.embed(Bar), LogView.fill]`; TitleBar 设 title/logo_kind=Log/
  on_close/on_minimize/on_maximize/bind_maximized; Bar 不再作 sibling。
- **Verify**: `cargo build`; 三件套。人工开窗看标题+三键。
- **Files**: `src/main.rs` | M

### A2: Bar 适配 embed 槽几何
- **Acceptance**: Bar `layout/paint/event` 用槽传入的 area(非整窗 Column 行宽); `label_width`/`input_area`
  以槽为基准; Bar 为 Hidden 时高度 0(标题栏仅标题+三键)。
- **Verify**: `cargo test`(Bar 既有单测适配槽 area)。人工: 原始模式无搜索时标题栏无输入, 表格/搜索有输入。
- **Files**: `src/view.rs`(Bar) | M

### A3: 标题串 + 窗口配置一致性
- **Acceptance**: TitleBar.title = `丹青日志 POC [JSONL] — <file>`(或 `文件名 · 模式`); `WindowConfig.title`
  同串; 表格/原始模式标题随 `mode` 变化(如 JSONL 后缀)。
- **Verify**: 人工 — 任务栏标题与 TitleBar 一致; Ctrl+T 切模式标题带/不带 JSONL。
- **Files**: `src/main.rs` | S

### A4: 键盘/焦点/IME 回归
- **Acceptance**: 栏聚焦时 Enter 应用搜索/过滤、Esc 清/关、PageUp/PageDown 滚动、中文 IME 候选窗贴框、
  粘贴、点击落焦; `app_key_filter` Ctrl+T 在栏聚焦时仍生效; Tab/方向键导航不冲突。
- **Verify**: 人工验收清单(参考 v1 todo-v1-textinput.md)+ `cargo test`。
- **Files**: `src/main.rs`, `src/view.rs` | M

### A5: 三件套 + 人工验收
- **Acceptance**: 三件套绿; 手动验证三键(最小/最大/还原/关闭)、标题拖拽移窗、双击标题最大化。
- **Verify**: `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`; 人工。
- **Files**: — | S

### Checkpoint: 模块验收 (A5 后)
- [ ] 三件套绿; 三键/拖拽/输入/IME/快捷键全部人工过; 进 review。

## Risks and Mitigations
| 风险 | 影响 | 缓解 |
|---|---|---|
| Bar 进槽后焦点/IME 链路断 | 高 | 沿用框架 titlebar-embed 的 child-node/转发; A4 人工回归 |
| 标题栏高与 Bar 高不匹配(布局跳动) | 中 | 标题栏高 = max(自身, 槽高); Bar Hidden 时无输入列 |
| 三窗键最大化态 bind 未维护 | 中 | LogApp 加 `maximized` 字段, sync 到 TitleBar(框架 Handler 回调) |

## Open Questions
- `maximized` 应用态从哪读(bind_maximized 回调)? → 框架 Handler `maximize_window` 已调
  `app.maximized_changed(bool)`, 产品 App 实现该 hook 存 `maximized` 字段, plan 按此。
- 标题串最终文案(发布命名时定) → 本期用现状 `丹青日志 POC [JSONL] — <file>`。
