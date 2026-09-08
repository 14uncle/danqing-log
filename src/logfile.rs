//! @author 十四叔
//! @date 2026/09/05
//!
//! 日志文件引擎: mmap + 行偏移索引 + 全文正则搜索。
//!
//! POC 的碾压主张全部落在这一层:
//! - 秒开 = mmap 零拷贝, 建立映射本身 O(1);
//! - 行索引 = memchr 扫 `\n` (SIMD) + 步进表 (每 16 行一记, ~3.2MB/GB,
//!   段内前扫定位, 见 INDEX_STRIDE 注释); ≥64MB 分段并行构建
//!   (单线程 1GB 冷扫 1028ms 破 1s 线, 2026-09-08 实测触发并行化, 见 build_line_index);
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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
/// overwrite (内容原位被换)。len+mtime 检不出「轮转后新文件更大」(create 流派
/// 新文件首块与旧文件不同), 故加 `head` (前 64 字节 FNV 哈希) 区分同文件增长与轮转。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    /// 文件长度。
    pub len: u64,
    /// 修改时间 (取不到为 None, 比对时 None≠Some 视为过期)。
    pub mtime: Option<SystemTime>,
    /// 前 64 字节 FNV-1a 哈希 (区分同文件增长 vs 轮转/覆写)。
    pub head: u64,
}

impl FileStat {
    /// 取路径当前状态。
    pub fn of(path: &Path) -> std::io::Result<Self> {
        let m = std::fs::metadata(path)?;
        Ok(Self {
            len: m.len(),
            mtime: m.modified().ok(),
            head: hash_head(path)?,
        })
    }
}

/// 「索引已取消」错误文案 (取消识别常量; 显示侧据此区分主动取消与真失败,
/// 措辞改动只动这里 —— review O2)。
pub const INDEX_CANCELLED: &str = "索引已取消";

/// 前 64 字节 FNV-1a 哈希 (首块指纹, 无依赖)。
fn hash_head(path: &Path) -> std::io::Result<u64> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut buf = [0u8; 64];
    let n = f.read(&mut buf)?;
    Ok(fnv_head(&buf[..n]))
}

/// FNV-1a (首块指纹的纯函数半; 快照自足 stat 与读盘路径共用, 算法必须同源)。
fn fnv_head(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// stat 快照自足构造 (review R1): 描述「被索引的这份字节」而非完成时刻的
/// 路径 —— len = 打开时刻元数据, head = map 期首块指纹, mtime 取打开句柄
/// (轮转 rename 后句柄仍指旧 inode = 快照身份)。索引期间的增长/轮转由
/// 下一轮 250ms poll 检出 (cur != known) 驱动追平/重建; 取完成时刻路径
/// stat 会让增长缺口永久漏尾、轮转嫁接把旧偏移错写到新内容上。
fn snapshot_stat(file: &File, file_bytes: u64, head_hash: u64) -> FileStat {
    FileStat {
        len: file_bytes,
        mtime: file.metadata().ok().and_then(|m| m.modified().ok()),
        head: head_hash,
    }
}

/// 步进索引步长: 每 STRIDE 行记一个绝对偏移, 段内 memchr 前扫定位。
/// 16 = 内存 (3.2MB/GB) 与随机访问 (前扫 ≤15 行 ≈ 2.7KB) 的实测平衡点,
/// 退化预案 stride=8 见 tasks/plan.md 决策 1。
const INDEX_STRIDE: u64 = 16;

/// 并行索引阈值: 小于此单线程 (线程调度开销回不来, 64MB 串行 ≈ 27ms)。
const PARALLEL_MIN_BYTES: usize = 64 << 20;

/// 索引线程上限: 热扫被内存带宽封顶 (~4-6 线程饱和), 冷扫缺页并发收益
/// ~8 见顶 (2026-09-08 实测: 冷 1GB 缺页驱动 996MiB/s vs 裸顺序读 1655MiB/s),
/// 再多只剩调度税。
const MAX_INDEX_THREADS: usize = 8;

/// 进度/取消检查粒度: 每累计 8MB 做一次原子操作。
/// 每换行检查 = 千万次级/GB 的原子税, 必须节流 (async-open plan D2)。
const PROGRESS_CHECK_STRIDE: u64 = 8 << 20;

/// 索引进度/取消钩子 (async-open 异步打开管道)。
///
/// 同步 API ([`LogFile::open`] / [`LogFile::append_from`]) 走 `Default` 全 None,
/// 检查点全部短路, 零可测开销; 异步管道 (open.rs OpenJob) 注入 Arc 句柄。
/// 钩子只读不改: 分段/归属/查行语义与无钩子路径逐字节一致 (对拍网钉着)。
#[derive(Debug, Default, Clone)]
pub struct IndexHooks {
    /// 已扫字节累加器 (各 chunk worker 每 ≥8MB 节流向此加; 取消早退不结清尾量,
    /// 故 分子<分母 ⟺ 被取消)。
    pub progress: Option<Arc<AtomicU64>>,
    /// 取消旗标: true → 扫描循环下一检查点早退; open 完成处复查返回 Err。
    pub cancel: Option<Arc<AtomicBool>>,
}

impl IndexHooks {
    /// 进度累加 (无句柄短路)。
    fn report(&self, bytes: u64) {
        if let Some(p) = &self.progress {
            p.fetch_add(bytes, Ordering::Relaxed);
        }
    }

    /// 是否已请求取消 (无句柄恒 false)。
    fn is_cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
    }

    /// 取消检查点 (bail 版): 已取消则返回 [`INDEX_CANCELLED`] Err。
    /// 扫描完成处与各阶段边界统一走这里, 不做内联重复。
    fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            anyhow::bail!(INDEX_CANCELLED);
        }
        Ok(())
    }
}

