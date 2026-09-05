//! @author 十四叔
//! @date 2026/09/05
//!
//! 日志文件引擎: mmap + 行偏移索引 + 全文正则搜索。
//!
//! POC 的碾压主张全部落在这一层:
//! - 秒开 = mmap 零拷贝, 建立映射本身 O(1);
//! - 行索引 = memchr 扫 `\n` (SIMD) + 步进表 (每 16 行一记, ~3.2MB/GB,
//!   段内前扫定位, 见 INDEX_STRIDE 注释);
//! - 全文搜索 = regex::bytes 直接跑在映射页上, 内核按需调页, 无用户态缓冲拷贝。
//!
//! POC 边界 (见意图文档「三大技术风险」):
//! - 编码: core-viewer T2 已落地 UTF-8/UTF-16(转码副本)/GBK(行级 CP936)/Latin-1
//!   兜底, 见 encoding.rs;
//! - tail 截断与轮转: 快照 + 重建原语在 T3; mmap 期间文件被外部截断的风险
//!   以 T3 Windows 实测表为准 (Linux 的 SIGBUS 假设未必适用)。

use std::borrow::Cow;
use std::fs::File;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use memmap2::Mmap;

use crate::encoding::{self, Encoding};

/// 打开统计: 截图弹药的原材料, 全部实测不估算。
#[derive(Debug, Clone)]
pub struct OpenStats {
    /// 文件字节数 (磁盘原始大小; UTF-16 转码副本不按此计)。
    pub file_bytes: u64,
    /// 建立内存映射耗时 (微秒)。
    pub map_us: u64,
    /// 行索引构建耗时 (UTF-16 含转码)。
    pub index: Duration,
    /// 行数。
    pub line_count: u64,
    /// 行索引驻留字节 (步进表堆占用, shrink 后实测)。
    pub index_bytes: usize,
    /// 检出的原始编码 (状态栏展示; 数据实际编码见 [`LogFile::encoding`])。
    pub encoding: Encoding,
}

impl OpenStats {
    /// 索引吞吐 (MiB/s)。
    pub fn index_mib_per_s(&self) -> f64 {
        let secs = self.index.as_secs_f64();
        if secs <= 0.0 {
            return f64::INFINITY;
        }
        (self.file_bytes as f64 / (1024.0 * 1024.0)) / secs
    }
}

/// 文件状态快照: 过期检测用 (live-tail 轮询的判定原料)。
///
/// Windows 实测 (tasks/plan.md 附录, mmap_lab): 映射存活期外部**截断被 OS 拒绝**,
/// 真正的过期通道是 rename/delete (视图滞留旧内容)、append (增长)、
/// overwrite (内容原位被换) —— 全部可由 len+mtime 变化检出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    /// 文件长度。
    pub len: u64,
    /// 修改时间 (取不到为 None, 比对时 None≠Some 视为过期)。
    pub mtime: Option<SystemTime>,
}

impl FileStat {
    /// 取路径当前状态。
    pub fn of(path: &Path) -> std::io::Result<Self> {
        let m = std::fs::metadata(path)?;
        Ok(Self {
            len: m.len(),
            mtime: m.modified().ok(),
        })
    }
}

/// 步进索引步长: 每 STRIDE 行记一个绝对偏移, 段内 memchr 前扫定位。
/// 16 = 内存 (3.2MB/GB) 与随机访问 (前扫 ≤15 行 ≈ 2.7KB) 的实测平衡点,
/// 退化预案 stride=8 见 tasks/plan.md 决策 1。
const INDEX_STRIDE: u64 = 16;

/// 数据载体: UTF-8/GBK/Latin-1 走 mmap 零拷贝; UTF-16 为打开时转码的 UTF-8 副本。
enum FileData {
    Mapped(Mmap),
    Owned(Vec<u8>),
}

impl FileData {
    fn as_bytes(&self) -> &[u8] {
        match self {
            FileData::Mapped(m) => &m[..],
            FileData::Owned(v) => &v[..],
        }
    }
}

