//! @author 十四叔
//! @date 2026/09/08
//!
//! 异步打开管道 (async-open 模块, spec: docs/specs/SPEC-async-open.md):
//! OpenJob = AsyncJob 消费方 + 进度/取消通道。四条打开路径
//! (启动 / Ctrl+O·拖拽 / 轮转重建 / tail 巨量追平) 统一经此进 worker 线程,
//! UI 线程只做 launch 与每帧 poll。
//!
//! 取消 = drop 语义 (plan D1): 应用层把 `Option<OpenJob>` 置 None,
//! worker 持有的 cancel Arc 继续存活, 扫描循环按 8MB 粒度早退;
//! 晚到结果无人 poll, 随 done Arc 回收 —— 半成品索引永不进入应用层。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

use anyhow::Result;

use crate::jsonl::{self, Clause, Schema};
use crate::levels::{self, LevelCounts};
use crate::logfile::{AppendOutcome, IndexHooks, LogFile};
use crate::search::AsyncJob;

/// 打开作业类型 (tick pickup 分派用; 换入链语义见 plan D4)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenKind {
    /// 启动 / Ctrl+O / 拖拽: 全新文件, 换入走 reload 重置链。
    Fresh,
    /// 轮转/截断: 同路径全量重建, 换入走 rebuild 重置链 (书签 retain 等)。
    Rebuild,
    /// tail 巨量追平 (≥32MB 新字节): 增量 append, 换入走 append 链。
    Append,
}

/// 打开作业产物: 新文件 + JSONL 列定义 + 附加上下文。
pub struct OpenOutcome {
    /// 新打开的日志文件 (索引已建完)。
    pub file: LogFile,
    /// JSONL 列定义 (Fresh/Rebuild 检出才有; 增量追加恒 None —— schema 沿用
    /// 应用层现有值; 追加退化为重建时按 Rebuilt 臂重新发现, 见 `rebuilt`)。
    pub schema: Option<Schema>,
    /// worker 已算好的增量过滤命中 (review R2: 巨量追平 + 过滤激活时,
    /// 增量过滤随 worker 下沉, 落点只合并不扫描; 无过滤/非追加臂恒 None)。
    pub incremental_hits: Option<Vec<u64>>,
    /// 追加退化全量重建 (review R3: UTF-16/缩容兜底) —— 落点须按 Rebuild
    /// 重置链分派, 不能凭发起意图走 Append 链。
    pub rebuilt: bool,
    /// 级别计数 (level-histogram): 在 worker 内算好, 与 file 同批交付 ——
    /// UI 侧不存在「行数已更新、计数还是旧的」窗口。
    ///
    /// **四臂一律是绝对量** (与 `file` 对应的全文件计数), 落点只需覆盖。
    ///
    /// 之所以不做「增量 vs 全量」的按臂语义 (初版如此, review R1 后统一):
    /// 那种契约要在每个落点判断「这份是增量还是全量」, 判错一次就是计数翻倍
    /// 或旧数据全丢。改成一律绝对量之后, 误用空间消失; 增量省下的扫描由
    /// [`crate::levels::update_for_append`] 在 worker 内完成 (重叠一行)。
    pub level_counts: LevelCounts,
    /// 计数所用的级别类列名 (`None` = 走行口径)。落点据此重建点选子句,
    /// 也据此决定后续增量追加该走哪条口径 —— 口径必须全程一致, 否则
    /// 同一个侧栏里会混进两种数法。
    pub level_column: Option<String>,
}

/// 打开管道的阶段 (进度显示用)。
///
/// 状态栏的「索引 N ms」只是第 0 段的耗时, **不含**列发现与级别计数 ——
/// 后两段没有细粒度进度, 若不在文案里说出来, 界面就会卡在「99%」不动,
/// 用户看到的等待于是和那个数字对不上 (2026-09-12 用户反馈)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpenPhase {
    /// 建索引 (有细粒度字节进度)。
    Index = 0,
    /// JSONL 列发现 (serde 解析采样, 成本随行宽)。
    Schema = 1,
    /// 级别计数 (一趟并行扫描)。
    Levels = 2,
}