/// 步进索引段: 并行构建时每个线程一段, 段内语义与全局步进表一致
/// (第 j 项 = 段内第 j*STRIDE 行起点的绝对字节偏移)。
///
/// 不变式: 无空段 (0 行段合并时丢弃); 段按 `base_line` 严格递增;
/// `strides` 非空且 `strides[0]` = 段首行起点。单段 (串行/小文件) 布局
/// 与 2026-09-08 并行化前的全局表逐字节一致。
#[derive(Debug, Clone)]
struct Segment {
    /// 段首行的全局行号 (段内第 k 行 = 全局 base_line + k)。
    base_line: u64,
    /// 段内步进表 (绝对字节偏移)。
    strides: Vec<u64>,
}

/// 索引驻留字节 (各段步进表堆占用之和)。
fn index_resident_bytes(segments: &[Segment]) -> usize {
    segments
        .iter()
        .map(|s| s.strides.len() * std::mem::size_of::<u64>())
        .sum()
}

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

/// 追加结果分派 (review R3): 真增量 vs 退化全量重建。
/// 调用方按此分派换入链 (Append 链保过滤增量 / Rebuild 链清失效状态),
/// 不凭「发起时想做什么」猜 —— worker 打开时文件可能已轮转/缩容。
pub enum AppendOutcome {
    /// 同文件增长的增量追加 (索引只扫新字节)。
    Appended(LogFile),
    /// 退化为全量重建 (UTF-16 转码副本不适用增量; 或缩容/轮转防御兜底)。
    Rebuilt(LogFile),
}