/// 已映射的日志文件: 步进行索引 + 只读访问。
pub struct LogFile {
    data: FileData,
    /// 存储字节的编码 (UTF-16 文件转码后为 Utf8; 原始检出编码在 stats.encoding)。
    encoding: Encoding,
    /// 步进表: 第 j 项 = 第 j*STRIDE 行的起始字节偏移。
    stride_offsets: Vec<u64>,
    /// 总行数 (步进表长度 × STRIDE ≠ 行数, 单独存)。
    line_count: u64,
    /// 打开时的文件状态快照 (过期检测基准)。
    stat: FileStat,
    stats: OpenStats,
}

impl LogFile {
    /// 打开并索引文件。
    ///
    /// 编码: BOM → 交替 NUL → UTF-8 合法性 → GBK 统计 → Latin-1 兜底 (encoding.rs);
    /// UTF-16 打开时转码 UTF-8 内存副本, 其余编码原字节索引 + 行级解码。
    pub fn open(path: &Path) -> Result<Self> {
        let t0 = Instant::now();
        let file = File::open(path).with_context(|| format!("打开文件失败: {}", path.display()))?;
        let file_bytes = file.metadata().context("读取文件元信息失败")?.len();
        // 安全性: 映射只读; 已知风险 = 映射期间外部截断 (见模块头注释, T3 实测校准)。
        let map = unsafe { Mmap::map(&file).context("建立内存映射失败")? };
        let map_us = t0.elapsed().as_micros() as u64;

        let head = &map[..map.len().min(encoding::SAMPLE)];
        let detected = encoding::detect(head);
        let (data, data_enc) = if detected.is_utf16() {
            // 2 字节编码不适合字节级索引: 一次性转码 UTF-8 副本 (1GB UTF-16 ≈ 500MB UTF-8)
            let raw =
                std::fs::read(path).with_context(|| format!("读取文件失败: {}", path.display()))?;
            drop(map);
            let utf8 = encoding::transcode_utf16(detected == Encoding::Utf16Le, &raw);
            (FileData::Owned(utf8), Encoding::Utf8)
        } else {
            (FileData::Mapped(map), detected)
        };

        let t1 = Instant::now();
        let (mut stride_offsets, line_count) = build_line_index(data.as_bytes());
        stride_offsets.shrink_to_fit();
        let index = t1.elapsed();
        let index_bytes = stride_offsets.len() * std::mem::size_of::<u64>();
        let stat = FileStat::of(path).unwrap_or(FileStat {
            len: file_bytes,
            mtime: None,
        });

        let stats = OpenStats {
            file_bytes,
            map_us,
            index,
            line_count,
            index_bytes,
            encoding: detected,
        };
        Ok(Self {
            data,
            encoding: data_enc,
            stride_offsets,
            line_count,
            stat,
            stats,
        })
    }

    /// 打开统计 (供状态栏与基准输出)。
    pub fn stats(&self) -> &OpenStats {
        &self.stats
    }

    /// 行数。
    pub fn line_count(&self) -> u64 {
        self.line_count
    }

    /// 存储字节的编码 (行级解码/高亮测量的解码路径以此为准;
    /// 原始检出编码见 stats().encoding —— UTF-16 文件两者不同: 原始 Utf16*, 存储 Utf8)。
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// 第 i 行原始字节 (不含 `\n` / `\r`)。越界返回空片。
    ///
    /// 步进索引定位: 二分步进表定段 → 段内 memchr 前扫 (i % STRIDE) 个换行。
    pub fn line(&self, i: u64) -> &[u8] {
        if i >= self.line_count {
            return &[];
        }
        let data = self.data.as_bytes();
        let mut start = self.stride_offsets[(i / INDEX_STRIDE) as usize] as usize;
        for _ in 0..(i % INDEX_STRIDE) {
            // 索引由同一份数据建出, 扫描必命中; None 分支为防御 (索引一致性不信赖)
            match memchr::memchr(b'\n', &data[start..]) {
                Some(p) => start += p + 1,
                None => return &[],
            }
        }
        let end = match memchr::memchr(b'\n', &data[start..]) {
            Some(p) => start + p,
            None => data.len(),
        };
        let mut s = &data[start..end];
        if s.last() == Some(&b'\r') {
            s = &s[..s.len() - 1];
        }
        s
    }

