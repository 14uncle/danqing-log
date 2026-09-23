//! @author 十四叔
//! @date 2026/09/23
//!
//! 导出核心 (SPEC-v1x-export): 行集快照 + 原始行 writer + 流式写出循环骨架。
//!
//! 不做什么: CSV/pretty 格式器 (T2/T3)、作业语义 (在途/取消删半成品, T4)、UI (T5)。
//! 写出循环的契约 (plan 关键事实 3): 全文顺序走 `lines()` 单遍 + 行集游标,
//! **禁止**全量 `line(i)` 随机访问 (logfile.rs 的 235ms→1072ms 教训);
//! 全集 raw 走 `bytes()` 整拷 —— 字节保真 (原行尾/无终行尾/混合行尾) 且最快。

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use danqing::encoding::decode_line;
use danqing_logfile::jsonl;
use danqing_logfile::logfile::LogFile;

use crate::search::AsyncJob;

/// 行集快照 (spec D1): 三来源 (过滤命中 / 搜索命中 / 全集) 收口成一个升序行号集。
///
/// 冻结语义: 构造时 clamp 到当时行数 —— live-tail 之后的增长不进本次导出
/// (交付物要可复述: 「这是 X 点 Y 分的结果」)。持有方 clone 的 `Arc<LogFile>`
/// 同时冻结了内容快照, 行集与内容口径一致。
pub struct ExportSet {
    /// 全集: 走 `bytes()` 整拷快速路径, `lines` 恒空 (不存 0..n 大向量)。
    full: bool,
    /// 升序去重的行号集 (full 时为空)。
    lines: Vec<u64>,
    /// 冻结时的总行数。
    line_count: u64,
}

impl ExportSet {
    /// 全集 (无过滤/无搜索, 或明确要整文件)。
    pub fn all(line_count: u64) -> Self {
        Self {
            full: true,
            lines: Vec::new(),
            line_count,
        }
    }

    /// 稀疏行集 (过滤/搜索命中)。排序去重 + clamp 到 `line_count` (越界丢弃)。
    pub fn from_lines(mut lines: Vec<u64>, line_count: u64) -> Self {
        lines.retain(|&i| i < line_count);
        lines.sort_unstable();
        lines.dedup();
        Self {
            full: false,
            lines,
            line_count,
        }
    }

    pub fn is_full(&self) -> bool {
        self.full
    }

    /// 待导出行数 (full = 冻结行数)。
    pub fn len(&self) -> u64 {
        if self.full {
            self.line_count
        } else {
            self.lines.len() as u64
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 稀集的行号切片 (full 时为空 —— 判定走 [`Self::is_full`])。
    pub fn lines(&self) -> &[u64] {
        &self.lines
    }

    /// 冻结时的总行数 (行集来源文件的口径)。
    pub fn line_count(&self) -> u64 {
        self.line_count
    }
}

/// D1 行集口径的唯一真身 (2026-09-23 评审 defer, 自 main 迁入):
/// 行集来源**按模式取一** —— JSONL 看过滤, 明文看搜索; 另一模式的活动项
/// **不改口径** (JSONL 搜索是高亮导航, 明文无过滤通路)。都无 = 全集。
pub fn line_set_of(
    line_count: u64,
    is_jsonl: bool,
    filtered: Option<&[u64]>,
    search_hits: Option<&[u64]>,
) -> ExportSet {
    match if is_jsonl { filtered } else { search_hits } {
        Some(hits) => ExportSet::from_lines(hits.to_vec(), line_count),
        None => ExportSet::all(line_count),
    }
}

/// 默认名 scope 中缀 (spec D8, 与 D1 口径同源): JSONL 看过滤, 明文看搜索。
pub fn scope_suffix(is_jsonl: bool, has_filtered: bool, has_search: bool) -> &'static str {
    match (is_jsonl, has_filtered, has_search) {
        (true, true, _) => "-filtered",
        (false, _, true) => "-searched",
        _ => "",
    }
}

/// 扩展名按格式分派 (spec D8 / 评审 R1): 原始行保源扩展名, 美化 `.json`, CSV `.csv`。
pub fn ext_for(pick: ExportPick, source_ext: Option<&str>) -> String {
    match pick {
        ExportPick::Raw => source_ext
            .map(str::to_string)
            .unwrap_or_else(|| "log".to_string()),
        ExportPick::Pretty => "json".to_string(),
        ExportPick::Csv => "csv".to_string(),
    }
}

/// 行尾策略 (plan 衍生设计): 文件级探测, 统一写出。
///
/// 为什么文件级不是逐行: `LogFile::line()` 剥行尾 (`\n`/`\r` 都不含), 逐行精确
/// 行尾需要引擎新 API; 全集整拷路径不受此限 (原字节)。**混合行尾文件**在稀疏
/// 路径按多数口径统一写出 —— 已知局限 (SPEC 已知局限节)。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineEnd {
    Lf,
    CrLf,
}

impl LineEnd {
    pub fn as_bytes(self) -> &'static [u8] {
        match self {
            LineEnd::Lf => b"\n",
            LineEnd::CrLf => b"\r\n",
        }
    }
}

/// 文件级行尾探测: 采样**前 8 个** `\n`, 前邻 `\r` 记 CRLF 票, 否则 LF 票;
/// 多数胜; 平票或无行尾 → [`LineEnd::Lf`] (保守默认, 与引擎逐行一致)。
pub fn detect_line_end(data: &[u8]) -> LineEnd {
    const SAMPLE: usize = 8;
    let mut lf = 0usize;
    let mut crlf = 0usize;
    let mut from = 0usize;
    for _ in 0..SAMPLE {
        let Some(p) = memchr::memchr(b'\n', &data[from..]) else {
            break;
        };
        let abs = from + p;
        if abs > 0 && data[abs - 1] == b'\r' {
            crlf += 1;
        } else {
            lf += 1;
        }
        from = abs + 1;
    }
    if crlf > lf {
        LineEnd::CrLf
    } else {
        LineEnd::Lf
    }
}

/// 写出结果: 取消时半成品文件的删除是 T4 作业语义, 此处只如实报停止点。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WriteOutcome {
    Done { lines: u64 },
    Cancelled { lines: u64 },
}

/// 写出收尾统一形 (raw/pretty/csv 三处 writer 共用同一判定)。
fn outcome_of(completed: bool, lines: u64) -> WriteOutcome {
    if completed {
        WriteOutcome::Done { lines }
    } else {
        WriteOutcome::Cancelled { lines }
    }
}

/// 原始行导出 (spec D3): 全集 = 存储字节整拷; 稀疏 = 行内容 + 文件级行尾。
///
/// - GBK/Latin-1: 数据是 mmap 原字节, 稀疏路径逐行保真 (行尾统一除外), 整拷逐字节相等。
/// - UTF-16: 打开时已转码 UTF-8 副本 (引擎口径), 导出的是**转码后**的行 —— 已知局限。
/// - `progress` 累计**已写行数**; `cancel` 每行协作检查。
pub fn write_raw<W: Write>(
    file: &LogFile,
    set: &ExportSet,
    out: &mut W,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> io::Result<WriteOutcome> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(WriteOutcome::Cancelled { lines: 0 });
    }
    if set.is_full() {
        return write_full(file, out, cancel, progress);
    }
    write_sparse(file, set, out, cancel, progress)
}

