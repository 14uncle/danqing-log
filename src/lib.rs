//! @author 十四叔
//! @date 2026/09/05
//!
//! 丹青日志 —— 库根: 引擎层供 GUI (main.rs) 与基准工具 (bin/logbench) 共用。
//!
//! logfile/jsonl 引擎已拆为兄弟 crate `danqing-logfile` (2026-09-10)。

pub use danqing_logfile::{jsonl, logfile};
pub mod expand;
pub mod open;
pub mod search;
