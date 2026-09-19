//! @author 十四叔
//! @date 2026/09/19
//!
//! 商店版授权 (SPEC-v1x-licensing T5): WinRT broker 授权查询 + 购买拉起。
//!
//! **不可单测区** —— broker 是进程外系统服务; 可测的「快照 → 授权状态」
//! 映射在 lib `danqing_log::license::map_store_snapshot` (四态测试齐全),
//! 本模块只负责把 broker 的返回翻成快照。
//! 第一手参考: `danqing-pomodoro/src/license.rs` (2026-09-04 实测成稿)。
//! 纪律: 错误一律 fail-open (免费层 / 失败态), 不崩不 panic ——
//! 授权故障永远不能挡住应用本体。
//! 两条有意为之 (评审时显性化, 2026-09-19): ① `RoInitialize` 不配对
//! `RoUninitialize` —— worker 线程即弃, 单元引用计数留到进程结束
//! (调用次数个位数, pomodoro 成稿同款); ② 不带 `catch_unwind` ——
//! 本仓 release 是 `panic = "abort"`, 它只在 dev 生效, 本模块全程
//! `Result` 纪律才是真正的兜底。

use windows::Services::Store::{StoreContext, StoreProduct, StorePurchaseStatus};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::Win32::UI::Shell::IInitializeWithWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId,
    IsWindowVisible,
};
use windows::core::{BOOL, Interface};

use danqing_log::license::StoreSnapshot;

/// 付费层 add-on 的 Offer ID —— **占位**; 用户在 Partner Center 创建 add-on 时
/// 必须与此值一致 (pomodoro 同款约定: `danqing-pomodoro-full` / 9P4B2MPB8HNN)。
pub(crate) const FULL_VERSION_OFFER_ID: &str = "danqing-log-full";

/// 购买结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PurchaseOutcome {
    /// 购买成功或此前已购。
    Purchased,
    /// 用户取消 (StorePurchaseStatus::NotPurchased)。
    Cancelled,
    /// 网络/服务/API 错误, 可重试。
    Failed,
}

/// WinRT DateTime (1601 纪元, 100ns  tick) → Unix epoch 秒。
fn datetime_to_epoch(ticks: i64) -> u64 {
    let secs = ticks / 10_000_000 - 11_644_473_600;
    u64::try_from(secs).unwrap_or(0)
}

/// 查询授权快照 (阻塞; 调用方负责放工作线程 —— broker 调用可能耗时)。
pub(crate) fn query_snapshot() -> StoreSnapshot {
    const NONE: StoreSnapshot = StoreSnapshot {
        addon_active: false,
        addon_expires_epoch: None,
    };
    // 非打包环境 (开发运行/便携版): 商店 API 不可用, 直接免费态快照
    if !danqing::platform::is_packaged() {
        return NONE;
    }
    // WinRT 异步调用需 COM 单元; 同类型重复初始化返回 S_FALSE (Ok),
    // 仅已有 STA 时报 RO_E_CHANGE_MODE —— 成败都继续, 失败时后续调用自会报错。
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let context = match StoreContext::GetDefault() {
        Ok(ctx) => ctx,
        Err(e) => {
            log::warn!("StoreContext::GetDefault failed: {e}");
            return NONE;
        }
    };
    // 查 add-on 级内购许可证。
    // pomodoro 实测坑: StoreAppLicense::IsActive 是**应用级**许可证,
    // 免费上架时所有安装者都为 true, 拿它判断内购永远为真 ——
    // **必须遍历 AddOnLicenses** 匹配我们的 Offer ID 且 IsActive。
    let add_ons = context
        .GetAppLicenseAsync()
        .and_then(|op| op.get())
        .and_then(|license| license.AddOnLicenses());
    let add_ons = match add_ons {
        Ok(m) => m,
        Err(e) => {
            log::warn!("store 授权查询失败: {e}");
            return NONE;
        }
    };
    for pair in &add_ons {
        let Ok(lic) = pair.Value() else { continue };
        let token = lic
            .InAppOfferToken()
            .map(|t| t.to_string_lossy())
            .unwrap_or_default();
        if token != FULL_VERSION_OFFER_ID {
            continue;
        }
        let active = lic.IsActive().unwrap_or(false);
        let expires = lic
            .ExpirationDate()
            .ok()
            .map(|dt| datetime_to_epoch(dt.UniversalTime));
        log::info!("store add-on license: active={active} expires={expires:?}");
        return StoreSnapshot {
            addon_active: active,
            addon_expires_epoch: expires,
        };
    }
    // add-on 未购买/未创建时走到这里 —— 免费态 (上架前查不到 add-on 属预期)
    NONE
}