/// 全集整拷: `bytes()` 分块写出 (1 MiB/块, 块间查取消) —— 字节保真的构造保证。
fn write_full<W: Write>(
    file: &LogFile,
    out: &mut W,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> io::Result<WriteOutcome> {
    const CHUNK: usize = 1 << 20;
    let data = file.bytes();
    let total = data.len();
    let line_count = file.line_count();
    // 进度按字节比例折行数, 乘法走 u128 (review Optional: done×line_count 在
    // 40GiB+/大行数下 u64 溢出 —— debug panic 会连带卡死 in-flight 态)。
    let projected = |done: usize| -> u64 {
        ((done as u128 * line_count as u128) / total.max(1) as u128) as u64
    };
    let mut done = 0usize;
    while done < total {
        if cancel.load(Ordering::Relaxed) {
            let lines = projected(done);
            progress.store(lines, Ordering::Relaxed);
            return Ok(WriteOutcome::Cancelled { lines });
        }
        let end = (done + CHUNK).min(total);
        out.write_all(&data[done..end])?;
        done = end;
        let lines = projected(done);
        progress.store(lines, Ordering::Relaxed);
    }
    progress.store(line_count, Ordering::Relaxed);
    Ok(WriteOutcome::Done { lines: line_count })
}

/// 稀疏导出: `lines()` 单遍 + 行集游标 (升序单向走, 总成本 = 一次顺序扫描)。
fn write_sparse<W: Write>(
    file: &LogFile,
    set: &ExportSet,
    out: &mut W,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> io::Result<WriteOutcome> {
    let data = file.bytes();
    let end = detect_line_end(data).as_bytes();
    // 末行无行尾 (review B-R2): 源不以 `\n` 收尾时最后一行**不补**行尾 ——
    // D3「与源文件对应行逐字节相同」含行尾维度; 全集/稀疏对同一行集输出必须一致。
    let last_line = file.line_count().saturating_sub(1);
    let last_bare = !data.is_empty() && data.last() != Some(&b'\n');
    // BOM 对齐全集 (review B): 引擎 line(0) 剥 BOM (`logfile.rs:620`), 不补则稀疏
    // 与全集整拷首行不一致 —— 这里按源补回。
    if data.starts_with(UTF8_BOM) {
        out.write_all(UTF8_BOM)?;
    }
    let mut written = 0u64;
    let completed = visit_rows(file, set, cancel, progress, |i, line| {
        out.write_all(line)?;
        if !(last_bare && i == last_line) {
            out.write_all(end)?;
        }
        written += 1;
        Ok(())
    })?;
    Ok(outcome_of(completed, written))
}

/// 行遍历骨架 (csv/pretty 共用): `lines()` 单遍 + 行集游标, 每命中一行回调一次。
///
/// 取消语义: 命中行回调前查 [`AtomicBool`], 置位即停 —— 回调**不**再执行。
/// 返回值 = 是否走完全部行集 (false = 中途取消)。回调携带**文件行号** (稀疏
/// raw 的末行无行尾判定要用)。
fn visit_rows(
    file: &LogFile,
    set: &ExportSet,
    cancel: &AtomicBool,
    progress: &AtomicU64,
    mut f: impl FnMut(u64, &[u8]) -> io::Result<()>,
) -> io::Result<bool> {
    let want = set.lines();
    let mut cursor = 0usize;
    let mut visited = 0u64;
    let full = set.is_full();
    for (i, line) in file.lines() {
        if !full {
            if cursor >= want.len() {
                break;
            }
            if want[cursor] != i {
                continue;
            }
            cursor += 1;
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        f(i, line)?;
        visited += 1;
        progress.store(visited, Ordering::Relaxed);
    }
    Ok(true)
}

// ---------------- CSV (spec D5) ----------------

/// UTF-8 BOM (Excel 中文不乱码的唯一可靠手段; CSV 写出 + 稀疏 raw 对齐全集)。
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// RFC4180 字段转义: 含逗号/引号/换行 → 整体加引号, 内部 `"` → `""`。行尾 CRLF 由调用方拼。
fn csv_escape(field: &str, out: &mut Vec<u8>) {
    let need_quote =
        field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r');
    if !need_quote {
        out.extend_from_slice(field.as_bytes());
        return;
    }
    out.push(b'"');
    for b in field.as_bytes() {
        if *b == b'"' {
            out.extend_from_slice(b"\"\"");
        } else {
            out.push(*b);
        }
    }
    out.push(b'"');
}

/// Excel 公式注入中和 (review A-R3): CSV 的目标就是 Excel, 而日志内容不可信 ——
/// `=HYPERLINK(...)` 这类单元格经 RFC4180 引号后**仍会被 Excel 当公式执行**。
/// 首字符 `=`/`@`/Tab/CR 恒加 `'` 前缀; `+`/`-` 仅在**非纯数字**时加
/// (负数/正数列保数值语义, 不被文本化)。
fn neutralize_formula(field: &str) -> std::borrow::Cow<'_, str> {
    let risky = match field.as_bytes().first() {
        Some(b'=' | b'@' | b'\t' | b'\r') => true,
        Some(b'+' | b'-') => field.parse::<f64>().is_err(),
        _ => false,
    };
    if risky {
        std::borrow::Cow::Owned(format!("'{field}"))
    } else {
        std::borrow::Cow::Borrowed(field)
    }
}

/// 取本行的 parse 源字节 (review B-R3): UTF-8 存储 (含 UTF-16 转码副本) **直接
/// 按原字节** parse —— 先 `decode_line` 的 lossy 会把非法字节洗成 U+FFFD,
/// 可能把本该「失败行原样」的行洗成合法 JSON (交付物被静默改写)。
/// 仅非 UTF-8 存储 (GBK/Latin-1) 才解码后 parse。
fn parse_source<'a>(
    enc: danqing::encoding::Encoding,
    line: &'a [u8],
    decoded: &'a str,
) -> &'a [u8] {
    if matches!(enc, danqing::encoding::Encoding::Utf8) {
        line
    } else {
        decoded.as_bytes()
    }
}