/// 已映射的日志文件: 分段步进行索引 + 只读访问。
pub struct LogFile {
    data: FileData,
    /// 存储字节的编码 (UTF-16 文件转码后为 Utf8; 原始检出编码在 stats.encoding)。
    encoding: Encoding,
    /// 步进索引段 (不变式见 Segment; 非空 ⟺ line_count > 0)。
    segments: Vec<Segment>,
    /// 总行数 (各段行数之和, 单独存)。
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
        Self::open_with_hooks(path, &IndexHooks::default())
    }

    /// 带进度/取消钩子的打开 (async-open 管道; 同步语义见 [`Self::open`])。
    ///
    /// 取消语义: 扫描循环按 8MB 粒度早退, 完成处复查命中则返回
    /// 「索引已取消」Err —— 半成品索引永不交给调用方。UTF-16 转码副本
    /// 路径无细粒度进度 (read/transcode 两段各一次取消检查)。
    pub fn open_with_hooks(path: &Path, hooks: &IndexHooks) -> Result<Self> {
        let t0 = Instant::now();
        let file = File::open(path).with_context(|| format!("打开文件失败: {}", path.display()))?;
        let file_bytes = file.metadata().context("读取文件元信息失败")?.len();
        // 安全性: 映射只读; 已知风险 = 映射期间外部截断 (见模块头注释, T3 实测校准)。
        let map = unsafe { Mmap::map(&file).context("建立内存映射失败")? };
        let map_us = t0.elapsed().as_micros() as u64;

        let head = &map[..map.len().min(encoding::SAMPLE)];
        let detected = encoding::detect(head);
        // 快照首块指纹 (磁盘原字节, map 期取定; UTF-16 转码前同样成立)
        let head_hash = fnv_head(&map[..map.len().min(64)]);
        let (data, data_enc) = if detected.is_utf16() {
            // 2 字节编码不适合字节级索引: 一次性转码 UTF-8 副本 (1GB UTF-16 ≈ 500MB UTF-8)
            let raw =
                std::fs::read(path).with_context(|| format!("读取文件失败: {}", path.display()))?;
            hooks.check_cancelled()?;
            drop(map);
            let utf8 = encoding::transcode_utf16(detected == Encoding::Utf16Le, &raw);
            hooks.check_cancelled()?;
            (FileData::Owned(utf8), Encoding::Utf8)
        } else {
            (FileData::Mapped(map), detected)
        };

        let t1 = Instant::now();
        let (segments, line_count) = build_line_index_with(data.as_bytes(), hooks);
        hooks.check_cancelled()?;
        let index = t1.elapsed();
        let index_bytes = index_resident_bytes(&segments);
        // stat 快照自足 (review R1, 语义见 snapshot_stat)
        let stat = snapshot_stat(&file, file_bytes, head_hash);

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
            segments,
            line_count,
            stat,
            stats,
        })
    }

    /// 空占位 (无参启动): 内存空数据, 0 行 UTF-8, 统计全零。
    /// 读路径与真实空文件同语义; GUI 层靠自身标记区分「占位」与「真实的空文件」。
    pub fn empty() -> Self {
        Self {
            data: FileData::Owned(Vec::new()),
            encoding: Encoding::Utf8,
            segments: Vec::new(),
            line_count: 0,
            stat: FileStat {
                len: 0,
                mtime: None,
                head: 0,
            },
            stats: OpenStats {
                file_bytes: 0,
                map_us: 0,
                index: Duration::ZERO,
                line_count: 0,
                index_bytes: 0,
                encoding: Encoding::Utf8,
            },
        }
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
    /// 定位: 二分段基准定段 → 段内步进表 → memchr 前扫 (local % STRIDE) 个换行。
    pub fn line(&self, i: u64) -> &[u8] {
        if i >= self.line_count {
            return &[];
        }
        let data = self.data.as_bytes();
        let seg = self.segment_of_line(i);
        let local = i - seg.base_line;
        let mut start = seg.strides[(local / INDEX_STRIDE) as usize] as usize;
        for _ in 0..(local % INDEX_STRIDE) {
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

    /// 全局行号 → 所在段 (不变式: 无空段 + base_line 严格递增 ⇒ 恰有一段)。
    /// 调用方保证 i < line_count。
    fn segment_of_line(&self, i: u64) -> &Segment {
        let idx = self
            .segments
            .partition_point(|s| s.base_line <= i)
            .saturating_sub(1);
        &self.segments[idx]
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
            self.segments[0].strides[0] as usize
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
        let seg = self.segment_of_line(start_line);
        let local = start_line - seg.base_line;
        let mut start = seg.strides[(local / INDEX_STRIDE) as usize] as usize;
        for _ in 0..(local % INDEX_STRIDE) {
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

    /// 字节偏移 → 行号: 先按段首行起点二分段 (分段是字节的连续划分),
    /// 再段内步进表二分 + memchr 前扫 (≤STRIDE-1 行)。
    /// 仅供 search 的命中回落 (命中数封顶 cap, 成本有界)。
    fn line_of_offset(&self, off: u64) -> u64 {
        let data = self.data.as_bytes();
        let seg_idx = self
            .segments
            .partition_point(|s| s.strides[0] <= off)
            .saturating_sub(1);
        let seg = &self.segments[seg_idx];
        let entry = seg.strides.partition_point(|&o| o <= off).saturating_sub(1);
        let mut line = seg.base_line + entry as u64 * INDEX_STRIDE;
        let mut p = seg.strides[entry] as usize;
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
        // 同步调用方不区分增量/重建 (分派语义见 AppendOutcome, 异步管道用)
        match Self::append_from_with_hooks(old, path, &IndexHooks::default())? {
            AppendOutcome::Appended(f) | AppendOutcome::Rebuilt(f) => Ok(f),
        }
    }

    /// 带进度/取消钩子的追加 (async-open 追平管道; 进度分母 = 新字节数)。
    ///
    /// 退化分支 (UTF-16/缩容) 同样带钩子并返回 [`AppendOutcome::Rebuilt`]
    /// (review R3): 被取消的 job 不在兜底分支白跑全量, 调用方按产物分派
    /// 换入链而非按发起意图。
    pub fn append_from_with_hooks(
        old: &Self,
        path: &Path,
        hooks: &IndexHooks,
    ) -> Result<AppendOutcome> {
        // UTF-16 转码副本: 索引建在 UTF-8 副本上, 增量不适用 → 全量
        if matches!(old.data, FileData::Owned(_)) {
            return Ok(AppendOutcome::Rebuilt(Self::open_with_hooks(path, hooks)?));
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
            return Ok(AppendOutcome::Rebuilt(Self::open_with_hooks(path, hooks)?));
        }

        let t1 = Instant::now();
        let (mut segments, line_count) = append_index(
            &old.segments,
            old.line_count,
            old.data.as_bytes(),
            new_data,
            hooks,
        );
        if hooks.is_cancelled() {
            anyhow::bail!(INDEX_CANCELLED);
        }
        if let Some(last) = segments.last_mut() {
            last.strides.shrink_to_fit();
        }
        let index = t1.elapsed();
        let index_bytes = index_resident_bytes(&segments);
        // stat 快照自足 (review R1, 语义见 snapshot_stat)
        let stat = snapshot_stat(
            &file,
            file_bytes,
            fnv_head(&new_data[..new_data.len().min(64)]),
        );

        let stats = OpenStats {
            file_bytes,
            map_us,
            index,
            line_count,
            index_bytes,
            encoding: old.stats.encoding,
        };
        Ok(AppendOutcome::Appended(Self {
            data: FileData::Mapped(map),
            encoding: old.encoding,
            segments,
            line_count,
            stat,
            stats,
        }))
    }
}

/// 步进行索引: SIMD memchr 扫 `\n`, 每 STRIDE 行记一个起点偏移, 并数总行数。
///
/// UTF-8 BOM 跳过 (首行从 BOM 之后开始)。文件以 `\n` 结尾时末尾换行不产生
/// 新行 (不存在「最后一空行」)。
/// ≥PARALLEL_MIN_BYTES 走分段并行: 单线程 1GB 冷扫实测 1028ms (2026-09-08,
/// 缺页驱动 I/O 996MiB/s vs 裸顺序读 1655MiB/s) 破了 1s 线, 触发并行化;
/// 多线程缺页并发同时提速冷盘。分段布局与串行单段仅内部排列不同,
/// 查行语义由「并行 == 串行」对拍钉死 (见 tests)。
/// 带钩子的构建入口 (async-open 进度/取消; 同步调用传 Default 全 None 钩子)。
fn build_line_index_with(data: &[u8], hooks: &IndexHooks) -> (Vec<Segment>, u64) {
    let threads = if data.len() >= PARALLEL_MIN_BYTES {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(MAX_INDEX_THREADS)
    } else {
        1
    };
    if threads <= 1 {
        let (mut strides, count) = scan_chunk(data, 0, true, data.len() as u64, hooks);
        strides.shrink_to_fit();
        let segments = if count > 0 {
            vec![Segment {
                base_line: 0,
                strides,
            }]
        } else {
            Vec::new()
        };
        return (segments, count);
    }
    build_index_parallel(data, threads, hooks)
}

/// 并行分段构建: 按字节等分, 每线程扫一段 (scan_chunk 的行归属规则保证不重不漏),
/// 合并时丢弃空段 (如整段被一条超长行占满)、按序累算 base_line。
fn build_index_parallel(data: &[u8], threads: usize, hooks: &IndexHooks) -> (Vec<Segment>, u64) {
    let total = data.len() as u64;
    let chunk_len = data.len().div_ceil(threads);
    let mut segments = Vec::new();
    let mut total_lines = 0u64;
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for c in 0..threads {
            let start = c * chunk_len;
            if start >= data.len() {
                break;
            }
            let end = (start + chunk_len).min(data.len());
            let slice = &data[start..end];
            handles.push(s.spawn(move || scan_chunk(slice, start as u64, c == 0, total, hooks)));
        }
        // 按 chunk 顺序 join: 合并确定性, base_line 单调
        for h in handles {
            let (mut strides, count) = h.join().expect("索引线程 panic");
            if count == 0 {
                continue; // 空段丢弃, 保「无空段」不变式
            }
            strides.shrink_to_fit();
            segments.push(Segment {
                base_line: total_lines,
                strides,
            });
            total_lines += count;
        }
    });
    (segments, total_lines)
}

/// 扫一段字节: 产段内步进表 (绝对偏移) + 段内行数。串行与并行的同一工函数。
///
/// 行归属: chunk 拥有「起点落在 (base, base+len] 的行」(由其内 `\n` 派生);
/// chunk 0 额外拥有第 0 行 (BOM 跳过)。全文件末尾 `\n` 不产生新行
/// (仅末 chunk 的最后一字节触发)。无 `\n` 的 chunk 产 0 行 (空段由合并丢弃)。
/// 钩子: 每累计 ≥8MB 报一次进度 + 查一次取消 (节流, 见 PROGRESS_CHECK_STRIDE);
/// 取消早退时不结清尾量 (分子<分母 ⟺ 被取消), 半成品由 open 完成处复查拦截。
fn scan_chunk(
    slice: &[u8],
    base: u64,
    is_first: bool,
    total_len: u64,
    hooks: &IndexHooks,
) -> (Vec<u64>, u64) {
    // 预分配: 经验值 ~64 字节/行 + 步进 16, 避免 Vec 反复扩容
    let mut strides = Vec::with_capacity(slice.len() / 64 / INDEX_STRIDE as usize + 16);
    let mut count = 0u64;
    if is_first && !slice.is_empty() {
        let bom = if slice.starts_with(&[0xEF, 0xBB, 0xBF]) {
            3u64
        } else {
            0
        };
        strides.push(bom); // 第 0 行
        count = 1;
    }
    let mut last_report = 0u64; // slice 内已报进度的偏移
    for p in memchr::memchr_iter(b'\n', slice) {
        let next = base + p as u64 + 1;
        if next == total_len {
            break; // 末尾换行不产生新行
        }
        if count % INDEX_STRIDE == 0 {
            strides.push(next);
        }
        count += 1;
        if p as u64 - last_report >= PROGRESS_CHECK_STRIDE {
            hooks.report(p as u64 - last_report);
            last_report = p as u64;
            if hooks.is_cancelled() {
                return (strides, count); // 早退不结清尾量
            }
        }
    }
    hooks.report(slice.len() as u64 - last_report); // 尾量结清: 完成时分子=分母
    (strides, count)
}

/// 增量索引: 只扫 `[old_len, new_len)` 的新字节, 往末段追加 stride 项 + 行数。
///
/// 续行/复活语义: 旧数据以 `\n` 结尾时, 那个 trailing `\n` 因新数据到达而「复活」
/// (它现在结束一行, 新行从 old_len 开始); 否则旧末行续着, 新字节里第一个 `\n`
/// 结束它。每 16 行 (末段段内行号) 补一条 stride 项 (行起始偏移)。
/// 正确性靠「append == 全量重建」语义对拍 —— 分段布局可与全量重建不同
/// (追加只扩末段, 重段边界不挪动), 查行语义一致。
fn append_index(
    old_segments: &[Segment],
    old_line_count: u64,
    old_data: &[u8],
    new_data: &[u8],
    hooks: &IndexHooks,
) -> (Vec<Segment>, u64) {
    let old_len = old_data.len();
    if new_data.len() <= old_len {
        return (old_segments.to_vec(), old_line_count);
    }
    let tail = &new_data[old_len..];
    let mut segments = old_segments.to_vec();
    let mut count = old_line_count;

    // 旧数据以 \n 结尾 (或空): trailing \n 复活, 第 old_line_count 行从 old_len 开始
    if old_len == 0 || old_data[old_len - 1] == b'\n' {
        if segments.is_empty() {
            // 旧文件零行 ⟺ 空文件: 首段从此开始
            segments.push(Segment {
                base_line: 0,
                strides: Vec::new(),
            });
        }
        let last = segments.last_mut().expect("上一行保证非空");
        let local = count - last.base_line;
        if local % INDEX_STRIDE == 0 {
            last.strides.push(old_len as u64);
        }
        count += 1;
    }

    // 扫描新字节: 每个非尾 \n 结束一行, 下一行从其后开始
    let mut last_report = 0u64; // tail 内已报进度的偏移 (钩子节流同 scan_chunk)
    for p in memchr::memchr_iter(b'\n', tail) {
        let abs = old_len + p;
        if abs + 1 == new_data.len() {
            break; // 末尾换行不产生新行
        }
        let last = segments.last_mut().expect("旧文件非空必有段");
        let local = count - last.base_line;
        if local % INDEX_STRIDE == 0 {
            last.strides.push((abs + 1) as u64);
        }
        count += 1;
        if p as u64 - last_report >= PROGRESS_CHECK_STRIDE {
            hooks.report(p as u64 - last_report);
            last_report = p as u64;
            if hooks.is_cancelled() {
                return (segments, count); // 早退不结清尾量
            }
        }
    }
    hooks.report(tail.len() as u64 - last_report); // 尾量结清
    (segments, count)
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
    fn empty_placeholder_matches_empty_file_semantics() {
        // 无参启动的空占位: 与真实空文件同语义 (0 行/越界空片), 编码恒 UTF-8
        let lf = LogFile::empty();
        assert_eq!(lf.line_count(), 0);
        assert_eq!(lf.line(0), b"");
        assert_eq!(lf.encoding(), Encoding::Utf8);
        assert_eq!(lf.stats().file_bytes, 0);
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

    /// 用现成索引拼一个 LogFile (串行/并行/追加产物的语义对拍载体)。
    fn logfile_from_parts(content: Vec<u8>, segments: Vec<Segment>, line_count: u64) -> LogFile {
        let file_bytes = content.len() as u64;
        LogFile {
            data: FileData::Owned(content),
            encoding: Encoding::Utf8,
            segments,
            line_count,
            stat: FileStat {
                len: file_bytes,
                mtime: None,
                head: 0,
            },
            stats: OpenStats {
                file_bytes,
                map_us: 0,
                index: Duration::ZERO,
                line_count,
                index_bytes: 0,
                encoding: Encoding::Utf8,
            },
        }
    }

    /// 刁难语料: BOM + 300KB 超长行 (4 段并行下整段无换行 → 制造空段)
    /// + 变长行 (0..200B 含空行) + 末尾无换行。返回 (字节, 每行起点偏移表)。
    fn tricky_corpus() -> (Vec<u8>, Vec<u64>) {
        let mut content = Vec::new();
        content.extend_from_slice(b"\xEF\xBB\xBF");
        let mut starts = vec![3u64]; // 第 0 行 = BOM 后的超长行
        content.extend_from_slice(&vec![b'L'; 300_000]);
        content.push(b'\n');
        let mut rng = Rng(0xDEAD_BEEF_1234_5678);
        for _ in 0..1500 {
            starts.push(content.len() as u64);
            let len = (rng.next() % 200) as usize;
            for _ in 0..len {
                content.push(b'a' + (rng.next() % 26) as u8);
            }
            content.push(b'\n');
        }
        starts.push(content.len() as u64);
        content.extend_from_slice(b"tail-no-newline");
        (content, starts)
    }

    #[test]
    fn parallel_segments_match_serial_semantics() {
        let (content, starts) = tricky_corpus();
        let content_len = content.len() as u64;
        let (ser_segs, ser_n) = build_line_index_with(&content, &IndexHooks::default()); // 串行 (< 并行阈值)
        let (par_segs, par_n) = build_index_parallel(&content, 4, &IndexHooks::default()); // 强制 4 段
        assert_eq!(ser_n, par_n, "行数一致");
        assert_eq!(ser_segs.len(), 1, "小文件串行单段");
        assert!(
            par_segs.len() > 1,
            "确实多段 (空段已丢弃): {} 段",
            par_segs.len()
        );
        // 段不变式: base_line 严格递增 + 无空段
        for w in par_segs.windows(2) {
            assert!(w[0].base_line < w[1].base_line, "base_line 严格递增");
        }
        for s in &par_segs {
            assert!(!s.strides.is_empty(), "无空段");
        }
        let a = logfile_from_parts(content.clone(), ser_segs, ser_n);
        let b = logfile_from_parts(content, par_segs, par_n);
        assert_eq!(a.line_count(), starts.len() as u64, "行数 == 语料预期");
        // line(i) 逐行对拍
        for i in 0..a.line_count() {
            assert_eq!(a.line(i), b.line(i), "行 {i} 串并一致");
        }
        // 偏移→行号对拍: 行起点 / 换行符本身 / 均匀探针
        for (i, &st) in starts.iter().enumerate() {
            assert_eq!(b.line_of_offset(st), i as u64, "行 {i} 起点回落");
            if i + 1 < starts.len() {
                let nl = starts[i + 1] - 1; // 行 i 的换行符属于行 i
                assert_eq!(b.line_of_offset(nl), i as u64, "行 {i} 换行符归属");
            }
        }
        for off in (0..content_len).step_by(4093) {
            assert_eq!(
                a.line_of_offset(off),
                b.line_of_offset(off),
                "偏移 {off} 串并一致"
            );
        }
        // lines_from 跨段边界对拍 (段基 ±1)
        let bases: Vec<u64> = b.segments.iter().map(|s| s.base_line).collect();
        for probe in bases
            .iter()
            .flat_map(|&base| [base.saturating_sub(1), base, base + 1])
        {
            if probe >= a.line_count() {
                continue;
            }
            let wa: Vec<&[u8]> = a.lines_from(probe).map(|(_, s)| s).collect();
            let wb: Vec<&[u8]> = b.lines_from(probe).map(|(_, s)| s).collect();
            assert_eq!(wa, wb, "lines_from({probe}) 串并一致");
        }
    }

    #[test]
    fn append_on_parallel_segments_matches_rebuild() {
        // 变体 A: 旧数据以 \n 结尾 (trailing 复活); 变体 B: 旧末行无换行 (续行)
        for resurrect in [true, false] {
            let mut old = Vec::new();
            let mut rng = Rng(0xABCD_EF01_2345_6789);
            for _ in 0..800 {
                let len = (rng.next() % 120) as usize;
                for _ in 0..len {
                    old.push(b'a' + (rng.next() % 26) as u8);
                }
                old.push(b'\n');
            }
            if !resurrect {
                old.extend_from_slice(b"partial");
            }
            let (old_segs, old_n) = build_index_parallel(&old, 4, &IndexHooks::default()); // 强制多段
            assert!(old_segs.len() > 1, "确实多段 (复活={resurrect})");
            let mut new = old.clone();
            new.extend_from_slice(b"new-line-one\nnew-line-two\nthird-no-newline");
            let (app_segs, app_n) =
                append_index(&old_segs, old_n, &old, &new, &IndexHooks::default());
            let (full_segs, full_n) = build_line_index_with(&new, &IndexHooks::default()); // 串行全量
            assert_eq!(app_n, full_n, "行数一致 (复活={resurrect})");
            let a = logfile_from_parts(new.clone(), app_segs, app_n);
            let f = logfile_from_parts(new, full_segs, full_n);
            for i in 0..f.line_count() {
                assert_eq!(a.line(i), f.line(i), "行 {i} 追加==全量 (复活={resurrect})");
            }
        }
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
    fn file_stat_head_distinguishes_rotation_with_larger_file() {
        // create 流派轮转 + 新文件更大: 仅 len+mtime 会误判为增长, head 哈希兜住
        let path = temp_path("rotate-larger");
        std::fs::write(&path, b"AAAA\nBBBB\n").unwrap();
        let lf = LogFile::open(&path).unwrap();
        let known = lf.stat_snapshot();
        std::fs::rename(&path, path.with_extension("rotated")).unwrap();
        // 新文件更大且首块不同 (X 不是 A)
        std::fs::write(&path, b"XXXX\nYYYY\nZZZZ\nWWWW\n").unwrap();
        let cur = FileStat::of(&path).unwrap();
        assert!(cur.len > known.len, "新文件更大 (纯 len 会误判增长)");
        assert_ne!(cur.head, known.head, "首块不同 → 判轮转");
        assert!(lf.is_stale(&path), "过期检出");
        std::fs::remove_file(&path).ok();
        std::fs::remove_file(path.with_extension("rotated")).ok();
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

    /// MiB 级临时日志 (钩子测试需要跨过 8MB 检查点; open_with 的内存语料
    /// 走默认钩子, 故单独落盘)。
    fn temp_big_file(tag: &str, mib: usize) -> std::path::PathBuf {
        let path = temp_path(tag);
        {
            let mut f = File::create(&path).unwrap();
            let chunk = b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abc\n";
            for _ in 0..(mib << 20) / chunk.len() {
                f.write_all(chunk).unwrap();
            }
        }
        path
    }

    #[test]
    fn hooks_cancelled_open_returns_error_and_early_exits() {
        let path = temp_big_file("hooks-cancel", 20);
        let progress = Arc::new(AtomicU64::new(0));
        let hooks = IndexHooks {
            progress: Some(Arc::clone(&progress)),
            cancel: Some(Arc::new(AtomicBool::new(true))), // 预置取消
        };
        let err = LogFile::open_with_hooks(&path, &hooks)
            .err()
            .expect("预置取消应报错");
        assert!(
            err.to_string().contains("索引已取消"),
            "取消报「索引已取消」: {err:#}"
        );
        let done = progress.load(Ordering::Relaxed);
        assert!(
            done < 20 << 20,
            "早退证据: 首个 8MB 检查点即停, 分子 {done} < 分母"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn hooks_progress_reports_exact_total() {
        let path = temp_big_file("hooks-progress", 20);
        let progress = Arc::new(AtomicU64::new(0));
        let hooks = IndexHooks {
            progress: Some(Arc::clone(&progress)),
            cancel: None,
        };
        let file_len = std::fs::metadata(&path).unwrap().len();
        LogFile::open_with_hooks(&path, &hooks).expect("打开成功");
        assert_eq!(
            progress.load(Ordering::Relaxed),
            file_len,
            "尾量结清: 完成时分子=分母"
        );
        // 并行臂同款结清 (强制 4 段, 小语料)
        let data: Vec<u8> = (0..100_000).flat_map(|_| b"line-here\n".to_vec()).collect();
        let par_progress = Arc::new(AtomicU64::new(0));
        let par_hooks = IndexHooks {
            progress: Some(Arc::clone(&par_progress)),
            cancel: None,
        };
        build_index_parallel(&data, 4, &par_hooks);
        assert_eq!(
            par_progress.load(Ordering::Relaxed),
            data.len() as u64,
            "并行各段结清之和 = 总字节"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn hooks_cancelled_append_returns_error() {
        // 追平管道: append_from_with_hooks 预置取消 → 「索引已取消」
        let path = temp_big_file("hooks-append", 1);
        let lf = LogFile::open(&path).unwrap();
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            f.write_all(b"more\nlines\n").unwrap();
        }
        let hooks = IndexHooks {
            progress: None,
            cancel: Some(Arc::new(AtomicBool::new(true))),
        };
        let err = LogFile::append_from_with_hooks(&lf, &path, &hooks)
            .err()
            .expect("预置取消应报错");
        assert!(err.to_string().contains(INDEX_CANCELLED), "{err:#}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn stat_snapshot_matches_path_stat_when_unchanged() {
        // review R1: stat 快照自足构造必须与读盘口径同源 —— 文件未动时
        // len/head 与 FileStat::of 完全一致 (竞态窗口的行为差异见 R1 注释,
        // 无法在单测确定性复现; 本测试钉住常态路径不被改坏)
        let path = temp_path("stat-identity");
        std::fs::write(&path, b"alpha\nbeta\ngamma\n").unwrap();
        let lf = LogFile::open(&path).unwrap();
        let snap = lf.stat_snapshot();
        let cur = FileStat::of(&path).unwrap();
        assert_eq!(snap.len, cur.len, "len = 快照字节数");
        assert_eq!(snap.head, cur.head, "head 与读盘指纹同源一致");
        assert!(!lf.is_stale(&path), "未动不过期");
        // append_from 的 stat 同样快照自足
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"delta\n")
            .unwrap();
        let appended = LogFile::append_from(&lf, &path).unwrap();
        let snap2 = appended.stat_snapshot();
        let cur2 = FileStat::of(&path).unwrap();
        assert_eq!(snap2.len, cur2.len);
        assert_eq!(snap2.head, cur2.head);
        assert!(!appended.is_stale(&path), "追加后新旧一致不过期");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn append_outcome_marks_rebuilt_and_fallback_carries_hooks() {
        // review R3: 缩容兜底 → Rebuilt 分派 + 钩子生效 (取消不白跑)
        let path = temp_path("append-rebuilt");
        std::fs::write(&path, b"l1\nl2\nl3\nl4\nl5\n").unwrap();
        let lf = LogFile::open(&path).unwrap();
        std::fs::rename(&path, path.with_extension("old")).expect("映射存活期改名合法 (T3)");
        std::fs::write(&path, b"tiny\n").unwrap(); // 同路径更小的新文件 → 缩容兜底
        let out = LogFile::append_from_with_hooks(&lf, &path, &IndexHooks::default()).unwrap();
        match out {
            AppendOutcome::Rebuilt(f) => {
                assert_eq!(f.line_count(), 1, "全量重建看到新文件内容")
            }
            AppendOutcome::Appended(_) => panic!("缩容必须判 Rebuilt"),
        }
        // 兜底分支取消旗标生效 (review R3a: 不在退化路径白跑全量)
        let hooks = IndexHooks {
            progress: None,
            cancel: Some(Arc::new(AtomicBool::new(true))),
        };
        let err = LogFile::append_from_with_hooks(&lf, &path, &hooks)
            .err()
            .expect("预置取消应报错");
        assert!(err.to_string().contains(INDEX_CANCELLED), "{err:#}");
        std::fs::remove_file(&path).ok();
        std::fs::remove_file(path.with_extension("old")).ok();
    }
}