    /// 第 i 行解码为 UTF-8 文本 (按检出编码; 非原生 UTF-8 走行级转码)。
    pub fn line_lossy(&self, i: u64) -> Cow<'_, str> {
        encoding::decode_line(self.encoding, self.line(i))
    }

    /// 全文顺序行迭代器: 单次 memchr 扫描, 每行 O(1)。
    /// 全文谓词 (过滤等) 的正确访问方式 —— 步进索引下 line(i) 是随机访问,
    /// 逐行全量遍历走它会把定位成本乘进行数 (实测过滤 235ms → 1072ms 的教训)。
    pub fn lines(&self) -> LineIter<'_> {
        let start = if self.line_count > 0 {
            self.stride_offsets[0] as usize
        } else {
            0
        };
        LineIter {
            data: self.data.as_bytes(),
            next: 0,
            start,
            count: self.line_count,
        }
    }

    /// 从第 start_line 行起的顺序迭代器 (live-tail 增量过滤: 只扫新行, 不重扫前文)。
    /// start_line == 0 等价 [`Self::lines`]; 越界返回空迭代器。
    pub fn lines_from(&self, start_line: u64) -> LineIter<'_> {
        let data = self.data.as_bytes();
        if start_line >= self.line_count {
            return LineIter {
                data,
                next: start_line,
                start: data.len(),
                count: self.line_count,
            };
        }
        // 步进定位 start_line 的起始字节 + 段内前扫
        let mut start = self.stride_offsets[(start_line / INDEX_STRIDE) as usize] as usize;
        for _ in 0..(start_line % INDEX_STRIDE) {
            match memchr::memchr(b'\n', &data[start..]) {
                Some(p) => start += p + 1,
                None => break,
            }
        }
        LineIter {
            data,
            next: start_line,
            start,
            count: self.line_count,
        }
    }

    /// 字节偏移 → 行号: 步进表二分定段 + 段内 memchr 前扫 (≤STRIDE-1 行)。
    /// 仅供 search 的命中回落 (命中数封顶 cap, 成本有界)。
    fn line_of_offset(&self, off: u64) -> u64 {
        let data = self.data.as_bytes();
        let seg = self
            .stride_offsets
            .partition_point(|&o| o <= off)
            .saturating_sub(1);
        let mut line = seg as u64 * INDEX_STRIDE;
        let mut p = self.stride_offsets[seg] as usize;
        while line + 1 < self.line_count {
            match memchr::memchr(b'\n', &data[p..]) {
                // 换行符在 off 之前 → off 属于下一行, 前进
                Some(q) if ((p + q) as u64) < off => {
                    p += q + 1;
                    line += 1;
                }
                _ => break,
            }
        }
        line
    }

    /// 全文正则搜索: 直接扫映射页, 命中字节偏移经步进索引回落到行号。
    ///
    /// 返回 (行号列表, 总命中数, 耗时); 行号列表封顶 `cap` 条防内存爆,
    /// 总命中数不受 cap 影响 (如实报告)。
    pub fn search(&self, re: &regex::bytes::Regex, cap: usize) -> (Vec<u64>, u64, Duration) {
        let t = Instant::now();
        let mut lines = Vec::new();
        let mut total = 0u64;
        for m in re.find_iter(self.data.as_bytes()) {
            total += 1;
            if lines.len() < cap {
                let line = self.line_of_offset(m.start() as u64);
                // 同一行多次命中只收一次 (列表语义)
                if lines.last() != Some(&line) {
                    lines.push(line);
                }
            }
        }
        (lines, total, t.elapsed())
    }

    /// 搜索/过滤查询转码到存储编码 (GBK 中文查询走 CP936; 其余原样)。
    pub fn encode_query(&self, q: &str) -> Vec<u8> {
        encoding::encode_query(self.encoding, q)
    }

    /// 打开时的文件状态快照。
    pub fn stat_snapshot(&self) -> FileStat {
        self.stat
    }

    /// 路径当前状态与快照不一致 (或文件已不可读) = 过期。
    /// 过期语义不区分成因 (增长/覆写/轮转重建), 由调用方决定重建策略。
    pub fn is_stale(&self, path: &Path) -> bool {
        match FileStat::of(path) {
            Ok(cur) => cur != self.stat,
            Err(_) => true, // 文件没了 (delete/轮转间隙) = 过期
        }
    }

    /// 原位重建: 重新打开+索引, 成功后整体换入 (视图永远只见一致快照)。
    /// live-tail 的截断/轮转恢复原语; 失败时 self 不变 (旧视图继续可用)。
    pub fn rebuild(&mut self, path: &Path) -> Result<()> {
        let new = Self::open(path)?;
        *self = new;
        Ok(())
    }

    /// 增长追加: 重新 mmap + 只对新字节区间增量索引, 返回新 LogFile (旧实例由调用方
    /// 的 Arc 保活, 无 Mutex 无悬垂)。UTF-16 (转码副本) 与缩容退化全量 open。
    pub fn append_from(old: &Self, path: &Path) -> Result<Self> {
        // UTF-16 转码副本: 索引建在 UTF-8 副本上, 增量不适用 → 全量
        if matches!(old.data, FileData::Owned(_)) {
            return Self::open(path);
        }
        let t0 = Instant::now();
        let file = File::open(path).with_context(|| format!("打开文件失败: {}", path.display()))?;
        let file_bytes = file.metadata().context("读取文件元信息失败")?.len();
        let map = unsafe { Mmap::map(&file).context("建立内存映射失败")? };
        let map_us = t0.elapsed().as_micros() as u64;

        let old_len = old.data.as_bytes().len();
        let new_data = &map[..];
        // 缩容/无增长: 全量重建 (调用方通常据 stat 判过期, 这里是防御兜底)
        if new_data.len() <= old_len {
            drop(map);
            return Self::open(path);
        }

        let t1 = Instant::now();
        let (mut stride_offsets, line_count) = append_index(
            &old.stride_offsets,
            old.line_count,
            old.data.as_bytes(),
            new_data,
        );
        stride_offsets.shrink_to_fit();
        let index = t1.elapsed();
        let index_bytes = stride_offsets.len() * std::mem::size_of::<u64>();
        let stat = FileStat::of(path).unwrap_or(FileStat {
            len: file_bytes,
            mtime: None,
        });

        let stats = OpenStats {
            file_bytes,
            map_us,
            index,
            line_count,
            index_bytes,
            encoding: old.stats.encoding,
        };
        Ok(Self {
            data: FileData::Mapped(map),
            encoding: old.encoding,
            stride_offsets,
            line_count,
            stat,
            stats,
        })
    }
}

