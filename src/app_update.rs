//! @author 十四叔
//! @date 2026/09/06
//!
//! 更新检查接线: **双轨分派** (2026-09-22 起完整双轨, 翻案 09-13「商店版整条关掉」,
//! 翻案记录见 SPEC-update-badge D5)。
//!
//! - **GitHub 轨 (便携版)**: 薄封装 `danqing::update`, 查 `releases/latest`;
//! - **商店轨 (MSIX)**: StoreContext 查/拉更新, 照 `danqing-pomodoro/src/update.rs`
//!   成稿移植 (它 2026-09-05 就双轨齐备)。
//!
//! 分轨判据是**运行时**包标识 (`danqing::platform::is_packaged`), 不是编译期开关
//! ——商店版与便携版是同一个二进制 (09-13 架构决定保留, 只翻「关掉」那半);
//! pomodoro 的 `feature = "store"` 是它三版本构建的历史形态, 不学。
//!
//! 约定不变: 任何一步解析/读写/网络/商店失败都按「无新版」静默处理, 不打扰用户。

use danqing::update::{UpdateHint, UpdateSpec, current_hint, spawn_check};

/// 商店轨按钮文案: 框架只产 GitHub 轨的「前往下载」, 商店轨是应用内「更新」。
const STORE_ACTION: &str = "更新";

/// GitHub 轨发布页 URL —— **D2 不变量的编译期定锚** (远端 JSON 永不作定位符):
/// `/releases/latest` 由 GitHub 302 到最新 release 详情页, tag 不拼进 URL。
/// [`spec`] 与 [`action_target`] 共用这一份, 执行面与动作模型面同源。
const GITHUB_RELEASES_PAGE: &str = "https://github.com/14uncle/danqing-log/releases/latest";

/// 更新动作目标 (纯模型, 双臂锁死 —— 与 [`track_of`] 同族, 测试钉两臂):
/// GitHub 臂 = 开发布页 (编译期常量 URL); 商店臂 = 拉系统更新对话框 (无 URL)。
/// 把行为收敛成可枚举的模型, 「臂对调 / 臂体改开别的 URL」在测试里当场红。
enum ActionTarget {
    /// 开发布页 (URL 必须来自 [`GITHUB_RELEASES_PAGE`], 测试钉)。
    ReleasesPage(&'static str),
    /// 拉起系统商店更新对话框 (无 URL —— 商店轨不引站外, D5 理由②)。
    StoreUpdate,
}

/// 动作目标**唯一模型** (纯函数): 按轨分派, 双臂 [`dispatch_polarity_is_never_inverted`]
/// 锁极性、本模型锁目标 —— 合起来「点下去会发生什么」全在锁内。
fn action_target(track: Track) -> ActionTarget {
    match track {
        Track::Store => ActionTarget::StoreUpdate,
        Track::GitHub => ActionTarget::ReleasesPage(GITHUB_RELEASES_PAGE),
    }
}

/// 更新轨道 (D6): 分轨判定的唯一模型。
enum Track {
    /// GitHub 轨 (便携版): HTTP 查 `releases/latest`, 动作跳发布页。
    GitHub,
    /// 商店轨 (MSIX): StoreContext 查/拉更新, **不发 HTTP**。
    Store,
}

/// 轨道判定**唯一模型** (纯函数, 测试双臂锁死): 打包 → 商店轨; 裸 exe → GitHub 轨。
///
/// **判据取反是本模块最大的风险** (D6): 取反 = 商店版走 GitHub 轨发 HTTP =
/// 隐私政策的「商店版从不发起 HTTP」变假话。单点模型 + [`github_http_allowed`] 双保险。
fn track_of(packaged: bool) -> Track {
    if packaged {
        Track::Store
    } else {
        Track::GitHub
    }
}

/// 进程内包标识是常量 ——运行时判定收敛在**这一处读取**, 各出口只 `match track()`。
fn track() -> Track {
    track_of(danqing::platform::is_packaged())
}

/// GitHub 轨的隐私边界**双保险** (D6): 打包环境任何情况下不发 HTTP、不跳站外。
///
/// 与 [`track_of`] 刻意**独立判定** (直读包标识, 不经分派模型) —— 即便 `track_of`
/// 被改错、或 `match` 两臂被对调, GitHub 臂体内的这道闸也拦得住; 隐私主张
/// 「商店版从不发起 HTTP」被破需要**两次独立错误**。测试锁双臂。
fn github_http_allowed(packaged: bool) -> bool {
    !packaged
}

/// 注入框架的产品身份 (每次调用现合成: 商店轨的版本号是运行时事实, 不能进 `const`)。
fn spec() -> UpdateSpec {
    UpdateSpec {
        repo: "14uncle/danqing-log",
        user_agent: "danqing-log",
        releases_page: GITHUB_RELEASES_PAGE,
        current_version: current_version(),
    }
}

/// 当前版本号: GitHub 轨取编译期包版本, 商店轨取 MSIX 包身份版本
/// (build_msix.ps1 的 -Version 独立于 Cargo.toml, 二进制自报不可信;
/// 进程级 OnceLock 缓存在 store 模块内, 见 [`store::package_version`])。
pub fn current_version() -> &'static str {
    match track() {
        Track::Store => store::package_version(),
        Track::GitHub => env!("CARGO_PKG_VERSION"),
    }
}

