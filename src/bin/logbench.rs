//! @author 十四叔
//! @date 2026/09/05
//!
//! 无窗口基准: 打开/索引/搜索/过滤/随机访问全链路实测, 控制台表格输出。
//!
//! 用法: logbench <日志文件> [--filter "level=ERROR status=50*"] [正则...]
//! 不带正则时跑四个默认模式 (字面带量 / 简单交替 / 数字后缀 / 时间戳结构)。
//! --filter 走 JSONL 字段过滤 (前提②的引擎数字)。
//! 输出即开枪前提的截图弹药, 数字可直接抄进意图文档与 Show HN 稿。

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use danqing_log::export::{ExportFormat, ExportSet, WriteOutcome};
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
    let raw: Vec<String> = std::env::args().skip(1).collect();
    // merge-timeline T0b: `--merge <文件...>` 走合并基准, 与单文件口径分家。
    if raw.first().is_some_and(|a| a == "--merge") {
        merge_main(&raw[1..]);
        return;
    }
    let mut args = raw.into_iter();
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!(
            "用法: logbench <日志文件> [--filter \"level=ERROR status=50*\"] [--analyze <字段>] [--export <raw|pretty|csv>] [正则...]"
        );
        std::process::exit(2);
    };
    let mut filter: Option<String> = None;
    let mut analyze: Option<String> = None;
    let mut export_fmt: Option<String> = None;
    let mut patterns: Vec<String> = Vec::new();
    let mut it = args;
    while let Some(a) = it.next() {
        if a == "--filter" {
            filter = it.next();
        } else if a == "--analyze" {
            analyze = it.next();
        } else if a == "--export" {
            export_fmt = it.next();
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

    // --analyze 的作用域跟随 --filter (D3): 命中行集留给分析复用
    let mut filtered_rows: Option<Vec<u64>> = None;
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
        filtered_rows = Some(hits);
    }

    if let Some(field) = &analyze {
        println!(
            "
== 字段分析 =="
        );
        println!("字段           : {field}");
        let t = Instant::now();
        let a = danqing_log::analysis::analyze_field(&file, field, filtered_rows.as_deref());
        let el = t.elapsed();
        println!("作用域         : {} 行", a.scope_rows);
        if a.skipped > 0 {
            println!("跳过           : {} 行 (取不到/嵌套/类型不符)", a.skipped);
        }
        println!("耗时           : {} ms", el.as_millis());
        match &a.result {
            danqing_log::analysis::AnalysisResult::Numeric(st) => {
                println!(
                    "数值           : count={} min={} max={} mean={:.2} p50={} p95={} p99={}{}",
                    st.count,
                    st.min,
                    st.max,
                    st.mean,
                    st.p50,
                    st.p95,
                    st.p99,
                    if st.sampled {
                        "  (分位数为采样估计)"
                    } else {
                        ""
                    },
                );
            }
            danqing_log::analysis::AnalysisResult::Enum(en) => {
                println!("枚举 (Top {})  :", en.top.len());
                for (v, c) in &en.top {
                    println!("  {v:<32} {c}");
                }
                if en.others > 0 {
                    println!("  其他(共 {} 行)", en.others);
                }
            }
        }
    }

    // 导出 (SPEC-v1x-export D9): 与 app 同源调导出核心 —— 数字与真实路径同一份代码。
    // 行集跟随 --filter (同 --analyze 的作用域口径); 无过滤 = 全集。
    if let Some(fmt) = &export_fmt {
        println!("\n== 导出 (SPEC-v1x-export D9) ==");
        let set = match &filtered_rows {
            Some(hits) => ExportSet::from_lines(hits.clone(), file.line_count()),
            None => ExportSet::all(file.line_count()),
        };
        let columns: Vec<String> = schema
            .as_ref()
            .map(|s| s.columns.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();
        let (format, ext) = match fmt.as_str() {
            "raw" => (ExportFormat::Raw, "log"),
            "pretty" => (ExportFormat::Pretty, "json"),
            "csv" => (ExportFormat::Csv { columns }, "csv"),
            other => {
                eprintln!("未知导出格式: {other} (raw|pretty|csv)");
                std::process::exit(2);
            }
        };
        let out_path = std::env::temp_dir().join(format!("logbench-export.{ext}"));
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let progress = std::sync::atomic::AtomicU64::new(0);
        let t = Instant::now();
        let mut w = std::io::BufWriter::new(std::fs::File::create(&out_path).expect("建导出文件"));
        let res = format.write(&file, &set, &mut w, &cancel, &progress);
        let (outcome, bad) = res.expect("导出写出");
        w.flush().expect("flush");
        let el = t.elapsed();
        let bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
        let secs = el.as_secs_f64();
        let out_mib = bytes as f64 / (1024.0 * 1024.0);
        let thr = if secs > 0.0 {
            out_mib / secs
        } else {
            f64::INFINITY
        };
        let lines = match outcome {
            WriteOutcome::Done { lines } => lines,
            WriteOutcome::Cancelled { lines } => lines,
        };
        println!("格式           : {fmt}");
        println!(
            "行集           : {} 行 ({})",
            set.len(),
            if set.is_full() {
                "全集"
            } else {
                "过滤命中"
            }
        );
        println!(
            "写出           : {lines} 行 / {out_mib:.1} MiB   {} ms ({thr:.0} MiB/s)",
            el.as_millis()
        );
        if bad > 0 {
            println!("非 JSON 原样   : {bad} 行");
        }
        println!("输出           : {}", out_path.display());
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

// ─── merge-timeline T9 实测: 四组数字 (打开 / 重归并 / 隐藏重建 / 跟随追加) ───
//
// **走产品路径** (`merge_view::build_merge` / `MergeState`) —— 数字口径 = 用户在
// 应用里真走的那条链 (打开+探测+schema 发现+提取+归并), 不是引擎裸拼。T0c 的
// CP0 校准数字 (1525ms) 是引擎三段口径, 两者并列报告 (disclose 差异来源)。

fn merge_main(files: &[String]) {
    use danqing_log::merge_view::{self, MergeState};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    if files.len() < 2 {
        eprintln!("用法: logbench --merge <文件...> (≥2 源)");
        std::process::exit(2);
    }
    let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    println!("== 合并基准 (merge-timeline T9, 产品路径, 四组) ==");
    println!("源数: {}", paths.len());

    // ── ① 打开 (合并就绪) ─────────────────────────────────────────────
    let t = Instant::now();
    let out = merge_view::build_merge(&paths, &[], &AtomicBool::new(false));
    let open_el = t.elapsed();
    for (p, why) in &out.rejected {
        println!("[拒收] {} —— {why} (SPEC D2)", p.display());
    }
    if out.sources.len() < 2 {
        println!("有效源 <2, 归并无意义。");
        return;
    }
    println!(
        "\n① 合并就绪 (打开+探测+schema+提取+归并): {} ms",
        open_el.as_millis()
    );
    let mut total_lines = 0u64;
    let mut total_bytes = 0u64;
    let mut ts_bytes = 0usize;
    for (i, s) in out.sources.iter().enumerate() {
        let st = s.file.stats();
        total_lines += s.file.line_count();
        total_bytes += st.file_bytes;
        ts_bytes += out.ts[i].len() * 8;
        println!(
            "   [src{i}] {} —— {:.0} MiB, {} 行, 通路 {}",
            s.path.display(),
            st.file_bytes as f64 / (1024.0 * 1024.0),
            s.file.line_count(),
            merge_view::route_label(&s.route)
        );
    }
    println!(
        "   合计 {} 行 / {:.2} GiB · 索引 {:.1} MiB ({} B) · ts 向量 {:.1} MiB",
        total_lines,
        total_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        out.index.index_bytes() as f64 / (1024.0 * 1024.0),
        out.index.index_bytes(),
        ts_bytes as f64 / (1024.0 * 1024.0)
    );
    println!("   D5 红线 (CP0 校准, 引擎三段口径): 3×1GB ≤ 1.6s / 8×200MB ≤ 1.0s");

    // ── ② 重归并 (加/减源 · 轮转 · 巨量追平共用的全量重跑) ─────────────
    let t = Instant::now();
    let out2 = merge_view::build_merge(&paths, &[], &AtomicBool::new(false));
    let rebuild_el = t.elapsed();
    println!(
        "\n② 重归并 (全量重跑, 旧 bundle 在此期间保持可见): {} ms   [同进程二次, 页缓存已热]",
        rebuild_el.as_millis()
    );

    // ── ③ 隐藏重建 (掩码重归并, 不重提时间戳) ─────────────────────────
    let mut st = MergeState::from_outcome(out2);
    let full_rows = st.row_count();
    let t = Instant::now();
    st.sources[0].hidden = true;
    st.rebuild_masked();
    let masked_el = t.elapsed();
    println!(
        "\n③ 隐藏重建 (源0 隐藏 → 掩码重归并, 不重提): {} ms → {} 行 (原 {full_rows} 行)",
        masked_el.as_millis(),
        st.row_count()
    );
    st.sources[0].hidden = false;
    st.rebuild_masked();

    // ── ④ 跟随追加 (live-tail 增量: append_from + 增量合流) ───────────
    // 真追加不许动基准数据 → 对**目标源做临时副本**, 副本参与建场 (归并源就是它),
    // 追加只落在副本上。两条形态都测: 追最新源 (尾行 ts 全局最大) = 常态快路;
    // 追滞后源 (尾行 ts 落后全局尾数小时) = 尾端回找的深路径。
    let leader = extreme_ts_source(&out.ts, true);
    let laggard = extreme_ts_source(&out.ts, false);
    // 块大小可调 (LOGBENCH_BLOCK_KIB): 小批验「每行成本 = 回找深度」模型。
    let block = std::env::var("LOGBENCH_BLOCK_KIB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4096)
        * 1024;
    println!(
        "\n④ 跟随追加 (每次 +{:.1} MiB 行对齐尾块, 目标源做临时副本):",
        block as f64 / 1048576.0
    );
    // 探针: 1 GiB 源的 mmap 建立 / 释放成本 —— 每次追加都要换入新快照 (旧映射
    // 随之释放), 这笔固定开销按**源大小**付, 与追加行数无关。
    {
        use danqing_log::logfile::LogFile;
        let t = Instant::now();
        let f = LogFile::open(&paths[0]).expect("探针源可开");
        let open_ms = t.elapsed();
        for l in 0..f.line_count() {
            std::hint::black_box(f.line(l)); // 触页: 逼近归并/显示读过的驻留态
        }
        let t = Instant::now();
        drop(f);
        println!(
            "   探针: 1 GiB 源 mmap 建立 {} ms / **释放 {} ms** (全页触过)",
            open_ms.as_millis(),
            t.elapsed().as_millis()
        );
    }
    append_case(
        "追最新源 (尾行 ts 全局最大 → 常态快路)",
        &paths,
        leader,
        block,
        false,
        "lead",
    );
    append_case(
        "追滞后源 (尾行 ts 落后全局尾 → 深回找)",
        &paths,
        laggard,
        block,
        false,
        "lag",
    );
    append_case(
        "追滞后源 · 追踪态 (T7 留档复核)",
        &paths,
        laggard,
        block,
        true,
        "lagtr",
    );
    std::fs::remove_file(copy_path(&paths[leader], "lead")).ok();
    std::fs::remove_file(copy_path(&paths[laggard], "lag")).ok();
    std::fs::remove_file(copy_path(&paths[laggard], "lagtr")).ok();
}

/// 尾行 ts 最大 / 最小的源序号 (基准自己挑靶, 不靠人工指定索引)。
fn extreme_ts_source(ts: &[Vec<i64>], want_max: bool) -> usize {
    // 空源 (无行) 给中性值, 它不该被挑成靶; 取最小侧用 `min_by_key` 而非
    // `-key` —— `i64::MIN` 取负会溢出 (debug panic / release 回绕, 评审 Nit)。
    let key = |i: usize| {
        ts[i]
            .last()
            .copied()
            .unwrap_or(if want_max { i64::MIN } else { i64::MAX })
    };
    if want_max {
        (0..ts.len()).max_by_key(|&i| (key(i), i)).unwrap_or(0)
    } else {
        (0..ts.len()).min_by_key(|&i| (key(i), i)).unwrap_or(0)
    }
}

/// 目标源的临时副本路径 (追加只落副本)。`tag` 区分同源多例 (D3 异步释放下
/// 旧映射可能仍在, 同路径重建会被 Windows 映射语义拒 —— 每例一个名)。
fn copy_path(src: &std::path::Path, tag: &str) -> PathBuf {
    let stem = src.file_name().and_then(|s| s.to_str()).unwrap_or("src");
    std::env::temp_dir().join(format!(
        "danqing-mergebench-{}-{stem}-{tag}",
        std::process::id()
    ))
}

/// 单源追加剧组: 副本建场 → (可选) 追踪 → 追加 → 计时 + 真值位诊断。
fn append_case(
    label: &str,
    paths: &[PathBuf],
    target: usize,
    budget: usize,
    with_trace: bool,
    tag: &str,
) {
    use danqing_log::logfile::LogFile;
    use danqing_log::merge_view::{self, MergeState};
    use std::sync::atomic::AtomicBool;

    let copy = copy_path(&paths[target], tag);
    if std::fs::copy(&paths[target], &copy).is_err() {
        println!("   {label}: 跳过 (临时副本创建失败)");
        return;
    }
    let mut p2 = paths.to_vec();
    p2[target] = copy.clone();
    let mut st =
        MergeState::from_outcome(merge_view::build_merge(&p2, &[], &AtomicBool::new(false)));
    let block = tail_block(&copy, budget);
    // 真值位诊断: 新行尾 ts 之后还剩多少合并行 = 尾端回找要走的步数。
    // (块是文件尾部的副本, 其 ts 上界 = 该源当前尾行 ts。)
    let new_max = st.ts[target].last().copied().unwrap_or(0);
    let depth = st.index.rows().iter().filter(|r| r.ts > new_max).count();

    if with_trace {
        let value = shortest_line(&block);
        let files: Vec<std::sync::Arc<LogFile>> = st
            .sources
            .iter()
            .map(|s| std::sync::Arc::clone(&s.file))
            .collect();
        let clause = merge_view::trace_clause(&value);
        let t = Instant::now();
        let (hits, _) = merge_view::trace_hits(&files, &clause);
        let scan = t.elapsed();
        let t = Instant::now();
        let n = st.apply_trace(hits.clone(), (0, 0));
        let push = t.elapsed();
        let t = Instant::now();
        st.apply_trace(hits, (0, 0));
        let repush = t.elapsed();
        println!(
            "      追踪 \"{value}\": 全源扫描 {} ms → {n} 命中; 过滤集重推 {} ms / 再推 {} ms (O(合并行数))",
            scan.as_millis(),
            push.as_millis(),
            repush.as_millis()
        );
    }

    append_block(&copy, &block);
    // 选中位变体 (LOGBENCH_SEL_TAIL): 选中行落在索引深处 → 追加后的
    // (源,行)→位置 锚定走线性扫 (position_of), 成本随合并行数走。
    if std::env::var("LOGBENCH_SEL_TAIL").is_ok() {
        st.selected = st.row_count().saturating_sub(1);
    }
    let t = Instant::now();
    let new_file = LogFile::append_from(&st.sources[target].file, &copy).expect("副本可读");
    let incr = t.elapsed();
    let t = Instant::now();
    let added = st.append_source(target as u32, new_file);
    let merge_incr = t.elapsed();
    println!(
        "   {label}: 行索引增量 {} ms + 合流 {} ms = {} ms → +{added} 行 (时间线 {} 行)",
        incr.as_millis(),
        merge_incr.as_millis(),
        (incr + merge_incr).as_millis(),
        st.row_count()
    );
    println!(
        "      诊断: 新行真值深度 {depth} 行 (T9 起插入无帽 = 真值位; 产品侧 >20000 行/轮交 worker 重归并)"
    );
}

/// 取文件尾部的**行对齐**字节块 (≤ `budget`; 起点必须是行首, 末尾保持原样)。
/// 追加它 = 真实「新行落到时间线尾」形态 (时间戳与被复制段相同、行号更大,
/// 归并序紧跟在原段之后)。
fn tail_block(path: &std::path::Path, budget: usize) -> Vec<u8> {
    let bytes = std::fs::read(path).expect("基准源可读");
    let start = bytes.len().saturating_sub(budget);
    let head = bytes[start..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|i| start + i + 1)
        .unwrap_or(start);
    bytes[head..].to_vec()
}

/// 追加字节块 (OpenOptions append; 基准源只许追加不许改写)。
fn append_block(path: &std::path::Path, block: &[u8]) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("基准源可追加");
    f.write_all(block).expect("追加写失败");
    f.flush().expect("flush 失败");
}

/// 块内**最短非空行** (trim 后) —— 追踪值取它: 用户可选的「选中一个值」形态,
/// 且短行不会把 trace 值撑成半行文本。
fn shortest_line(block: &[u8]) -> String {
    block
        .split(|&b| b == b'\n')
        .map(|l| String::from_utf8_lossy(l).trim().to_string())
        .filter(|l| !l.is_empty())
        .min_by_key(|l| l.len())
        .unwrap_or_else(|| "x".to_string())
}
