//! @author 十四叔
//! @date 2026/09/19
//!
//! 授权与收银台 (SPEC-v1x-licensing): 便携版 license key 离线签名校验 +
//! 付费功能门控查询。商店版 Store trial/add-on 查询在 T5 进本模块。
//!
//! 只做离线校验, 零网络请求 (D2)。私钥永不入本仓库 —— 签发工具
//! (`src/bin/keygen.rs`) 从仓库外路径读私钥, 本文件只 embed 公钥常量。
//! 公钥占位为全零时任何 key 都验不过, 等于「收银台未开业」—— 这是刻意的
//! 安全默认, 等用户生成真密钥对后回填公钥、商店开业。

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signature, VerifyingKey};

/// 便携版购买页 URL (D8): **单点常量**。`None` = 店铺未开业 —— UI 层此时
/// 隐藏「获取付费层」按钮, 不给死链接。用户开店后回填 `Some(url)`。
pub const PURCHASE_URL: Option<&str> = None;

/// key 串格式: `loglens1.<payload_b64url>.<sig_b64url>`。
const KEY_PREFIX: &str = "loglens1";
/// payload 里的产品名 —— 别的产品线 (或别家软件) 的 key 不能激活本应用。
const PRODUCT: &str = "danqing-log";
/// 当前 payload 版本。格式演进时旧版本 key 必须明确拒绝, 不猜着解。
const PAYLOAD_VERSION: u64 = 1;

/// 产品公钥 —— 2026-09-19 用户生成真密钥对后回填 (私钥在仓库外
/// `~/.danqing-log-keys/danqing-log.secret`, 永不入库)。此前为全零占位
/// = fail-closed「收银台未开业」; 回填后便携版激活通路真实可用。
pub const PRODUCT_PUBKEY: [u8; 32] = [
    137, 81, 131, 96, 243, 121, 90, 1, 90, 246, 84, 95, 150, 186, 184, 224, 173, 33, 217, 176, 248,
    24, 187, 184, 106, 149, 190, 38, 30, 164, 250, 247,
];

/// 付费档位 (D7: 同一 key 格式, seat 数不技术强制, 君子协定写明)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Personal,
    Enterprise,
}

/// 校验通过的 key 载荷。
///
/// **手写 Debug**: 邮箱走遮蔽 (与展示姿态一致) —— derive 会把完整邮箱留在
/// 格式化输出里, 将来任何一行 `log::info!("{entitlement:?}")` 都会把它写进
/// 日志文件 (安全评审 Nit)。
#[derive(Clone, PartialEq, Eq)]
pub struct Payload {
    pub tier: Tier,
    pub email: String,
    pub issued_at: String,
    pub nonce: String,
    /// 预留字段: 买断制当前永不过期, **解析但校验侧不执行** (将来策略变化时
    /// 旧 key 仍有表达能力; 别以为签个带 expires 的 key 就会自动到期)。
    pub expires: Option<String>,
}

impl std::fmt::Debug for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Payload")
            .field("tier", &self.tier)
            .field("email", &mask_email(&self.email))
            .field("issued_at", &self.issued_at)
            .field("nonce", &self.nonce)
            .field("expires", &self.expires)
            .finish()
    }
}

/// key 校验失败的三态 (设置卡许可区按态分文案, spec D8)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyError {
    /// 格式错: 前缀不对 / 不是三段 / base64 或 JSON 解不出 / 必填字段缺失 / 版本未知。
    Malformed,
    /// 签名错: 格式都对但签名验不过 (篡改 / 别的密钥签的 / 公钥占位未回填)。
    BadSignature,
    /// 产品不匹配: 签名有效, 但 payload 的 product 不是本应用。
    WrongProduct,
}

