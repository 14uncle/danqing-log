//! @author 十四叔
//! @date 2026/09/19
//!
//! keygen —— license key 签发工具 (SPEC-v1x-licensing D6)。
//!
//! **私钥永不入库**: 私钥文件 (32 字节, hex 编码) 只存在仓库外, 由命令行
//! 路径指定。本工具本身入库可审计。
//!
//! 用法:
//!   keygen generate <secret_path>                生成新私钥 (已存在则拒绝覆盖)
//!   keygen pubkey  <secret_path>                 打印公钥 hex (贴进 license.rs 的 PRODUCT_PUBKEY)
//!   keygen sign    <secret_path> <email> <tier>  签发 key (tier = personal | enterprise)

use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use ed25519_dalek::{Signer, SigningKey};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use danqing_log::license::{Tier, build_payload};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("keygen: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("generate") => {
            let path = arg(&args, 1, "generate <secret_path>")?;
            generate(Path::new(path))
        }
        Some("pubkey") => {
            let path = arg(&args, 1, "pubkey <secret_path>")?;
            let sk = read_secret(Path::new(path))?;
            let pk = sk.verifying_key();
            // 两种形态都给: hex 供肉眼/文档, Rust 数组字面量直接可贴进
            // `license.rs` 的 `PRODUCT_PUBKEY` (评审 Optional: 消掉手工转换
            // 64 个 hex 字节的手滑点)。
            println!("hex:   {}", hex_encode(&pk.to_bytes()));
            println!("rust:  {:?}", pk.to_bytes());
            Ok(())
        }
        Some("sign") => {
            let path = arg(&args, 1, "sign <secret_path> <email> <tier>")?;
            let email = arg(&args, 2, "sign <secret_path> <email> <tier>")?;
            let tier = match arg(&args, 3, "sign <secret_path> <email> <tier>")? {
                "personal" => Tier::Personal,
                "enterprise" => Tier::Enterprise,
                other => bail!("tier 只能是 personal | enterprise, 收到: {other}"),
            };
            let sk = read_secret(Path::new(path))?;
            println!("{}", sign_key(&sk, email, tier)?);
            Ok(())
        }
        _ => {
            bail!(
                "用法: keygen generate <secret_path> | keygen pubkey <secret_path> | \
                 keygen sign <secret_path> <email> <personal|enterprise>"
            )
        }
    }
}

fn arg<'a>(args: &'a [String], i: usize, usage: &str) -> Result<&'a str> {
    args.get(i)
        .map(String::as_str)
        .with_context(|| format!("缺参数, 用法: keygen {usage}"))
}

/// 生成新私钥。**已存在则拒绝覆盖** —— 覆盖私钥 = 已签发的全部 key 失去签发者。
fn generate(path: &Path) -> Result<()> {
    if path.exists() {
        bail!(
            "{} 已存在, 不覆盖 (要换密钥对请先手动挪走旧文件)",
            path.display()
        );
    }
    let mut secret = [0u8; 32];
    getrandom::getrandom(&mut secret).map_err(|e| anyhow::anyhow!("取操作系统随机数失败: {e}"))?;
    fs::write(path, hex_encode(&secret) + "\n")
        .with_context(|| format!("写入 {}", path.display()))?;
    println!("私钥已写入 {} —— 保管好, 别进任何仓库", path.display());
    Ok(())
}

fn read_secret(path: &Path) -> Result<SigningKey> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("读私钥文件 {} 失败 (路径应指向仓库外)", path.display()))?;
    let bytes = hex_decode(text.trim()).context("私钥文件内容不是合法 hex")?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("私钥必须是 32 字节 (64 个 hex 字符)"))?;
    Ok(SigningKey::from_bytes(&arr))
}

fn sign_key(sk: &SigningKey, email: &str, tier: Tier) -> Result<String> {
    let mut nonce = [0u8; 8];
    getrandom::getrandom(&mut nonce).map_err(|e| anyhow::anyhow!("取操作系统随机数失败: {e}"))?;
    let issued_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("系统时钟早于 1970?")?
        .as_secs()
        .to_string();
    let payload = build_payload(tier, email, &issued_at, &hex_encode(&nonce), None);
    let sig = sk.sign(payload.as_bytes());
    Ok(format!(
        "loglens1.{}.{}",
        URL_SAFE_NO_PAD.encode(payload.as_bytes()),
        URL_SAFE_NO_PAD.encode(sig.to_bytes())
    ))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        bail!("hex 长度必须是偶数, 收到 {}", s.len());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16).with_context(|| format!("hex 解码失败于偏移 {i}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let bytes = [0x00u8, 0xff, 0x07, 0xab];
        assert_eq!(hex_decode(&hex_encode(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn hex_decode_rejects_odd_length_and_bad_chars() {
        assert!(hex_decode("abc").is_err());
        assert!(hex_decode("zz").is_err());
        assert!(hex_decode("").is_ok_and(|v| v.is_empty()));
    }

    #[test]
    fn sign_produces_key_that_verifies() {
        // 端到端: 临时私钥文件 → read_secret → sign_key → license::verify_key 验回。
        // 并行测试flake教训在档: 临时文件名带进程 id, 不与任何其他测试共享。
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "danqing-log-keygen-test-{}.secret",
            std::process::id()
        ));
        let secret = [3u8; 32];
        fs::write(&path, hex_encode(&secret)).unwrap();
        let sk = read_secret(&path).unwrap();
        let pubkey = sk.verifying_key().to_bytes();
        let key = sign_key(&sk, "e2e@test", Tier::Personal).unwrap();
        let payload =
            danqing_log::license::verify_key(&key, &pubkey).expect("keygen 签的 key 必须验回");
        assert_eq!(payload.email, "e2e@test");
        assert_eq!(payload.tier, Tier::Personal);
        let _ = fs::remove_file(&path);
    }
}
