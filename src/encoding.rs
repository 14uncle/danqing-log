//! @author 十四叔
//! @date 2026/09/05
//!
//! 编码检测与转码 (core-viewer T2, 技术风险③)。
//!
//! 检测流水线: BOM → UTF-16 交替 NUL 启发 → UTF-8 合法性 → GBK 双字节统计
//! → Latin-1 降级兜底 (不猜具体单字节代码页, 原样映射显示, 永不崩)。
//! 零依赖: GBK 编解码走 Win32 CP936 直通 FFI (产品 Windows-only, 意图文档边界),
//! 不引 encoding_rs。
//!
//! 策略分治 (tasks/plan.md 决策 4):
//! - UTF-16 (LE/BE, 有无 BOM): 打开时一次性转码 UTF-8 内存副本 (2 字节编码不适合
//!   字节级 \n 索引), 之后全走 UTF-8 路径;
//! - UTF-8/GBK/Latin-1: 原字节索引 + 行级解码; 搜索查询经 [`encode_query`] 转码到
//!   文件编码再字节匹配 (GBK 中文查询可行; GBK trail byte 0x40–0xFE 不含 0x0A,
//!   字节级行索引安全)。

use std::borrow::Cow;

/// 检出的文件编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Gbk,
    /// 单字节兜底 (原字节逐字映射 U+0000–U+00FF, 不猜码)。
    Latin1,
}

impl Encoding {
    /// 状态栏/基准展示标签。
    pub fn label(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf16Le => "UTF-16LE",
            Encoding::Utf16Be => "UTF-16BE",
            Encoding::Gbk => "GBK",
            Encoding::Latin1 => "Latin-1",
        }
    }

    /// 是否 2 字节编码 (打开时需转码副本)。
    pub fn is_utf16(self) -> bool {
        matches!(self, Encoding::Utf16Le | Encoding::Utf16Be)
    }
}

/// 检测采样上限 (64KB)。
pub const SAMPLE: usize = 64 * 1024;
/// GBK 双字节覆盖率判定阈值 (实测校准: 中文 GBK 文本 ~100%, 随机字节 ~54%)。
const GBK_COVERAGE_MIN: f64 = 0.8;

/// 检测文件编码 (传入文件头部若干字节, 内部截到 SAMPLE)。
pub fn detect(head: &[u8]) -> Encoding {
    if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Encoding::Utf8;
    }
    if head.starts_with(&[0xFF, 0xFE]) {
        return Encoding::Utf16Le;
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return Encoding::Utf16Be;
    }
    let sample = &head[..head.len().min(SAMPLE)];
    if sample.is_empty() {
        return Encoding::Utf8;
    }
    // UTF-16 无 BOM 启发: 交替 NUL 模式。必须先于 UTF-8 合法性判 ——
    // UTF-16LE 的 ASCII 文本 (0x41 0x00 …) 是合法 UTF-8 (NUL 是合法码点)。
    if let Some(enc) = detect_utf16_by_nuls(sample) {
        return enc;
    }
    if utf8_valid(sample) {
        return Encoding::Utf8;
    }
    if gbk_coverage(sample) >= GBK_COVERAGE_MIN {
        return Encoding::Gbk;
    }
    Encoding::Latin1
}

/// 交替 NUL 模式判 UTF-16: NUL 占比 >12.5% 且奇偶分布 3:1 以上偏斜。
/// LE 的 ASCII 字符 = 低字节在前 (奇位为 0), BE 反之。
fn detect_utf16_by_nuls(sample: &[u8]) -> Option<Encoding> {
    let mut even_nul = 0usize;
    let mut odd_nul = 0usize;
    for (i, &b) in sample.iter().enumerate() {
        if b == 0 {
            if i % 2 == 0 {
                even_nul += 1;
            } else {
                odd_nul += 1;
            }
        }
    }
    if (even_nul + odd_nul) * 8 <= sample.len() {
        return None;
    }
    if odd_nul > even_nul * 3 {
        Some(Encoding::Utf16Le)
    } else if even_nul > odd_nul * 3 {
        Some(Encoding::Utf16Be)
    } else {
        None
    }
}

/// UTF-8 合法性 (容忍采样边界截断的半个字符)。
fn utf8_valid(sample: &[u8]) -> bool {
    match std::str::from_utf8(sample) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none() && e.valid_up_to() + 4 >= sample.len(),
    }
}