/// CSV 导出 (spec D5): BOM + 表头 (schema 首见序) + 数据行, 行尾 CRLF。
///
/// 口径 (实现中修正 plan 原案「不逐行 parse」): 单元格值走 `parse_line` +
/// [`jsonl::cell_display`] —— 与表格单元格显示**同口径构造保证** (字符串反转义裸值,
/// 嵌套紧凑 JSON); 一次 parse 供全部 K 列, 不是 K 次提取。代价计入 D9 目标 (≤30s 量级),
/// T6 实测说话。非 UTF-8 源 (GBK) 先按检出编码解码再 parse —— 输出一律 UTF-8。
/// 解析失败行: 整行文本进**第一列** (不丢行, 与 pretty 的失败行原样同哲学) + 计数返回。
///
/// 返回 `(写出结果, 解析失败行数)`。
pub fn write_csv<W: Write>(
    file: &LogFile,
    set: &ExportSet,
    columns: &[String],
    out: &mut W,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> io::Result<(WriteOutcome, u64)> {
    out.write_all(UTF8_BOM)?;
    // 表头 (列名同样走转义 —— 列名含逗号并非不可能)
    let mut head = Vec::new();
    for (ci, col) in columns.iter().enumerate() {
        if ci > 0 {
            head.push(b',');
        }
        csv_escape(col, &mut head);
    }
    head.extend_from_slice(b"\r\n");
    out.write_all(&head)?;

    let mut bad = 0u64;
    let mut written = 0u64;
    let enc = file.encoding();
    let completed = visit_rows(file, set, cancel, progress, |_i, line| {
        let decoded = decode_line(enc, line);
        let mut row = Vec::new();
        match jsonl::parse_line(parse_source(enc, line, &decoded)) {
            Some(value) => {
                for (ci, col) in columns.iter().enumerate() {
                    if ci > 0 {
                        row.push(b',');
                    }
                    let cell = value
                        .get(col.as_str())
                        .map(jsonl::cell_display)
                        .unwrap_or_default();
                    csv_escape(&neutralize_formula(&cell), &mut row);
                }
            }
            None => {
                // 非 JSON 行: 整行文本进第一列 (文本输出对 UTF-8 坏字节只能 lossy),
                // 其余空 —— 不丢行。
                bad += 1;
                csv_escape(&decoded, &mut row);
                row.extend(std::iter::repeat_n(b',', columns.len().saturating_sub(1)));
            }
        }
        row.extend_from_slice(b"\r\n");
        out.write_all(&row)?;
        written += 1;
        Ok(())
    })?;
    Ok((outcome_of(completed, written), bad))
}

// ---------------- JSON 美化 (spec D4) ----------------

/// JSON 美化导出 (spec D4): 逐行 parse → `to_string_pretty` (indent 2), 记录之间空一行;
/// 解析失败行**原样字节**写出并计数 (不丢行、不猜、不中断)。输出 UTF-8 + `\n`。
///
/// 与 CSV 同径: 非 UTF-8 源先解码再 parse (转义语义交给 serde, 不手搓)。
/// 返回 `(写出结果, 解析失败行数)`。
pub fn write_pretty<W: Write>(
    file: &LogFile,
    set: &ExportSet,
    out: &mut W,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> io::Result<(WriteOutcome, u64)> {
    let mut bad = 0u64;
    let mut written = 0u64;
    let mut first = true;
    let enc = file.encoding();
    let completed = visit_rows(file, set, cancel, progress, |_i, line| {
        if !first {
            out.write_all(b"\n")?; // 记录间空一行 (空行 = 记录分隔符, 首记录前无;
            // 连续失败行**同样**以空行分隔 —— 口径: 记录分隔对成败行一视同仁)
        }
        first = false;
        let decoded = decode_line(enc, line);
        match jsonl::parse_line(parse_source(enc, line, &decoded)) {
            Some(value) => match serde_json::to_string_pretty(&value) {
                Ok(pretty) => {
                    out.write_all(pretty.as_bytes())?;
                    out.write_all(b"\n")?;
                }
                Err(_) => {
                    // 序列化失败视同失败行 (不 expect —— worker panic 会卡死 in-flight 态)
                    bad += 1;
                    out.write_all(line)?;
                    out.write_all(b"\n")?;
                }
            },
            None => {
                bad += 1;
                out.write_all(line)?; // 原样字节 (spec D4: 保持字节)
                out.write_all(b"\n")?;
            }
        }
        written += 1;
        Ok(())
    })?;
    Ok((outcome_of(completed, written), bad))
}

// ---------------- 默认文件名 (T5, spec D8) ----------------

/// 当前时刻戳 (系统时钟 → [`timestamp_stamp`])。调用方不碰 `SystemTime`。
pub fn now_stamp() -> String {
    timestamp_stamp(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    )
}

/// epoch 秒 → `yyyyMMdd-HHmmss` (**UTC**)。纯函数手写 civil-from-days
/// (Howard Hinnant 算法), 零新依赖 (Cargo 无 time/chrono, 不为此引)。
/// 时间戳的职责是**文件名唯一名**, 不承担叙事 —— 需要本地时间再议。
pub fn timestamp_stamp(epoch_secs: i64) -> String {
    let days = epoch_secs.div_euclid(86_400);
    let tod = epoch_secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

/// Hinnant days→civil: 返回 (年, 月, 日), 输入 = 1970-01-01 起的天数。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 默认导出文件名 (spec D8): `<stem><scope>-<stamp>.<ext>`。
/// `scope`: `""`(全集) / `"-filtered"` / `"-searched"` —— 带时间戳是交付习惯
/// (同一结果多次导出不互覆)。
pub fn default_export_name(stem: &str, scope: &str, stamp: &str, ext: &str) -> String {
    format!("{stem}{scope}-{stamp}.{ext}")
}

// ---------------- 导出作业 (T4: 语义层) ----------------

/// 格式选择 (菜单/消息载荷; 评审 Nit: 杀 idx 魔法数)。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ExportPick {
    Raw,
    Pretty,
    Csv,
}

/// 导出格式 (spec 范围三格式)。
pub enum ExportFormat {
    /// 原始行 (两模式可用, spec D3)。
    Raw,
    /// JSON 美化 (仅 JSONL, spec D4)。
    Pretty,
    /// CSV (仅 JSONL, spec D5; 列 = schema 首见序)。
    Csv { columns: Vec<String> },
}

impl ExportFormat {
    /// 按格式分派写出 (作业体 / logbench 共用同一分派 —— 不留第二份三臂 match)。
    /// 返回 `(写出结果, 解析失败行数)` (raw 恒 0)。
    pub fn write<W: Write>(
        &self,
        log: &LogFile,
        set: &ExportSet,
        out: &mut W,
        cancel: &AtomicBool,
        progress: &AtomicU64,
    ) -> io::Result<(WriteOutcome, u64)> {
        match self {
            ExportFormat::Raw => write_raw(log, set, out, cancel, progress).map(|o| (o, 0)),
            ExportFormat::Pretty => write_pretty(log, set, out, cancel, progress),
            ExportFormat::Csv { columns } => write_csv(log, set, columns, out, cancel, progress),
        }
    }
}

/// 作业收尾态。Cancelled / Failed 时**半成品已删** (spec D2: 内容不完整的
/// 「结果文件」比没有文件更坏), `path` 仍如实报告 (删失败时 UI 按路径提示)。
pub enum ExportEnd {
    Done { lines: u64, bad_lines: u64 },
    Cancelled { lines: u64 },
    Failed { error: String },
}

pub struct ExportResult {
    pub path: PathBuf,
    pub end: ExportEnd,
}

/// 导出作业 (spec D2): `AsyncJob` 范式 + 在途标志 + 进度 + 取消删半成品。
///
/// - **单作业**: 进行中 `launch` 拒绝 (返回 false), 不排队不并行。
/// - **取消**: 置协作 [`AtomicBool`], worker 停止并删半成品后以 Cancelled 收尾。
/// - **换文件** (`invalidate`): 在途作业取消 + 代次作废 —— 旧结果不贴到新文件
///   (AsyncJob 代次语义, 与 async-open 的 filter/search 同纪律)。
pub struct ExportJob {
    job: AsyncJob<ExportResult>,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
    total: u64,
    running: bool,
}

impl Default for ExportJob {
    fn default() -> Self {
        Self::new()
    }
}

impl ExportJob {
    pub fn new() -> Self {
        Self {
            job: AsyncJob::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(AtomicU64::new(0)),
            total: 0,
            running: false,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// 已写行数 (worker 协作更新, 每帧读无锁争用)。
    pub fn progress(&self) -> u64 {
        self.progress.load(Ordering::Relaxed)
    }

    /// 冻结行集的总行数 (进度分母)。
    pub fn total(&self) -> u64 {
        self.total
    }

    /// 发起导出。进行中拒绝 (false) —— 入口侧据此变「取消」而非再开 (spec D2)。
    pub fn launch(
        &mut self,
        file: Arc<LogFile>,
        set: ExportSet,
        format: ExportFormat,
        path: PathBuf,
    ) -> bool {
        if self.running {
            return false;
        }
        // 每轮**新建** Arc 对 (review B-Critical): 跨代次共享 cancel/progress 会让
        // 新 launch 的重置**撤销旧 worker 的取消** —— invalidate 后立刻再 launch
        // 时两代同写一对 Arc, 旧作业「以为已取消」却继续写盘。各代各持各的。
        self.cancel = Arc::new(AtomicBool::new(false));
        self.progress = Arc::new(AtomicU64::new(0));
        self.total = set.len();
        self.running = true;
        let cancel = Arc::clone(&self.cancel);
        let progress = Arc::clone(&self.progress);
        self.job
            .launch(move || run_export_job(&file, &set, &format, &path, &cancel, &progress));
        true
    }

    /// 请求取消 (协作式: worker 在下一行/块边界停止, 删半成品后收尾)。
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// 每帧拾取完成/取消结果 (仅最新代次; 无结果零成本)。
    pub fn poll(&mut self) -> Option<ExportResult> {
        let r = self.job.poll()?;
        self.running = false;
        Some(r)
    }

    /// 换文件失效: 在途取消 + 结果作废。
    pub fn invalidate(&mut self) {
        if self.running {
            self.cancel.store(true, Ordering::Relaxed);
        }
        self.job.invalidate();
        self.running = false;
    }
}

/// 半成品路径: 目标旁 `<name>.partial`。
fn partial_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_owned();
    os.push(".partial");
    PathBuf::from(os)
}

/// 删半成品; 删除失败降级为在错误尾注带上路径 (spec D2: 「删除失败降级提示路径」)。
fn remove_partial(partial: &Path, error: &mut String) {
    if let Err(del) = std::fs::remove_file(partial) {
        if del.kind() != io::ErrorKind::NotFound {
            error.push_str(&format!(
                "; 半成品删除失败 (残留在 {}): {del}",
                partial.display()
            ));
        }
    }
}

/// 写出到 `.partial` (建文件 → 流式写出 → flush → **先关句柄**)。
fn write_partial(
    log: &LogFile,
    set: &ExportSet,
    format: &ExportFormat,
    partial: &Path,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> io::Result<(WriteOutcome, u64)> {
    let fs_file = std::fs::File::create(partial)?;
    let mut w = io::BufWriter::new(fs_file);
    let r = format.write(log, set, &mut w, cancel, progress)?;
    w.flush()?;
    drop(w); // 关句柄再 rename/remove (Windows 语义友好; 评审 Nit)
    Ok(r)
}

/// 作业体 (worker 线程内同步执行): 写 `.partial` → 成功 rename 原子替换目标。
///
/// **目标保护** (review A-Critical): 直接 `File::create(目标)` 会**先截断目标** ——
/// 覆盖导出后取消/失败 = 吃掉用户原先的完整文件。故全程只动 `.partial`:
/// Done → rename 替换 (Windows MoveFileEx 语义: 目标存在即替换);
/// Cancelled / Failed / panic → 只删 `.partial`, 目标原样。
/// panic 收口 (review A-R5): worker 炸掉而 AsyncJob 不交结果 = in-flight 卡死,
/// 导出按钮变死键 —— catch_unwind 收成 Failed。
fn run_export_job(
    log: &LogFile,
    set: &ExportSet,
    format: &ExportFormat,
    path: &Path,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> ExportResult {
    let path_buf = path.to_path_buf();
    let partial = partial_path(path);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        write_partial(log, set, format, &partial, cancel, progress)
    }));
    let end = match outcome {
        Err(_) => {
            let mut error = "导出线程意外崩溃 (panic)".to_string();
            remove_partial(&partial, &mut error);
            ExportEnd::Failed { error }
        }
        Ok(Err(e)) => {
            let mut error = e.to_string();
            remove_partial(&partial, &mut error);
            ExportEnd::Failed { error }
        }
        Ok(Ok((WriteOutcome::Done { lines }, bad))) => match std::fs::rename(&partial, path) {
            Ok(()) => ExportEnd::Done {
                lines,
                bad_lines: bad,
            },
            Err(e) => {
                let mut error = format!("写出完成但替换目标失败: {e}");
                remove_partial(&partial, &mut error);
                ExportEnd::Failed { error }
            }
        },
        Ok(Ok((WriteOutcome::Cancelled { lines }, _))) => {
            let mut error = String::new();
            remove_partial(&partial, &mut error);
            ExportEnd::Cancelled { lines }
        }
    };
    ExportResult {
        path: path_buf,
        end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;

    /// 独立临时文件 (并行测试共享临时文件 flake 的教训: 逐测唯一路径)。
    fn fixture(name: &str, content: &[u8]) -> (PathBuf, LogFile) {
        let path =
            std::env::temp_dir().join(format!("dq-export-{}-{}.log", std::process::id(), name));
        std::fs::write(&path, content).unwrap();
        let file = LogFile::open(&path).unwrap();
        (path, file)
    }

    fn run(file: &LogFile, set: &ExportSet, cancel: &AtomicBool) -> (Vec<u8>, WriteOutcome, u64) {
        let mut out = Vec::new();
        let progress = AtomicU64::new(0);
        let outcome = write_raw(file, set, &mut out, cancel, &progress).unwrap();
        (out, outcome, progress.load(Ordering::Relaxed))
    }

    /// ② 全集整拷与源文件逐字节相等 —— 含 CRLF、无终行尾的边界 (重构损耗零容忍)。
    #[test]
    fn raw_full_copy_is_byte_identical_to_source() {
        let content = b"aa\r\nbb\r\ncc"; // 末行无行尾: 任何「行+行尾」拼接都会多出字节
        let (path, file) = fixture("full-identical", content);
        let set = ExportSet::all(file.line_count());
        let (out, outcome, progress) = run(&file, &set, &AtomicBool::new(false));
        assert_eq!(out, content, "全集导出必须与源文件逐字节相等");
        assert!(matches!(outcome, WriteOutcome::Done { .. }));
        assert_eq!(progress, file.line_count());
        let _ = std::fs::remove_file(path);
    }

    /// ① CRLF 源稀疏导出行尾保真 (文件级探测 → 统一 CRLF 写出)。
    #[test]
    fn raw_sparse_keeps_crlf_line_endings() {
        let content = b"aa\r\nbb\r\ncc\r\n";
        let (path, file) = fixture("sparse-crlf", content);
        let set = ExportSet::from_lines(vec![0, 2], file.line_count());
        let (out, outcome, _) = run(&file, &set, &AtomicBool::new(false));
        assert_eq!(out, b"aa\r\ncc\r\n", "稀疏导出必须保留 CRLF 行尾");
        assert_eq!(outcome, WriteOutcome::Done { lines: 2 });
        let _ = std::fs::remove_file(path);
    }

    /// ③ GBK 源不解码、逐行字节保真 (行内容 = 源切片)。
    #[test]
    fn raw_sparse_gbk_bytes_untouched() {
        // 「你好」GBK = C4 E3 BA C3; 混入 ASCII 保证行边界可读
        let content = b"\xC4\xE3\xBA\xC3 one\nascii two\n\xC4\xE3\xBA\xC3 three\n";
        let (path, file) = fixture("sparse-gbk", content);
        let set = ExportSet::from_lines(vec![0, 2], file.line_count());
        let (out, _, _) = run(&file, &set, &AtomicBool::new(false));
        assert_eq!(
            out, b"\xC4\xE3\xBA\xC3 one\n\xC4\xE3\xBA\xC3 three\n",
            "GBK 字节不得被解码/转码动过"
        );
        let _ = std::fs::remove_file(path);
    }

    /// ④ 稀疏行集: 空集 / 首行 / 末行 / 跳跃。
    #[test]
    fn raw_sparse_handles_jumps_first_last_and_empty() {
        let content = b"l0\nl1\nl2\nl3\nl4\nl5\n";
        let (path, file) = fixture("sparse-shapes", content);
        let n = file.line_count();
        assert_eq!(n, 6);

        let (out, _, _) = run(
            &file,
            &ExportSet::from_lines(vec![], n),
            &AtomicBool::new(false),
        );
        assert_eq!(out, b"", "空集导出为空");

        let (out, _, _) = run(
            &file,
            &ExportSet::from_lines(vec![0], n),
            &AtomicBool::new(false),
        );
        assert_eq!(out, b"l0\n");

        let (out, _, _) = run(
            &file,
            &ExportSet::from_lines(vec![5], n),
            &AtomicBool::new(false),
        );
        assert_eq!(out, b"l5\n");

        let (out, _, _) = run(
            &file,
            &ExportSet::from_lines(vec![0, 2, 4], n),
            &AtomicBool::new(false),
        );
        assert_eq!(out, b"l0\nl2\nl4\n");
        let _ = std::fs::remove_file(path);
    }

    /// ⑤ 取消协作: 起手即取消 → 零写出、Cancelled。
    #[test]
    fn write_loop_stops_on_cancel() {
        let content = b"x\ny\nz\n";
        let (path, file) = fixture("cancel", content);
        let set = ExportSet::all(file.line_count());
        let (out, outcome, progress) = run(&file, &set, &AtomicBool::new(true));
        assert_eq!(out, b"", "取消后不得有写出");
        assert_eq!(outcome, WriteOutcome::Cancelled { lines: 0 });
        assert_eq!(progress, 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn detect_line_end_samples_and_defaults_to_lf() {
        assert_eq!(detect_line_end(b"a\r\nb\r\nc\r\n"), LineEnd::CrLf);
        assert_eq!(detect_line_end(b"a\nb\nc\n"), LineEnd::Lf);
        assert_eq!(
            detect_line_end(b"no newline at all"),
            LineEnd::Lf,
            "无行尾回退 LF"
        );
        assert_eq!(detect_line_end(b""), LineEnd::Lf);
        // 混合: 前 8 个行尾里 CRLF 占多数 → CrLf
        assert_eq!(
            detect_line_end(b"a\r\nb\r\nc\r\nd\n"),
            LineEnd::CrLf,
            "多数票胜出"
        );
    }

    #[test]
    fn export_set_sorts_dedups_and_clamps() {
        let set = ExportSet::from_lines(vec![5, 1, 1, 99], 6);
        assert!(!set.is_full());
        assert_eq!(set.lines(), &[1, 5]);
        assert_eq!(set.len(), 2);

        let all = ExportSet::all(4);
        assert!(all.is_full());
        assert_eq!(all.len(), 4);
        assert_eq!(all.line_count(), 4);
        assert!(ExportSet::from_lines(vec![], 3).is_empty());
    }

    // ---------------- CSV (spec D5 判据) ----------------

    fn csv_run(file: &LogFile, set: &ExportSet, columns: &[&str]) -> (Vec<u8>, WriteOutcome, u64) {
        let cols: Vec<String> = columns.iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        let progress = AtomicU64::new(0);
        let (outcome, bad) = write_csv(
            file,
            set,
            &cols,
            &mut out,
            &AtomicBool::new(false),
            &progress,
        )
        .unwrap();
        (out, outcome, bad)
    }

    /// ①BOM 头字节 + ③schema 列序 = 首见序 (表头按传入列序逐字节)。
    #[test]
    fn csv_starts_with_bom_and_keeps_column_order() {
        let content = b"{\"b\":2,\"a\":1}\n";
        let (path, file) = fixture("csv-bom", content);
        let set = ExportSet::all(file.line_count());
        let (out, _, _) = csv_run(&file, &set, &["b", "a"]);
        assert_eq!(&out[..3], b"\xEF\xBB\xBF", "CSV 必须以 UTF-8 BOM 开头");
        assert_eq!(
            &out[3..],
            b"b,a\r\n2,1\r\n",
            "表头与数据行都按 schema 首见序"
        );
        let _ = std::fs::remove_file(path);
    }

    /// ②引号/逗号/换行/CJK/空串 各形态与手算期望逐字节对拍。
    #[test]
    fn csv_escapes_quotes_commas_newlines_and_keeps_cjk() {
        let content =
            b"{\"msg\":\"say \\\"hi\\\", now\",\"cn\":\"\xe4\xbd\xa0\xe5\xa5\xbd\",\"empty\":\"\",\"multi\":\"l1\\nl2\"}\n";
        let (path, file) = fixture("csv-escape", content);
        let set = ExportSet::all(file.line_count());
        let (out, _, _) = csv_run(&file, &set, &["msg", "cn", "empty", "multi"]);
        let body = &out[3..]; // 去 BOM
        // 手算期望:
        //   msg    = say "hi", now   → 含逗号+引号 → "say ""hi"", now"
        //   cn     = 你好            → 普通
        //   empty  = 空串            → 空
        //   multi  = l1\nl2 (真换行) → 含换行 → "l1\nl2"
        assert_eq!(
            body,
            b"msg,cn,empty,multi\r\n\"say \"\"hi\"\", now\",\xe4\xbd\xa0\xe5\xa5\xbd,,\"l1\nl2\"\r\n"
        );
        let _ = std::fs::remove_file(path);
    }

    /// ④缺字段 = 空串; schema 外字段忽略 (口径与表格显示一致)。
    #[test]
    fn csv_missing_field_is_empty_and_extra_field_ignored() {
        let content = b"{\"a\":1,\"hidden\":\"x\"}\n{\"a\":2}\n";
        let (path, file) = fixture("csv-shapes", content);
        let set = ExportSet::all(file.line_count());
        let (out, _, _) = csv_run(&file, &set, &["a", "missing"]);
        assert_eq!(&out[3..], b"a,missing\r\n1,\r\n2,\r\n");
        // hidden 列不在 schema: 不出现在任何位置
        assert!(!out.windows(6).any(|w| w == b"hidden"));
        let _ = std::fs::remove_file(path);
    }

    /// ②续: 嵌套紧凑 JSON 与字符串反转义 = 单元格显示同口径 (cell_display)。
    #[test]
    fn csv_cells_match_display_semantics() {
        let content = b"{\"obj\":{\"a\":1},\"msg\":\"say \\\"hi\\\" now\"}\n";
        let (path, file) = fixture("csv-display", content);
        let set = ExportSet::all(file.line_count());
        let (out, _, _) = csv_run(&file, &set, &["obj", "msg"]);
        assert_eq!(
            &out[3..],
            b"obj,msg\r\n\"{\"\"a\"\":1}\",\"say \"\"hi\"\" now\"\r\n",
            "嵌套 = 紧凑 JSON; 字符串 = 反转义裸值 (cell_display 口径); 含引号字段整体加引号"
        );
        let _ = std::fs::remove_file(path);
    }

    /// ⑤GBK 源解码进 UTF-8 输出 (serde 在 GBK 原字节上会挂 —— 先解码再 parse)。
    #[test]
    fn csv_gbk_source_values_decode_to_utf8() {
        // {"msg":"<你好 GBK>"} 单行
        let mut content = b"{\"msg\":\"\xC4\xE3\xBA\xC3\"}".to_vec();
        content.push(b'\n');
        let (path, file) = fixture("csv-gbk", &content);
        assert_eq!(
            file.encoding(),
            danqing::encoding::Encoding::Gbk,
            "夹具必须被检出为 GBK (探针: 检出错了测试就测不到解码路径)"
        );
        let set = ExportSet::all(file.line_count());
        let (out, _, bad) = csv_run(&file, &set, &["msg"]);
        assert_eq!(bad, 0);
        assert_eq!(
            &out[3..],
            "msg\r\n你好\r\n".as_bytes(),
            "GBK 值必须解码为 UTF-8 写出"
        );
        let _ = std::fs::remove_file(path);
    }

    /// 非 JSON 行不丢: 整行进第一列 + 计数。
    #[test]
    fn csv_keeps_unparseable_lines_with_count() {
        let content = b"not json\n{\"a\":1}\n";
        let (path, file) = fixture("csv-bad", content);
        let set = ExportSet::all(file.line_count());
        let (out, outcome, bad) = csv_run(&file, &set, &["a"]);
        assert_eq!(bad, 1);
        assert_eq!(outcome, WriteOutcome::Done { lines: 2 });
        assert_eq!(&out[3..], b"a\r\nnot json\r\n1\r\n");
        let _ = std::fs::remove_file(path);
    }

    // ---------------- JSON 美化 (spec D4 判据) ----------------

    fn pretty_run(file: &LogFile, set: &ExportSet) -> (Vec<u8>, WriteOutcome, u64) {
        let mut out = Vec::new();
        let progress = AtomicU64::new(0);
        let (outcome, bad) =
            write_pretty(file, set, &mut out, &AtomicBool::new(false), &progress).unwrap();
        (out, outcome, bad)
    }

    /// 与 serde_json 原生 pretty 全等对拍 + 记录间空一行 + UTF-8 LF。
    #[test]
    fn pretty_matches_serde_native_with_blank_line_separators() {
        let content = b"{\"a\":1,\"b\":[1,2]}\n{\"c\":\"x\\ny\"}\n";
        let (path, file) = fixture("pretty-native", content);
        let set = ExportSet::all(file.line_count());
        let (out, outcome, bad) = pretty_run(&file, &set);
        assert_eq!(bad, 0);
        assert_eq!(outcome, WriteOutcome::Done { lines: 2 });

        let v1: serde_json::Value = serde_json::from_str(r#"{"a":1,"b":[1,2]}"#).unwrap();
        let v2: serde_json::Value = serde_json::from_str(r#"{"c":"x\ny"}"#).unwrap();
        let mut expected = serde_json::to_string_pretty(&v1).unwrap().into_bytes();
        expected.push(b'\n');
        expected.push(b'\n'); // 记录间空一行
        expected.extend_from_slice(serde_json::to_string_pretty(&v2).unwrap().as_bytes());
        expected.push(b'\n');
        assert_eq!(out, expected, "pretty 必须与 serde_json 原生输出全等");
        assert!(!out.windows(2).any(|w| w == b"\r\n"), "输出行尾必须是 LF");
        let _ = std::fs::remove_file(path);
    }

    /// 解析失败行原样字节 + 计数 + 顺序保持。
    #[test]
    fn pretty_keeps_bad_lines_raw_with_count() {
        let content = b"not json\n{\"a\":1}\nalso \xFF bad\n";
        let (path, file) = fixture("pretty-bad", content);
        let set = ExportSet::all(file.line_count());
        let (out, _, bad) = pretty_run(&file, &set);
        assert_eq!(bad, 2);
        let v1: serde_json::Value = serde_json::from_str(r#"{"a":1}"#).unwrap();
        let mut expected = b"not json\n\n".to_vec();
        expected.extend_from_slice(serde_json::to_string_pretty(&v1).unwrap().as_bytes());
        expected.push(b'\n');
        expected.push(b'\n');
        expected.extend_from_slice(b"also \xFF bad\n");
        assert_eq!(out, expected, "失败行必须原样字节保真, 顺序不变");
        let _ = std::fs::remove_file(path);
    }

    /// 单记录: 无前导空行、以 `\n` 收尾; 中文经解码语义正确进 pretty。
    #[test]
    fn pretty_single_record_and_cjk() {
        let content = b"{\"msg\":\"\xe4\xbd\xa0\xe5\xa5\xbd\"}\n";
        let (path, file) = fixture("pretty-cjk", content);
        let set = ExportSet::all(file.line_count());
        let (out, _, _) = pretty_run(&file, &set);
        assert_eq!(
            String::from_utf8_lossy(&out),
            "{\n  \"msg\": \"你好\"\n}\n",
            "单记录无空行分隔, 中文正确"
        );
        let _ = std::fs::remove_file(path);
    }

    // ---------------- 默认文件名 (T5 判据) ----------------

    /// D1 折叠后的取源优先级 (评审 defer 迁入): 模式决定看哪份活动项, 另一份不改口径。
    #[test]
    fn line_set_of_picks_source_by_mode() {
        // JSONL: 过滤是唯一来源, 搜索命中 (即便有) 不改口径
        let s = line_set_of(10, true, Some(&[2, 1]), Some(&[9]));
        assert_eq!(s.lines(), &[1, 2]);
        // 明文: 搜索是唯一来源
        let s = line_set_of(10, false, Some(&[0]), Some(&[3]));
        assert_eq!(s.lines(), &[3]);
    }

    /// D8 命名矩阵: scope 随模式/活动项, 扩展名随格式 (评审 R1)。
    #[test]
    fn scope_and_ext_follow_d1_d8_matrix() {
        assert_eq!(scope_suffix(true, true, true), "-filtered");
        assert_eq!(scope_suffix(true, false, true), "", "JSONL 搜索不给 scope");
        assert_eq!(scope_suffix(false, true, true), "-searched");
        assert_eq!(scope_suffix(false, false, false), "");
        assert_eq!(ext_for(ExportPick::Raw, Some("jsonl")), "jsonl");
        assert_eq!(ext_for(ExportPick::Raw, None), "log");
        assert_eq!(ext_for(ExportPick::Pretty, Some("jsonl")), "json");
        assert_eq!(ext_for(ExportPick::Csv, Some("jsonl")), "csv");
    }

    #[test]
    fn timestamp_stamp_handles_epoch_leap_and_day_boundaries() {
        assert_eq!(timestamp_stamp(0), "19700101-000000");
        assert_eq!(timestamp_stamp(86_399), "19700101-235959");
        assert_eq!(timestamp_stamp(1_709_164_800), "20240229-000000", "闰日");
        assert_eq!(
            timestamp_stamp(1_709_251_200),
            "20240301-000000",
            "闰日后一天"
        );
    }

    #[test]
    fn default_export_name_carries_scope_and_ext() {
        assert_eq!(
            default_export_name("app", "-filtered", "20260923-101112", "csv"),
            "app-filtered-20260923-101112.csv"
        );
        assert_eq!(
            default_export_name("app", "", "20260923-101112", "log"),
            "app-20260923-101112.log"
        );
        assert_eq!(
            default_export_name("app", "-searched", "20260923-101112", "json"),
            "app-searched-20260923-101112.json"
        );
    }

    // ---------------- 导出作业 (T4 语义判据) ----------------

    fn out_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dq-export-out-{}-{}.txt", std::process::id(), name))
    }

    /// 自旋拾取 (worker 是真线程; 上限 5s, 超时即测试失败)。
    fn wait_poll(job: &mut ExportJob) -> ExportResult {
        for _ in 0..500 {
            if let Some(r) = job.poll() {
                return r;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("导出作业 5s 未收尾");
    }

    #[test]
    fn export_job_completes_and_reports() {
        let content = b"aa\nbb\ncc\n";
        let (src, file) = fixture("job-done", content);
        let file = Arc::new(file);
        let dest = out_path("job-done");
        let mut job = ExportJob::new();
        assert!(job.launch(
            Arc::clone(&file),
            ExportSet::all(file.line_count()),
            ExportFormat::Raw,
            dest.clone(),
        ));
        let r = wait_poll(&mut job);
        assert!(matches!(
            r.end,
            ExportEnd::Done {
                lines: 3,
                bad_lines: 0
            }
        ));
        assert_eq!(std::fs::read(&dest).unwrap(), content);
        assert!(!job.is_running());
        let _ = std::fs::remove_file(src);
        let _ = std::fs::remove_file(dest);
    }

    /// spec D2 取消 = 删半成品 (内容不完整的「结果文件」比没有文件更坏)。
    /// 直接调作业体 + 起手即取消: **确定性**命中删除路径 (不赌 worker 竞态)。
    #[test]
    fn cancel_deletes_partial_file() {
        let content = b"x\ny\nz\n";
        let (src, file) = fixture("job-cancel", content);
        let dest = out_path("job-cancel");
        let cancel = AtomicBool::new(true);
        let progress = AtomicU64::new(0);
        let r = run_export_job(
            &file,
            &ExportSet::all(file.line_count()),
            &ExportFormat::Raw,
            &dest,
            &cancel,
            &progress,
        );
        assert!(matches!(r.end, ExportEnd::Cancelled { .. }));
        assert!(!dest.exists(), "半成品必须删除");
        assert!(!partial_path(&dest).exists(), ".partial 也必须清干净");
        let _ = std::fs::remove_file(src);
    }

    /// review A-Critical 回归锁: 目标已有完整文件时, 取消导出**不得动它**。
    /// 修前: `File::create(目标)` 先截断, 取消时 `remove_file` 把用户的旧文件
    /// 整个吃掉 (覆盖导出上周的 results.csv 后取消 → 旧文件消失)。
    #[test]
    fn cancel_keeps_existing_target_intact() {
        let (src, file) = fixture("job-cancel-keep", b"x\ny\n");
        let dest = out_path("job-cancel-keep");
        std::fs::write(&dest, b"last week result").unwrap();
        let cancel = AtomicBool::new(true);
        let progress = AtomicU64::new(0);
        let r = run_export_job(
            &file,
            &ExportSet::all(file.line_count()),
            &ExportFormat::Raw,
            &dest,
            &cancel,
            &progress,
        );
        assert!(matches!(r.end, ExportEnd::Cancelled { .. }));
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            b"last week result",
            "取消导出不得碰已存在的目标文件"
        );
        assert!(!partial_path(&dest).exists(), ".partial 必须清干净");
        let _ = std::fs::remove_file(src);
        let _ = std::fs::remove_file(dest);
    }

    /// 中途取消的不变式: **Cancelled ⟹ 半成品已删** (Done = 取消没赶上, 成品完整自洽)。
    /// 大文件压出真中途窗口; 不锁结果类型, 只锁不变式 (防 flake)。
    #[test]
    fn job_cancel_mid_run_upholds_no_partial_invariant() {
        let mut content = Vec::new();
        for i in 0..100_000u64 {
            content.extend_from_slice(format!("line {i}\n").as_bytes());
        }
        let (src, file) = fixture("job-cancel-mid", &content);
        let file = Arc::new(file);
        let dest = out_path("job-cancel-mid");
        let mut job = ExportJob::new();
        assert!(job.launch(
            Arc::clone(&file),
            ExportSet::all(file.line_count()),
            ExportFormat::Raw,
            dest.clone(),
        ));
        // 等到真中途 (0 < progress < total) 再取消
        for _ in 0..500 {
            let p = job.progress();
            if p > 0 && p < job.total() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        job.cancel();
        let r = wait_poll(&mut job);
        match r.end {
            ExportEnd::Cancelled { .. } => {
                assert!(!dest.exists(), "取消收尾必须删半成品");
            }
            ExportEnd::Done { .. } => {
                assert!(dest.exists(), "完成收尾是成品, 不删");
            }
            ExportEnd::Failed { error } => panic!("不应失败: {error}"),
        }
        let _ = std::fs::remove_file(src);
        let _ = std::fs::remove_file(&dest);
    }

    /// spec D2 单作业: 进行中再 launch 拒绝 (入口变「取消」的依据)。
    #[test]
    fn in_flight_launch_is_rejected() {
        let content = b"1\n2\n";
        let (src, file) = fixture("job-single", content);
        let file = Arc::new(file);
        let mut job = ExportJob::new();
        let dest1 = out_path("job-single-1");
        let dest2 = out_path("job-single-2");
        assert!(job.launch(
            Arc::clone(&file),
            ExportSet::all(file.line_count()),
            ExportFormat::Raw,
            dest1.clone(),
        ));
        // 第一次 worker 即便已飞快完成, 未 poll 前 running 仍为 true —— 拒绝是构造保证
        assert!(
            !job.launch(
                Arc::clone(&file),
                ExportSet::all(file.line_count()),
                ExportFormat::Raw,
                dest2.clone(),
            ),
            "进行中不产生第二作业"
        );
        let _ = wait_poll(&mut job);
        let _ = std::fs::remove_file(src);
        let _ = std::fs::remove_file(dest1);
    }

    /// 换文件 (async-open) 时在途结果作废 —— 旧结果不贴新文件 (AsyncJob 代次纪律)。
    #[test]
    fn invalidate_drops_stale_result() {
        let content = b"a\nb\n";
        let (src, file) = fixture("job-stale", content);
        let file = Arc::new(file);
        let dest = out_path("job-stale");
        let mut job = ExportJob::new();
        assert!(job.launch(
            Arc::clone(&file),
            ExportSet::all(file.line_count()),
            ExportFormat::Raw,
            dest.clone(),
        ));
        // 等 worker 飞完 (progress 到顶), 但**不 poll** —— 模拟「结果已在途、用户已换文件」
        for _ in 0..500 {
            if job.progress() == job.total() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        job.invalidate();
        assert!(job.poll().is_none(), "旧代次结果必须作废");
        assert!(!job.is_running());
        let _ = std::fs::remove_file(src);
        let _ = std::fs::remove_file(dest);
    }

    /// 建文件失败: 无半成品可删, 如实报 Failed。
    #[test]
    fn failed_export_reports_error_and_leaves_no_file() {
        let content = b"a\n";
        let (src, file) = fixture("job-fail", content);
        let file = Arc::new(file);
        // 输出路径指向已存在目录 → rename(partial → 目录) 必败 (半成品旁路设计下
        // File::create 只动 `.partial`, 失败点在替换目标那一步)
        let dir = std::env::temp_dir().join(format!("dq-export-dir-fail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut job = ExportJob::new();
        assert!(job.launch(
            Arc::clone(&file),
            ExportSet::all(file.line_count()),
            ExportFormat::Raw,
            dir.clone(),
        ));
        let r = wait_poll(&mut job);
        assert!(matches!(r.end, ExportEnd::Failed { .. }));
        assert!(dir.is_dir(), "只删导出文件, 不碰目录");
        assert!(!partial_path(&dir).exists(), "失败收尾必须清掉 .partial");
        let _ = std::fs::remove_file(src);
        let _ = std::fs::remove_dir(dir);
    }

    /// review B-Critical 回归锁: invalidate 后**立刻**再 launch, 旧 worker 晚到
    /// 不得吞掉新作业 —— 修前两条缺陷叠加: ①跨代次共享 cancel Arc (新 launch 的
    /// 重置撤销旧 worker 的取消) ②AsyncJob 单槽被旧轮覆写 (新结果永久丢失 →
    /// running 卡死)。锁: 最终 poll 必须交付**新作业** (path B) 且 running 复位。
    #[test]
    fn invalidate_then_relaunch_keeps_new_job_alive() {
        let mut content = Vec::new();
        for i in 0..200_000u64 {
            content.extend_from_slice(format!("line {i}\n").as_bytes());
        }
        let (src, file) = fixture("job-relaunch", &content);
        let file = Arc::new(file);
        let dest_a = out_path("job-relaunch-a");
        let dest_b = out_path("job-relaunch-b");
        let mut job = ExportJob::new();
        assert!(job.launch(
            Arc::clone(&file),
            ExportSet::all(file.line_count()),
            ExportFormat::Raw,
            dest_a.clone(),
        ));
        job.invalidate(); // 模拟 apply_fresh 换文件
        let (src2, file2) = fixture("job-relaunch-small", b"tiny\n");
        let file2 = Arc::new(file2);
        assert!(job.launch(
            Arc::clone(&file2),
            ExportSet::all(file2.line_count()),
            ExportFormat::Raw,
            dest_b.clone(),
        ));
        // 等旧 worker 真越过它的收尾 (大文件 + 取消协作), 新作业结果必须是唯一交付
        let mut result = None;
        for _ in 0..500 {
            if let Some(r) = job.poll() {
                result = Some(r);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let r = result.expect("新作业结果必须交付 (修前: 被旧轮覆写永久吞掉)");
        assert_eq!(r.path, dest_b, "交付的必须是新作业");
        assert!(matches!(r.end, ExportEnd::Done { lines: 1, .. }));
        assert!(!job.is_running(), "running 必须复位, 否则导出按钮变死键");
        let _ = std::fs::remove_file(src);
        let _ = std::fs::remove_file(src2);
        let _ = std::fs::remove_file(dest_b);
    }

    // ---------------- 评审回归锁 (2026-09-23 双路评审) ----------------

    /// review B-R2: 源末行**无行尾**时, 稀疏导出不得无中生有补行尾 ——
    /// D3「对应行逐字节相同」含行尾维度 (全集/稀疏对同一行集必须一致)。
    #[test]
    fn sparse_keeps_bare_last_line_without_added_ending() {
        let content = b"aa\r\nbb\r\ncc"; // 末行无行尾
        let (path, file) = fixture("sparse-bare-last", content);
        let n = file.line_count();
        let (out, _, _) = run(
            &file,
            &ExportSet::from_lines(vec![2], n),
            &AtomicBool::new(false),
        );
        assert_eq!(out, b"cc", "末行无行尾: 原样, 不补 CRLF");
        let (out, _, _) = run(
            &file,
            &ExportSet::from_lines(vec![0, 2], n),
            &AtomicBool::new(false),
        );
        assert_eq!(out, b"aa\r\ncc", "非末行照常带行尾");
        let _ = std::fs::remove_file(path);
    }

    /// review B: 源带 UTF-8 BOM 时稀疏导出补回 BOM —— 引擎 line(0) 剥 BOM,
    /// 不补则与全集整拷首行不一致。
    #[test]
    fn sparse_preserves_utf8_bom_like_full_copy() {
        let content = b"\xEF\xBB\xBFaa\nbb\n";
        let (path, file) = fixture("sparse-bom", content);
        let n = file.line_count();
        let (out, _, _) = run(
            &file,
            &ExportSet::from_lines(vec![0], n),
            &AtomicBool::new(false),
        );
        assert_eq!(out, b"\xEF\xBB\xBFaa\n", "BOM 必须补回");
        let _ = std::fs::remove_file(path);
    }

    /// review A-R3: Excel 公式注入中和 —— `=`/`@` 恒加 `'` 前缀; `+`/`-` 非纯数字
    /// 才加 (负数列保数值)。
    #[test]
    fn csv_neutralizes_excel_formulas_but_keeps_numbers() {
        let content =
            b"{\"m\":\"=HYPERLINK(1)\",\"n\":-5,\"p\":\"+3\",\"q\":\"-1+1\",\"r\":\"@x\"}\n";
        let (path, file) = fixture("csv-formula", content);
        let set = ExportSet::all(file.line_count());
        let (out, _, _) = csv_run(&file, &set, &["m", "n", "p", "q", "r"]);
        assert_eq!(
            &out[3..],
            "m,n,p,q,r\r\n'=HYPERLINK(1),-5,+3,'-1+1,'@x\r\n".as_bytes(),
            "公式中和: =@恒加引号撇, +-仅非数字加, 数值原样"
        );
        let _ = std::fs::remove_file(path);
    }

    /// review B-R3: UTF-8 源的非法字节行**不得**被 lossy 解码洗成合法 JSON。
    /// 夹具口径: 坏字节必须藏在编码检测采样窗 (`danqing-encoding::SAMPLE` =
    /// 头 64 KiB) **之后**, 文件才会被检出为 UTF-8 —— 坏字节在窗内会被检出成
    /// GBK, 测不到这条路径 (前两版夹具都踩了这个)。
    #[test]
    fn invalid_utf8_line_stays_raw_bad_not_washed() {
        let mut content = Vec::new();
        // 先铺 >64 KiB 的合法 UTF-8, 把坏字节挤出采样窗
        while content.len() <= 64 * 1024 {
            content.extend_from_slice(b"{\"a\":1,\"pad\":\"012345678901234567890123456789\"}\n");
        }
        content.extend_from_slice(b"{\"m\":\"\xFF\"}\n"); // 采样窗外的非法字节
        let (path, file) = fixture("bad-utf8", &content);
        assert!(
            matches!(file.encoding(), danqing::encoding::Encoding::Utf8),
            "探针: 夹具必须检出为 UTF-8, 否则测的不是这条路径"
        );
        let set = ExportSet::all(file.line_count());
        let (out, _, bad) = pretty_run(&file, &set);
        assert_eq!(bad, 1, "非法字节行必须计为失败行");
        assert!(
            out.ends_with(b"\n\n{\"m\":\"\xFF\"}\n"),
            "末条 = 非法行原样字节 (修前被 U+FFFD 洗成合法 JSON 且 bad 不计)"
        );
        assert!(out.contains(&0xFF), "pretty 输出必须保留原始坏字节");
        let (_, _, bad) = csv_run(&file, &set, &["a", "m"]);
        assert_eq!(bad, 1, "CSV 同口径: 非法行计数");
        let _ = std::fs::remove_file(path);
    }

    /// parse_source 口径探针 (review B-R3): UTF-8 源按**原字节** parse,
    /// 非 UTF-8 源才走解码副本 —— lossy 会改字节是探针前提。
    #[test]
    fn parse_source_uses_raw_bytes_for_utf8_storage() {
        let line = b"{\"m\":\"\xFF\"}";
        let decoded = decode_line(danqing::encoding::Encoding::Utf8, line);
        assert_ne!(decoded.as_bytes(), line, "探针: lossy 必然改字节");
        assert_eq!(
            parse_source(danqing::encoding::Encoding::Utf8, line, &decoded),
            line,
            "UTF-8 存储必须按原字节 parse"
        );
        assert_eq!(
            parse_source(danqing::encoding::Encoding::Gbk, line, &decoded),
            decoded.as_bytes(),
            "非 UTF-8 存储才用解码副本"
        );
    }

    /// 冻结快照语义 (spec D1): 作业持有的 `Arc<LogFile>` 是启动时刻的映射 ——
    /// live-tail 的增长形态是 **append** (映射定长, 新字节在映射外), 导出仍是
    /// 冻结那一刻的内容。注: 同路径**覆写**会透过共享映射窜进快照 (Windows
    /// mmap 语义), 不属产品增长形态 —— 已知边界, 记入评审记。
    #[test]
    fn export_writes_frozen_snapshot_not_appended_growth() {
        let content = b"one\ntwo\n";
        let (path, file) = fixture("frozen", content);
        // append (live-tail 真实增长形态)
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        std::io::Write::write_all(&mut f, b"three\n").unwrap();
        drop(f);
        let set = ExportSet::all(file.line_count());
        let (out, _, _) = run(&file, &set, &AtomicBool::new(false));
        assert_eq!(out, content, "导出必须是冻结快照, 不含其后 append 的增长");
        let _ = std::fs::remove_file(path);
    }
}
