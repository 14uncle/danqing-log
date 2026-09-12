//! @author 十四叔
//! @date 2026/09/12
//!
//! 日志级别分类与计数 (level-histogram 模块, spec: docs/specs/SPEC-level-histogram.md):
//! 把「级别分布」从渲染期逐可见行提前到打开期一趟算清。
//!
//! 本模块只做纯逻辑 (分类 + 计数), 不含任何 UI; 侧栏呈现见 `src/histogram.rs`。
//!
//! 为什么与 `view.rs` 的 `level_color` 分桶不同: 后者是**整行着色**, INFO 走默认色
//! 是有意的降噪策略 (用户 2026-09-06 验收); 而计数必须有 INFO 桶当基线 —— 没有基线
//! 就看不出错误有多稀少。两者有意不共用, 详见 spec 的 Never 项。

use crate::jsonl::{self, Schema};
use crate::logfile::LogFile;

/// 级别桶 (严重度降序, 顺序即侧栏显示序)。
///
/// `DebugTrace` 合并 DEBUG 与 TRACE: 两者在既有配色里同为灰, 拆开无信息增量。
/// `FATAL` 与 `ERROR` 拆开: FATAL 才是「一眼」要抓的那一行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Fatal,
    Error,
    Warn,
    Info,
    DebugTrace,
    /// 未识别出级别的行 (无级别词, 或词在判据区间之外)。
    Other,
}

impl Level {
    /// 全部桶, 严重度降序 —— 侧栏按此序渲染。
    pub const ALL: [Level; 6] = [
        Level::Fatal,
        Level::Error,
        Level::Warn,
        Level::Info,
        Level::DebugTrace,
        Level::Other,
    ];

    /// 侧栏显示的短名。
    pub fn label(self) -> &'static str {
        match self {
            Level::Fatal => "FATAL",
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::DebugTrace => "DEBUG",
            Level::Other => "其他",
        }
    }
}

/// 判据区间: 行首字节数。日志级别几乎都在行首; 200 沿用 `view.rs::level_color`
/// 的既定区间。区间之外出现的级别词不认 —— 避免把正文里的 "error" 当成级别。
const LEVEL_HEAD_BYTES: usize = 200;

/// 关键词表: (关键词, 位)。
const KEYWORDS: [(&[u8], u8); 6] = [
    (b"FATAL", 1 << 0),
    (b"ERROR", 1 << 1),
    (b"WARN", 1 << 2),
    (b"INFO", 1 << 3),
    (b"DEBUG", 1 << 4),
    (b"TRACE", 1 << 5),
];

/// 优先级降序 (首个置位的桶胜出)。`DEBUG`/`TRACE` 两位共指 [`Level::DebugTrace`]。
const PRIORITY: [(Level, u8); 5] = [
    (Level::Fatal, 1 << 0),
    (Level::Error, 1 << 1),
    (Level::Warn, 1 << 2),
    (Level::Info, 1 << 3),
    (Level::DebugTrace, (1 << 4) | (1 << 5)),
];

/// 扫一组首字母 (恒 3 个, 即 `memchr3` 上限), 命中处比对完整关键词并置位。
///
/// 为什么按首字母分组扫而不是逐个 memmem: 一行最多要走 6 个关键词, 而
/// `memmem` 每次调用都要为 needle 建 prefilter —— 在 200 字节这种短 haystack 上,
/// 这笔 setup 开销比扫描本身还贵 (实测 1GB 明文 157ms)。`memchr3` 是纯 SIMD 字节
/// 搜索、零 setup, 两趟覆盖 6 个首字母 (`"FEW"` 与 `"IDT"`), 只在真正撞上大写字母
/// 的位置才做一次 `starts_with` 比对 —— 日志行里大写字母本来就稀。
/// 改成这个写法后 1GB 明文 94ms / JSONL 75ms。
fn scan_group(head: &[u8], firsts: [u8; 3], hits: &mut u8) {
    let [f0, f1, f2] = firsts;
    let mut from = 0usize;
    while from < head.len() {
        let Some(off) = memchr::memchr3(f0, f1, f2, &head[from..]) else {
            return;
        };
        let at = from + off;
        let first = head[at];
        for (kw, bit) in KEYWORDS {
            // 首字节先筛, 故每个命中点至多一次 starts_with。
            if kw[0] == first && head[at..].starts_with(kw) {
                *hits |= bit;
            }
        }
        from = at + 1;
    }
}

/// 行级别分类: 读行首 [`LEVEL_HEAD_BYTES`] 字节, 按优先级首个命中即定。
///
/// 优先级 `FATAL` > `ERROR` > `WARN` > `INFO` > `DEBUG`/`TRACE` > `其他`,
/// 故一行含多个级别词时**只归一个桶**。
///
/// **大小写敏感** (与 `view.rs::level_color` / `level_cell_color` 一致): 否则
/// `"no errors found"` 这类正文会污染计数, 而柱条数字必须可信。代价是
/// `"error: ..."` 这类小写级别不识别, 归入 `其他`。
///
/// 与「逐个关键词 memmem 早退」的朴素写法**语义完全等价** (含优先级与大小写),
/// 差别只在常数: 朴素版 1GB 明文 157ms, 本版实测见 `tasks/todo-level-histogram.md`。
pub fn classify_level(line: &[u8]) -> Level {
    let head = &line[..line.len().min(LEVEL_HEAD_BYTES)];
    let mut hits: u8 = 0;
    scan_group(head, *b"FEW", &mut hits);
    scan_group(head, *b"IDT", &mut hits);
    for (level, bit) in PRIORITY {
        if hits & bit != 0 {
            return level;
        }
    }
    Level::Other
}

/// 级别类列名清单 (小写比对): 列发现的首见顺序里找这些名字。
///
/// 只认清单内的名字是**有意的保守**: 猜错列 (比如把 `msg` 当级别) 会给出
/// 一份看似合理实则全错的计数, 比「找不到 → 降级只读」糟得多。
const LEVEL_COLUMN_NAMES: [&str; 6] = [
    "level",
    "severity",
    "lvl",
    "loglevel",
    "log_level",
    "priority",
];