/// GBK 双字节覆盖率: 合法对 (lead 0x81–0xFE, trail 0x40–0xFE 除 0x7F) 占非 ASCII
/// 字节的比例。顺序消费 (对命中即跳 2), 防随机字节虚高。
fn gbk_coverage(sample: &[u8]) -> f64 {
    let mut i = 0;
    let mut covered = 0usize;
    let mut nonascii = 0usize;
    while i < sample.len() {
        let b = sample[i];
        if b.is_ascii() {
            i += 1;
            continue;
        }
        if (0x81..=0xFE).contains(&b)
            && i + 1 < sample.len()
            && (0x40..=0xFE).contains(&sample[i + 1])
            && sample[i + 1] != 0x7F
        {
            covered += 2;
            nonascii += 2;
            i += 2;
        } else {
            nonascii += 1;
            i += 1;
        }
    }
    if nonascii == 0 {
        return 0.0;
    }
    covered as f64 / nonascii as f64
}

/// UTF-16 → UTF-8 转码 (剥 BOM; 奇数尾字节丢弃)。le = 是否小端。
pub fn transcode_utf16(le: bool, raw: &[u8]) -> Vec<u8> {
    let mut body = raw;
    if body.starts_with(&[0xFF, 0xFE]) || body.starts_with(&[0xFE, 0xFF]) {
        body = &body[2..];
    }
    let units: Vec<u16> = body
        .chunks_exact(2)
        .map(|c| {
            if le {
                u16::from_le_bytes([c[0], c[1]])
            } else {
                u16::from_be_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units).into_bytes()
}

/// 行级解码到 UTF-8 文本。Utf16 不应出现在行级 (打开时已转码), 到达即防御性 lossy。
pub fn decode_line(enc: Encoding, raw: &[u8]) -> Cow<'_, str> {
    match enc {
        Encoding::Utf8 => String::from_utf8_lossy(raw),
        Encoding::Latin1 => Cow::Owned(raw.iter().map(|&b| char::from(b)).collect()),
        Encoding::Gbk => Cow::Owned(gbk::decode(raw)),
        Encoding::Utf16Le | Encoding::Utf16Be => String::from_utf8_lossy(raw),
    }
}

/// 搜索/过滤查询转码到文件编码字节 (GBK 中文查询的核心; 其余编码原样)。
pub fn encode_query(enc: Encoding, q: &str) -> Vec<u8> {
    match enc {
        Encoding::Gbk => gbk::encode(q),
        _ => q.as_bytes().to_vec(),
    }
}

/// GBK (CP936) 编解码: Win32 直通, 零 crate 依赖。
/// 非 Windows 平台降级 lossy (产品边界 = Windows-only, 此处仅为可编译)。
#[cfg(windows)]
mod gbk {
    use std::ptr;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MultiByteToWideChar(
            code_page: u32,
            flags: u32,
            mb: *const u8,
            mb_len: i32,
            wide: *mut u16,
            wide_len: i32,
        ) -> i32;
        fn WideCharToMultiByte(
            code_page: u32,
            flags: u32,
            wide: *const u16,
            wide_len: i32,
            mb: *mut u8,
            mb_len: i32,
            default_char: *const u8,
            used_default: *mut i32,
        ) -> i32;
    }

    const CP_GBK: u32 = 936;

    pub fn decode(raw: &[u8]) -> String {
        if raw.is_empty() {
            return String::new();
        }
        let len = raw.len().min(i32::MAX as usize) as i32;
        unsafe {
            let wlen = MultiByteToWideChar(CP_GBK, 0, raw.as_ptr(), len, ptr::null_mut(), 0);
            if wlen <= 0 {
                return String::from_utf8_lossy(raw).into_owned();
            }
            let mut buf = vec![0u16; wlen as usize];
            let n = MultiByteToWideChar(CP_GBK, 0, raw.as_ptr(), len, buf.as_mut_ptr(), wlen);
            if n <= 0 {
                return String::from_utf8_lossy(raw).into_owned();
            }
            String::from_utf16_lossy(&buf[..n as usize])
        }
    }

    pub fn encode(s: &str) -> Vec<u8> {
        let wide: Vec<u16> = s.encode_utf16().collect();
        if wide.is_empty() {
            return Vec::new();
        }
        unsafe {
            let blen = WideCharToMultiByte(
                CP_GBK,
                0,
                wide.as_ptr(),
                wide.len() as i32,
                ptr::null_mut(),
                0,
                ptr::null(),
                ptr::null_mut(),
            );
            if blen <= 0 {
                return s.as_bytes().to_vec();
            }
            let mut buf = vec![0u8; blen as usize];
            let n = WideCharToMultiByte(
                CP_GBK,
                0,
                wide.as_ptr(),
                wide.len() as i32,
                buf.as_mut_ptr(),
                blen,
                ptr::null(),
                ptr::null_mut(),
            );
            if n <= 0 {
                return s.as_bytes().to_vec();
            }
            buf.truncate(n as usize);
            buf
        }
    }
}