/// 步进行索引: SIMD memchr 扫 `\n`, 每 STRIDE 行记一个起点偏移, 并数总行数。
///
/// UTF-8 BOM 跳过 (首行从 BOM 之后开始)。文件以 `\n` 结尾时末尾换行不产生
/// 新行 (不存在「最后一空行」)。
/// 单线程先行: 实测吞吐达标则不并行化 (简洁优先; 1GB 目标 < 1s)。
fn build_line_index(data: &[u8]) -> (Vec<u64>, u64) {
    if data.is_empty() {
        return (Vec::new(), 0);
    }
    let bom = if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3u64
    } else {
        0
    };
    // 预分配: 经验值 ~64 字节/行 + 步进 16, 避免 Vec 反复扩容
    let mut strides = Vec::with_capacity(data.len() / 64 / INDEX_STRIDE as usize + 16);
    strides.push(bom); // 第 0 行
    let mut count = 1u64;
    for p in memchr::memchr_iter(b'\n', data) {
        let next = p as u64 + 1;
        if next == data.len() as u64 {
            break; // 末尾换行不产生新行
        }
        if count % INDEX_STRIDE == 0 {
            strides.push(next);
        }
        count += 1;
    }
    (strides, count)
}

/// 增量索引: 只扫 `[old_len, new_len)` 的新字节, 追加 stride 表 + 行数。
///
/// 续行/复活语义: 旧数据以 `\n` 结尾时, 那个 trailing `\n` 因新数据到达而「复活」
/// (它现在结束一行, 新行从 old_len 开始); 否则旧末行续着, 新字节里第一个 `\n`
/// 结束它。每 16 行补一条 stride 项 (行起始偏移)。正确性靠「append == 全量重建」对拍。
fn append_index(
    old_strides: &[u64],
    old_line_count: u64,
    old_data: &[u8],
    new_data: &[u8],
) -> (Vec<u64>, u64) {
    let old_len = old_data.len();
    if new_data.len() <= old_len {
        return (old_strides.to_vec(), old_line_count);
    }
    let tail = &new_data[old_len..];
    let mut strides = old_strides.to_vec();
    let mut count = old_line_count;

    // 旧数据以 \n 结尾 (或空): trailing \n 复活, 第 old_line_count 行从 old_len 开始
    if old_len == 0 || old_data[old_len - 1] == b'\n' {
        if count % INDEX_STRIDE == 0 {
            strides.push(old_len as u64);
        }
        count += 1;
    }

    // 扫描新字节: 每个非尾 \n 结束一行, 下一行从其后开始
    for p in memchr::memchr_iter(b'\n', tail) {
        let abs = old_len + p;
        if abs + 1 == new_data.len() {
            break; // 末尾换行不产生新行
        }
        if count % INDEX_STRIDE == 0 {
            strides.push((abs + 1) as u64);
        }
        count += 1;
    }
    (strides, count)
}