/// 字段值分类: **前缀匹配**, 与过滤子句 `Op::Prefix` 同语义。
///
/// 为什么不复用 [`classify_level`] (子串匹配): 计数口径必须与**点下去之后的
/// 筛选结果**一致, 否则柱条数字与实际行数打架 (spec D2 一致性红线)。
/// 过滤的等值比较是**字节全等**, 若计数按子串, 文件里写 `WARNING` 时柱条数得到
/// 而 `level=WARN` 筛出 0 行。改成前缀匹配 + 子句用 `level=WARN*` 之后,
/// 两边对同一 token 走同一个判断 —— 一致性是构造保证, 不是碰巧。
///
/// 六个 token 首字母互不相同 (F/E/W/I/D/T), 故前缀匹配天然互斥, 无需优先级。
pub fn classify_field_value(value: &[u8]) -> Level {
    // 首字母互斥 (F/E/W/I/D/T) → 一次分派 + 一次前缀校验, 无优先级问题。
    match value.first() {
        Some(b'F') if value.starts_with(b"FATAL") => Level::Fatal,
        Some(b'E') if value.starts_with(b"ERROR") => Level::Error,
        Some(b'W') if value.starts_with(b"WARN") => Level::Warn,
        Some(b'I') if value.starts_with(b"INFO") => Level::Info,
        Some(b'D') if value.starts_with(b"DEBUG") => Level::DebugTrace,
        Some(b'T') if value.starts_with(b"TRACE") => Level::DebugTrace,
        _ => Level::Other,
    }
}

/// 从列定义里找级别类列 (见 [`LEVEL_COLUMN_NAMES`])。找不到 → None (降级只读)。
pub fn find_level_column(schema: &Schema) -> Option<&str> {
    schema.columns.iter().find_map(|c| {
        let name = c.name.as_str();
        LEVEL_COLUMN_NAMES
            .iter()
            .any(|k| name.eq_ignore_ascii_case(k))
            .then_some(name)
    })
}

/// 该桶的点选子句 (前缀通配 `col=TOKEN*`); 无单子句可表达的桶 → None。
pub fn field_query(column: &str, level: Level) -> Option<String> {
    let token = match level {
        Level::Fatal => "FATAL",
        Level::Error => "ERROR",
        Level::Warn => "WARN",
        Level::Info => "INFO",
        // 合并桶: 一个 level= 子句表达不了「DEBUG 或 TRACE」(空格分词是 AND);
        // 要让它可点须把该桶拆成两行 = 改 spec D1。
        Level::DebugTrace => return None,
        Level::Other => return None,
    };
    Some(format!("{column}={token}*"))
}

/// 每桶的点选子句表 (索引序同 [`Level::ALL`])。
/// 明文 / 无级别类列时全 `None` → 侧栏只读。
pub type LevelQueries = [Option<String>; Level::ALL.len()];

/// 由字段列名生成点选子句表。
pub fn level_queries_for(column: &str) -> LevelQueries {
    std::array::from_fn(|i| field_query(column, Level::ALL[i]))
}

/// 全 `None` 的子句表 (只读侧栏)。
pub fn no_level_queries() -> LevelQueries {
    std::array::from_fn(|_| None)
}

/// JSONL 字段口径全文件计数。
///
/// 明文文件或 JSONL 但无级别类列时**不要**调这个 —— 前者没有字段概念,
/// 后者 `column` 无从取得; 两条路径各自与自己的可点行为对齐 (spec D2)。
pub fn count_levels_field(file: &LogFile, column: &str) -> LevelCounts {
    count_levels_field_from(file, column, 0)
}

/// JSONL 字段口径增量计数: 只数 `[from, line_count)` 的行。
///
/// 无级别字段的行 (含非 JSONL 行) 一律归 `其他` —— 「6 桶之和 == 总行数」
/// 这条不变量对字段口径同样成立。
pub fn count_levels_field_from(file: &LogFile, column: &str, from: u64) -> LevelCounts {
    let total = file.line_count();
    if from >= total {
        return LevelCounts::default();
    }
    let len = total - from;
    let threads = if len < PARALLEL_COUNT_MIN_LINES {
        1
    } else {
        default_threads()
    };
    let classify =
        |line: &[u8]| jsonl::extract_field(line, column).map_or(Level::Other, classify_field_value);
    count_ranges(file, from, len, threads, &classify)
}

/// 追加的**绝对**计数更新: 在 `old_counts` 基础上, 用「重叠一行」重算
/// `[old.line_count()-1, new.line_count())` 并交付新的绝对计数。
///
/// 为什么必须重叠一行 (review R1, 已复现): 旧快照的末行可能**以无换行结尾**,
/// 外部把那一行补全时会把它改判 (例如从「其他」变成 ERROR)。纯增量
/// `[old.line_count(), new)` **不含该行**, 于是那一行永不重算, 侧栏静默偏低
/// 且随 tail 时长累积。从 `old.line_count()-1` 起重算即可覆盖它。
///
/// 旧快照本就以换行结尾时, 那一行的重算结果不变 (减旧类 == 加新类), 故
/// **无条件启用是安全的** —— 不需要先判断旧尾是不是 `\n`, 也就不会因为判据
/// 写错而再多一类 bug。
///
/// 交付绝对量 (而非增量) 是有意的: 调用方只需覆盖, 不存在「增删当全量」
/// 或「全量当增量」的误用空间。
pub fn update_for_append(
    old: &LogFile,
    new: &LogFile,
    old_counts: LevelCounts,
    column: Option<&str>,
) -> LevelCounts {
    let from = old.line_count().saturating_sub(1);
    let mut c = old_counts;
    // 退掉重叠区间在旧文件上的分类, 再按新文件重算同一区间
    match column {
        Some(col) => {
            c.sub(&count_levels_field_from(old, col, from));
            c.merge(&count_levels_field_from(new, col, from));
        }
        None => {
            c.sub(&count_levels_from(old, from));
            c.merge(&count_levels_from(new, from));
        }
    }
    c
}

/// 各桶计数。索引序 = [`Level::ALL`] 序 (即严重度降序)。
///
/// 用定长数组而非 HashMap: 桶数是编译期常量, 查表 O(1) 且无哈希开销 ——
/// 计数是每行都要走的热路径。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LevelCounts {
    counts: [u64; Level::ALL.len()],
}