impl OpenPhase {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Schema,
            2 => Self::Levels,
            _ => Self::Index,
        }
    }

    /// 显示名; `Index` 返回 None —— 那时有百分比可显示, 不该抢掉它。
    fn name(self) -> Option<&'static str> {
        match self {
            Self::Index => None,
            Self::Schema => Some("列发现中"),
            Self::Levels => Some("级别计数中"),
        }
    }
}

/// 打开作业: worker 线程跑 open (+JSONL 检出), 应用层 tick poll 拾取。
///
/// 生命周期由 `Option<OpenJob>` 表达: Some 在途, None 无作业/已取消。
/// drop 即取消: cancel 旗标随 Arc 留在 worker, 扫描循环早退 (见 Drop)。
pub struct OpenJob {
    job: AsyncJob<Result<OpenOutcome>>,
    /// 已扫字节 (worker 经 IndexHooks 累加; 进度显示用)。
    progress: Arc<AtomicU64>,
    /// 进度分母 (Fresh/Rebuild = 文件字节, 发起时 stat 实测; 取不到 = 0,
    /// 显示侧按「读取中…」不确定进度处理)。
    total: u64,
    cancel: Arc<AtomicBool>,
    kind: OpenKind,
    path: PathBuf,
    /// 发起时刻 —— 用于「产物落地耗时」日志: 与 worker 内部的分阶段计时对照,
    /// 可把「worker 慢」与「落地后渲染慢」分开。
    launched_at: std::time::Instant,
    /// 当前阶段 (worker 写, UI 读) —— 进度显示用, 见 [`OpenPhase`]。
    phase: Arc<AtomicU8>,
}

impl OpenJob {
    /// 发起打开/重建: worker 内 open → JSONL detect → discover_schema 全链。
    pub fn launch(kind: OpenKind, path: &Path) -> Self {
        let total = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let JobParts {
            mut job,
            progress,
            cancel,
            hooks,
            phase,
        } = job_parts();
        let phase_w = Arc::clone(&phase);
        let path_buf = path.to_path_buf();
        job.launch(move || {
            run_catched(move || {
                // 分阶段计时: 状态栏的「索引 N ms」只是 `LogFile::open` 的耗时,
                // **不含**其后的列发现与级别计数 —— 用户看到的等待与那个数字
                // 不符时, 唯一能定位的就是这组日志。
                let t_phase = std::time::Instant::now();
                let file = LogFile::open_with_hooks(&path_buf, &hooks)?;
                let t_index = t_phase.elapsed();
                phase_w.store(OpenPhase::Schema as u8, Ordering::Relaxed);
                let t_phase = std::time::Instant::now();
                let schema = if jsonl::detect(&file) {
                    jsonl::discover_schema(&file)
                } else {
                    None
                };
                let t_schema = t_phase.elapsed();
                // 计数在索引趟之外单独一趟并行扫描 (D6); 随 file 一起交卷。
                // 口径按模式分 (spec D2): JSONL 且有级别类列 → 按字段值;
                // 否则按行首子串。两者各自与自己的可点行为对齐。
                let level_column = schema
                    .as_ref()
                    .and_then(levels::find_level_column)
                    .map(str::to_string);
                phase_w.store(OpenPhase::Levels as u8, Ordering::Relaxed);
                let t_phase = std::time::Instant::now();
                let level_counts = match &level_column {
                    Some(col) => levels::count_levels_field(&file, col),
                    None => levels::count_levels(&file),
                };
                log::info!(
                    "perf open_phases: 索引 {}ms · 列发现 {}ms · 级别计数 {}ms ({} 行, 口径={})",
                    t_index.as_millis(),
                    t_schema.as_millis(),
                    t_phase.elapsed().as_millis(),
                    file.line_count(),
                    level_column.as_deref().unwrap_or("行"),
                );
                Ok(OpenOutcome {
                    file,
                    schema,
                    incremental_hits: None,
                    rebuilt: false,
                    level_counts,
                    level_column,
                })
            })
        });
        Self {
            job,
            progress,
            total,
            cancel,
            kind,
            path: path.to_path_buf(),
            launched_at: std::time::Instant::now(),
            phase,
        }
    }

