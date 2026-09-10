# Spec: 系统托盘右键菜单

## Objective

为丹青日志添加系统托盘右键菜单，提供「设置」和「退出」两个入口。

## 菜单项

| ID | Label | 快捷键 | 行为 |
|---|---|---|---|
| `TOGGLE_VISIBLE` | 显示/隐藏 | Ctrl+Shift+P | 显隐窗口 |
| `QUIT` | 退出 | Ctrl+Shift+Q | 退出应用 |

> 框架预定义 `tray_action_ids::{TOGGLE_VISIBLE, QUIT}` + `shortcut_for_id`，
> 产品只需实现 `tray_menu()` + `tray_action()` 映射。

## 实现

1. `src/tray.rs` — 菜单构建 + action 映射
2. `src/main.rs` — `impl App for LogApp` 补 `tray_menu()` / `tray_action()`

## Success Criteria

- 右键托盘图标弹出菜单，两项 + 分隔符
- 点「显示/隐藏」切窗口可见性
- 点「退出」干净退出
- `cargo test` + `cargo clippy` 绿