impl LevelCounts {
    /// 记一行。
    pub fn add(&mut self, level: Level) {
        self.counts[level as usize] += 1;
    }

    /// 减去另一份计数 (追加时把重叠区间的旧分类退掉)。
    ///
    /// **饱和减**: 在正确的前提下不会下溢, 但真下溢时宁可钳到 0,
    /// 不要回绕成天文数字那样更难查的现象。
    pub fn sub(&mut self, other: &LevelCounts) {
        for (a, b) in self.counts.iter_mut().zip(other.counts.iter()) {
            *a = a.saturating_sub(*b);
        }
    }

    /// 合并另一份计数 (并行分段归并用)。
    pub fn merge(&mut self, other: &LevelCounts) {
        for (a, b) in self.counts.iter_mut().zip(other.counts.iter()) {
            *a += *b;
        }
    }

    /// 某桶计数。
    pub fn get(&self, level: Level) -> u64 {
        self.counts[level as usize]
    }

    /// 全部行数 (含「其他」桶)。
    pub fn total(&self) -> u64 {
        self.counts.iter().sum()
    }
}

/// 计入并行分段的最小行数: 低于此值直接顺序算 —— 小文件 spawn 线程的开销
/// 比省下的扫描还贵, 而小文件顺序算本来就在毫秒级。
const PARALLEL_COUNT_MIN_LINES: u64 = 100_000;

/// 并行计数的线程上限。
///
/// 不照抄 `danqing-logfile` 的 `MAX_FILTER_THREADS = 8`: 实测 20 核机上 8 线程
/// 是瓶颈 (明文 1GB 219ms), 放到 16 线程后 157ms / JSONL 175→120ms —— 计数是
/// 一次性短任务, 多吃几个核不伤常驻吞吐。上限保留是为了不在超多核机器上
/// 起一堆线程做同一个小活。
const MAX_COUNT_THREADS: usize = 16;

/// 默认并行度 (空文件/小文件由调用方降到 1)。
fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(MAX_COUNT_THREADS)
}

/// 全文件级别计数 (明文口径: 逐行 [`classify_level`])。
///
/// 顺序/并行的选择对调用方透明; 结果与顺序遍历逐行分类**完全一致**
/// (`parallel_equals_sequential` 单测钉着)。
pub fn count_levels(file: &LogFile) -> LevelCounts {
    let total = file.line_count();
    let threads = if total < PARALLEL_COUNT_MIN_LINES {
        1
    } else {
        default_threads()
    };
    count_range_parallel(file, 0, total, threads)
}

/// 增量计数: 只数 `[from, line_count)` 的行。
///
/// tail 追加的落点用这个 —— 追加是**每 250ms 就可能发生**的同步热路径,
/// 在那儿全量重算等于每次追加卡一次全文件扫描 (1GB ≈ 94ms)。
/// 小增量 (常态 KB 级) 直接顺序算; 巨量追平走并行分段。
pub fn count_levels_from(file: &LogFile, from: u64) -> LevelCounts {
    let total = file.line_count();
    if from >= total {
        return LevelCounts::default();
    }
    let len = total - from;
    let threads = if len < PARALLEL_COUNT_MIN_LINES {
        1
    } else {
        default_threads()
    };
    count_range_parallel(file, from, len, threads)
}

/// 指定线程数的计数 —— **测试专用守卫口**: 小 fixture 上强制分段以覆盖段边界。
///
/// 不作公开 API: `threads` 只被钳到 `line_count` 而非 `MAX_COUNT_THREADS`,
/// 传一个大数 + 小文件会 spawn 出 `line_count` 个线程。生产路径 (按
/// `default_threads()` 钳到 ≤16) 不需要它, 暴露出去纯属 footgun。
#[cfg(test)]
pub fn count_levels_with_threads(file: &LogFile, threads: usize) -> LevelCounts {
    let total = file.line_count();
    count_range_parallel(file, 0, total, threads)
}

/// 并行分段计数骨架: 把 `[from, from + len)` 切成至多 `threads` 段,
/// 各段用 [`LogFile::lines_from`] 自行迭代, 按 `classify` 分类后归并。
///
/// 分类器做成参数而非写死 [`classify_level`], 因为两条口径要复用它:
/// 行口径直接分类行内容, 字段口径先 `extract_field` 再分类字段值。
///
/// 分段起点定位是 O(log) 二分 (`lines_from` 内部), 不引入线性开销。
fn count_ranges(
    file: &LogFile,
    from: u64,
    len: u64,
    threads: usize,
    classify: &(dyn Fn(&[u8]) -> Level + Sync),
) -> LevelCounts {
    let threads = (threads.max(1) as u64).min(len.max(1));
    if len == 0 || threads <= 1 {
        return count_range_with(file, from, len, classify);
    }

    // 均分段: ceil 保证段数不超过 threads; 末段用 min 收窄, 不越界。
    let seg = len.div_ceil(threads);
    let end = from + len;
    let mut ranges = Vec::with_capacity(threads as usize);
    let mut start = from;
    while start < end {
        let l = seg.min(end - start);
        ranges.push((start, l));
        start += l;
    }

    // scope 借用 `&LogFile` 即可 —— 现有代码已把 `Arc<LogFile>` 跨线程移动
    // (main.rs 的 filter/search job), 故 Send + Sync 成立, 无需再 Arc 一层。
    let mut parts: Vec<LevelCounts> = Vec::with_capacity(ranges.len());
    std::thread::scope(|s| {
        let handles: Vec<_> = ranges
            .iter()
            .map(|&(start, len)| s.spawn(move || count_range_with(file, start, len, classify)))
            .collect();
        for h in handles {
            // worker panic 就让它冒泡。**不要**指望调用方一定有 catch_unwind:
            // release profile 是 `panic = "abort"`, 那时进程直接终止; 而
            // main.rs 的 tail 同步追加路径本就没有 catch_unwind。所以这里是
            // 「panic 不静默」而非「panic 可恢复」——静默吞掉会交付一份错的计数。
            parts.push(h.join().expect("计数 worker panic"));
        }
    });

    let mut acc = LevelCounts::default();
    for p in &parts {
        acc.merge(p);
    }
    acc
}

