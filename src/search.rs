//! @author 十四叔
//! @date 2026/09/05
//!
//! 搜索/过滤的异步作业与命中导航 (core-viewer T5)。
//!
//! AsyncJob 是 POC 过滤架构 (worker 线程 + rev 计数 + tick 拾取) 的泛化,
//! 同构服务字段过滤与正则搜索。唤醒零附加机制: OnDemand 可见态 ~60fps tick
//! 轮询 rev, 完成至显示 ≤16ms (boost_frames 仅 Adaptive 生效, 用不上)。

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// 异步作业: launch 起工作线程, poll 心跳拾取, 代次防乱序覆盖
/// (两次快速 Enter: 旧轮晚到的结果被丢弃, 不覆盖新轮)。
pub struct AsyncJob<T> {
    done: Arc<Mutex<Option<(u64, T)>>>,
    rev: Arc<AtomicU64>,
    /// 发起代次 (单调递增)。
    generation: u64,
    seen_rev: u64,
}

impl<T: Send + 'static> Default for AsyncJob<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + 'static> AsyncJob<T> {
    pub fn new() -> Self {
        Self {
            done: Arc::new(Mutex::new(None)),
            rev: Arc::new(AtomicU64::new(0)),
            generation: 0,
            seen_rev: 0,
        }
    }

    /// 发起新一轮 (代次 +1)。
    pub fn launch<F>(&mut self, work: F)
    where
        F: FnOnce() -> T + Send + 'static,
    {
        self.generation += 1;
        let generation = self.generation;
        let done = Arc::clone(&self.done);
        let rev = Arc::clone(&self.rev);
        std::thread::spawn(move || {
            let out = work();
            {
                let mut slot = done.lock().unwrap();
                // 代次拒旧覆盖 (export review B-Critical): 完成顺序颠倒时, 旧轮
                // 晚到**不得**覆写新轮结果 —— 单槽被旧轮覆写后 poll 只 take 一次,
                // 新轮结果会永久丢失 (invalidate 后立刻再 launch 的交错可复现)。
                // 槽里已有更高代次时本结果直接丢弃。
                let stale = matches!(&*slot, Some((g, _)) if *g >= generation);
                if !stale {
                    *slot = Some((generation, out));
                }
            }
            rev.fetch_add(1, Ordering::Release);
        });
    }

    /// 拾取完成结果 (仅当属于最新一轮; 每帧调用, 无结果零成本)。
    pub fn poll(&mut self) -> Option<T> {
        let r = self.rev.load(Ordering::Acquire);
        if r == self.seen_rev {
            return None;
        }
        self.seen_rev = r;
        let (generation, out) = self.done.lock().unwrap().take()?;
        if generation != self.generation {
            return None; // 乱序完成的旧轮
        }
        Some(out)
    }

    /// 使在途作业失效 (代次 +1, 不发起新工作): 旧轮晚到的结果按乱序丢弃。
    /// 跨作业失效场景: async-open 换入新文件后, 旧文件上的在途 filter/search
    /// 结果不得贴到新文件 (review C1); AsyncJob 自身的代次只覆盖「同 job 连续
    /// launch」, 换文件这种外部失效须由持有方显式调用。
    pub fn invalidate(&mut self) {
        self.generation += 1;
    }
}

/// 命中导航: 升序命中表 (文件行号) + 环绕跳转。纯逻辑, 与渲染解耦供单测。
pub struct SearchNav {
    hits: Arc<Vec<u64>>,
    /// 总命中数 (含 cap 外未收集部分, 如实展示)。
    total: u64,
    /// 当前命中下标 (未定位 = None)。
    current: Option<usize>,
}

impl SearchNav {
    pub fn new(hits: Arc<Vec<u64>>, total: u64) -> Self {
        Self {
            hits,
            total,
            current: None,
        }
    }