    /// 发起增量追平: 旧 Arc 进 worker 闭包 (视图同时持旧快照, 两读并行)。
    /// `filter` = (过滤子句, 旧行数) —— 过滤激活时增量过滤随 worker 下沉
    /// (review R2: GB 级追平的落点扫描曾是 UI 线程秒级冻结); None = 无过滤。
    /// schema 沿用应用层现有值 (同文件类型不变), 真增量臂产物恒 None;
    /// 退化重建臂 (UTF-16/缩容) 重新发现 schema 并置 `rebuilt` (review R3)。
    pub fn launch_append(
        path: &Path,
        old: Arc<LogFile>,
        new_bytes: u64,
        filter: Option<(Vec<Clause>, u64)>,
        level_column: Option<String>,
        old_counts: LevelCounts,
    ) -> Self {
        let JobParts {
            mut job,
            progress,
            cancel,
            hooks,
            phase,
        } = job_parts();
        let phase_w = Arc::clone(&phase);
        let path_buf = path.to_path_buf();
        job.launch(move || {
            run_catched(move || {
                let (file, rebuilt) =
                    match LogFile::append_from_with_hooks(&old, &path_buf, &hooks)? {
                        AppendOutcome::Appended(f) => (f, false),
                        AppendOutcome::Rebuilt(f) => (f, true),
                    };
                // 退化重建臂: 内容可能换格式 (review R4 同源), schema 重新发现
                let schema = if rebuilt && jsonl::detect(&file) {
                    jsonl::discover_schema(&file)
                } else {
                    None
                };
                // 增量过滤只在真增量臂有意义 (重建臂走 Rebuild 链, 过滤全清)。
                // 起点**退一行**: 旧快照末行可能以无换行结尾、并被本次追加补全改判
                // (review R1)。落点会先摘掉 `>= 该起点` 的旧命中, 故重跑重叠行不会
                // 重复计入 —— 计数与过滤必须同起点, 否则 D2 红线当场破裂。
                let incremental_hits = match (&filter, rebuilt) {
                    (Some((clauses, old_line_count)), false) => Some(jsonl::run_filter_from(
                        &file,
                        clauses,
                        old_line_count.saturating_sub(1),
                    )),
                    _ => None,
                };
                // 退化重建臂 → 按新格式重认列 + 全量; 真增量臂 → 重叠一行的**绝对量**
                // 更新 (列名由调用方传入: 该臂 schema 恒 None, worker 拿不到, 而口径
                // 必须与已显示的那份一致)。
                phase_w.store(OpenPhase::Levels as u8, Ordering::Relaxed);
                let (level_counts, level_column) = if rebuilt {
                    let col = schema
                        .as_ref()
                        .and_then(levels::find_level_column)
                        .map(str::to_string);
                    let c = match &col {
                        Some(name) => levels::count_levels_field(&file, name),
                        None => levels::count_levels(&file),
                    };
                    (c, col)
                } else {
                    let c =
                        levels::update_for_append(&old, &file, old_counts, level_column.as_deref());
                    (c, level_column)
                };
                Ok(OpenOutcome {
                    file,
                    schema,
                    incremental_hits,
                    rebuilt,
                    level_counts,
                    level_column,
                })
            })
        });
        Self {
            job,
            progress,
            total: new_bytes,
            cancel,
            kind: OpenKind::Append,
            path: path.to_path_buf(),
            launched_at: std::time::Instant::now(),
            phase,
        }
    }

    /// 拾取完成结果 (每帧调用, 无结果零成本; 代次防乱序由 AsyncJob 保证)。
    pub fn poll(&mut self) -> Option<Result<OpenOutcome>> {
        self.job.poll()
    }

