//! @author 十四叔
//! @date 2026/09/05
//!
//! 测试日志生成器: 合成指定体积的写实日志, 供 POC 基准与演示。
//!
//! 用法: genlog <输出路径> <目标 MiB> [--jsonl] [--nested]
//! 默认生成明文日志 (时间戳+级别+组件+请求字段); --jsonl 生成扁平结构化日志
//! (开枪前提②的演示数据); --nested 生成嵌套 JSONL (user 对象 + tags 数组,
//! 点路径过滤/嵌套展开的靶子)。输出确定性 (xorshift 伪随机), 同参数同文件。

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

/// xorshift64 伪随机序列 (确定性, 免依赖)。
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// 级别按千分比分布: 940 INFO / 30 DEBUG / 20 WARN / 9 ERROR / 1 FATAL。
fn level(permille: u64) -> &'static str {
    match permille {
        0..=939 => "INFO",
        940..=969 => "DEBUG",
        970..=989 => "WARN",
        990..=998 => "ERROR",
        _ => "FATAL",
    }
}

const COMPONENTS: &[&str] = &[
    "api-worker-1",
    "api-worker-2",
    "api-worker-3",
    "db-proxy",
    "cache-layer",
    "auth-service",
    "scheduler",
];

/// 命令行解析结果。
#[derive(Debug)]
struct Cli {
    path: PathBuf,
    mib: u64,
    /// 生成 JSONL (nested 隐含 true)。
    jsonl: bool,
    /// 生成嵌套 JSONL。
    nested: bool,
    /// merge-timeline T0a: 多源合并 fixture 模式 (`--merge`)。
    merge: bool,
    /// 源数 (merge 模式第三位置参数; 单文件模式恒 0)。
    sources: u64,
}