/// 解析并校验 key。顺序固定: **先验签 (对原始 payload 字节) 再解 JSON** ——
/// 篡改 payload 必须报签名错, 不能报成格式错。
/// 长度闸: 合法 key 约 300–400 字节, 4 KiB 有 10 倍余量 —— 超长输入直接
/// 格式错, 不进 base64/验签 (安全评审 Optional: 超大粘贴的线性扫描自残面)。
pub fn verify_key(key: &str, pubkey: &[u8; 32]) -> Result<Payload, KeyError> {
    const MAX_KEY_LEN: usize = 4096;
    let trimmed = key.trim();
    if trimmed.len() > MAX_KEY_LEN {
        return Err(KeyError::Malformed);
    }
    let mut parts = trimmed.split('.');
    let (Some(prefix), Some(payload_b64), Some(sig_b64), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(KeyError::Malformed);
    };
    if prefix != KEY_PREFIX {
        return Err(KeyError::Malformed);
    }
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| KeyError::Malformed)?;
    let sig_bytes: [u8; 64] = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or(KeyError::Malformed)?;

    // 先验签
    let vk = VerifyingKey::from_bytes(pubkey).map_err(|_| KeyError::BadSignature)?;
    let sig = Signature::from_bytes(&sig_bytes);
    vk.verify_strict(&payload_bytes, &sig)
        .map_err(|_| KeyError::BadSignature)?;

    // 验签通过才解 JSON
    let v: serde_json::Value =
        serde_json::from_slice(&payload_bytes).map_err(|_| KeyError::Malformed)?;
    let obj = v.as_object().ok_or(KeyError::Malformed)?;
    if obj.get("v").and_then(|x| x.as_u64()) != Some(PAYLOAD_VERSION) {
        return Err(KeyError::Malformed);
    }
    match obj.get("product").and_then(|x| x.as_str()) {
        None => return Err(KeyError::Malformed),
        Some(p) if p != PRODUCT => return Err(KeyError::WrongProduct),
        _ => {}
    }
    let tier = match obj.get("tier").and_then(|x| x.as_str()) {
        Some("personal") => Tier::Personal,
        Some("enterprise") => Tier::Enterprise,
        _ => return Err(KeyError::Malformed),
    };
    let required_str = |name: &str| -> Result<String, KeyError> {
        obj.get(name)
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or(KeyError::Malformed)
    };
    Ok(Payload {
        tier,
        email: required_str("email")?,
        issued_at: required_str("issued_at")?,
        nonce: required_str("nonce")?,
        expires: obj
            .get("expires")
            .and_then(|x| x.as_str())
            .map(str::to_owned),
    })
}

/// 构造 payload JSON 串 —— keygen (`src/bin/keygen.rs`) 的唯一签发入口。
/// 字段名集合与 [`verify_key`] 的解析是同一真身: 两边不许各写一份。
pub fn build_payload(
    tier: Tier,
    email: &str,
    issued_at: &str,
    nonce: &str,
    expires: Option<&str>,
) -> String {
    let tier_str = match tier {
        Tier::Personal => "personal",
        Tier::Enterprise => "enterprise",
    };
    let mut v = serde_json::json!({
        "v": PAYLOAD_VERSION,
        "product": PRODUCT,
        "tier": tier_str,
        "email": email,
        "issued_at": issued_at,
        "nonce": nonce,
    });
    if let Some(e) = expires {
        v["expires"] = serde_json::Value::String(e.to_owned());
    }
    v.to_string()
}

// ─── T3: 授权状态机 + 持久化 ───────────────────────────────────────────

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 付费功能清单 (D3) —— 门控点位在各功能模块的 spec, 这里只给机制。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    FieldAnalytics,
    Export,
    WorkspaceSessions,
}

impl Feature {
    /// 升级提示对话框里的功能名 (T7)。
    pub fn label(&self) -> &'static str {
        match self {
            Feature::FieldAnalytics => "字段分析",
            Feature::Export => "导出",
            Feature::WorkspaceSessions => "工作台会话",
        }
    }
}

/// 付费来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaidSource {
    /// 便携版: license key 激活。携带载荷供许可区展示 (邮箱尾缀遮蔽等)。
    License(Payload),
    /// 商店版: add-on 内购 (T5 接线)。
    StoreAddOn,
}

/// 授权状态 (D1)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entitlement {
    Free,
    /// 商店 trial。expires_epoch 来自 broker 返回值 —— 时长是 Partner Center
    /// 后台配置, 代码不写死。
    Trial {
        expires_epoch: u64,
    },
    Paid {
        source: PaidSource,
    },
}

impl Entitlement {
    /// 门控查询 (D3)。三条付费腿同一规则: Paid 全开; Trial 未过期开。
    pub fn allows(&self, feature: Feature) -> bool {
        self.allows_at(feature, now_epoch())
    }

