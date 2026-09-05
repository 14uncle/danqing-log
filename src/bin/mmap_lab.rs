//! @author 十四叔
//! @date 2026/09/05
//!
//! T3 实验台: Windows 上 mmap 存活期外部修改行为实测。
//!
//! 用法: mmap_lab <truncate|rename|delete|append|overwrite>
//! 每个场景独立进程跑 (访问失效页面会崩, 崩 = 实验结果本身, 不传染其它场景)。
//! 结果抄进 tasks/plan.md 附录与意图文档 POC 边界修正。

use std::fs;
use std::io::Write;

use danqing_log::logfile::LogFile;

fn main() {
    let Some(scenario) = std::env::args().nth(1) else {
        eprintln!("用法: mmap_lab <truncate|rename|delete|append|overwrite>");
        std::process::exit(2);
    };
    let dir = std::env::temp_dir().join(format!("mmap-lab-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("victim.log");
    {
        let mut f = fs::File::create(&path).unwrap();
        for i in 0..1000 {
            writeln!(f, "line {i:04} filler filler filler").unwrap();
        }
    }

    let lf = LogFile::open(&path).unwrap();
    println!(
        "打开        : {} 行, line(500) = {:?}",
        lf.line_count(),
        lf.line_lossy(500)
    );

    let outcome: std::io::Result<&str> = match scenario.as_str() {
        // 截断 + 重写 (copytruncate 流派的核心动作)
        "truncate" => {
            fs::File::create(&path).and_then(|mut f| writeln!(f, "short").map(|_| "截断成功"))
        }
        // 改名 (create 流派轮转的第一步)
        "rename" => fs::rename(&path, dir.join("renamed.log")).map(|_| "改名成功"),
        // 删除 (无人引用场景的清理)
        "delete" => fs::remove_file(&path).map(|_| "删除成功"),
        // 追加 (tail 的常态增长)
        "append" => fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut f| writeln!(f, "appended line").map(|_| "追加成功")),
        // 原位覆写 (同长覆写, 不改文件长度)
        "overwrite" => fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .and_then(|mut f| writeln!(f, "OVERWRITTEN-HEAD-PADDING-XXX").map(|_| "覆写成功")),
        _ => {
            eprintln!("未知场景: {scenario}");
            std::process::exit(2);
        }
    };
    match &outcome {
        Ok(m) => println!("修改        : {m}"),
        Err(e) => println!("修改被拒    : {e}"),
    }

    // 修改后读映射视图: 若页面失效, 此行不会执行 (进程崩 = 结果)
    println!(
        "修改后读    : line(0) = {:?} line(500) = {:?}",
        lf.line_lossy(0),
        lf.line_lossy(500)
    );
    println!("场景 {scenario} 完成, 进程存活");
    fs::remove_dir_all(&dir).ok();
}