/// 动作按钮文案**单点** (按轨, 进程常量): GitHub「前往下载」/ 商店「更新」。
/// [`override_action`] 与版本行按钮构造共用这一份, 防两处文案漂移。
fn action_label_of(track: Track) -> &'static str {
    match track {
        Track::Store => STORE_ACTION,
        // 框架原产文案是 GitHub 臂的真单点 (应用侧不另抄字面量; 测试钉「前往下载」防漂)。
        Track::GitHub => danqing::update::update_action_text(),
    }
}

/// 当前轨道的动作按钮文案 (版本行 `Link` 构造时取; 轨是进程常量, 构造期定案)。
pub fn action_label() -> &'static str {
    action_label_of(track())
}

/// 动作按钮文案覆写 (纯函数): 两轨都从 [`action_label_of`] 单点取值 ——
/// GitHub 臂覆写为同值 (与框架 `update_hint` 原产「前往下载」一致, 测试钉),
/// 商店臂「前往下载」→「更新」。
fn override_action(mut hint: UpdateHint, track: Track) -> UpdateHint {
    hint.action = action_label_of(track);
    hint
}

/// 启动时一次性调用: 读缓存 → 后台线程检查 → 发布到全局。按轨道分派运输。
pub fn init() {
    match track() {
        Track::Store => spawn_check_store(),
        Track::GitHub => spawn_check_github(),
    }
}

/// 当前更新提示 (每帧查询; 无新版/无缓存 → None)。两轨通用, 商店轨覆写按钮文案。
pub fn hint() -> Option<UpdateHint> {
    let hint = current_hint(&spec())?;
    Some(override_action(hint, track()))
}

/// 执行更新动作 (设置卡「版本」行按钮; 行为按轨道分派)。
///
/// 原名 `go_download` —— 那是 GitHub 轨语义; 双轨后名字随语义改。
/// 分派走 [`action_target`] 纯模型 (目标与极性都在锁内), 臂体保持双保险。
pub fn perform_action() {
    match action_target(track()) {
        ActionTarget::StoreUpdate => store::request_update(),
        ActionTarget::ReleasesPage(url) => {
            // 模型面 URL 是执行面的**指认** (打开仍走框架 `perform_action(&spec())`) ——
            // 消费它做同源自证, 两面若漂移 debug 构建当场炸, 别等测试才发现。
            debug_assert_eq!(url, GITHUB_RELEASES_PAGE, "动作模型与执行面 URL 漂移");
            perform_action_github();
        }
    }
}

/// GitHub 轨启动检查。臂体自带隐私双保险 (D6) —— 打包环境不该走到这里。
fn spawn_check_github() {
    if !github_http_allowed(danqing::platform::is_packaged()) {
        log::warn!("打包环境拒绝 GitHub 轨更新检查 (隐私边界双保险, 不应到达)");
        return;
    }
    spawn_check(spec());
}

/// GitHub 轨动作: 跳发布页手动下载。臂体自带双保险 —— 商店用户不引站外 (D5 理由②)。
fn perform_action_github() {
    if !github_http_allowed(danqing::platform::is_packaged()) {
        log::warn!("打包环境拒绝 GitHub 轨跳转 (隐私边界双保险, 不应到达)");
        return;
    }
    danqing::update::perform_action(&spec());
}