/// 顺序行迭代器 (见 [`LogFile::lines`])。
pub struct LineIter<'a> {
    data: &'a [u8],
    /// 下一行号。
    next: u64,
    /// 下一行起始字节。
    start: usize,
    count: u64,
}

impl<'a> Iterator for LineIter<'a> {
    type Item = (u64, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.count {
            return None;
        }
        let i = self.next;
        let end = match memchr::memchr(b'\n', &self.data[self.start..]) {
            Some(p) => self.start + p,
            None => self.data.len(),
        };
        let mut s = &self.data[self.start..end];
        if s.last() == Some(&b'\r') {
            s = &s[..s.len() - 1];
        }
        self.next += 1;
        self.start = if end < self.data.len() {
            end + 1
        } else {
            self.data.len()
        };
        Some((i, s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 落一个临时文件并打开 (mmap 需要真实文件)。
    /// 文件名必须全局唯一: Windows 拒绝截断/删除仍被映射的文件
    /// (ERROR_USER_MAPPED_FILE, T2 实测), pid+长度相同即撞名。
    fn open_with(content: &[u8]) -> LogFile {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "danqing-log-test-{}-{}-{}.log",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            content.len()
        ));
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(content).unwrap();
        }
        let lf = LogFile::open(&path).unwrap();
        std::fs::remove_file(&path).ok();
        lf
    }

    #[test]
    fn indexes_lines_and_strips_crlf() {
        let lf = open_with(b"alpha\r\nbeta\r\ngamma\r\n");
        assert_eq!(lf.line_count(), 3);
        assert_eq!(lf.line(0), b"alpha");
        assert_eq!(lf.line(1), b"beta");
        assert_eq!(lf.line(2), b"gamma");
    }