    /// 进度: (已扫字节, 进度分母)。Append 的分母是新字节数。
    pub fn progress(&self) -> (u64, u64) {
        (self.progress.load(Ordering::Relaxed), self.total)
    }

    /// 作业类型 (pickup 分派)。
    pub fn kind(&self) -> OpenKind {
        self.kind
    }

    /// 目标路径 (换入时更新应用层 path / 状态文案)。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 当前阶段的显示名; `None` = 仍在建索引 (那时有百分比可显示)。
    pub fn phase_name(&self) -> Option<&'static str> {
        OpenPhase::from_u8(self.phase.load(Ordering::Relaxed)).name()
    }

    /// 自发起至今的耗时 (产物落地日志用)。
    pub fn elapsed_since_launch(&self) -> std::time::Duration {
        self.launched_at.elapsed()
    }
}

impl Drop for OpenJob {
    /// drop 即取消: 旗标置位 (worker 扫描循环早退), 结果无人 poll 自然回收。
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// launch 样板 (job + 进度/取消 Arc + 引擎钩子) 一次构造, 两个发起口共享。
struct JobParts {
    job: AsyncJob<Result<OpenOutcome>>,
    progress: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    hooks: IndexHooks,
    phase: Arc<AtomicU8>,
}

fn job_parts() -> JobParts {
    let progress = Arc::new(AtomicU64::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let phase = Arc::new(AtomicU8::new(OpenPhase::Index as u8));
    let hooks = IndexHooks {
        progress: Some(Arc::clone(&progress)),
        cancel: Some(Arc::clone(&cancel)),
    };
    JobParts {
        job: AsyncJob::new(),
        progress,
        cancel,
        hooks,
        phase,
    }
}

/// worker 闭包的 panic 护栏: detached 线程 panic 不得让应用层永久 Loading
/// (评审 FYI→Required 升档): 转为 Err 交付, 落点按各类失败语义处理
/// (Fresh notice / Rebuild·Append 静默待下轮 poll 重试)。
fn run_catched<F>(work: F) -> Result<OpenOutcome>
where
    F: FnOnce() -> Result<OpenOutcome> + Send + 'static,
{
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("打开 worker panic (详见 panic 日志)")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::levels::Level;
    use std::io::Write;
    use std::time::{Duration, Instant};

    /// 落一个临时文件 (与 logfile::tests 同款唯一命名: Windows 映射语义)。
    fn temp_file(content: &[u8]) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "danqing-log-open-{}-{}.log",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(content).unwrap();
        }
        path
    }