/// 商店轨启动检查: 与框架 `spawn_check` 同构 (缓存闸门 → 后台线程 → 发布),
/// 唯一差异是运输换成 StoreContext + 缓存分文件。
fn spawn_check_store() {
    // 缓存仅对写入它的版本有效: 商店轨 UnknownVersion 无版本号, 无法经 is_newer
    // 自纠, 换版 (含商店自动更新) 后旧缓存必须作废重查 (框架 usable_cache 同款闸门)。
    let cached = store_cache_path()
        .and_then(|p| danqing::update::load_cache_from(&p))
        .filter(|c| c.checked_version == current_version());
    let fresh = cached.as_ref().is_some_and(|c| c.is_fresh(now_secs()));
    danqing::update::publish(cached);
    if fresh {
        return;
    }
    // 后台线程不 join: 进程退出即终止, 无泄漏。
    std::thread::spawn(|| match store::check_update() {
        Some(status) => {
            let cache = danqing::update::CheckCache {
                checked_at_secs: now_secs(),
                checked_version: current_version().to_string(),
                status,
            };
            match store_cache_path() {
                Some(path) => {
                    if let Err(err) = danqing::update::save_cache_to(&path, &cache) {
                        log::warn!("更新检查缓存写入失败: {err}");
                    }
                }
                None => log::warn!("配置目录不可得, 更新检查结果不落盘"),
            }
            danqing::update::publish(Some(cache));
        }
        None => log::warn!("更新检查失败, 本次会话不再重试"),
    });
}

/// 商店轨缓存文件名 (纯函数): 与框架 `cache_path` **同规则**从 repo slug 派生,
/// 不手抄 —— 仓库改名时两处一起动, 防「手工副本漂移」(评审 Nit)。
fn store_cache_file_name(repo: &str) -> String {
    format!("update-check-{}-msix.json", repo.replace('/', "-"))
}

/// 商店轨缓存路径: 与便携轨**分文件**防串味 —— 运行时分轨的单二进制在同机双装
/// (便携 + 商店) 时共用 `%APPDATA%\danqing\`, 共用一份缓存会互相污染 (便携的
/// `KnownVersion` 让商店版显示带版本号的假提示, 反向把 `UnknownVersion` 喂给便携版)。
/// pomodoro 编译期分轨天然不串, 这是单二进制特有的坑 (D6 衍生设计 #2)。
fn store_cache_path() -> Option<std::path::PathBuf> {
    let s = spec();
    danqing::update::cache_path(&s).map(|p| p.with_file_name(store_cache_file_name(s.repo)))
}

/// 当前 wall-clock 秒 (缓存新鲜度基准; 失败回退 0 视为过期)。框架同款语义。
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// 微软商店: 包身份版本读取 + 更新查/拉 (照 danqing-pomodoro `update.rs` 成稿移植)
// ---------------------------------------------------------------------------

mod store {
    use danqing::update::UpdateStatus;
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::Services::Store::{StoreContext, StorePackageUpdate};
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

    /// 更新拉起在途标志: 系统对话框存活期间忽略重复点击 (防重入, IAP 购买同款纪律)。
    static UPDATE_REQUESTING: AtomicBool = AtomicBool::new(false);

    /// 防重入标志的复位守卫: 即使 [`request_update_inner`] 中途 panic (debug 展开),
    /// 标志也不许粘死 —— 粘死 = 「更新」按钮这一会话都点不动 (评审 Nit)。
    /// release 是 `panic = "abort"`, 进程直接亡, 本守卫只为 debug/unwind 兜底。
    struct ResetRequesting;

    impl Drop for ResetRequesting {
        fn drop(&mut self) {
            UPDATE_REQUESTING.store(false, Ordering::Release);
        }
    }