    #[test]
    fn handles_missing_trailing_newline() {
        let lf = open_with(b"one\ntwo");
        assert_eq!(lf.line_count(), 2);
        assert_eq!(lf.line(1), b"two");
    }

    #[test]
    fn empty_file_has_zero_lines() {
        let lf = open_with(b"");
        assert_eq!(lf.line_count(), 0);
        assert_eq!(lf.line(0), b"", "越界返回空片");
    }

    #[test]
    fn skips_utf8_bom() {
        let lf = open_with(b"\xEF\xBB\xBFhello\n");
        assert_eq!(lf.line(0), b"hello", "BOM 不进首行");
    }

    #[test]
    fn utf16le_with_bom_transcoded_on_open() {
        // UTF-16LE 带 BOM: 打开时转码 UTF-8 副本, 之后全走 UTF-8 路径
        let mut raw = vec![0xFF, 0xFE];
        for u in "INFO 你好\nWARN 世界\n".encode_utf16() {
            raw.extend_from_slice(&u.to_le_bytes());
        }
        let lf = open_with(&raw);
        assert_eq!(lf.stats().encoding, Encoding::Utf16Le, "检出原始编码");
        assert_eq!(
            lf.encoding(),
            Encoding::Utf8,
            "存储编码已转 UTF-8 (搜索模式构造的依据)"
        );
        assert_eq!(lf.line_count(), 2);
        assert_eq!(lf.line_lossy(0), "INFO 你好");
        assert_eq!(lf.line_lossy(1), "WARN 世界");
        let re = regex::bytes::Regex::new("WARN").unwrap();
        let (lines, total, _) = lf.search(&re, 100);
        assert_eq!(lines, vec![1], "转码副本上搜索照常");
        assert_eq!(total, 1);
    }

    #[test]
    fn utf16be_and_bare_le_supported() {
        // BE 带 BOM
        let mut raw = vec![0xFE, 0xFF];
        for u in "alpha\nbeta\n".encode_utf16() {
            raw.extend_from_slice(&u.to_be_bytes());
        }
        let lf = open_with(&raw);
        assert_eq!(lf.stats().encoding, Encoding::Utf16Be);
        assert_eq!(lf.line_count(), 2);
        assert_eq!(lf.line_lossy(1), "beta");
        // LE 无 BOM: 交替 NUL 启发检出
        let raw: Vec<u8> = "INFO no bom here\nWARN second\n"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let lf = open_with(&raw);
        assert_eq!(lf.stats().encoding, Encoding::Utf16Le, "无 BOM 启发检出");
        assert_eq!(lf.line_lossy(0), "INFO no bom here");
    }

    #[test]
    fn gbk_file_indexes_and_decodes() {
        // GBK: 原字节索引 (trail byte 不含 0x0A) + 行级 CP936 解码
        let lf = open_with(b"INFO \xD6\xD0\xCE\xC4\nWARN \xB4\xED\xCE\xF3\n");
        assert_eq!(lf.stats().encoding, Encoding::Gbk);
        assert_eq!(lf.line_count(), 2);
        assert_eq!(lf.line_lossy(0), "INFO 中文");
        assert_eq!(lf.line_lossy(1), "WARN 错误");
        // GBK 中文查询: 转码后字节搜索 (memmem 直白验证)
        let q = lf.encode_query("中文");
        assert!(
            memchr::memmem::find(lf.line(0), &q).is_some(),
            "GBK 查询字节命中行 0"
        );
        assert!(memchr::memmem::find(lf.line(1), &q).is_none());
    }

    #[test]
    fn search_maps_offsets_to_lines_deduped() {
        let lf = open_with(b"INFO ok\nERROR a ERROR b\nINFO fine\nERROR c\n");
        let re = regex::bytes::Regex::new("ERROR").unwrap();
        let (lines, total, _) = lf.search(&re, 100);
        assert_eq!(total, 3, "三处命中");
        assert_eq!(lines, vec![1, 3], "同双命中行只收一次");
    }