    /// 可测形态: 「现在」由调用方给。
    pub fn allows_at(&self, _feature: Feature, now: u64) -> bool {
        match self {
            Entitlement::Free => false,
            Entitlement::Paid { .. } => true,
            Entitlement::Trial { expires_epoch } => now < *expires_epoch,
        }
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// license 文件真实路径: 与 config.toml 同目录, 独立文件 —— 不缠 config 的
/// 整文件同源写约束 (config.rs 模块头)。pub 给 main 侧的 `license_path()` 用:
/// 测试注入时它会换成临时邻居路径, 不碰这里。
pub fn default_license_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("danqing-log")
        .join("license.key")
}

/// 启动时从真实路径加载 (D4: 失效只以启动时判定, 会话内不踢人)。
///
/// **只在非 test 构建里有调用点** (同 config.rs 的守卫思路: 测试一律走
/// `load_from` 注入路径, 不碰用户真文件)。
#[cfg_attr(test, allow(dead_code))]
pub fn load() -> Entitlement {
    load_from(&default_license_path(), &PRODUCT_PUBKEY)
}

/// 从指定路径加载。文件不存在 = 免费 (正常路径); 内容坏 → Free + warn,
/// 不崩不 panic —— 授权加载失败绝不能挡住免费层。
pub fn load_from(path: &Path, pubkey: &[u8; 32]) -> Entitlement {
    let Ok(text) = fs::read_to_string(path) else {
        return Entitlement::Free;
    };
    match verify_key(text.trim(), pubkey) {
        Ok(payload) => Entitlement::Paid {
            source: PaidSource::License(payload),
        },
        Err(e) => {
            log::warn!(
                "license 文件 {} 校验失败 ({e:?}), 按免费层启动",
                path.display()
            );
            Entitlement::Free
        }
    }
}

/// 激活失败的分态: key 本身的问题 vs 落盘失败 (会话内已生效但重启会丢)。
#[derive(Debug)]
pub enum ActivateError {
    Key(KeyError),
    Persist(String),
}

/// 激活: 校验通过 → 持久化 → 返回载荷。即时生效由调用方更新自身状态 (D4)。
/// 父目录不存在时先建 —— 全新机器上可能连 `danqing-log/` 配置目录都还没有
/// (用户从未改过任何设置就直接贴 key), 不能让激活栽在这上面。
pub fn activate(key: &str, path: &Path, pubkey: &[u8; 32]) -> Result<Payload, ActivateError> {
    let payload = verify_key(key, pubkey).map_err(ActivateError::Key)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| ActivateError::Persist(format!("{}: {e}", parent.display())))?;
    }
    fs::write(path, format!("{}\n", key.trim()))
        .map_err(|e| ActivateError::Persist(format!("{}: {e}", path.display())))?;
    Ok(payload)
}

// ─── T5: 商店版授权映射 (纯函数区; WinRT 互操作在 main 侧 store_license.rs) ───

/// 商店授权快照 —— broker 查询结果的纯数据形态。
/// WinRT broker 不可单测, 故「快照 → 授权状态」的映射独立成纯函数,
/// 可测面全部集中在这里 (spec T5 验收: 映射纯函数四态全覆盖)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreSnapshot {
    /// 我们的 add-on 在许可证列表里且 `IsActive`。
    /// (pomodoro 实测坑: `StoreAppLicense::IsActive` 是应用级许可证,
    /// 免费上架时所有安装者都为 true —— 必须遍历 add-on, 不许简化。)
    pub addon_active: bool,
    /// add-on 许可证的过期时间 (epoch 秒)。durable 买断 add-on 不过期,
    /// 微软对不过期写法的实测是 DateTime 最大值 (≈ 公元 9999 年)。
    pub addon_expires_epoch: Option<u64>,
}

/// 过期时间达到此阈值 (≈ 公元 9900 年) 视为 durable 买断, 否则是 trial。
const DURABLE_THRESHOLD_EPOCH: u64 = 250_000_000_000;

/// 快照 → 授权状态。trial 过期与否**不在这里判** —— 映射只产出
/// `Trial { expires_epoch }`, 有效性由 `allows_at` 统一回答 (单点口径)。
pub fn map_store_snapshot(snap: &StoreSnapshot) -> Entitlement {
    if !snap.addon_active {
        return Entitlement::Free;
    }
    match snap.addon_expires_epoch {
        Some(e) if e < DURABLE_THRESHOLD_EPOCH => Entitlement::Trial { expires_epoch: e },
        // 不过期 (None) 或 ≈ DateTime 最大值 = durable 买断
        _ => Entitlement::Paid {
            source: PaidSource::StoreAddOn,
        },
    }
}