    /// 读 MSIX 包身份版本 (Major.Minor.Build, 丢弃 build_msix.ps1 恒写 0 的 Revision);
    /// 无包身份 (`cargo run` 直跑) 回退编译期版本 + warn。
    ///
    /// 进程级 OnceLock 缓存: 版本号是进程生命周期常量 (商店轨更新必须重启进程才生效),
    /// 而本函数经 [`super::current_version`] 每帧被 UI 绑定多次调用。
    pub fn package_version() -> &'static str {
        static PACKAGE_VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        PACKAGE_VERSION.get_or_init(package_version_uncached)
    }

    /// 实际读取 (每进程一次, 见 [`package_version`])。
    fn package_version_uncached() -> String {
        let read = (|| -> windows::core::Result<String> {
            let pkg = windows::ApplicationModel::Package::Current()?;
            let version = pkg.Id()?.Version()?;
            // 丢弃 Revision: build_msix.ps1 恒写 0
            Ok(format!(
                "{}.{}.{}",
                version.Major, version.Minor, version.Build
            ))
        })();
        match read {
            Ok(version) => version,
            Err(err) => {
                log::warn!("无 MSIX 包身份, 版本号回退编译期值: {err}");
                env!("CARGO_PKG_VERSION").to_string()
            }
        }
    }

    /// 查商店应用更新: 有待装更新 → `UnknownVersion`; 无更新 → `UpToDate`;
    /// 非 MSIX 环境/任何 API 失败 → None 静默 (家族约定)。
    ///
    /// 注意 `StorePackageUpdate.Package` 是「被更新的当前包」, 新版本号不可得,
    /// 所以商店轨只有有无、没有版本 (pomodoro 2026-09-05 侧载实测)。
    pub fn check_update() -> Option<UpdateStatus> {
        if !danqing::platform::is_packaged() {
            return None;
        }
        match check_update_inner() {
            Ok(status) => Some(status),
            Err(err) => {
                log::warn!("商店更新检查失败: {err}");
                None
            }
        }
    }

    /// 同步执行商店更新查询 (调用方负责线程)。
    fn check_update_inner() -> windows::core::Result<UpdateStatus> {
        // WinRT 异步调用需 COM 单元 (同 store_license 纪律: 成败都继续,
        // 线程退出不配对 RoUninitialize)。
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let context = StoreContext::GetDefault()?;
        let updates = context.GetAppAndOptionalStorePackageUpdatesAsync()?.get()?;
        let count = updates.Size()?;
        log::info!("商店更新查询: 待装更新 {count} 个");
        Ok(if count == 0 {
            UpdateStatus::UpToDate
        } else {
            UpdateStatus::UnknownVersion
        })
    }

    /// 拉起商店系统更新 UI (后台线程: 同步等待会阻塞 UI)。
    /// 系统对话框接管进度/安装/重启提示; 任何失败仅一行 warn。
    pub fn request_update() {
        if !danqing::platform::is_packaged() {
            return; // 双保险: 非 MSIX 不产生提示, 正常不可达
        }
        // 防重入: 系统对话框在途期间忽略重复点击。
        if UPDATE_REQUESTING.swap(true, Ordering::AcqRel) {
            return;
        }
        std::thread::spawn(|| {
            let _reset = ResetRequesting;
            if let Err(err) = request_update_inner() {
                log::warn!("拉起商店更新失败: {err}");
            }
        });
    }

    /// 同步执行商店更新拉起 (调用方负责线程)。
    fn request_update_inner() -> windows::core::Result<()> {
        use windows::Win32::UI::Shell::IInitializeWithWindow;
        use windows::core::Interface;

        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let context = StoreContext::GetDefault()?;
        // 显示 UI 的 Store 调用必须先挂主窗口属主 (IInitializeWithWindow 约定,
        // store_license 购买链路同款)。
        let Some(hwnd) = crate::store_license::find_main_window() else {
            log::warn!("主窗口未找到, 无法挂更新对话框属主");
            return Ok(());
        };
        unsafe { context.cast::<IInitializeWithWindow>()?.Initialize(hwnd)? };
        let updates = context.GetAppAndOptionalStorePackageUpdatesAsync()?.get()?;
        // WinRT 引用类型的 IIterable<T> 由 Vec<Option<T>> 转换
        // (T::Default = Option<T>, 与 HSTRING 等值类型的直转不同)。
        let updates: Vec<Option<StorePackageUpdate>> = updates.into_iter().map(Some).collect();
        if updates.is_empty() {
            // 用户点击与检查之间更新已被安装/撤下: 无事发生
            return Ok(());
        }
        let updates: windows_collections::IIterable<StorePackageUpdate> = updates.into();
        context
            .RequestDownloadAndInstallStorePackageUpdatesAsync(&updates)?
            .get()?;
        log::info!("商店更新流程已结束 (系统对话框接管后续)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use danqing::update::{CheckCache, UpdateStatus, update_hint};

    /// D6 回归锁: 分轨极性**双臂锁死** —— [`track_of`] 与 [`github_http_allowed`]
    /// 两个纯模型一起锁: 前者管派轨, 后者是 GitHub 臂体内的隐私双保险 (直读包标识,
    /// 独立于分派)。任何一个取反 = 商店版发 HTTP = 隐私主张变假, 此锁必须红。
    ///
    /// 评审实锤过旧锁的缺口: 只锁谓词恒等函数时, 在 `init()` 的 if 上取反
    /// 测试全绿、商店版照样发 HTTP —— 故臂体另有 [`github_http_allowed`] 闸,
    /// **两次独立错误**才能破隐私主张 (PoC: 把 `track_of` 或 `match` 臂对调,
    /// 打包环境仍在 `spawn_check_github` 被闸下)。
    /// 旧 `updates_are_enabled_in_a_plain_binary` 的「取反 = 静默关检查」语义并入本锁。
    #[test]
    fn dispatch_polarity_is_never_inverted() {
        assert!(
            matches!(track_of(true), Track::Store),
            "打包 (MSIX) 必须走商店轨"
        );
        assert!(
            matches!(track_of(false), Track::GitHub),
            "裸 exe (便携) 必须走 GitHub 轨"
        );
        assert!(github_http_allowed(false), "裸 exe 允许 GitHub 轨 HTTP");
        assert!(
            !github_http_allowed(true),
            "打包环境任何情况下不发 HTTP (臂体双保险)"
        );
    }

    /// D2 双臂动作目标锁 (SPEC-update-hint-ui §5 的「动作分派双臂」在模型层兑现):
    /// GitHub 臂 = 开**编译期常量**发布页 (URL 恒为 `/releases/latest`, 远端 tag 永不
    /// 拼入 —— `releases_page: &'static str` 类型本身也拼不出动态串); 商店臂 = 拉系统
    /// 更新对话框 (无 URL, 不引站外)。执行面同源: [`spec`] 与 [`action_target`] 共用
    /// [`GITHUB_RELEASES_PAGE`] (三等式钉死, 臂体换 URL 源/臂对调须精确红)。
    #[test]
    fn action_target_dual_arm_never_splices_remote_tag() {
        match action_target(Track::GitHub) {
            ActionTarget::ReleasesPage(url) => assert_eq!(
                url, "https://github.com/14uncle/danqing-log/releases/latest",
                "GitHub 臂必须开编译期常量发布页 (D2)"
            ),
            ActionTarget::StoreUpdate => panic!("GitHub 臂不许走商店更新"),
        }
        assert!(
            matches!(action_target(Track::Store), ActionTarget::StoreUpdate),
            "商店臂必须拉系统更新对话框 (无 URL)"
        );
        assert_eq!(
            spec().releases_page,
            GITHUB_RELEASES_PAGE,
            "UpdateSpec 与动作模型同源 (执行面/模型面不许各持一份 URL)"
        );
    }

    /// 商店轨按钮文案覆写为「更新」(框架只产「前往下载」); GitHub 轨保持原文案;
    /// `UpToDate` → None (「无新版零痕迹」的纯路径, SPEC §5 承诺的臂)。
    /// 纯函数形态两臂都测 (不碰全局 publish —— 全局静态测试并行互踩)。
    #[test]
    fn store_track_hint_overrides_action_to_update() {
        // UnknownVersion 是商店轨的产物 (拿不到新版号): status 无版本号。
        let cache = CheckCache {
            checked_at_secs: 1,
            checked_version: "1.0.0".to_string(),
            status: UpdateStatus::UnknownVersion,
        };
        let hint = update_hint("1.0.0", Some(&cache)).expect("应有提示");
        assert_eq!(hint.status, "有新版本", "商店轨拿不到新版号, 不显版本");
        assert_eq!(
            override_action(hint, Track::Store).action,
            action_label_of(Track::Store),
            "商店轨按钮是应用内「更新」, 不是「前往下载」"
        );

        // GitHub 轨: KnownVersion 带版本号, 动作文案保持框架原样。
        let cache = CheckCache {
            checked_at_secs: 1,
            checked_version: "1.0.0".to_string(),
            status: UpdateStatus::KnownVersion("v9.9.9".to_string()),
        };
        let hint = update_hint("1.0.0", Some(&cache)).expect("应有提示");
        assert_eq!(hint.status, "有新版本 v9.9.9");
        // 文案单点: 覆写值取 action_label_of; GitHub 臂与框架 update_hint 原产
        // 「前往下载」一致 (钉两处漂移)。
        assert_eq!(
            override_action(hint, Track::GitHub).action,
            action_label_of(Track::GitHub)
        );
        assert_eq!(action_label_of(Track::GitHub), "前往下载");

        // UpToDate → None: 无新版零痕迹 (D3)。
        let cache = CheckCache {
            checked_at_secs: 1,
            checked_version: "1.0.0".to_string(),
            status: UpdateStatus::UpToDate,
        };
        assert!(
            update_hint("1.0.0", Some(&cache)).is_none(),
            "UpToDate 不许产生任何提示"
        );
    }

    /// 缓存文件名从 repo slug 派生 (与框架 `cache_path` 同规则), 防手工副本漂移。
    #[test]
    fn store_cache_file_name_derives_from_repo_slug() {
        assert_eq!(
            store_cache_file_name("14uncle/danqing-log"),
            "update-check-14uncle-danqing-log-msix.json"
        );
    }
}