    #[test]
    fn search_cap_does_not_hide_total() {
        let lf = open_with(b"x\nx\nx\nx\n");
        let re = regex::bytes::Regex::new("x").unwrap();
        let (lines, total, _) = lf.search(&re, 2);
        assert_eq!(lines.len(), 2, "cap 生效");
        assert_eq!(total, 4, "总数如实");
    }

    /// xorshift64 (测试用确定性伪随机, 与 genlog 同款)。
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    #[test]
    fn stride_index_memory_bound() {
        // 10 万行: 稠密 u64 索引 = 800KB; 步进 16 索引 = 6,250 项 × 8B = 50KB。
        // T1 验收: 索引驻留从 48MB/GB 压到 ≤16MB/GB (本测试的缩小比例尺)。
        let mut content = Vec::new();
        for _ in 0..100_000 {
            content.extend_from_slice(b"line-of-some-text\n");
        }
        let lf = open_with(&content);
        assert_eq!(lf.line_count(), 100_000);
        assert!(
            lf.stats().index_bytes <= 60_000,
            "步进索引驻留应 ≤60KB (稠密索引为 800KB): 实测 {}",
            lf.stats().index_bytes
        );
    }

    #[test]
    fn stride_index_matches_dense_semantics() {
        // 变长行 (0..200B, 含空行) + 末行无换行, 逐行内容对拍。
        let mut rng = Rng(0x1234_5678_9ABC_DEF0);
        let mut content = Vec::new();
        let mut expected: Vec<Vec<u8>> = Vec::new();
        for _ in 0..5000 {
            let len = (rng.next() % 200) as usize;
            let line: Vec<u8> = (0..len).map(|_| b'a' + (rng.next() % 26) as u8).collect();
            content.extend_from_slice(&line);
            content.push(b'\n');
            expected.push(line);
        }
        content.extend_from_slice(b"tail-no-newline");
        expected.push(b"tail-no-newline".to_vec());
        let lf = open_with(&content);
        assert_eq!(lf.line_count(), expected.len() as u64, "行数一致");
        for (i, e) in expected.iter().enumerate() {
            assert_eq!(lf.line(i as u64), &e[..], "行 {i} 内容一致");
        }
        // 顺序迭代器与随机访问同一份语义 (全文谓词走 walker, 见 lines() 注释)
        let walked: Vec<&[u8]> = lf.lines().map(|(_, s)| s).collect();
        assert_eq!(walked.len(), expected.len(), "walker 行数一致");
        for (w, e) in walked.iter().zip(expected.iter()) {
            assert_eq!(*w, &e[..], "walker 与 line(i) 输出一致");
        }
        // walker 行号递增且从 0 起
        let ids: Vec<u64> = lf.lines().map(|(i, _)| i).collect();
        assert!(ids.windows(2).all(|w| w[1] == w[0] + 1), "walker 行号连续");
    }

    #[test]
    fn lines_walker_handles_empty_and_unterminated() {
        let lf = open_with(b"");
        assert_eq!(lf.lines().count(), 0, "空文件零行");
        let lf = open_with(b"only-no-newline");
        let rows: Vec<&[u8]> = lf.lines().map(|(_, s)| s).collect();
        assert_eq!(rows, vec![&b"only-no-newline"[..]], "末行无换行");
    }

    #[test]
    fn search_maps_offsets_across_stride_segments() {
        // 命中点散布在多个步进段 (段长 16 行), 验证偏移→行号映射跨段正确。
        let mut content = Vec::new();
        for i in 0..1000 {
            if i == 500 || i == 999 {
                content.extend_from_slice(b"MARK\n");
            } else {
                content.extend_from_slice(b"plain\n");
            }
        }
        let lf = open_with(&content);
        let re = regex::bytes::Regex::new("MARK").unwrap();
        let (lines, total, _) = lf.search(&re, 100);
        assert_eq!(lines, vec![500, 999], "跨段行号映射");
        assert_eq!(total, 2);
    }

