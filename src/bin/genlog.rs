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

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(path), Some(mib)) = (args.next().map(PathBuf::from), args.next()) else {
        eprintln!("用法: genlog <输出路径> <目标 MiB> [--jsonl] [--nested]");
        std::process::exit(2);
    };
    let nested = args.any(|a| a == "--nested");
    let jsonl = args.any(|a| a == "--jsonl") || nested;
    let target: u64 = match mib.parse::<u64>() {
        Ok(m) if m > 0 => m * 1024 * 1024,
        _ => {
            eprintln!("目标 MiB 必须是正整数: {mib}");
            std::process::exit(2);
        }
    };

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
