# Spec: 标题栏窗件 (app-chrome)

> @author 十四叔 · @date 2026/09/06
> 前置: interview-me + 用户裁决 A(框架侧 TitleBar 嵌槽) + 「过滤+搜索都进 titlebar」。
> 依赖框架 `danqing` 的 `titlebar-embed` 槽(`../danqing/docs/specs/SPEC-titlebar-embed.md`)。
> 把 danqing-log 的自绘 24px 表头(`chrome_top`)替换为框架 TitleBar, 嵌入现有 `Bar` 双上下文输入。

## Objective

现状(2026-09-06 复核修正): 窗口**顶部无任何标题条** —— winit 窗口 `decorations=false`(框架全局),
无自绘标题; 标题 `丹青日志 POC [JSONL] — <file>` 只是 **winit `WindowConfig.title`**(`main.rs:817`),
仅任务栏/Alt-Tab 可见。`view.rs` 的 `HEADER_H`/`chrome_top` 是**表格模式列头**(24px), 非窗口标题。
顶部唯一内容 = `Column` 的 `Bar` 行(过滤/搜索双上下文输入), `LogView.fill` 在其下。

目标: **新增**框架 `TitleBar` 置顶, 一条 = `logo + 标题 + 内嵌过滤/搜索 Bar + 最小/最大/关闭三键`。
窗口能最小化/最大化/还原/关闭(框架 `TitleBar` 三键 + `WindowAction`), 输入框进标题区(klogg 式搜索在顶),
标题搬进 TitleBar(否则任务栏外不可见)。`Bar` 从 `Column` 的 sibling 挪进 TitleBar 的 embed 槽。
`LogView` 表格模式列头(`HEADER_H`)与此无关, **保留不动**。

- 标题 = 文件名 + 模式(如 `demo.jsonl · JSONL 表格`), 或维持现状 `丹青日志 POC [JSONL] — <文件>`。
- 三键: `TitleBar.on_close/on_minimize/on_maximize` → `WindowAction`(`bind_maximized` 读应用 `maximized`)。

## Tech Stack

- 不改依赖(框架已含 TitleBar/三键)。
- 复用 `Bar`(现双输入 + `ActiveBar` 上下文切换 + clear-rev 转发), 只需把它**从 `Column` 的 sibling
  挪进 TitleBar 的 embed 槽**。

## Commands

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --release -- <日志>   # 人工: 三键 + 输入在标题区 + 拖拽不冲突
```

## Project Structure

```
src/view.rs   ← 删 chrome_top/HEADER_H 自绘表头; LogView 顶部不再画标题; 表格/列表区从 TitleBar 下起
src/main.rs   ← view() = Column[TitleBar(embed Bar), LogView.fill]; 接 on_close/on_minimize/on_maximize
                → WindowAction; bind_maximized; 标题串组装(文件名+模式)
src/view.rs (Bar) ← 保留双输入/上下文/转发, 位置字段改为「嵌入 TitleBar 槽」几何
```

## Code Style

沿用现有中文文档 + `Bar` 不重写(只改宿住位置)。关键翻转:

- `main.rs`: `TitleBar::themed(...).logo_kind(LogoKind::Log).on_close(|| WindowAction::Close)`
  `.on_minimize(|| WindowAction::Minimize).on_maximize(|| WindowAction::MaximizeOrRestore)
  .bind_maximized(|s| s.<maximized>).embed(bar)`。
- 去掉单独 `Bar` sibling + 自绘表头; LogView `chrome_top` 归零, 表格/列表顶到 TitleBar 下缘。
- 三窗键的键盘/焦点路由由框架 TitleBar 处理; 输入框焦点/IME/命中仍走 Bar 既有转发。
- 标题串与模式指示(JSONL/表格/行数可后续加, 本期只保基础 + 文件/模式)。

## Testing Strategy

- 单测: 引擎不变(不涉)。UI 交互走人工验收清单: 三键可用(最小化/最大化/还原/关闭);
  输入框在标题区, 点击落焦、中文 IME 候选窗贴框、粘贴可用; 在标题非按钮区拖拽可移动窗口、
  双击标题最大化; `Ctrl+T`/`/`/`Ctrl+F` 模式切换在输入聚焦时仍生效(回归 v1 行为)。
- 三件套全绿(引擎测试不改)。

## Boundaries

- **Always**: 复用框架 TitleBar(不复制按钮逻辑); 保留 Bar 双上下文(过滤+搜索);
  三窗键接 `WindowAction`; 标题串含文件名/模式。
- **Ask first**: 标题区再放额外控件(行数/文件大小徽标等, 本期只标题+输入+键);
  改变三键布局/风格(走框架 `TitleBarStyle`, 不自绘); 底部状态栏改动(见 settings spec, 本期不动)。
- **Never**: 复制窗口按钮几何/消息逻辑到产品; 用户未指示不 commit/push。

## Success Criteria

- [ ] 顶部一条: logo + 标题 + 过滤/搜索输入 + 三键; 无独立 Bar 行、无自绘表头。
- [ ] 最小化/最大化(含还原图标)/关闭 全可用; 最大化态还原图标正确(bind_maximized)。
- [ ] 输入在标题区: 点击落焦、中文 IME、粘贴、回车应用、Esc 清除 全正常(回归 v1)。
- [ ] 标题区拖拽移窗、双击最大化不误触输入/按钮; `Ctrl+T` 等模式切换聚焦输入时仍生效。
- [ ] 三件套绿。人工验收「通过」。

## Open Questions

- 标题串最终文案(`丹青日志 POC [JSONL] — <file>` vs 精简) → 发布命名定稿时一并定, 本期用现状文案。
- 三键悬停/关闭按钮 hover 若要贴合深色主题, 走框架 `TitleBar::bind_theme`, 本期按默认即可。