/// 邮箱遮蔽显示 (D8): `alice@example.com` → `a***@example.com`。
/// 本地段只留首字符; 没有 `@` 或本地段为空的畸形输入整体遮蔽为 `***`, 不猜。
pub fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) if !local.is_empty() => {
            let first = local.chars().next().expect("已判非空");
            format!("{first}***@{domain}")
        }
        _ => "***".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// 测试专用私钥。入库无害: 产品公钥常量与它无关, 它签的 key 在
    /// 真公钥下必败 (有测试守着这一条)。
    const TEST_SECRET: [u8; 32] = [7u8; 32];

    fn test_pubkey() -> [u8; 32] {
        SigningKey::from_bytes(&TEST_SECRET)
            .verifying_key()
            .to_bytes()
    }

    /// 用测试私钥签一个 payload JSON 串, 拼出完整 key。
    fn sign(payload_json: &str) -> String {
        let sig = SigningKey::from_bytes(&TEST_SECRET).sign(payload_json.as_bytes());
        format!(
            "{KEY_PREFIX}.{}.{}",
            URL_SAFE_NO_PAD.encode(payload_json.as_bytes()),
            URL_SAFE_NO_PAD.encode(sig.to_bytes())
        )
    }

    const GOOD: &str = r#"{"v":1,"product":"danqing-log","tier":"personal","email":"a@b.c","issued_at":"2026-09-19","nonce":"n1"}"#;

    #[test]
    fn valid_personal_key_verifies() {
        let p = verify_key(&sign(GOOD), &test_pubkey()).expect("有效 key 必须过");
        assert_eq!(p.tier, Tier::Personal);
        assert_eq!(p.email, "a@b.c");
        assert_eq!(p.issued_at, "2026-09-19");
        assert_eq!(p.nonce, "n1");
        assert_eq!(p.expires, None);
    }

    #[test]
    fn valid_enterprise_key_verifies() {
        let key = sign(&GOOD.replace("\"personal\"", "\"enterprise\""));
        assert_eq!(
            verify_key(&key, &test_pubkey())
                .expect("有效 key 必须过")
                .tier,
            Tier::Enterprise
        );
    }

    #[test]
    fn expires_field_roundtrips() {
        let payload = GOOD.replace(
            "\"nonce\":\"n1\"",
            "\"nonce\":\"n1\",\"expires\":\"2027-01-01\"",
        );
        let p = verify_key(&sign(&payload), &test_pubkey()).expect("带 expires 的有效 key 必须过");
        assert_eq!(p.expires.as_deref(), Some("2027-01-01"));
    }

    #[test]
    fn tampered_payload_fails_signature_not_format() {
        // 翻 payload 段中间一个字符 (保持合法 base64url 字符): 字节变了, 签名必败 ——
        // 且必须报签名错, 不许报格式错 (顺序红线: 先验签再解 JSON)。
        let key = sign(GOOD);
        let mut parts: Vec<String> = key.split('.').map(str::to_owned).collect();
        let mid = parts[1].len() / 2;
        let flipped = if parts[1].as_bytes()[mid] == b'A' {
            'B'
        } else {
            'A'
        };
        parts[1].replace_range(mid..=mid, &flipped.to_string());
        let tampered = parts.join(".");
        assert_eq!(
            verify_key(&tampered, &test_pubkey()),
            Err(KeyError::BadSignature)
        );
    }

    #[test]
    fn tampered_signature_fails() {
        // 翻签名段**首字符**: 必须落在解码后的字节里 (末字符只有 2 个有效位,
        // 翻它会被 base64 解码头尾位检查拦成格式错 —— 那种报 Malformed 是对的)。
        let key = sign(GOOD);
        let mut parts: Vec<String> = key.split('.').map(str::to_owned).collect();
        let flipped = if parts[2].as_bytes()[0] == b'A' {
            'B'
        } else {
            'A'
        };
        parts[2].replace_range(0..=0, &flipped.to_string());
        let tampered = parts.join(".");
        assert_eq!(
            verify_key(&tampered, &test_pubkey()),
            Err(KeyError::BadSignature)
        );
    }

    #[test]
    fn truncated_key_is_malformed() {
        let key = sign(GOOD);
        let truncated = &key[..key.len() / 2];
        assert_eq!(
            verify_key(truncated, &test_pubkey()),
            Err(KeyError::Malformed)
        );
    }

    #[test]
    fn wrong_prefix_is_malformed() {
        let key = sign(GOOD).replacen(KEY_PREFIX, "otherapp1", 1);
        assert_eq!(verify_key(&key, &test_pubkey()), Err(KeyError::Malformed));
    }

    #[test]
    fn wrong_product_is_wrong_product() {
        let key = sign(&GOOD.replace("\"danqing-log\"", "\"danqing-pomodoro\""));
        assert_eq!(
            verify_key(&key, &test_pubkey()),
            Err(KeyError::WrongProduct)
        );
    }

    #[test]
    fn unknown_payload_version_is_malformed() {
        let key = sign(&GOOD.replace("\"v\":1", "\"v\":2"));
        assert_eq!(verify_key(&key, &test_pubkey()), Err(KeyError::Malformed));
    }

    #[test]
    fn missing_required_field_is_malformed() {
        let no_email = r#"{"v":1,"product":"danqing-log","tier":"personal","issued_at":"2026-09-19","nonce":"n1"}"#;
        assert_eq!(
            verify_key(&sign(no_email), &test_pubkey()),
            Err(KeyError::Malformed)
        );
    }

    #[test]
    fn placeholder_product_pubkey_rejects_any_key() {
        // 收银台未开业的安全默认: 全零公钥下连测试私钥签的 key 都必须败。
        assert_eq!(
            verify_key(&sign(GOOD), &PRODUCT_PUBKEY),
            Err(KeyError::BadSignature)
        );
    }

    #[test]
    fn build_payload_roundtrips_through_verify() {
        // keygen 与 verify 共用同一构造器的回归锁: 签发端产出的 key 必须能验回。
        let payload = build_payload(Tier::Enterprise, "x@y.z", "1760000000", "deadbeef", None);
        let p =
            verify_key(&sign(&payload), &test_pubkey()).expect("build_payload 产出的 key 必须验回");
        assert_eq!(p.tier, Tier::Enterprise);
        assert_eq!(p.email, "x@y.z");
        assert_eq!(p.nonce, "deadbeef");
    }

    // ─── T3 测试 ───

    use std::io::Write as _;

    /// 临时 license 路径 (不碰用户真文件; 并行 flake 教训: 文件名带 pid + 用例名)。
    fn temp_license_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "danqing-log-license-test-{}-{tag}.key",
            std::process::id()
        ))
    }

    #[test]
    fn free_allows_no_paid_feature() {
        let e = Entitlement::Free;
        for f in [
            Feature::FieldAnalytics,
            Feature::Export,
            Feature::WorkspaceSessions,
        ] {
            assert!(!e.allows_at(f, 1_700_000_000));
        }
    }

    #[test]
    fn paid_allows_every_feature() {
        let e = Entitlement::Paid {
            source: PaidSource::StoreAddOn,
        };
        for f in [
            Feature::FieldAnalytics,
            Feature::Export,
            Feature::WorkspaceSessions,
        ] {
            assert!(e.allows_at(f, 1_700_000_000));
        }
    }

    #[test]
    fn trial_allows_only_before_expiry() {
        let e = Entitlement::Trial {
            expires_epoch: 1_700_000_000,
        };
        assert!(e.allows_at(Feature::Export, 1_699_999_999));
        assert!(!e.allows_at(Feature::Export, 1_700_000_000));
    }

    #[test]
    fn activate_persists_and_load_roundtrips() {
        let path = temp_license_path("roundtrip");
        let key = sign(GOOD);
        let payload = activate(&key, &path, &test_pubkey()).expect("有效 key 激活必须成");
        assert_eq!(payload.email, "a@b.c");
        let loaded = load_from(&path, &test_pubkey());
        assert_eq!(
            loaded,
            Entitlement::Paid {
                source: PaidSource::License(payload)
            }
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_missing_file_is_free() {
        let path = temp_license_path("missing");
        let _ = std::fs::remove_file(&path); // 确保不存在
        assert_eq!(load_from(&path, &test_pubkey()), Entitlement::Free);
    }

    #[test]
    fn load_garbage_file_is_free_not_panic() {
        let path = temp_license_path("garbage");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "这不是一个 key").unwrap();
        drop(f);
        assert_eq!(load_from(&path, &test_pubkey()), Entitlement::Free);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_wrong_product_key_is_free() {
        let path = temp_license_path("wrongproduct");
        let key = sign(&GOOD.replace("\"danqing-log\"", "\"danqing-pomodoro\""));
        std::fs::write(&path, &key).unwrap();
        assert_eq!(load_from(&path, &test_pubkey()), Entitlement::Free);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn activate_rejects_bad_key_and_writes_nothing() {
        let path = temp_license_path("reject");
        let _ = std::fs::remove_file(&path);
        // 形状合法但签名对不上 (别的私钥签的) → BadSignature;「形状都错」
        // 那条路已被 empty_and_garbage / truncated 等用例覆盖。
        let foreign = SigningKey::from_bytes(&[9u8; 32]).sign(GOOD.as_bytes());
        let bad_key = format!(
            "{KEY_PREFIX}.{}.{}",
            URL_SAFE_NO_PAD.encode(GOOD.as_bytes()),
            URL_SAFE_NO_PAD.encode(foreign.to_bytes())
        );
        let err = activate(&bad_key, &path, &test_pubkey()).unwrap_err();
        assert!(matches!(err, ActivateError::Key(KeyError::BadSignature)));
        assert!(!path.exists(), "坏 key 不许落盘");
    }

    // ─── T5 测试: 商店快照映射四态 ───

    #[test]
    fn store_snapshot_without_addon_is_free() {
        let snap = StoreSnapshot {
            addon_active: false,
            addon_expires_epoch: None,
        };
        assert_eq!(map_store_snapshot(&snap), Entitlement::Free);
    }

    #[test]
    fn store_snapshot_durable_addon_is_paid() {
        // durable 买断: 不过期 (None) 或过期时间 ≈ DateTime 最大值
        for expires in [None, Some(253_402_300_799)] {
            let snap = StoreSnapshot {
                addon_active: true,
                addon_expires_epoch: expires,
            };
            assert_eq!(
                map_store_snapshot(&snap),
                Entitlement::Paid {
                    source: PaidSource::StoreAddOn
                }
            );
        }
    }

    #[test]
    fn store_snapshot_finite_expiry_is_trial() {
        let snap = StoreSnapshot {
            addon_active: true,
            addon_expires_epoch: Some(1_800_000_000), // 2027 年某日, 有限 = trial
        };
        let e = map_store_snapshot(&snap);
        assert_eq!(
            e,
            Entitlement::Trial {
                expires_epoch: 1_800_000_000
            }
        );
        // 有效性口径单点在 allows_at: 未过期开、过期关
        assert!(e.allows_at(Feature::Export, 1_799_999_999));
        assert!(!e.allows_at(Feature::Export, 1_800_000_000));
    }

    // ─── T6 测试: 邮箱遮蔽 ───

    #[test]
    fn mask_email_keeps_first_char_and_domain() {
        assert_eq!(mask_email("alice@example.com"), "a***@example.com");
        assert_eq!(mask_email("a@b.c"), "a***@b.c");
    }

    #[test]
    fn mask_email_malformed_input_fully_masked() {
        assert_eq!(mask_email("no-at-sign"), "***");
        assert_eq!(mask_email("@domain.com"), "***");
        assert_eq!(mask_email(""), "***");
    }

    // ─── 评审修复 (2026-09-19): 长度闸 + Debug 遮蔽 ───

    #[test]
    fn oversize_key_is_malformed_without_decoding() {
        let huge = format!("{KEY_PREFIX}.{}.x", "a".repeat(5000));
        assert_eq!(verify_key(&huge, &test_pubkey()), Err(KeyError::Malformed));
    }

    #[test]
    fn payload_debug_masks_email() {
        let p = verify_key(&sign(GOOD), &test_pubkey()).unwrap();
        let dbg = format!("{p:?}");
        assert!(!dbg.contains("a@b.c"), "Debug 不得含完整邮箱: {dbg}");
        assert!(dbg.contains("a***@b.c"));
        // 派生 Debug 的 Entitlement 也走同一个遮蔽 (Payload 手写 Debug 罩着)
        let e = Entitlement::Paid {
            source: PaidSource::License(p),
        };
        assert!(!format!("{e:?}").contains("a@b.c"));
    }

    #[test]
    fn empty_and_garbage_inputs_are_malformed() {
        assert_eq!(verify_key("", &test_pubkey()), Err(KeyError::Malformed));
        assert_eq!(
            verify_key("loglens1", &test_pubkey()),
            Err(KeyError::Malformed)
        );
        assert_eq!(
            verify_key("loglens1.x.y.z", &test_pubkey()),
            Err(KeyError::Malformed)
        );
        assert_eq!(verify_key("  ", &test_pubkey()), Err(KeyError::Malformed));
    }
}