    /// 唯一临时路径 (不创建; 配合 Windows 映射文件语义手工管理生命周期)。
    fn temp_path(tag: &str) -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "danqing-log-t3-{tag}-{}-{}.log",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ))
    }

    #[test]
    fn stale_detection_on_append_and_delete() {
        // 追加 (tail 常态增长): len 变 → 过期; 删除: 不可读 → 过期
        let path = temp_path("stale");
        std::fs::write(&path, b"a\nb\n").unwrap();
        let lf = LogFile::open(&path).unwrap();
        assert!(!lf.is_stale(&path), "未动不过期");
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, b"c\n"))
            .expect("映射存活期追加合法 (T3 实测)");
        assert!(lf.is_stale(&path), "追加后过期");
        std::fs::remove_file(&path).unwrap();
        assert!(lf.is_stale(&path), "删除后过期 (不可读)");
    }

    #[test]
    fn rebuild_after_create_rotation() {
        // create 流派轮转: rename 旧文件 + 新建同名 → rebuild 见到新内容
        let path = temp_path("rotate");
        std::fs::write(&path, b"old1\nold2\nold3\n").unwrap();
        let mut lf = LogFile::open(&path).unwrap();
        assert_eq!(lf.line_count(), 3);
        std::fs::rename(&path, path.with_extension("1")).expect("映射存活期改名合法 (T3 实测)");
        assert!(lf.is_stale(&path), "路径已指向别处");
        std::fs::write(&path, b"new1\n").unwrap();
        lf.rebuild(&path).expect("重建成功");
        assert_eq!(lf.line_count(), 1, "重建后行数反映新文件");
        assert_eq!(lf.line(0), b"new1");
        assert!(!lf.is_stale(&path), "重建后不过期");
        std::fs::remove_file(&path).ok();
        std::fs::remove_file(path.with_extension("1")).ok();
    }

    #[test]
    fn line_out_of_range_returns_empty_not_crash() {
        // 越界防御 (截断生存的第一道: 永不 panic/崩)
        let lf = open_with(b"a\nb\n");
        assert_eq!(lf.line(2), b"");
        assert_eq!(lf.line(u64::MAX), b"");
    }

    #[test]
    fn append_from_matches_full_rebuild() {
        // 分多次追加 (跨越步进边界 16), 每次 append_from 与全量 open 对拍
        let path = temp_path("append");
        std::fs::write(&path, b"alpha\nbeta\ngamma\n").unwrap();
        let mut cur = LogFile::open(&path).unwrap();
        let chunks: [&[u8]; 3] = [
            b"delta\nepsilon\n",
            b"zeta\neta\ntheta\niota\nkappa\nlambda\nmu\nnu\nxi\nomicron\npi\nrho\nsigma\ntau\n",
            b"upsilon\nphi\nchi\npsi\nomega\n",
        ];
        for chunk in chunks {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(chunk)
                .unwrap();
            let appended = LogFile::append_from(&cur, &path).unwrap();
            let full = LogFile::open(&path).unwrap();
            assert_eq!(appended.line_count(), full.line_count(), "行数一致");
            for i in 0..appended.line_count() {
                assert_eq!(appended.line(i), full.line(i), "行 {i} 内容一致");
            }
            cur = appended;
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn append_handles_partial_line_continuation() {
        // 旧末尾无 \n: 新字节先续旧行, 再开新行
        let path = temp_path("append-cont");
        std::fs::write(&path, b"hello ").unwrap();
        let lf = LogFile::open(&path).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"world\nnext\n")
            .unwrap();
        let appended = LogFile::append_from(&lf, &path).unwrap();
        let full = LogFile::open(&path).unwrap();
        assert_eq!(appended.line_count(), full.line_count(), "行数一致");
        assert_eq!(appended.line(0), b"hello world", "续行拼接");
        assert_eq!(appended.line(1), b"next");
        std::fs::remove_file(&path).ok();
    }
}
