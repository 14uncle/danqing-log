//! @author 十四叔
//! @date 2026/09/05
//!
//! 无窗口基准: 打开/索引/搜索/过滤/随机访问全链路实测, 控制台表格输出。
//!
//! 用法: logbench <日志文件> [--filter "level=ERROR status=50*"] [正则...]
//! 不带正则时跑四个默认模式 (字面带量 / 简单交替 / 数字后缀 / 时间戳结构)。
//! --filter 走 JSONL 字段过滤 (前提②的引擎数字)。
//! 输出即开枪前提的截图弹药, 数字可直接抄进意图文档与 Show HN 稿。

use std::path::PathBuf;
use std::time::Instant;

use danqing_log::jsonl;
use danqing_log::levels;
use danqing_log::logfile::LogFile;

/// 默认基准模式: 从「纯字面高频」到「结构正则」递增难度。
const DEFAULT_PATTERNS: &[&str] = &[
    "ERROR",
    "ERROR|FATAL",
    r"user_42\d{4}",
    r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}",
];

/// 随机访问采样行数 (xorshift 伪随机, 无额外依赖)。
const RANDOM_SAMPLE: usize = 100_000;
/// 搜索行号收集上限 (防命中过密时内存爆; 总数不受限)。
const HIT_CAP: usize = 1_000_000;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("用法: logbench <日志文件> [--filter \"level=ERROR status=50*\"] [正则...]");
        std::process::exit(2);
    };
    let mut filter: Option<String> = None;
    let mut patterns: Vec<String> = Vec::new();
    let mut it = args;
    while let Some(a) = it.next() {
        if a == "--filter" {
            filter = it.next();
        } else {
            patterns.push(a);
        }
    }

    let t_all = Instant::now();
    let file = match LogFile::open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("打开失败: {e:#}");
            std::process::exit(1);
        }
    };
    let s = file.stats();
    let mib = s.file_bytes as f64 / (1024.0 * 1024.0);

    println!("== 打开 ==");
    println!("文件           : {}", path.display());
    println!("大小           : {mib:.1} MiB ({} bytes)", s.file_bytes);
    println!("编码           : {}", s.encoding.label());
    println!("mmap 建立      : {} µs", s.map_us);
    println!(
        "行索引         : {} ms ({:.0} MiB/s)",
        s.index.as_millis(),
        s.index_mib_per_s()
    );
    println!(
        "索引驻留       : {:.2} MiB ({} bytes, 步进索引)",
        s.index_bytes as f64 / (1024.0 * 1024.0),
        s.index_bytes
    );
    println!("行数           : {}", s.line_count);

    if let Some(q) = &filter {
        println!("\n== JSONL 字段过滤 ==");
        println!("查询           : {q}");
        let t = Instant::now();
        let hits = jsonl::run_filter(&file, &jsonl::parse_query(q));
        let el = t.elapsed();
        let secs = el.as_secs_f64();
        let thr = if secs > 0.0 {
            mib / secs
        } else {
            f64::INFINITY
        };
        println!(
            "命中行         : {} / {}   {} ms ({thr:.0} MiB/s)",
            hits.len(),
            s.line_count,
            el.as_millis(),
        );
    }

    println!("\n== 全文搜索 ==");
    let owned: Vec<String>;
    let pats: &[String] = if patterns.is_empty() {
        owned = DEFAULT_PATTERNS.iter().map(|p| p.to_string()).collect();
        &owned
    } else {
        &patterns
    };
    for p in pats {
        let re = match regex::bytes::Regex::new(p) {
            Ok(r) => r,
            Err(e) => {
                println!("{p:<40} 编译失败: {e}");
                continue;
            }
        };
        let (lines, total, elapsed) = file.search(&re, HIT_CAP);
        let secs = elapsed.as_secs_f64();
        let thr = if secs > 0.0 {
            mib / secs
        } else {
            f64::INFINITY
        };
        println!(
            "{p:<40} {:>12} 命中行 / {:>12} 总命中   {:>7} ms  ({thr:.0} MiB/s)",
            lines.len(),
            total,
            elapsed.as_millis(),
        );
    }

    println!("\n== 随机访问 ({RANDOM_SAMPLE} 行) ==");
    let t = Instant::now();
    let count = file.line_count().max(1);
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut bytes = 0usize;
    for _ in 0..RANDOM_SAMPLE {
        // xorshift64
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        bytes += file.line_lossy(rng % count).len();
    }
    let elapsed = t.elapsed();
    println!(
        "解码 {RANDOM_SAMPLE} 随机行 (共 {bytes} 字节): {} ms ({:.2} µs/行)",
        elapsed.as_millis(),
        elapsed.as_micros() as f64 / RANDOM_SAMPLE as f64,
    );

    // 级别计数 (level-histogram T2): 独立于索引趟的一趟并行扫描,
    // 索引耗时不受其影响 —— 两个数字必须分开测, 否则无法验证 D6。
    println!("\n== 级别计数 ==");
    let t = Instant::now();
    let counts = levels::count_levels(&file);
    let elapsed = t.elapsed();
    let secs = elapsed.as_secs_f64();
    let thr = if secs > 0.0 {
        mib / secs
    } else {
        f64::INFINITY
    };
    println!(
        "计数墙钟       : {} ms ({thr:.0} MiB/s)",
        elapsed.as_millis()
    );
    for l in levels::Level::ALL {
        println!("{:<14} : {}", l.label(), counts.get(l));
    }
    println!("合计           : {} 行", counts.total());

    println!("\n== 总计 ==");
    println!("端到端 (打开+全部基准): {} ms", t_all.elapsed().as_millis());
}
