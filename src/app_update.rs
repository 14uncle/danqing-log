//! @author 十四叔
//! @date 2026/09/06
//!
//! 更新检查接线: 薄封装 `danqing::update`, 供产品侧初始化时调用。
//!
//! **商店版 (MSIX) 整条关掉** —— 见 [`enabled`]。

use danqing::update::{UpdateHint, UpdateSpec, current_hint, perform_action, spawn_check};

/// 产品身份: 仓库 / UA / 发布页 / 当前版本 (编译期常量)。
const SPEC: UpdateSpec = UpdateSpec {
    repo: "14uncle/danqing-log",
    user_agent: "danqing-log",
    releases_page: "https://github.com/14uncle/danqing-log/releases/latest",
    current_version: env!("CARGO_PKG_VERSION"),
};

/// 本渠道要不要检查更新 —— **商店版不要** (2026-09-13 定)。
///
/// 三个理由, 一个比一个实在:
///
/// 1. **商店代管更新**: MSIX 应用由平台推送新版本, 自己再查一遍是多余的,
///    两边的节奏还可能不一致。
/// 2. **别把用户导向站外**: 商店版提示「有新版本」时点开跳 GitHub, 既是绕过商店,
///    也可能让他下成**便携版** —— 于是同一台机器上出现两份安装、两套配置, 更乱。
/// 3. **它曾经是这个应用唯一的联网行为**。关掉之后商店版**零网络请求** ——
///    隐私政策可以干净地写成「不收集、不传输、不联网」, 而不是先声明一条
///    「会向 GitHub 发一次请求」。`runFullTrust` 本来就已经被商店强制标成
///    「收集个人信息 = 是」, 能少一条要解释的就少一条。
///
/// 判据是**运行时**的包标识 (`danqing::platform::is_packaged`), 不是编译期开关 ——
/// 商店版与便携版是同一个二进制, 加 feature 分区会带来「手上这个包是哪个构建」
/// 的混淆。包标识本来就是运行时事实, 就按运行时问。
pub fn enabled() -> bool {
    !danqing::platform::is_packaged()
}

/// 启动时一次性调用: 读缓存 → 后台线程检查 → 发布到全局。
pub fn init() {
    if !enabled() {
        return;
    }
    spawn_check(SPEC);
}

/// 当前更新提示 (每帧查询; 无新版/无缓存 → None)。
///
/// 商店版恒为 `None`: 除了不发起检查, 还要挡住**便携版留下的缓存**被误用
/// (正常情况下两渠道的配置目录互不相通, 但这条不靠那个前提成立)。
pub fn hint() -> Option<UpdateHint> {
    if !enabled() {
        return None;
    }
    current_hint(&SPEC)
}

/// 执行更新动作 (GitHub 轨: 跳发布页)。
pub fn go_download() {
    perform_action(&SPEC);
}

#[cfg(test)]
mod tests {
    use super::enabled;

    /// 裸二进制 (`cargo test` 跑的这个) 必然不是打包环境 → 更新检查必须**开着**。
    ///
    /// 这条挡的是本模块最大的风险: 判据**取反**。把便携版误判成商店版, 后果是
    /// **静默**不再检查更新 —— 不报错、不崩, 只是从此收不到新版本。
    #[test]
    fn updates_are_enabled_in_a_plain_binary() {
        assert!(enabled(), "裸 exe 不该被当成商店版而关掉更新检查");
    }
}