    pub fn hits(&self) -> &Arc<Vec<u64>> {
        &self.hits
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    /// 当前命中行号。
    pub fn current_line(&self) -> Option<u64> {
        self.current.map(|i| self.hits[i])
    }

    /// 应用后首跳: 第一个 >= from 的命中; 全部小于 from 则环绕回首条。
    pub fn jump_first_from(&mut self, from: u64) -> Option<u64> {
        if self.hits.is_empty() {
            return None;
        }
        let idx = self.hits.partition_point(|&h| h < from);
        let idx = if idx >= self.hits.len() { 0 } else { idx };
        self.current = Some(idx);
        Some(self.hits[idx])
    }

    /// 下一命中 (环绕)。
    pub fn jump_next(&mut self) -> Option<u64> {
        if self.hits.is_empty() {
            return None;
        }
        let i = match self.current {
            None => 0,
            Some(i) => (i + 1) % self.hits.len(),
        };
        self.current = Some(i);
        Some(self.hits[i])
    }

    /// 上一命中 (环绕)。
    pub fn jump_prev(&mut self) -> Option<u64> {
        if self.hits.is_empty() {
            return None;
        }
        let i = match self.current {
            None => self.hits.len() - 1,
            Some(0) => self.hits.len() - 1,
            Some(i) => i - 1,
        };
        self.current = Some(i);
        Some(self.hits[i])
    }

    /// (第 k 条, 收集到 n 条) 1-based, 供状态栏; 未定位 = None。
    pub fn position(&self) -> Option<(usize, usize)> {
        self.current.map(|i| (i + 1, self.hits.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use danqing::encoding::bytes_as_literal_regex;

    #[test]
    fn nav_first_from_positions() {
        let hits = Arc::new(vec![10, 20, 30]);
        let mut nav = SearchNav::new(hits, 3);
        assert_eq!(nav.jump_first_from(0), Some(10), "从头");
        assert_eq!(nav.jump_first_from(10), Some(10), "恰在命中");
        assert_eq!(nav.jump_first_from(15), Some(20), "中间取后");
        assert_eq!(nav.jump_first_from(31), Some(10), "越过末尾环绕回首");
    }

    #[test]
    fn nav_next_prev_wrap() {
        let mut nav = SearchNav::new(Arc::new(vec![5, 9]), 2);
        assert_eq!(nav.jump_next(), Some(5), "未定位先跳首条");
        assert_eq!(nav.jump_next(), Some(9));
        assert_eq!(nav.jump_next(), Some(5), "末尾环绕");
        assert_eq!(nav.jump_prev(), Some(9), "反向环绕");
        assert_eq!(nav.position(), Some((2, 2)));
    }

    #[test]
    fn nav_empty_hits() {
        let mut nav = SearchNav::new(Arc::new(vec![]), 0);
        assert_eq!(nav.jump_first_from(0), None);
        assert_eq!(nav.jump_next(), None);
        assert_eq!(nav.jump_prev(), None);
        assert_eq!(nav.position(), None);
    }

    #[test]
    fn async_job_delivers_latest_generation() {
        let mut job: AsyncJob<u64> = AsyncJob::new();
        job.launch(|| 42);
        let mut got = None;
        for _ in 0..1000 {
            if let Some(v) = job.poll() {
                got = Some(v);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(got, Some(42), "1s 内必交付");
        assert_eq!(job.poll(), None, "结果只取一次");
    }

    #[test]
    fn async_job_invalidate_discards_inflight_result() {
        // review C1 机制钉: launch 后不 poll, invalidate, 结果完成后 poll 必须 None
        let mut job: AsyncJob<u64> = AsyncJob::new();
        job.launch(|| 7);
        job.invalidate();
        let mut got = None;
        for _ in 0..1000 {
            if let Some(v) = job.poll() {
                got = Some(v);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(got, None, "失效轮的结果必须被丢弃");
        // invalidate 不影响后续新一轮交付
        job.launch(|| 8);
        let mut got2 = None;
        for _ in 0..1000 {
            if let Some(v) = job.poll() {
                got2 = Some(v);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(got2, Some(8), "新一轮照常交付");
    }

    #[test]
    fn bytes_literal_regex_escapes() {
        assert_eq!(bytes_as_literal_regex(&[0xD6, 0xD0]), "(?-u)\\xD6\\xD0");
        let re = regex::bytes::Regex::new(&bytes_as_literal_regex(&[0xD6, 0xD0])).unwrap();
        assert!(
            re.is_match(b"a\xD6\xD0b"),
            "转义模式匹配原始字节 (Unicode 模式下 \\xD6 会展开成码点 UTF-8, 必挂)"
        );
        assert!(
            !re.is_match("\u{D6}\u{D0}".as_bytes()),
            "不匹配码点 UTF-8 展开"
        );
    }

    /// 代次拒旧覆盖 (export review B-Critical 回归锁): invalidate 后立刻再 launch,
    /// 完成顺序颠倒 (旧轮晚到) 时**新轮结果不得丢失**。
    /// 修前行为: 旧轮覆写单槽 → poll take 到旧代次丢弃 → 新结果永久蒸发。
    #[test]
    fn late_stale_result_does_not_overwrite_newer() {
        let mut job: AsyncJob<&'static str> = AsyncJob::new();
        job.launch(|| {
            std::thread::sleep(std::time::Duration::from_millis(200));
            "old"
        });
        job.invalidate(); // 旧轮作废
        job.launch(|| "new"); // 新轮先完成
        // 自旋等新轮结果 (上限 5s); 旧轮 200ms 后才到, 到达时必须被拒收
        let mut got = None;
        for _ in 0..500 {
            if let Some(v) = job.poll() {
                got = Some(v);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(got, Some("new"), "新轮结果必须交付, 不得被旧轮覆写吞掉");
        // 等旧轮真到达 (越过 200ms) 再确认它没有借尸还魂
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(job.poll(), None, "旧轮晚到结果不得再被交付");
    }
}
