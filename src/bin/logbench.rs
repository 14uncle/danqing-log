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

/// 分段表小标题 (集中一处: 这几段由脚本拼接进本文件, 反斜杠转义易出错)。
const PHASES_BANNER: &str = "\n== 打开管道分段 (GUI worker 实际做的五段) ==";
const LEVELS_BANNER: &str = "\n== 级别计数 (与上面分段同一份结果) ==";
const QUERIES_BANNER: &str = "\n== 点选子句 (与柱条数字同口径) ==";

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
    let open_wall = t_all.elapsed(); // LogFile::open 的总墙钟
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
    // open 的总墙钟 vs 它内部被计时的部分: 两者之差是**没被计入任何数字**的耗时
    // (UTF-16 的 read + transcode 就在这里 —— 状态栏的「mmap」「索引」都不含它)
    println!("open 总墙钟    : {} ms", open_wall.as_millis());
    if s.preprocess > std::time::Duration::ZERO {
        println!(
            "  └ 其中转码   : {} ms   (UTF-16 读整文件 + 转码; **不进 mmap/索引**)",
            s.preprocess.as_millis()
        );
    }
    println!("行数           : {}", s.line_count);

    // 打开管道的**真实五段** (2026-09-12): 状态栏那个「索引 N ms」只是
    // `LogFile::open` 里 build_line_index 那一段 —— 不含 map/检测/列发现/级别计数。
    // 用户报「索引 92ms 却等了十几秒」时, 就是靠这段定位到列发现的。
    // 同一命令即可复查, 不必进 GUI 翻日志。
    let t = Instant::now();
    let is_jsonl = jsonl::detect(&file);
    let t_detect = t.elapsed();

    let t = Instant::now();
    let schema = if is_jsonl {
        jsonl::discover_schema(&file)
    } else {
        None
    };
    let t_schema = t.elapsed();
    let level_column = schema
        .as_ref()
        .and_then(levels::find_level_column)
        .map(str::to_string);

    let t = Instant::now();
    let counts = match &level_column {
        Some(col) => levels::count_levels_field(&file, col),
        None => levels::count_levels(&file),
    };
    let t_levels = t.elapsed();

    println!("{}", PHASES_BANNER);
    println!(
        "前置(读+转码)  : {:>7} ms   <- 仅 UTF-16; **不进「索引」也不进 mmap**",
        s.preprocess.as_millis()
    );
    println!(
        "建索引         : {:>7} ms   <- 状态栏原来只报这一段",
        s.index.as_millis()
    );
    println!(
        "JSONL 检测     : {:>7} ms   jsonl={is_jsonl}",
        t_detect.as_millis()
    );
    println!(
        "列发现         : {:>7} ms   列数={:?}",
        t_schema.as_millis(),
        schema.as_ref().map(|x| x.columns.len())
    );
    println!(
        "级别计数       : {:>7} ms   口径={}",
        t_levels.as_millis(),
        level_column.as_deref().unwrap_or("行")
    );
    println!(
        "五段合计       : {:>7} ms   (= open 总墙钟 + 检测 + 列发现 + 计数)",
        s.open.as_millis() + t_detect.as_millis() + t_schema.as_millis() + t_levels.as_millis()
    );

    if let Some(q) = &filter {
        println!("\n== JSONL 字段过滤 ==");
        println!("查询           : {q}");
        // 键名规范化必须与 app 同源 —— 否则 `--filter "LEVEL=ERROR"` 在这里报 0 命中、
        // 在 app 里筛出错误行, 而本工具正是 §4 性能数字与 §7 验收弹药的来源
        // (审查抓到: 这是 `parse_query` 的第二条构造线, 违反 spec D9)。
        let mut clauses = jsonl::parse_query(q);
        if let Some(s) = &schema {
            jsonl::normalize_clause_keys(&mut clauses, s);
        }
        let t = Instant::now();
        let hits = jsonl::run_filter(&file, &clauses);
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

    println!("{}", LEVELS_BANNER);
    let secs = t_levels.as_secs_f64();
    let thr = if secs > 0.0 {
        mib / secs
    } else {
        f64::INFINITY
    };
    println!(
        "墙钟           : {} ms ({thr:.0} MiB/s)",
        t_levels.as_millis()
    );
    for l in levels::Level::ALL {
        println!("{:<14} : {}", l.label(), counts.get(l));
    }
    println!("合计           : {} 行", counts.total());

    // 点选子句 (仅 JSONL 字段口径有): 柱条数字与筛选口径的对照, 一并打印
    if let Some(col) = level_column.as_deref() {
        println!("{}", QUERIES_BANNER);
        for l in levels::Level::ALL {
            println!(
                "{:<14} : {:>9}   {}",
                l.label(),
                counts.get(l),
                levels::field_query(col, l).unwrap_or_else(|| "(只读)".into())
            );
        }
    }

    println!("\n== 总计 ==");
    println!("端到端 (打开+全部基准): {} ms", t_all.elapsed().as_millis());
}