/// 行口径的并行分段计数 (分类器固定为 [`classify_level`])。
fn count_range_parallel(file: &LogFile, from: u64, len: u64, threads: usize) -> LevelCounts {
    count_ranges(file, from, len, threads, &classify_level)
}

/// 行口径的单段顺序计数 (顺序版参照物; 生产路径走 [`count_levels`] 的并行骨架,
/// 这里只给对拍单测当基准)。
#[cfg(test)]
fn count_range(file: &LogFile, start: u64, len: u64) -> LevelCounts {
    count_range_with(file, start, len, &classify_level)
}

/// 单段顺序计数: 从 `start` 行起数 `len` 行, 按 `classify` 分类。
fn count_range_with(
    file: &LogFile,
    start: u64,
    len: u64,
    classify: &(dyn Fn(&[u8]) -> Level + Sync),
) -> LevelCounts {
    let mut c = LevelCounts::default();
    if len == 0 {
        return c;
    }
    for (_no, line) in file.lines_from(start).take(len as usize) {
        c.add(classify(line));
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 桶的全集恰好 6 个且互不相同 —— 计数不漏不重的结构前提。
    #[test]
    fn level_all_is_complete_and_unique() {
        assert_eq!(Level::ALL.len(), 6, "恰好 6 桶");
        let mut seen = std::collections::HashSet::new();
        for l in Level::ALL {
            assert!(seen.insert(l), "{l:?} 在 ALL 里重复");
        }
    }

    /// 分类是全函数: 任何输入都落到恰好一个桶 (返回单值即结构上保证唯一)。
    #[test]
    fn classification_is_total() {
        let cases: &[&[u8]] = &[
            b"",
            b"   ",
            b"2026-09-05 12:00:01 INFO ok",
            b"\xff\xfe\x00 bad utf8",
            &[0u8; 300],
        ];
        for c in cases {
            let _ = classify_level(c); // 不 panic 即通过
        }
    }

    /// FATAL 优先于 ERROR: 两者同现时归 FATAL (一眼要抓的是它)。
    #[test]
    fn fatal_beats_error() {
        assert_eq!(classify_level(b"FATAL during ERROR handling"), Level::Fatal);
        assert_eq!(classify_level(b"ERROR caused FATAL"), Level::Fatal);
    }

    /// 优先级全序: WARN 先于 INFO, ERROR 先于 WARN。
    #[test]
    fn precedence_is_ordered() {
        assert_eq!(classify_level(b"INFO WARN both here"), Level::Warn);
        assert_eq!(classify_level(b"WARN ERROR both here"), Level::Error);
        assert_eq!(classify_level(b"INFO DEBUG both here"), Level::Info);
    }

    /// INFO 单独成桶 —— 这是与 `level_color` 分桶的关键分歧 (那里 INFO 降噪到默认色)。
    #[test]
    fn info_has_its_own_bucket() {
        assert_eq!(classify_level(b"2026-09-05 12:00:01 INFO ok"), Level::Info);
    }

    /// DEBUG 与 TRACE 合并同桶。
    #[test]
    fn debug_and_trace_share_a_bucket() {
        assert_eq!(classify_level(b"DEBUG cache miss"), Level::DebugTrace);
        assert_eq!(classify_level(b"TRACE entering fn"), Level::DebugTrace);
    }

    /// 无级别词 → 其他。
    #[test]
    fn unrecognised_falls_back_to_other() {
        assert_eq!(
            classify_level(b"2026-09-05 12:00:01 hello world"),
            Level::Other
        );
        assert_eq!(classify_level(b""), Level::Other);
    }

    /// 小写级别词不识别 (大小写敏感的代价, 有意为之): 正文里的 "errors" 不得
    /// 污染 ERROR 桶, 而柱条数字必须可信。
    #[test]
    fn lowercase_prose_does_not_pollute_counts() {
        assert_eq!(
            classify_level(b"finished with no errors found"),
            Level::Other
        );
        assert_eq!(classify_level(b"informational message"), Level::Other);
        assert_eq!(classify_level(b"error: disk full"), Level::Other);
    }

    /// 大小写敏感的直接断言 (与上一条同源, 分开钉住两个方向)。
    #[test]
    fn matching_is_case_sensitive() {
        assert_eq!(classify_level(b"ERROR disk full"), Level::Error);
        assert_eq!(classify_level(b"error disk full"), Level::Other);
        assert_eq!(
            classify_level(b"WARM up complete"),
            Level::Other,
            "WARM 不含 WARN"
        );
    }

    /// 只扫行首 200 字节: 级别词在区间之外不认。
    #[test]
    fn only_head_window_is_scanned() {
        let mut far = vec![b'x'; LEVEL_HEAD_BYTES + 10];
        far.extend_from_slice(b"ERROR disk full");
        assert_eq!(
            classify_level(&far),
            Level::Other,
            "级别词在第 {LEVEL_HEAD_BYTES} 字节之后, 不认"
        );

        // 边界内 (恰在区间内起) 应认出
        let mut near = vec![b'x'; LEVEL_HEAD_BYTES - 6];
        near.extend_from_slice(b"ERROR!");
        assert_eq!(classify_level(&near), Level::Error, "区间内应认出");
    }

    /// label 与桶一一对应, 无重复 (侧栏显示不串行)。
    #[test]
    fn labels_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for l in Level::ALL {
            assert!(seen.insert(l.label()), "{:?} 的 label 重复", l);
        }
    }

    /// **差分测试**: 优化版 (memchr3 扫描 + 位集) 与朴素版 (逐个 memmem 早退)
    /// 在对抗语料上必须逐行一致。
    ///
    /// 这条是重写分类器时的安全网 —— 优化只应改常数, 不应改语义。
    /// 语料专挑子串碰撞、优先级冲突与 200 字节边界: 这些正是两组实现可能分岔的地方。
    #[test]
    fn optimised_matches_naive_reference() {
        fn naive(line: &[u8]) -> Level {
            let head = &line[..line.len().min(LEVEL_HEAD_BYTES)];
            let has = |pat: &[u8]| memchr::memmem::find(head, pat).is_some();
            if has(b"FATAL") {
                Level::Fatal
            } else if has(b"ERROR") {
                Level::Error
            } else if has(b"WARN") {
                Level::Warn
            } else if has(b"INFO") {
                Level::Info
            } else if has(b"DEBUG") || has(b"TRACE") {
                Level::DebugTrace
            } else {
                Level::Other
            }
        }

        let mut corpus: Vec<Vec<u8>> = [
            "",
            "   ",
            "FATAL",
            "fatal",
            "ERROR ERROR ERROR",
            "TRACE DEBUG",
            "DEBUG TRACE",
            "INFO WARN",
            "WARN INFO",
            "WARN ERROR",
            "ERROR FATAL",
            "FATAL during ERROR handling",
            "WARNS",         // 子串命中
            "ERRORS",        // 大写复数也命中 ERROR
            "WARNING",       // 别名命中 WARN
            "INFORMATIONAL", // 大写长词命中 INFO
            "DEBUGGER",
            "TRACEROUTE",
            "MISINFORMATION",
            "EEERROR", // 前缀重复
            "ATATATA", // 大量大写但无命中
            "TATATATATATA",
            "no errors here", // 小写正文, 不应命中
            "error: disk full",
            "1970-01-01T00:00:00Z Fatal disk",
            "1970-01-01T00:00:00Z fatal disk", // 小写 Fatal 不认
            "F E W I D T",
        ]
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect();

        // 200 字节边界两侧
        let mut near = vec![b'x'; LEVEL_HEAD_BYTES - 5];
        near.extend_from_slice(b"ERROR");
        corpus.push(near);
        let mut across = vec![b'x'; LEVEL_HEAD_BYTES - 3];
        across.extend_from_slice(b"ERROR");
        corpus.push(across); // 关键词跨过边界, 截断后不成立
        corpus.push(vec![b'E'; LEVEL_HEAD_BYTES + 50]); // 全大写 E 但无 ERROR
        corpus.push(vec![b'\0'; LEVEL_HEAD_BYTES]); // NUL 填充

        for line in &corpus {
            let got = classify_level(line);
            let want = naive(line);
            assert_eq!(
                got,
                want,
                "分类分岔: {:?} (前 60 字节: {:?})",
                String::from_utf8_lossy(line),
                String::from_utf8_lossy(&line[..line.len().min(60)]),
            );
        }
    }

    // ---- T2: 计数 ----

    use std::io::Write;
    use std::path::PathBuf;

    /// 落一个临时文件 (与 open.rs::tests 同款唯一命名: Windows 映射语义)。
    fn temp_file(content: &[u8]) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "danqing-log-levels-{}-{}.log",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(content).unwrap();
        }
        path
    }

    /// 每桶各一行的 7 行块 (第 7 行无级别词 → 其他)。
    const ONE_OF_EACH: &str = "2026-09-05 12:00:01 FATAL disk failure\n\
                               2026-09-05 12:00:02 ERROR disk full\n\
                               2026-09-05 12:00:03 WARN slow query\n\
                               2026-09-05 12:00:04 INFO started\n\
                               2026-09-05 12:00:05 DEBUG cache miss\n\
                               2026-09-05 12:00:06 TRACE entering\n\
                               2026-09-05 12:00:07 plain line\n";

    /// `n` 个块拼接 (每桶恰 `n` 行, 总行数 `7n`)。
    fn blocks(n: usize) -> Vec<u8> {
        ONE_OF_EACH.repeat(n).into_bytes()
    }

    /// `blocks(n)` 的计数期望: 每块 7 行 = FATAL/ERROR/WARN/INFO/其他 各 1,
    /// **DEBUG 与 TRACE 各 1 但同桶** → DebugTrace 每块 2。
    fn expected(n: u64) -> LevelCounts {
        let mut c = LevelCounts::default();
        for _ in 0..n {
            for l in [
                Level::Fatal,
                Level::Error,
                Level::Warn,
                Level::Info,
                Level::Other,
            ] {
                c.add(l);
            }
            c.add(Level::DebugTrace); // DEBUG 行
            c.add(Level::DebugTrace); // TRACE 行
        }
        c
    }

    /// `Level as usize` 必须等于它在 `ALL` 里的下标 —— `LevelCounts` 靠这个索引。
    #[test]
    fn level_discriminant_matches_all_order() {
        for (i, l) in Level::ALL.iter().enumerate() {
            assert_eq!(*l as usize, i, "{l:?} 的判别式与 ALL 下标不符");
        }
    }

    /// 空文件: 全零, 不 panic, 不 spawn (走顺序分支)。
    #[test]
    fn empty_file_counts_all_zero() {
        let p = temp_file(b"");
        let f = LogFile::open(&p).unwrap();
        let c = count_levels(&f);
        assert_eq!(c.total(), 0, "空文件总计数为 0");
        assert_eq!(c, LevelCounts::default());
        let _ = std::fs::remove_file(&p);
    }

    /// 每桶各 1 行, 按块重复 3 次 → 每桶 3。
    #[test]
    fn each_bucket_is_counted() {
        let p = temp_file(&blocks(3));
        let f = LogFile::open(&p).unwrap();
        assert_eq!(f.line_count(), 21, "7 行 × 3 块");

        let c = count_levels(&f);
        assert_eq!(c.get(Level::Fatal), 3);
        assert_eq!(c.get(Level::Error), 3);
        assert_eq!(c.get(Level::Warn), 3);
        assert_eq!(c.get(Level::Info), 3);
        assert_eq!(
            c.get(Level::DebugTrace),
            6,
            "DEBUG 与 TRACE 同桶, 每块 2 行"
        );
        assert_eq!(c.get(Level::Other), 3);
        assert_eq!(c.total(), 21, "6 桶之和 == 总行数");
        assert_eq!(c, expected(3));
        let _ = std::fs::remove_file(&p);
    }

    /// 并行 == 顺序 == 手算期望 (段数 1/3/7/8/64 全覆盖)。
    #[test]
    fn parallel_equals_sequential() {
        let p = temp_file(&blocks(1000)); // 7000 行
        let f = LogFile::open(&p).unwrap();
        let total = f.line_count();
        assert_eq!(total, 7000);

        let serial = count_range(&f, 0, total);
        assert_eq!(serial, expected(1000), "顺序基准 == 手算期望");

        for threads in [1, 3, 7, 8, 64] {
            let par = count_levels_with_threads(&f, threads);
            assert_eq!(par, serial, "{threads} 线程与顺序结果不一致");
            assert_eq!(par.total(), total, "{threads} 线程: 6 桶之和 != 总行数");
        }
        let _ = std::fs::remove_file(&p);
    }

    /// 段边界: 行数不被段数整除时, 每行仍**恰好**归一个桶 (不重不漏)。
    #[test]
    fn uneven_segmentation_loses_no_line() {
        // 7001 行: 3 段 = 2334 / 2334 / 2333
        let mut content = blocks(1000);
        content.extend_from_slice(b"2026-09-05 12:00:08 ERROR tail\n");
        let p = temp_file(&content);
        let f = LogFile::open(&p).unwrap();
        assert_eq!(f.line_count(), 7001);

        let mut want = expected(1000);
        want.add(Level::Error); // 多出的尾行
        for threads in [1, 2, 3, 5, 8] {
            let got = count_levels_with_threads(&f, threads);
            assert_eq!(got, want, "{threads} 线程: 不整除分段漏行或重计");
            assert_eq!(got.total(), 7001);
        }
        let _ = std::fs::remove_file(&p);
    }

    /// 单行文件 (边界: 行数 1 时的分段)。
    #[test]
    fn single_line_file() {
        let p = temp_file(b"2026-09-05 12:00:01 ERROR only\n");
        let f = LogFile::open(&p).unwrap();
        assert_eq!(f.line_count(), 1);
        for threads in [1, 3, 8] {
            let c = count_levels_with_threads(&f, threads);
            assert_eq!(c.get(Level::Error), 1);
            assert_eq!(c.total(), 1);
        }
        let _ = std::fs::remove_file(&p);
    }

    // ---- T4: JSONL 字段口径 ----

    /// 字段值走**前缀**匹配: 这是与过滤 `Op::Prefix` 同构的那一半。
    #[test]
    fn field_value_matches_by_prefix() {
        assert_eq!(classify_field_value(b"ERROR"), Level::Error);
        assert_eq!(classify_field_value(b"ERRORS"), Level::Error, "前缀含");
        assert_eq!(
            classify_field_value(b"WARNING"),
            Level::Warn,
            "别名靠前缀吃到"
        );
        assert_eq!(classify_field_value(b"INFO"), Level::Info);
        assert_eq!(classify_field_value(b"FATAL"), Level::Fatal);
        assert_eq!(classify_field_value(b"DEBUG"), Level::DebugTrace);
        assert_eq!(classify_field_value(b"TRACE"), Level::DebugTrace);
        // 非前缀的都不认 —— 与过滤侧 starts_with 一致
        assert_eq!(classify_field_value(b"XERROR"), Level::Other, "非前缀不认");
        assert_eq!(
            classify_field_value(b"ERR"),
            Level::Other,
            "ERR 不是 ERROR 的前缀方向"
        );
        assert_eq!(classify_field_value(b"error"), Level::Other, "大小写敏感");
        assert_eq!(classify_field_value(b""), Level::Other);
    }

    /// **D2 红线 (对抗样本)**: `level=INFO` 但正文含 `error` 的行必须计入 INFO 桶。
    ///
    /// 按行子串计数会把它算进 ERROR —— 而点该柱条应用的 `level=ERROR` 是字段
    /// 过滤, 筛出来的结果里没有它, 柱条数字与行数当场打架。
    #[test]
    fn field_counting_ignores_message_body() {
        let p = temp_file(
            b"{\"level\":\"INFO\",\"msg\":\"handle error failed\"}\n\
              {\"level\":\"INFO\",\"msg\":\"another ERROR in prose\"}\n",
        );
        let f = LogFile::open(&p).unwrap();
        // 行口径会看见正文里的 ERROR (小写的不算, 大写的算)
        assert_eq!(
            count_levels(&f).get(Level::Info),
            1,
            "行口径: 第二条被正文抢走"
        );
        // 字段口径不受正文影响
        let c = count_levels_field(&f, "level");
        assert_eq!(c.get(Level::Info), 2, "两条都是 INFO");
        assert_eq!(c.get(Level::Error), 0, "正文里的 ERROR 不得污染");
        assert_eq!(c.total(), 2);
        let _ = std::fs::remove_file(&p);
    }

    /// 列名识别: 认清单内的, 不认其它。
    #[test]
    fn find_level_column_recognises_only_known_names() {
        let p = temp_file(
            b"{\"level\":\"INFO\",\"msg\":\"a\",\"sev\":\"x\"}\n\
              {\"level\":\"ERROR\",\"msg\":\"b\",\"sev\":\"y\"}\n",
        );
        let f = LogFile::open(&p).unwrap();
        let schema = jsonl::discover_schema(&f).expect("JSONL schema");
        assert_eq!(find_level_column(&schema), Some("level"));

        // 只有 severity 时认 severity
        let p2 = temp_file(
            b"{\"severity\":\"WARN\",\"msg\":\"a\"}\n{\"severity\":\"INFO\",\"msg\":\"b\"}\n",
        );
        let f2 = LogFile::open(&p2).unwrap();
        let s2 = jsonl::discover_schema(&f2).expect("JSONL schema");
        assert_eq!(find_level_column(&s2), Some("severity"));

        // 没有级别类列 → None (调用方降级只读)
        let p3 = temp_file(b"{\"ts\":\"1\",\"msg\":\"a\"}\n{\"ts\":\"2\",\"msg\":\"b\"}\n");
        let f3 = LogFile::open(&p3).unwrap();
        let s3 = jsonl::discover_schema(&f3).expect("JSONL schema");
        assert_eq!(find_level_column(&s3), None, "猜错列不如不猜");
        for q in [p, p2, p3] {
            let _ = std::fs::remove_file(&q);
        }
    }

    /// 子句表: 四档可点、两档 None; 且索引与 `Level::ALL` 对齐。
    #[test]
    fn level_queries_table_is_aligned_and_sparse() {
        let q = level_queries_for("severity");
        assert_eq!(q[Level::Error as usize].as_deref(), Some("severity=ERROR*"));
        assert_eq!(q[Level::Info as usize].as_deref(), Some("severity=INFO*"));
        assert_eq!(q[Level::DebugTrace as usize], None);
        assert_eq!(q[Level::Other as usize], None);
        assert_eq!(q.iter().filter(|x| x.is_some()).count(), 4, "四档可点");

        assert!(no_level_queries().iter().all(Option::is_none));
    }

    /// 点选子句是前缀通配; `其他` 桶无子句。
    #[test]
    fn field_query_is_prefix_wildcard() {
        assert_eq!(
            field_query("level", Level::Error).as_deref(),
            Some("level=ERROR*")
        );
        assert_eq!(
            field_query("severity", Level::Warn).as_deref(),
            Some("severity=WARN*")
        );
        assert_eq!(
            field_query("level", Level::DebugTrace),
            None,
            "合并桶表达不了「DEBUG 或 TRACE」"
        );
        assert_eq!(field_query("level", Level::Other), None, "其他桶无子句");
    }

    /// **D2 红线的端到端钉子**: 字段计数与过滤命中数**逐桶相等**。
    ///
    /// 这条断言的就是 spec 的一致性红线 —— 不是「差不多」, 是同一个数字。
    #[test]
    fn field_counts_equal_filter_hits_bucket_by_bucket() {
        let content = b"{\"level\":\"INFO\",\"msg\":\"started\"}\n\
                        {\"level\":\"INFO\",\"msg\":\"handle error failed\"}\n\
                        {\"level\":\"ERROR\",\"msg\":\"disk full\"}\n\
                        {\"level\":\"WARNING\",\"msg\":\"slow query\"}\n\
                        {\"level\":\"ERR\",\"msg\":\"odd spelling\"}\n\
                        {\"severity\":\"DEBUG\",\"msg\":\"no level field\"}\n\
                        {\"level\":\"error\",\"msg\":\"lowercase\"}\n";
        let p = temp_file(content);
        let f = LogFile::open(&p).unwrap();
        assert_eq!(f.line_count(), 7);

        let counts = count_levels_field(&f, "level");
        assert_eq!(counts.get(Level::Info), 2, "含正文写 error 的那条");
        assert_eq!(counts.get(Level::Error), 1);
        assert_eq!(counts.get(Level::Warn), 1, "WARNING 靠前缀吃到");
        assert_eq!(counts.get(Level::Fatal), 0);
        assert_eq!(
            counts.get(Level::DebugTrace),
            0,
            "DEBUG 在 severity 列, 不算"
        );
        assert_eq!(
            counts.get(Level::Other),
            3,
            "ERR + 无 level 字段 + 小写 error"
        );
        assert_eq!(counts.total(), 7, "6 桶之和 == 总行数");

        for l in Level::ALL {
            let Some(q) = field_query("level", l) else {
                continue;
            };
            let hits = jsonl::run_filter(&f, &jsonl::parse_query(&q));
            assert_eq!(
                hits.len() as u64,
                counts.get(l),
                "{q} 的命中数 != 柱条数字 (D2 红线)"
            );
        }
        let _ = std::fs::remove_file(&p);
    }

    /// 字段口径的并行 == 顺序 (与行口径同款对拍)。
    #[test]
    fn field_counting_parallel_equals_sequential() {
        let mut content = Vec::new();
        for i in 0..3000 {
            let lv = match i % 4 {
                0 => "INFO",
                1 => "ERROR",
                2 => "WARNING",
                _ => "plain-word",
            };
            content.extend_from_slice(format!("{{\"level\":\"{lv}\",\"i\":{i}}}\n").as_bytes());
        }
        let p = temp_file(&content);
        let f = LogFile::open(&p).unwrap();
        let want = count_levels_field_from(&f, "level", 0);
        assert_eq!(want.total(), 3000);
        assert_eq!(want.get(Level::Info), 750);
        assert_eq!(want.get(Level::Warn), 750, "WARNING 计入 WARN");
        assert_eq!(want.get(Level::Other), 750);
        let _ = std::fs::remove_file(&p);
    }

    /// 多轮追加: 每轮「旧计数 + 新行增量」必须等于对当前文件的**全量重算**。
    ///
    /// 这是 tail 期间侧栏可信的唯一依据 —— 单轮对拍容易碰巧过, 连跑五轮且
    /// 每轮桶分布都不同 (交替 ERROR / WARN) 才逼出「漏行 / 重计 / 忘记合并」
    /// 这类错法。
    fn assert_incremental_matches_full(jsonl: bool) {
        let line_of = |round: usize, i: usize| -> String {
            let lv = if round.is_multiple_of(2) {
                "ERROR"
            } else {
                "WARN"
            };
            if jsonl {
                format!("{{\"level\":\"{lv}\",\"round\":{round},\"i\":{i}}}\n")
            } else {
                format!(
                    "2026-09-05 12:{:02}:{:02} {lv} tail {round}-{i}\n",
                    round,
                    i % 60
                )
            }
        };
        let col = "level";
        let count_full = |f: &LogFile| {
            if jsonl {
                count_levels_field(f, col)
            } else {
                count_levels(f)
            }
        };

        let mut init = String::new();
        for i in 0..100 {
            init.push_str(&line_of(0, i));
        }
        let p = temp_file(init.as_bytes());
        let mut file = LogFile::open(&p).unwrap();
        let mut acc = count_full(&file);
        assert_eq!(acc.total(), 100, "起始 {jsonl} 行");

        for round in 1..=5 {
            // 第 3 轮只落一个**无换行的半行** (级别词被切开); 第 4 轮把它补全 ——
            // review R1 的触发形态。纯增量会漏掉这一行, 重叠一行才不会。
            let add: String = match round {
                3 => {
                    if jsonl {
                        "{\"level\":\"ERR".to_string()
                    } else {
                        "2026-09-05 12:03:00 ".to_string()
                    }
                }
                4 => {
                    let head = if jsonl {
                        "OR\",\"round\":4,\"i\":0}
"
                    } else {
                        "ERROR round4-i0
"
                    };
                    format!("{head}{}", line_of(4, 1))
                }
                _ => {
                    let batch: usize = if round % 2 == 0 { 10 } else { 25 };
                    (0..batch).map(|i| line_of(round, i)).collect()
                }
            };
            {
                let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
                f.write_all(add.as_bytes()).unwrap();
            }
            let prev = file;
            file = LogFile::append_from(&prev, &p).unwrap();

            // 走**生产入口**, 而非在测试里手工 merge 增量
            acc = update_for_append(&prev, &file, acc, jsonl.then_some(col));
            assert_eq!(
                acc,
                count_full(&file),
                "jsonl={jsonl} 第 {round} 轮: 追加更新 != 全量重算"
            );
            assert_eq!(acc.total(), file.line_count(), "6 桶之和 == 总行数");
        }
        let _ = std::fs::remove_file(&p);
    }

    /// 明文口径的增量等价 (多轮)。
    #[test]
    fn incremental_matches_full_plain_text() {
        assert_incremental_matches_full(false);
    }

    /// JSONL 字段口径的增量等价 (多轮)。
    #[test]
    fn incremental_matches_full_jsonl_field() {
        assert_incremental_matches_full(true);
    }

    // ---- review R1: 追加的绝对计数更新 ----

    /// **R1 回归**: 旧快照末行以**无换行**结尾, 外部把它补全并改判 ——
    /// `update_for_append` 必须覆盖那一行。明文与 JSONL 字段口径各一遍。
    #[test]
    fn update_for_append_covers_completed_half_line() {
        // 明文: 半行 `... 12:00:01 ` 被补成 `... 12:00:01 ERROR disk`
        let p1 = temp_file(b"X\n2026-09-05 12:00:01 ");
        let old1 = LogFile::open(&p1).unwrap();
        assert_eq!(old1.line_count(), 2, "半行已是一行");
        let c1 = count_levels(&old1);
        assert_eq!(c1.get(Level::Error), 0, "半行未成词 → 其他");
        {
            let mut f = std::fs::OpenOptions::new().append(true).open(&p1).unwrap();
            f.write_all(b"ERROR disk\n").unwrap();
        }
        let new1 = LogFile::append_from(&old1, &p1).unwrap();
        assert_eq!(new1.line_count(), 2, "补全半行不增行数");
        let got1 = update_for_append(&old1, &new1, c1, None);
        assert_eq!(got1, count_levels(&new1), "补全后半行必须被重算 (明文)");
        assert_eq!(got1.get(Level::Error), 1);

        // JSONL 字段口径: 半行 `{"level":"ERR` 被补成完整对象
        let p2 = temp_file(b"{\"level\":\"ERR");
        let old2 = LogFile::open(&p2).unwrap();
        assert_eq!(old2.line_count(), 1);
        let c2 = count_levels_field(&old2, "level");
        assert_eq!(c2.get(Level::Error), 0, "半行取不到字段 → 其他");
        {
            let mut f = std::fs::OpenOptions::new().append(true).open(&p2).unwrap();
            f.write_all(b"OR\",\"m\":\"x\"}\n").unwrap();
        }
        let new2 = LogFile::append_from(&old2, &p2).unwrap();
        assert_eq!(new2.line_count(), 1, "补全半行不增行数");
        let got2 = update_for_append(&old2, &new2, c2, Some("level"));
        assert_eq!(
            got2,
            count_levels_field(&new2, "level"),
            "补全后半行必须被重算 (字段口径)"
        );
        assert_eq!(got2.get(Level::Error), 1);

        let _ = std::fs::remove_file(&p1);
        let _ = std::fs::remove_file(&p2);
    }

    /// 旧快照末行**完整** (以换行结尾) 时, 重叠一行是幂等的 ——
    /// 这正是「无条件启用安全」的依据, 不必先判断旧尾是不是 `\n`。
    #[test]
    fn update_for_append_is_idempotent_on_complete_tail() {
        let p = temp_file(b"INFO one\nERROR two\n");
        let old = LogFile::open(&p).unwrap();
        let c = count_levels(&old);
        {
            let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
            f.write_all(b"WARN three\n").unwrap();
        }
        let new = LogFile::append_from(&old, &p).unwrap();
        let got = update_for_append(&old, &new, c, None);
        assert_eq!(got, count_levels(&new));
        assert_eq!(got.get(Level::Info), 1);
        assert_eq!(got.get(Level::Error), 1);
        assert_eq!(got.get(Level::Warn), 1);
        assert_eq!(got.total(), 3);
        let _ = std::fs::remove_file(&p);
    }

    /// 空旧文件 (无重叠行可言) 不能因此漏计。
    #[test]
    fn update_for_append_from_empty_old_file() {
        let p = temp_file(b"");
        let old = LogFile::open(&p).unwrap();
        assert_eq!(old.line_count(), 0);
        let c = count_levels(&old);
        {
            let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
            f.write_all(b"ERROR one\nINFO two\n").unwrap();
        }
        let new = LogFile::append_from(&old, &p).unwrap();
        let got = update_for_append(&old, &new, c, None);
        assert_eq!(got, count_levels(&new));
        assert_eq!(got.total(), 2);
        let _ = std::fs::remove_file(&p);
    }

    /// 增量: 字段口径「全量」==「前 N 行 + 后段增量」。
    #[test]
    fn field_counting_incremental_matches_full() {
        let mut content = Vec::new();
        for i in 0..500 {
            content.extend_from_slice(format!("{{\"level\":\"ERROR\",\"i\":{i}}}\n").as_bytes());
        }
        let p = temp_file(&content);
        let f = LogFile::open(&p).unwrap();
        let mut acc = count_levels_field_from(&f, "level", 0);
        acc.merge(&count_levels_field_from(&f, "level", 500));
        assert_eq!(acc, count_levels_field(&f, "level"));
        let _ = std::fs::remove_file(&p);
    }

    /// 稀疏级别词: 只有一个块里的级别词命中, 其余全落「其他」——
    /// 钉住「其他」桶兜底不漏。
    #[test]
    fn other_bucket_absorbs_everything_unmatched() {
        let content = format!(
            "{}ERROR one real error\n{}",
            "2026-09-05 12:00:01 INFO noise\n".repeat(50),
            "2026-09-05 12:00:02 plain text\n".repeat(50),
        );
        let p = temp_file(content.as_bytes());
        let f = LogFile::open(&p).unwrap();
        let c = count_levels(&f);
        assert_eq!(c.get(Level::Info), 50);
        assert_eq!(c.get(Level::Other), 50, "50 行明文全落其他桶");
        assert_eq!(c.get(Level::Error), 1);
        assert_eq!(c.total(), 101);
        let _ = std::fs::remove_file(&p);
    }
}