/// 拉起商店购买对话框 (阻塞; 调用方负责放工作线程)。
pub(crate) fn purchase_full_version() -> PurchaseOutcome {
    if !danqing::platform::is_packaged() {
        log::info!("非 MSIX 环境, 不拉商店购买");
        return PurchaseOutcome::Failed;
    }
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let context = match StoreContext::GetDefault() {
        Ok(ctx) => ctx,
        Err(e) => {
            log::warn!("StoreContext::GetDefault failed: {e}");
            return PurchaseOutcome::Failed;
        }
    };
    // 桌面进程必须把购买对话框的属主设为我们的主窗口, 否则显示 UI 的
    // 调用直接失败 (IInitializeWithWindow 约定)。
    let Some(hwnd) = find_main_window() else {
        log::warn!("main window not found, cannot own purchase dialog");
        return PurchaseOutcome::Failed;
    };
    match context.cast::<IInitializeWithWindow>() {
        Ok(init) => {
            if let Err(e) = unsafe { init.Initialize(hwnd) } {
                log::warn!("IInitializeWithWindow::Initialize failed: {e}");
                return PurchaseOutcome::Failed;
            }
        }
        Err(e) => {
            log::warn!("cast to IInitializeWithWindow failed: {e}");
            return PurchaseOutcome::Failed;
        }
    }
    // 购买对话框的文档化入参是 StoreProduct (经 Partner Center 分配的 Store ID),
    // 不是开发者自取的 offer token —— 先按 InAppOfferToken 在目录中定位。
    let Some(product) = find_full_version_product(&context) else {
        return PurchaseOutcome::Failed;
    };
    let op = match product.RequestPurchaseAsync() {
        Ok(op) => op,
        Err(e) => {
            log::warn!("RequestPurchaseAsync failed: {e}");
            return PurchaseOutcome::Failed;
        }
    };
    match op.get() {
        Ok(result) => match result.Status() {
            Ok(StorePurchaseStatus::Succeeded | StorePurchaseStatus::AlreadyPurchased) => {
                PurchaseOutcome::Purchased
            }
            Ok(StorePurchaseStatus::NotPurchased) => PurchaseOutcome::Cancelled,
            Ok(status) => {
                let ext = result.ExtendedError().map(|h| h.0).unwrap_or(0);
                log::warn!("purchase status={} ext=0x{ext:08X}", status.0);
                PurchaseOutcome::Failed
            }
            Err(e) => {
                log::warn!("purchase Status() failed: {e}");
                PurchaseOutcome::Failed
            }
        },
        Err(e) => {
            log::warn!("purchase async wait failed: {e}");
            PurchaseOutcome::Failed
        }
    }
}

/// 在商店目录中定位付费层 add-on (durable; trial add-on 同属 Durable 类)。
fn find_full_version_product(context: &StoreContext) -> Option<StoreProduct> {
    use windows::core::HSTRING;
    use windows_collections::IIterable;

    let kinds: IIterable<HSTRING> = vec![HSTRING::from("Durable")].into();
    let products = context
        .GetAssociatedStoreProductsAsync(&kinds)
        .and_then(|op| op.get())
        .and_then(|result| result.Products());
    let products = match products {
        Ok(p) => p,
        Err(e) => {
            log::warn!("GetAssociatedStoreProducts failed: {e}");
            return None;
        }
    };
    for pair in &products {
        match pair.Value() {
            Ok(product) => {
                let token = product
                    .InAppOfferToken()
                    .map(|t| t.to_string_lossy())
                    .unwrap_or_default();
                if token == FULL_VERSION_OFFER_ID {
                    return Some(product);
                }
            }
            Err(_) => continue,
        }
    }
    // add-on 未创建/未发布时走到这里 —— 上架前必须先在 Partner Center 建好
    // (硬顺序: add-on 只能在父应用发布之后提交, pomodoro 实测)
    log::warn!("付费层 add-on 不在目录中 (offer token: {FULL_VERSION_OFFER_ID})");
    None
}

/// EnumWindows 找本进程主窗口 (购买对话框属主用)。
pub(crate) fn find_main_window() -> Option<HWND> {
    struct Ctx {
        pid: u32,
        found: HWND,
    }
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
        // GetWindow(GW_OWNER): 窗口无属主 (顶层) 时原始 API 返回 NULL,
        // windows crate 把 NULL 包装成 Err —— Err 即顶层窗口 (勿用 map(is_null))。
        let top_level = unsafe { GetWindow(hwnd, GW_OWNER) }.is_err();
        if pid == ctx.pid && visible && top_level && unsafe { GetWindowTextLengthW(hwnd) } > 0 {
            ctx.found = hwnd;
            return BOOL(0); // 找到即停止枚举
        }
        BOOL(1)
    }
    let mut ctx = Ctx {
        pid: std::process::id(),
        found: HWND::default(),
    };
    let _ = unsafe { EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut Ctx as isize)) };
    if ctx.found.0.is_null() {
        None
    } else {
        Some(ctx.found)
    }
}
