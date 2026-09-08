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
}

/// 解析命令行: genlog <输出路径> <目标 MiB> [--jsonl] [--nested]。
/// 未知 flag 与多余位置参数一律报错拒绝 —— 静默吞掉 = 用户要 A 得到 B 还报
/// 成功, 与曾修掉的 args.any() 耗迭代器是同一失败模式, 不能留同类洞。
fn parse_args(args: &[String]) -> Result<Cli, String> {
    const USAGE: &str = "用法: genlog <输出路径> <目标 MiB> [--jsonl] [--nested]";
    let mut positional: Vec<&String> = Vec::new();
    let mut jsonl = false;
    let mut nested = false;
    for a in args {
        match a.as_str() {
            "--jsonl" => jsonl = true,
            "--nested" => nested = true,
            _ if a.starts_with('-') => return Err(format!("未知参数: {a}\n{USAGE}")),
            _ => positional.push(a),
        }
    }
    let [path, mib] = positional.as_slice() else {
        return Err(USAGE.into());
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
    } = cli;
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
}
