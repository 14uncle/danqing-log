//! @author 十四叔
//! @date 2026/09/10

//! 系统托盘右键菜单 (设置 + 退出)。
//!
//! 菜单项 ID 用数字字符串 ("10"/"11"), 框架解析为 u8 后转交 `app.tray_action`。
//! 不复用框架预定义 ID (1/2/3), 避免与番茄钟语义冲突。

use danqing::tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};

/// 托盘菜单项 ID (产品自定义, 不与框架 tray_action_ids 冲突)。
pub const ACTION_SETTINGS: u8 = 10;
pub const ACTION_QUIT: u8 = 11;

/// 构建托盘菜单: 设置 + 分隔符 + 退出。
pub fn build_menu() -> Menu {
    let menu = Menu::new();

    let item_settings = MenuItem::with_id(ACTION_SETTINGS.to_string(), "设置", true, None);
    let separator = PredefinedMenuItem::separator();
    let item_quit = MenuItem::with_id(ACTION_QUIT.to_string(), "退出", true, None);

    if let Err(err) = menu.append(&item_settings) {
        log::warn!("添加托盘菜单项设置失败: {err}");
    }
    if let Err(err) = menu.append(&separator) {
        log::warn!("添加托盘分隔符失败: {err}");
    }
    if let Err(err) = menu.append(&item_quit) {
        log::warn!("添加托盘菜单项退出失败: {err}");
    }

    menu
}