/// 非 Windows 兜底 (不可达于产品环境, 保编译)。
#[cfg(not(windows))]
mod gbk {
    pub fn decode(raw: &[u8]) -> String {
        String::from_utf8_lossy(raw).into_owned()
    }
    pub fn encode(s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GBK 字节常量 (CP936 实测校准: 中=D6D0 文=CEC4 错=B4ED 误=CEF3 日=C8D5 志=DBBE;
    /// 初稿凭记忆把「文」写成 C4C4 被测试当场纠出 —— C4C4=哪)。
    const GBK_ZHONGWEN: &[u8] = b"\xD6\xD0\xCE\xC4";
    const GBK_LINE: &[u8] = b"INFO \xD6\xD0\xCE\xC4\xB4\xED\xCE\xF3\xC8\xD5\xD6\xBE\n";

    #[test]
    fn detect_bom_variants() {
        assert_eq!(detect(b"\xEF\xBB\xBFhello"), Encoding::Utf8);
        assert_eq!(detect(b"\xFF\xFEa\x00"), Encoding::Utf16Le);
        assert_eq!(detect(b"\xFE\xFF\x00a"), Encoding::Utf16Be);
    }

    #[test]
    fn detect_plain_utf8_and_ascii() {
        assert_eq!(detect(b"plain ascii log line\n"), Encoding::Utf8);
        assert_eq!(detect("中文 UTF-8 日志\n".as_bytes()), Encoding::Utf8);
        assert_eq!(detect(b""), Encoding::Utf8, "空文件按 UTF-8");
    }

    #[test]
    fn detect_utf16_without_bom_by_nul_pattern() {
        // UTF-16LE 无 BOM: "ABCD" → 41 00 42 00 43 00 44 00 (奇位全 0)
        let le: Vec<u8> = "INFO hello world 2026-09-05 这条日志没有BOM头"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert_eq!(detect(&le), Encoding::Utf16Le, "交替 NUL 判 LE");
        let be: Vec<u8> = "INFO hello world"
            .encode_utf16()
            .flat_map(|u| u.to_be_bytes())
            .collect();
        assert_eq!(detect(&be), Encoding::Utf16Be, "交替 NUL 判 BE");
    }

    #[test]
    fn detect_gbk_by_pair_coverage() {
        assert_eq!(detect(GBK_LINE), Encoding::Gbk, "GBK 中文行判 GBK");
    }

    #[test]
    fn detect_latin1_fallback_for_binary() {
        // 0x80 不是合法 GBK lead (0x81–0xFE), 覆盖率 0 → Latin-1
        let binary: Vec<u8> = (0..200)
            .map(|i| if i % 3 == 0 { 0x80 } else { 0x7F })
            .collect();
        assert_eq!(
            detect(&binary),
            Encoding::Latin1,
            "乱字节降级 Latin-1 不猜码"
        );
    }

    #[test]
    fn transcode_utf16_strips_bom_and_decodes() {
        let mut raw = vec![0xFF, 0xFE];
        for u in "AB\n中".encode_utf16() {
            raw.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(transcode_utf16(true, &raw), "AB\n中".as_bytes());
        let mut raw = vec![0xFE, 0xFF];
        for u in "AB\n中".encode_utf16() {
            raw.extend_from_slice(&u.to_be_bytes());
        }
        assert_eq!(transcode_utf16(false, &raw), "AB\n中".as_bytes());
    }

    #[test]
    fn gbk_roundtrip_via_win32() {
        assert_eq!(gbk::encode("中文"), GBK_ZHONGWEN, "CP936 编码");
        assert_eq!(gbk::decode(GBK_ZHONGWEN), "中文", "CP936 解码");
        assert_eq!(decode_line(Encoding::Gbk, GBK_LINE), "INFO 中文错误日志\n");
    }

    #[test]
    fn encode_query_transcodes_only_gbk() {
        assert_eq!(encode_query(Encoding::Gbk, "中文"), GBK_ZHONGWEN);
        assert_eq!(encode_query(Encoding::Utf8, "中文"), "中文".as_bytes());
    }

    #[test]
    fn latin1_decode_maps_bytes_verbatim() {
        let s = decode_line(Encoding::Latin1, &[0x41, 0xE9, 0xFF]);
        assert_eq!(s.chars().count(), 3);
        assert_eq!(s.chars().nth(1).unwrap(), '\u{E9}');
    }
}