/// 解析命令行: genlog <输出路径> <目标 MiB> [--jsonl] [--nested]
/// 或: genlog --merge <目录> <每源 MiB> <源数≥2>。
/// 未知 flag 与多余位置参数一律报错拒绝 —— 静默吞掉 = 用户要 A 得到 B 还报
/// 成功, 与曾修掉的 args.any() 耗迭代器是同一失败模式, 不能留同类洞。
fn parse_args(args: &[String]) -> Result<Cli, String> {
    const USAGE: &str = "用法: genlog <输出路径> <目标 MiB> [--jsonl] [--nested]\n  或: genlog --merge <目录> <每源 MiB> <源数≥2>";
    let mut positional: Vec<&String> = Vec::new();
    let mut jsonl = false;
    let mut nested = false;
    let mut merge = false;
    for a in args {
        match a.as_str() {
            "--jsonl" => jsonl = true,
            "--nested" => nested = true,
            "--merge" => merge = true,
            _ if a.starts_with('-') => return Err(format!("未知参数: {a}\n{USAGE}")),
            _ => positional.push(a),
        }
    }
    let (path, mib, sources) = if merge {
        let [path, mib, sources] = positional.as_slice() else {
            return Err(USAGE.into());
        };
        let sources: u64 = sources
            .parse()
            .ok()
            .filter(|n| *n >= 2)
            .ok_or_else(|| format!("源数必须是 ≥2 的整数: {sources}"))?;
        (path, mib, sources)
    } else {
        let [path, mib] = positional.as_slice() else {
            return Err(USAGE.into());
        };
        (path, mib, 0)
    };
    let mib: u64 = mib
        .parse()
        .ok()
        .filter(|m| *m > 0)
        .ok_or_else(|| format!("目标 MiB 必须是正整数: {mib}"))?;
    Ok(Cli {
        path: PathBuf::from(path),
        mib,
        jsonl: jsonl || nested,
        nested,
        merge,
        sources,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = match parse_args(&args) {
        Ok(cli) => cli,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let Cli {
        path,
        mib,
        jsonl,
        nested,
        merge,
        sources,
    } = cli;
    if merge {
        merge_gen(&path, mib, sources);
        return;
    }
    let target = mib * 1024 * 1024;

    let t = Instant::now();
    let file = File::create(&path).unwrap_or_else(|e| {
        eprintln!("创建文件失败 {}: {e}", path.display());
        std::process::exit(1);
    });
    let mut w = BufWriter::with_capacity(8 << 20, file);
    let mut rng = Rng(0x42);
    let mut line = String::with_capacity(256);
    let mut written = 0u64;
    let mut lines = 0u64;

    while written < target {
        line.clear();
        let mut r = || rng.next();
        let secs = r() % 86_400;
        let (h, m, sec) = (secs / 3600, (secs % 3600) / 60, secs % 60);
        let ms = r() % 1000;
        let lv = level(r() % 1000);
        let comp = COMPONENTS[(r() % COMPONENTS.len() as u64) as usize];
        let req = r();
        let user = r() % 1_000_000;
        let dur = r() % 3000;
        let bytes = r() % 65536;
        let status = [
            200, 200, 200, 200, 201, 204, 301, 400, 401, 403, 404, 500, 502,
        ][(r() % 13) as usize];
        let endpoint = r() % 100_000;
        if nested {
            // 嵌套 JSONL: user 为对象 (id+name), tags 数组 —— 点路径过滤/展开的靶子
            line.push_str(&format!(
                "{{\"ts\":\"2026-09-05T{h:02}:{m:02}:{sec:02}.{ms:03}Z\",\"level\":\"{lv}\",\"logger\":\"{comp}\",\"msg\":\"request completed\",\"req_id\":\"{req:016x}\",\"user\":{{\"id\":{user},\"name\":\"user_{user}\"}},\"tags\":[\"api\",\"orders\"],\"duration_ms\":{dur},\"bytes\":{bytes},\"status\":{status},\"path\":\"/api/v1/orders/{endpoint}\"}}\n"
            ));
        } else if jsonl {
            line.push_str(&format!(
                "{{\"ts\":\"2026-09-05T{h:02}:{m:02}:{sec:02}.{ms:03}Z\",\"level\":\"{lv}\",\"logger\":\"{comp}\",\"msg\":\"request completed\",\"req_id\":\"{req:016x}\",\"user\":\"user_{user}\",\"duration_ms\":{dur},\"bytes\":{bytes},\"status\":{status},\"path\":\"/api/v1/orders/{endpoint}\"}}\n"
            ));
        } else {
            line.push_str(&format!(
                "2026-09-05T{h:02}:{m:02}:{sec:02}.{ms:03}Z {lv:<5} [{comp}] request completed req_id={req:016x} user=user_{user} duration_ms={dur} bytes={bytes} status={status} path=/api/v1/orders/{endpoint}\n"
            ));
        }
        w.write_all(line.as_bytes()).unwrap();
        written += line.len() as u64;
        lines += 1;
    }
    w.flush().unwrap();

    println!(
        "生成 {}: {lines} 行, {:.1} MiB, {} ms ({:.0} MiB/s)",
        path.display(),
        written as f64 / (1024.0 * 1024.0),
        t.elapsed().as_millis(),
        written as f64 / (1024.0 * 1024.0) / t.elapsed().as_secs_f64(),
    );
}

// ─── merge-timeline T0a: 多源合并 fixture ───

/// 合并 fixture 的时间原点: 2026-09-27T00:00:00Z (与 danqing-logfile
/// timestamp.rs 测试同基准, 对拍时一眼可算)。
const MERGE_BASE_MS: i64 = 1_790_467_200_000;
/// 相关 req_id 节奏: 每源每第 CORRELATE_EVERY 行产一条 `c0ffee%011x`,
/// 同序号跨源同值 —— 追踪验收 (b) 的靶子: 合并视图按它过滤, 每源应各出一条。
const CORRELATE_EVERY: u64 = 997;

/// 单源真值 (真值表打印 + truth.txt, 人工验收 (d)「构造已知序」的对照)。
#[derive(Debug)]
struct SourceTruth {
    file: String,
    format_desc: &'static str,
    skew_ms: i64,
    lines: u64,
    first_ts_ms: i64,
    last_ts_ms: i64,
}

/// epoch 毫秒 → (年,月,日,时,分,秒,毫秒) UTC (civil_from_days, Hinnant 逆算法,
/// 与 danqing-logfile timestamp.rs 的 days_from_civil 同源互逆)。
fn civil_from_epoch_ms(epoch_ms: i64) -> (i64, u32, u32, u32, u32, u32, u32) {
    let days = epoch_ms.div_euclid(86_400_000);
    let rem = epoch_ms.rem_euclid(86_400_000);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (
        y,
        m,
        d,
        (rem / 3_600_000) as u32,
        (rem % 3_600_000 / 60_000) as u32,
        (rem % 60_000 / 1_000) as u32,
        (rem % 1_000) as u32,
    )
}

fn iso_z(epoch_ms: i64) -> String {
    let (y, mo, d, h, mi, s, ms) = civil_from_epoch_ms(epoch_ms);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{ms:03}Z")
}

fn log4j_comma(epoch_ms: i64) -> String {
    let (y, mo, d, h, mi, s, ms) = civil_from_epoch_ms(epoch_ms);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02},{ms:03}")
}

/// 每源时钟偏移 (确定性公式): -3.4s / -1.7s / 0 / +1.7s / +3.4s 循环 ——
/// 「测试机的钟比生产快 3.4 秒」的校准靶 (人工验收 d)。
fn source_skew_ms(i: u64) -> i64 {
    (i as i64 % 5 - 2) * 1700
}

/// 生成多源合并 fixture: 偶数源 .jsonl / 奇数源 .log, 格式轮换覆盖
/// ISO 串 / epoch 数 (JSONL) 与 ISO / log4j 逗号 / epoch 毫秒 (.log);
/// 每源带时钟偏移 + ~0.3% 乱序 + continuation/无 ts 行 + 跨源相关 req_id。
fn merge_gen(dir: &PathBuf, mib: u64, sources: u64) {
    std::fs::create_dir_all(dir).unwrap_or_else(|e| {
        eprintln!("创建目录失败 {}: {e}", dir.display());
        std::process::exit(1);
    });
    let target = mib * 1024 * 1024;
    let t_all = Instant::now();
    let mut truths: Vec<SourceTruth> = Vec::new();

    for i in 0..sources {
        let jsonl_kind = i % 2 == 0;
        let path = dir.join(if jsonl_kind {
            format!("src{i}.jsonl")
        } else {
            format!("src{i}.log")
        });
        // .log 源格式轮换: i%3 → 0=log4j 逗号, 1=ISO, 2=epoch 毫秒
        // (奇数 i 命中三种各有机会); JSONL 源: i%4==2 → ts 用 epoch 数, 否则 ISO 串。
        let (format_desc, log_fmt): (&'static str, u8) = if jsonl_kind {
            if i % 4 == 2 {
                ("JSONL ts=epoch 数", 2)
            } else {
                ("JSONL ts=ISO 串", 1)
            }
        } else {
            match i % 3 {
                0 => (".log log4j 逗号毫秒", 0),
                1 => (".log ISO-8601 Z", 1),
                _ => (".log epoch 毫秒", 2),
            }
        };
        let skew = source_skew_ms(i);
        let mut rng = Rng(0x42 + i);
        let file = File::create(&path).unwrap_or_else(|e| {
            eprintln!("创建文件失败 {}: {e}", path.display());
            std::process::exit(1);
        });
        let mut w = BufWriter::with_capacity(8 << 20, file);
        let mut ts = MERGE_BASE_MS + skew;
        let mut written = 0u64;
        let mut lines = 0u64;
        let mut first_ts = 0i64;
        let mut line = String::with_capacity(256);

        while written < target {
            ts += 1 + (rng.next() % 37) as i64;
            // ~0.3% 乱序: 回跳 0.5–2s (归并「钉住」语义的靶子)。
            if rng.next() % 1000 < 3 {
                ts -= 500 + (rng.next() % 1500) as i64;
            }
            let mut r = || rng.next();
            let lv = level(r() % 1000);
            let req = if lines > 0 && lines % CORRELATE_EVERY == 0 {
                format!("c0ffee{:011x}", lines / CORRELATE_EVERY)
            } else {
                format!("{req:016x}", req = r())
            };
            let user = r() % 1_000_000;
            let dur = r() % 3000;
            let bytes = r() % 65536;
            let status = [200, 200, 200, 201, 204, 400, 404, 500, 502][(r() % 9) as usize];
            let endpoint = r() % 100_000;
            line.clear();
            if jsonl_kind {
                if lines > 10 && r() % 200 == 0 {
                    // 0.5% 无 ts 字段行 (继承上一行的靶子)。
                    line.push_str(&format!(
                        "{{\"level\":\"{lv}\",\"logger\":\"src{i}\",\"msg\":\"request completed\",\"req_id\":\"{req}\",\"status\":{status}}}\n"
                    ));
                } else {
                    let ts_text = match log_fmt {
                        2 => format!("{ts}"),
                        _ => format!("\"{}\"", iso_z(ts)),
                    };
                    line.push_str(&format!(
                        "{{\"ts\":{ts_text},\"level\":\"{lv}\",\"logger\":\"src{i}\",\"msg\":\"request completed\",\"req_id\":\"{req}\",\"user\":\"user_{user}\",\"duration_ms\":{dur},\"bytes\":{bytes},\"status\":{status},\"path\":\"/api/v1/orders/{endpoint}\"}}\n"
                    ));
                }
            } else if lines > 10 && r() % 100 == 0 {
                // 1% continuation (stack trace, 无行首时间戳)。
                line.push_str(&format!(
                    "    at fake.stack.Frame$$NativeMethod(Frame.java:{n})\n",
                    n = r() % 500
                ));
            } else {
                let prefix = match log_fmt {
                    0 => log4j_comma(ts),
                    1 => iso_z(ts),
                    _ => format!("{ts}"),
                };
                line.push_str(&format!(
                    "{prefix} {lv:<5} [src{i}] request completed req_id={req} user=user_{user} duration_ms={dur} bytes={bytes} status={status} path=/api/v1/orders/{endpoint}\n"
                ));
            }
            if lines == 0 {
                first_ts = ts;
            }
            w.write_all(line.as_bytes()).unwrap();
            written += line.len() as u64;
            lines += 1;
        }
        w.flush().unwrap();
        truths.push(SourceTruth {
            file: path.display().to_string(),
            format_desc,
            skew_ms: skew,
            lines,
            first_ts_ms: first_ts,
            last_ts_ms: ts,
        });
    }

    // 真值表: 打印 + 落 truth.txt (验收对照留档)。
    let mut truth = String::new();
    truth.push_str("== 多源合并 fixture 真值表 ==\n");
    truth.push_str(&format!(
        "时间原点       : 2026-09-27T00:00:00Z ({MERGE_BASE_MS})\n"
    ));
    truth.push_str(&format!(
        "相关 req_id   : 每源每 {CORRELATE_EVERY} 行一条 c0ffee%011x (同序号跨源同值)\n"
    ));
    truth.push_str("时钟偏移公式   : (i%5-2)*1700 ms; 乱序 ~0.3% 回跳 0.5-2s; continuation/无 ts 行 ≈1%/0.5%\n");
    for (i, t) in truths.iter().enumerate() {
        truth.push_str(&format!(
            "[src{i}] {}\n    格式={}  偏移={}ms  行数={}  首={}({})  末={}({})\n",
            t.file,
            t.format_desc,
            t.skew_ms,
            t.lines,
            t.first_ts_ms,
            iso_z(t.first_ts_ms),
            t.last_ts_ms,
            iso_z(t.last_ts_ms),
        ));
    }
    print!("{truth}");
    let truth_path = dir.join("truth.txt");
    std::fs::write(&truth_path, &truth).unwrap_or_else(|e| {
        eprintln!("写真值表失败 {}: {e}", truth_path.display());
    });
    println!("合计 {} 源, {} ms", sources, t_all.elapsed().as_millis());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_plain_default() {
        let cli = parse_args(&args(&["out.log", "10"])).unwrap();
        assert!(!cli.jsonl && !cli.nested);
        assert_eq!(cli.mib, 10);
        assert_eq!(cli.path, PathBuf::from("out.log"));
    }

    #[test]
    fn parse_jsonl_flag_not_swallowed() {
        // 回归: 旧实现连续两次 args.any() 耗光迭代器, --jsonl 单传被静默吞掉
        let cli = parse_args(&args(&["out.log", "10", "--jsonl"])).unwrap();
        assert!(cli.jsonl && !cli.nested);
    }

    #[test]
    fn parse_nested_implies_jsonl() {
        let cli = parse_args(&args(&["out.log", "10", "--nested"])).unwrap();
        assert!(cli.nested && cli.jsonl);
    }

    #[test]
    fn parse_flags_may_precede_positionals() {
        let cli = parse_args(&args(&["--jsonl", "out.log", "10"])).unwrap();
        assert!(cli.jsonl);
        assert_eq!(cli.path, PathBuf::from("out.log"));
    }

    #[test]
    fn parse_rejects_unknown_flag() {
        let err = parse_args(&args(&["out.log", "10", "--jsnl"])).unwrap_err();
        assert!(err.contains("--jsnl"), "错误信息应带出原 flag: {err}");
    }

    #[test]
    fn parse_rejects_extra_positional() {
        assert!(parse_args(&args(&["a", "1", "b"])).is_err());
    }

    #[test]
    fn parse_rejects_bad_mib() {
        assert!(parse_args(&args(&["out.log", "0"])).is_err());
        assert!(parse_args(&args(&["out.log", "abc"])).is_err());
    }

    #[test]
    fn parse_requires_two_positionals() {
        assert!(parse_args(&args(&["out.log"])).is_err());
        assert!(parse_args(&args(&[])).is_err());
    }

    // ─── 多源合并 fixture (merge-timeline T0a) ───

    #[test]
    fn parse_merge_mode() {
        let cli = parse_args(&args(&["--merge", "dir", "1", "3"])).unwrap();
        assert!(cli.merge && cli.sources == 3 && cli.mib == 1);
    }

    #[test]
    fn parse_merge_rejects_bad_arity_and_sources() {
        assert!(parse_args(&args(&["--merge", "dir", "1"])).is_err());
        assert!(parse_args(&args(&["--merge", "dir", "1", "1"])).is_err()); // 源数 <2
        assert!(parse_args(&args(&["--merge", "dir", "1", "x"])).is_err());
    }

    #[test]
    fn merge_fixture_deterministic_and_truthful() {
        let da = std::env::temp_dir().join("danqing-genlog-merge-a");
        let db = std::env::temp_dir().join("danqing-genlog-merge-b");
        std::fs::remove_dir_all(&da).ok();
        std::fs::remove_dir_all(&db).ok();
        merge_gen(&da, 1, 3);
        merge_gen(&db, 1, 3);
        // 确定性: 同参数同字节。
        for f in ["src0.jsonl", "src1.log", "src2.jsonl"] {
            let a = std::fs::read(da.join(f)).unwrap();
            let b = std::fs::read(db.join(f)).unwrap();
            assert_eq!(a, b, "{f} 两次生成不一致");
        }
        // 真值对拍 ①: src0 (JSONL ISO 串, skew=-3400) 首行 ts 落在
        // [base-3400+1, base-3400+37] (首行 ts = base+skew+1+r%37)。
        let c0 = std::fs::read(da.join("src0.jsonl")).unwrap();
        let line0 = c0.split(|&b| b == b'\n').next().unwrap();
        let t0 = danqing_log::timestamp::parse_jsonl_field(line0, "ts", 0).unwrap();
        let lo = MERGE_BASE_MS - 3400 + 1;
        assert!(
            (lo..=lo + 36).contains(&t0),
            "src0 首行 ts={t0} 不在 [{lo}, {}]",
            lo + 36
        );
        // 真值对拍 ②: src2 (JSONL epoch 数, skew=0) 与 src0 首行 ts 差 ≈ 3400 (±37)。
        let c2 = std::fs::read(da.join("src2.jsonl")).unwrap();
        let line2 = c2.split(|&b| b == b'\n').next().unwrap();
        let t2 = danqing_log::timestamp::parse_jsonl_field(line2, "ts", 0).unwrap();
        let diff = t2 - t0;
        assert!((3400 - 37..=3400 + 37).contains(&diff), "偏移差 {diff}");
        // 真值对拍 ③: src1 (.log, i%3==1 → ISO Z, skew=-1700) 行首时间戳可解析。
        let c1 = std::fs::read(da.join("src1.log")).unwrap();
        let line1 = c1.split(|&b| b == b'\n').next().unwrap();
        let (t1, _) = danqing_log::timestamp::parse_prefix(
            line1,
            danqing_log::timestamp::TsFormat::Iso8601,
            0,
        )
        .unwrap();
        let lo1 = MERGE_BASE_MS - 1700 + 1;
        assert!((lo1..=lo1 + 36).contains(&t1));
        // 真值对拍 ④: 相关 req_id c0ffee00000000001 (%011x of 1) 出现在 ≥2 个源里。
        let needle = b"c0ffee00000000001";
        let hits = [&c0, &c1, &c2]
            .iter()
            .filter(|c| c.windows(needle.len()).any(|w| w == needle))
            .count();
        assert!(hits >= 2, "相关 req_id 只出现在 {hits} 个源");
        std::fs::remove_dir_all(&da).ok();
        std::fs::remove_dir_all(&db).ok();
    }
}
