//! @author 十四叔
//! @date 2026/09/06
//!
//! 更新检查接线: 薄封装 `danqing::update`, 供产品侧初始化时调用。

use danqing::update::{UpdateHint, UpdateSpec, current_hint, perform_action, spawn_check};

/// 产品身份: 仓库 / UA / 发布页 / 当前版本 (编译期常量)。
const SPEC: UpdateSpec = UpdateSpec {
    repo: "14uncle/danqing-log",
    user_agent: "danqing-log",
    releases_page: "https://github.com/14uncle/danqing-log/releases/latest",
    current_version: env!("CARGO_PKG_VERSION"),
};

/// 启动时一次性调用: 读缓存 → 后台线程检查 → 发布到全局。
pub fn init() {
    spawn_check(SPEC);
}

/// 当前更新提示 (每帧查询; 无新版/无缓存 → None)。
pub fn hint() -> Option<UpdateHint> {
    current_hint(&SPEC)
}

/// 执行更新动作 (GitHub 轨: 跳发布页)。
pub fn go_download() {
    perform_action(&SPEC);
}