    /// 阻塞等 poll 出结果 (测试用; 上限 5s 防悬挂)。
    fn wait_outcome(job: &mut OpenJob) -> Result<OpenOutcome> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(res) = job.poll() {
                return res;
            }
            assert!(Instant::now() < deadline, "worker 5s 未交卷 (悬挂?)");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn worker_opens_real_file_and_polls_result() {
        let path = temp_file(b"a\nb\nc\n");
        let mut job = OpenJob::launch(OpenKind::Fresh, &path);
        let out = wait_outcome(&mut job).expect("打开成功");
        assert_eq!(out.file.line_count(), 3);
        assert!(out.schema.is_none(), "明文无 schema");
        assert_eq!(job.kind(), OpenKind::Fresh);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn worker_detects_jsonl_schema() {
        let path = temp_file(b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n");
        let mut job = OpenJob::launch(OpenKind::Fresh, &path);
        let out = wait_outcome(&mut job).expect("打开成功");
        let schema = out.schema.expect("JSONL 检出 schema");
        assert_eq!(schema.columns.len(), 1);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn progress_total_matches_file_bytes_at_launch() {
        let content = b"x\ny\n";
        let path = temp_file(content);
        let job = OpenJob::launch(OpenKind::Fresh, &path);
        let (done, total) = job.progress();
        assert_eq!(total, content.len() as u64, "分母 = 文件字节");
        assert!(done <= total, "分子不超过分母");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn drop_cancels_flag_and_stays_clean() {
        let path = temp_file(b"a\nb\n");
        let job = OpenJob::launch(OpenKind::Fresh, &path);
        let flag = Arc::clone(&job.cancel);
        drop(job); // 取消 = drop: 旗标置位, worker 随 Arc 收尾, 无 panic 无悬挂
        assert!(flag.load(Ordering::Relaxed), "drop 后取消旗标置位");
        std::thread::sleep(Duration::from_millis(50));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn replaced_job_drops_and_new_job_delivers() {
        // review R5 行为钉: 两连开, 前者 drop (取消), 后者正常交付;
        // 前者晚到结果无任何观察通道 (job 已 drop, done Arc 随回收)
        let p1 = temp_file(b"a\n");
        let p2 = temp_file(b"b\nc\n");
        let first = OpenJob::launch(OpenKind::Fresh, &p1);
        let flag = Arc::clone(&first.cancel);
        drop(first);
        assert!(flag.load(Ordering::Relaxed), "前者已取消");
        let mut second = OpenJob::launch(OpenKind::Fresh, &p2);
        let out = wait_outcome(&mut second).expect("后者交付");
        assert_eq!(out.file.line_count(), 2, "后者内容正确");
        std::fs::remove_file(&p1).ok();
        std::fs::remove_file(&p2).ok();
    }

    #[test]
    fn append_worker_sinks_incremental_filter() {
        // review R2: 过滤激活时增量过滤随 worker 下沉
        let path =
            temp_file(b"{\"level\":\"INFO\"}\n{\"level\":\"ERROR\"}\n{\"level\":\"INFO\"}\n");
        let old = Arc::new(LogFile::open(&path).unwrap());
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            f.write_all(b"{\"level\":\"ERROR\"}\n{\"level\":\"INFO\"}\n")
                .unwrap();
        }
        let clauses = jsonl::parse_query("level=ERROR");
        let old_counts = levels::count_levels(&old);
        let mut job = OpenJob::launch_append(&path, old, 50, Some((clauses, 3)), None, old_counts);
        let out = wait_outcome(&mut job).expect("追平成功");
        assert!(!out.rebuilt, "真增量臂");
        assert_eq!(out.file.line_count(), 5);
        assert_eq!(
            out.incremental_hits,
            Some(vec![3]),
            "新行中第 3 行命中 (0 基)"
        );
        assert!(out.schema.is_none(), "真增量臂不重新发现 schema");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn append_worker_marks_rebuilt_and_rediscovers_schema() {
        // review R3/R4: 缩容兜底 → rebuilt 分派 + schema 重新发现
        let path = temp_file(b"plain one\nplain two\nplain three\n");
        let old = Arc::new(LogFile::open(&path).unwrap());
        std::fs::rename(&path, path.with_extension("old")).expect("映射存活期改名合法 (T3)");
        // 同路径更小的新文件 (JSONL, 格式也换了) → 缩容兜底
        std::fs::write(&path, b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n").unwrap();
        let old_counts = levels::count_levels(&old);
        let mut job = OpenJob::launch_append(&path, old, 10, None, None, old_counts);
        let out = wait_outcome(&mut job).expect("追平成功");
        assert!(out.rebuilt, "缩容 → Rebuilt 分派");
        assert!(out.schema.is_some(), "重建臂重新发现 schema");
        assert_eq!(out.file.line_count(), 3);
        assert!(
            out.incremental_hits.is_none(),
            "重建臂不算增量过滤 (走全清)"
        );
        std::fs::remove_file(&path).ok();
        std::fs::remove_file(path.with_extension("old")).ok();
    }

    /// Fresh 臂: 产物带全文件级别计数, 且 6 桶之和 == 总行数。
    #[test]
    fn fresh_outcome_carries_full_level_counts() {
        let path = temp_file(
            b"2026-09-05 12:00:01 FATAL a\n\
              2026-09-05 12:00:02 ERROR b\n\
              2026-09-05 12:00:03 WARN c\n\
              2026-09-05 12:00:04 INFO d\n\
              2026-09-05 12:00:05 DEBUG e\n\
              2026-09-05 12:00:06 TRACE f\n\
              2026-09-05 12:00:07 plain\n",
        );
        let mut job = OpenJob::launch(OpenKind::Fresh, &path);
        let out = wait_outcome(&mut job).expect("Fresh 打开成功");
        assert_eq!(out.level_counts.get(Level::Fatal), 1);
        assert_eq!(out.level_counts.get(Level::Error), 1);
        assert_eq!(out.level_counts.get(Level::Warn), 1);
        assert_eq!(out.level_counts.get(Level::Info), 1);
        assert_eq!(
            out.level_counts.get(Level::DebugTrace),
            2,
            "DEBUG+TRACE 同桶"
        );
        assert_eq!(out.level_counts.get(Level::Other), 1);
        assert_eq!(
            out.level_counts.total(),
            out.file.line_count(),
            "6 桶之和 == 总行数"
        );
        std::fs::remove_file(&path).ok();
    }

    /// 追平臂: 产物是**绝对量** (与新文件对应的全文件计数), 落点覆盖即可。
    ///
    /// 契约从「按臂而定 (增量 / 全量)」收敛成「一律绝对量」是 review R1 的连带结果:
    /// 那种契约要在每个落点判断「这份是增量还是全量」, 判错一次就是计数翻倍或旧数据
    /// 全丢。这条钉住新契约, 并验证「重叠一行」不会把旧行算两遍。
    #[test]
    fn append_outcome_levels_are_absolute() {
        let path = temp_file(
            b"2026-09-05 12:00:01 ERROR first
",
        );
        let old = Arc::new(LogFile::open(&path).unwrap());
        assert_eq!(old.line_count(), 1, "旧快照 1 行");
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            f.write_all(
                b"2026-09-05 12:00:02 INFO two
2026-09-05 12:00:03 ERROR three
",
            )
            .unwrap();
        }
        let old_counts = levels::count_levels(&old);
        assert_eq!(old_counts.get(Level::Error), 1, "旧快照那条 ERROR");
        let mut job = OpenJob::launch_append(&path, old, 64, None, None, old_counts);
        let out = wait_outcome(&mut job).expect("追平成功");
        assert!(!out.rebuilt, "纯追加不走重建臂");
        assert_eq!(out.file.line_count(), 3);
        assert_eq!(
            out.level_counts,
            levels::count_levels(&out.file),
            "必须是绝对量, 且 == 对新文件全量重算"
        );
        assert_eq!(out.level_counts.total(), 3);
        assert_eq!(
            out.level_counts.get(Level::Error),
            2,
            "旧 1 + 新 1; 重叠一行不得把旧行算两遍"
        );
        assert_eq!(out.level_counts.get(Level::Info), 1);
        std::fs::remove_file(&path).ok();
    }

    /// **R1 回归 (worker 路径)**: 旧快照末行无换行, 追平把它补全并改判 ——
    /// 产物必须覆盖那一行 (纯增量会漏掉它)。
    #[test]
    fn append_outcome_covers_completed_half_line() {
        let path = temp_file(
            b"X
2026-09-05 12:00:01 ",
        );
        let old = Arc::new(LogFile::open(&path).unwrap());
        assert_eq!(old.line_count(), 2);
        let old_counts = levels::count_levels(&old);
        assert_eq!(old_counts.get(Level::Error), 0, "半行未成词 → 其他");
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            f.write_all(
                b"ERROR disk
",
            )
            .unwrap();
        }
        let mut job = OpenJob::launch_append(&path, old, 64, None, None, old_counts);
        let out = wait_outcome(&mut job).expect("追平成功");
        assert_eq!(out.file.line_count(), 2, "补全半行不增行数");
        assert_eq!(
            out.level_counts,
            levels::count_levels(&out.file),
            "绝对量 == 全量重算"
        );
        assert_eq!(
            out.level_counts.get(Level::Error),
            1,
            "被补全的半行必须被重算"
        );
        assert_eq!(out.level_counts.get(Level::Other), 1, "只剩开头那行 X");
        std::fs::remove_file(&path).ok();
    }
}
