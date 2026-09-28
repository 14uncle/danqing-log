//! @author 十四叔
//! @date 2026/09/27
//!
//! 合并工作区视图模型 (merge-timeline 腿一 T3) —— `Workspace::Merge` 的状态 bundle。
//!
//! SPEC-v1x-merge-timeline D4: 单窗口双模式, Merge 与 Single 的字段并列互不动。
//! 归并走 AsyncJob (open/levels 同款后台作业先例): worker 开源 → 探测 → schema →
//! 提取 → 归并, UI 线程只收卷。
//!
//! **视图共享纪律**: 索引不共享引用 —— sync 每帧只拷**可见窗口**的行
//! ([`MergeRowView`], 窗口上限 [`WINDOW_ROWS`]), 滚动到任意位置 = 下一帧 sync 重切
//! 窗口。全量共享 Arc 在 live-tail 增量写下两边都别扭 (259MiB 索引换 Arc 克隆
//! 不可行; 原地写又被视图持有的快照卡死) —— 窗口拷贝是唯一两边都诚实的形态。

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use danqing_logfile::jsonl;
use danqing_logfile::logfile::LogFile;
use danqing_logfile::merge::{self, MergeIndex, MergeRow, SourceTimeline};
use danqing_logfile::timestamp::{self, TsRoute};

use crate::expand::ExpandMap;

/// 源上限 (SPEC D7): 第 9 个明示拒绝。
pub const MAX_SOURCES: usize = 8;
/// sync 窗口行数上限: ROW_HEIGHT=20px, 4K 窗 ≈110 可见行, 200 覆盖一切 sane 窗口;
/// paint 越出拷贝区间的行**诚实留空** (不猜内容)。
pub const WINDOW_ROWS: u64 = 200;

/// 一个合并源 (产品侧全拥有形态; 引擎 `SourceTimeline` 只是构建期的借用视图)。
pub struct MergeSource {
    pub path: PathBuf,
    pub file: Arc<LogFile>,
    /// JSONL 源有 schema (并集列的原料, T4)。
    pub schema: Option<Arc<jsonl::Schema>>,
    /// 时间戳通路 (探测结论, 提取与它同源)。
    pub route: TsRoute,
    /// 时钟偏移 ms (腿 D; 提取时已施加 —— 改它 = 该源重提 + 重归并, T5)。
    pub offset_ms: i64,
    /// 无 tz 格式的源时区偏移 ms (腿 D, T5 弹层编辑; 默认 0 = UTC)。
    pub tz_offset_ms: i64,
    /// 按源隐藏 (重建走掩码, 源序号不漂移)。
    pub hidden: bool,
}

impl MergeSource {
    /// 源显示名 (文件名; 弹层/行首源标签用)。
    pub fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
    }
}

/// (src, line) 打包成 u64 键 (书签集/展开态复用既有 u64 容器)。
pub fn pack_key(src: u32, line: u32) -> u64 {
    (u64::from(src) << 32) | u64::from(line)
}

/// pack_key 的逆运算。
pub fn unpack_key(k: u64) -> (u32, u32) {
    ((k >> 32) as u32, k as u32)
}

// ─── T4: 并集列模型 (SPEC D9/Q3) ─────────────────────────────────────

/// 并集列上限 (spec Q3 裁定): 各源 schema 列数差异大时列会爆炸,
/// 超上限按**首见序截断** —— 被截断的列数进 [`UnionColumns::truncated`],
/// 弹层可见提示 (用户要知道自己被截了)。
pub const UNION_COL_CAP: usize = 24;

/// 合并表格模式的并集列 (D9): 各源 schema 列名的并集。
pub struct UnionColumns {
    /// 列名 (首见序 = 源序遍历 × 各源 schema 列序, 去重)。
    pub names: Vec<String>,
    /// 被截断丢弃的列数 (弹层可见提示用; 0 = 没截)。
    pub truncated: usize,
}

/// 各源 schema 的列名并集 (D9)。
///
/// - **混合源**: 无 schema 的源 (.log) 不贡献列 —— 其原文走基座「消息」列;
/// - **首见序**: 按源序、再按各源 schema 的列序 (schema 本就是首见序);
///   跨源同名去重;
/// - **截断**: 超 [`UNION_COL_CAP`] 按首见截断, 计数进 `truncated`。
///
/// 纯函数 (输入只有 schema) —— 不碰文件, 断言锁好写。
pub fn union_columns<'a>(
    schemas: impl IntoIterator<Item = Option<&'a jsonl::Schema>>,
) -> UnionColumns {
    let mut names: Vec<String> = Vec::new();
    let mut truncated = 0usize;
    for schema in schemas.into_iter().flatten() {
        for col in &schema.columns {
            if names.iter().any(|n| n == &col.name) {
                continue;
            }
            if names.len() < UNION_COL_CAP {
                names.push(col.name.clone());
            } else {
                truncated += 1;
            }
        }
    }
    UnionColumns { names, truncated }
}

/// 行的并集列取值 (D9「异源缺列留空」): 该源 schema 缺列 = 空串;
/// 非 JSONL 源 (无 schema) 全空 —— 其原文走基座「消息」列;
/// 行 parse 失败 (残件) 全空。取值走**显示路径** (`parse_line` + `cell_display`,
/// 只 parse 可见行的口径), 不开提取岔路。
pub fn union_cells(
    union: &UnionColumns,
    schema: Option<&jsonl::Schema>,
    line: &[u8],
) -> Vec<String> {
    let n = union.names.len();
    let Some(schema) = schema else {
        return vec![String::new(); n]; // .log 源: 全空 (原文走消息列)
    };
    let Some(value) = jsonl::parse_line(line) else {
        return vec![String::new(); n]; // 残件: 不猜
    };
    let has_col = |name: &str| schema.columns.iter().any(|c| c.name == name);
    union
        .names
        .iter()
        .map(|name| {
            // 判据是 **schema 认不认识该列** (D9「异源缺列留空」), 不是行里碰巧
            // 有没有 —— 并集列从 schema 并出来, 该源缺列就该空; 行里的同名杂散
            // 字段不填 (填了就分不清「这源真有这列」)。
            if !has_col(name) {
                return String::new();
            }
            value
                .get(name)
                .map_or_else(String::new, jsonl::cell_display)
        })
        .collect()
}

/// 源的时间戳通路展示名 (源管理弹层「格式结论」列)。
pub fn route_label(route: &TsRoute) -> String {
    match route {
        TsRoute::LogPrefix(fmt) => fmt.label().to_string(),
        TsRoute::JsonlField(name) => format!("JSONL {name}"),
    }
}

/// 本机当前 UTC 偏移 ms (east 为正) —— 「源时区默认本地」(腿 D) 的默认值来源。
///
/// 兄弟 crate 零依赖红线不许引擎碰平台 API, 故放产品侧; Win32
/// `GetTimeZoneInformation` 裸 extern (danqing-encoding GBK FFI 同款范式,
/// 不为它扩 windows feature)。夏令时按**当前生效档**取 (bias+DaylightBias),
/// 历史时刻的 DST 不回溯 —— 日志跨 DST 的精确解释是手输时区的事, 不猜。
/// 非 Windows 回 0 (=UTC)。
pub fn local_tz_offset_ms() -> i64 {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct SystemTime {
            year: u16,
            month: u16,
            day_of_week: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            milliseconds: u16,
        }
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct TimeZoneInformation {
            bias: i32,
            standard_name: [u16; 32],
            standard_date: SystemTime,
            standard_bias: i32,
            daylight_name: [u16; 32],
            daylight_date: SystemTime,
            daylight_bias: i32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetTimeZoneInformation(tzi: *mut TimeZoneInformation) -> u32;
        }
        let zero_st = SystemTime {
            year: 0,
            month: 0,
            day_of_week: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            milliseconds: 0,
        };
        let mut tzi = TimeZoneInformation {
            bias: 0,
            standard_name: [0; 32],
            standard_date: zero_st,
            standard_bias: 0,
            daylight_name: [0; 32],
            daylight_date: zero_st,
            daylight_bias: 0,
        };
        // TIME_ZONE_ID: 0=未知 1=标准 2=夏令 (Win32 ABI)。
        let id = unsafe { GetTimeZoneInformation(&mut tzi) };
        let extra = if id == 2 {
            tzi.daylight_bias
        } else {
            tzi.standard_bias
        };
        // Win32 语义: UTC = 本地 + Bias 分钟 → 本地偏移 = -(Bias+extra)。
        -(i64::from(tzi.bias) + i64::from(extra)) * 60_000
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// 源时区手输解析 (弹层): 小时数 (`+8` / `-5.5`) 或 `±hh:mm` (`+05:30`) → ms
/// (east 为正)。空/双符号/越界 (|h|>23) 拒收 → None (调用方明示, 不猜)。
pub fn parse_tz_ms(input: &str) -> Option<i64> {
    let s = input.trim();
    let (neg, rest) = match s.as_bytes().first()? {
        b'+' => (false, &s[1..]),
        b'-' => (true, &s[1..]),
        _ => (false, s),
    };
    if rest.is_empty() || rest.starts_with(['+', '-']) {
        return None;
    }
    let ms = if let Some((h, m)) = rest.split_once(':') {
        let h: i64 = h.parse().ok()?;
        let m: i64 = m.parse().ok()?;
        if !(0..=23).contains(&h) || !(0..=59).contains(&m) {
            return None;
        }
        (h * 60 + m) * 60_000
    } else {
        let hours: f64 = rest.parse().ok()?;
        if !hours.is_finite() || hours.abs() > 23.0 {
            return None;
        }
        (hours * 3_600_000.0).round() as i64
    };
    Some(if neg { -ms } else { ms })
}

/// 重建换入时按**路径**把旧视图状态搬进新状态 (加/减源重建不丢书签/显隐/选中)。
///
/// 减源会让源序号漂移, 而 pack 键 = (源序号, 行号) —— 所以键的重映射只能走
/// **路径** (稳定身份), 不是序号。源消失 → 其书签/展开静默丢弃 (行都没了);
/// 显隐按路径继承 (藏了的源加新源后仍藏)。
pub fn carry_view_state(old: &MergeState, new: &mut MergeState) {
    // 源序号重映射: 按路径找新序号 (borrow 拆法: 自由函数, 不捕获 new 的闭包)。
    fn new_idx_of(sources: &[MergeSource], path: &std::path::Path) -> Option<u32> {
        sources
            .iter()
            .position(|s| s.path == path)
            .map(|i| i as u32)
    }
    fn remap_key(old: &MergeState, new_sources: &[MergeSource], k: u64) -> Option<u64> {
        let (src, line) = unpack_key(k);
        let path = &old.sources.get(src as usize)?.path;
        Some(pack_key(new_idx_of(new_sources, path)?, line))
    }
    // 显隐继承: 先从旧状态抄 (按路径), 记下是否有人隐藏。
    let mut any_hidden = false;
    for s in &mut new.sources {
        if let Some(old_s) = old.sources.iter().find(|o| o.path == s.path) {
            s.hidden = old_s.hidden;
            any_hidden |= s.hidden;
        }
    }
    // 书签/展开键重映射 (书签先取出再回填 —— 避免对 new.bookmarks 借用交叠)。
    let carried: BTreeSet<u64> = old
        .bookmarks
        .iter()
        .filter_map(|&k| remap_key(old, &new.sources, k))
        .collect();
    new.bookmarks = carried;
    let expands: Vec<(u64, usize)> = old
        .expanded
        .lines()
        .into_iter()
        .filter_map(|k| Some((remap_key(old, &new.sources, k)?, old.expanded.sub_count(k))))
        .collect();
    new.expanded = ExpandMap::new();
    for (k, n) in expands {
        new.expanded.expand(k, n);
    }
    // 选中行按 (路径, 文件行) 找回; 找不回就回首行。
    new.selected = old
        .row_at(old.selected)
        .and_then(|r| {
            let path = &old.sources.get(r.src as usize)?.path;
            let src = new_idx_of(&new.sources, path)?;
            new.position_of(src, r.line)
        })
        .unwrap_or(0);
    if any_hidden {
        new.rebuild_masked(); // 显隐继承了, 索引也得掩码 (否则「藏了还在」)
    }
}

/// 合并工作区状态 (SPEC D4: 与 Single 模式字段并列, 切换互不丢状态)。
pub struct MergeState {
    pub sources: Vec<MergeSource>,
    /// per-source 时间线 (与 sources 同序; 提取时已含时区+时钟偏移)。
    pub ts: Vec<Vec<i64>>,
    pub index: MergeIndex,
    /// 合并行空间的过滤 (None = 全量), 值 = 合并行号升序 —— req_id 追踪/过滤 (T6)。
    /// 重建 (显隐/偏移) 即作废: 过滤串与行集的对应关系不许跨重建漂 (filter_landed 同族纪律)。
    pub filtered: Option<Vec<u32>>,
    pub top_row: f64,
    pub selected: u64,
    /// 书签 (pack 键集)。
    pub bookmarks: BTreeSet<u64>,
    /// 展开态: ExpandMap 键 = pack_key(src, file_line)。
    pub expanded: ExpandMap,
    pub follow: bool,
}

impl MergeState {
    /// 时间线行数 (显示口径: 过滤后)。
    pub fn row_count(&self) -> u64 {
        match &self.filtered {
            Some(f) => f.len() as u64,
            None => self.index.len() as u64,
        }
    }

    /// 第 pos 个可见行 → 合并行。
    pub fn row_at(&self, pos: u64) -> Option<MergeRow> {
        match &self.filtered {
            Some(f) => f.get(pos as usize).map(|&r| self.index.row(r as usize)),
            None => {
                if pos < self.index.len() as u64 {
                    Some(self.index.row(pos as usize))
                } else {
                    None
                }
            }
        }
    }

    /// 按源隐藏/恢复后重建索引 (引擎掩码; 源序号不漂移, 旧书签/展开键不失效)。
    /// 过滤作废见字段注释。
    ///
    /// **选中行按 (源, 行) 锚定** (键不漂原则同书签): `selected` 是位置值,
    /// 掩码前后位置空间不同 —— 不锚定的话藏一个源, 选中会**静默跳到别的行**
    /// (指针语义家族病)。被藏源上的选中行不可见 → 回落 0。
    pub fn rebuild_masked(&mut self) {
        let anchor = self.row_at(self.selected).map(|r| (r.src, r.line));
        let tls: Vec<SourceTimeline<'_>> = self
            .sources
            .iter()
            .zip(self.ts.iter())
            .map(|(s, ts)| SourceTimeline {
                file: &s.file,
                ts: ts.clone(), // 掩码重建不产新 ts —— 但引擎要 owned Vec; clone 是 8B/行, 重建本就重
            })
            .collect();
        let visible: Vec<bool> = self.sources.iter().map(|s| !s.hidden).collect();
        self.index = merge::build_index_masked(&tls, &visible);
        self.filtered = None;
        self.selected = anchor
            .and_then(|(src, line)| self.position_of(src, line))
            .unwrap_or(0);
    }

    /// 改源时间参数 (腿 D/T5): **解析边界单源施加** —— 只重提该源时间戳 +
    /// 重归并, 文件行索引 (`LogFile`) 原样不动 (「不重建文件索引」)。
    /// 书签/展开键 = (源序号, 行号), 本就不含时间 → 不受影响; 选中按
    /// (源, 行) 锚定 (rebuild_masked 同规)。
    pub fn set_time_params(&mut self, src: usize, offset_ms: i64, tz_offset_ms: i64) {
        {
            let s = &mut self.sources[src];
            s.offset_ms = offset_ms;
            s.tz_offset_ms = tz_offset_ms;
        }
        let (file, route, tz, off) = {
            let s = &self.sources[src];
            (
                Arc::clone(&s.file),
                s.route.clone(),
                s.tz_offset_ms,
                s.offset_ms,
            )
        };
        self.ts[src] = merge::extract_timeline(&file, extractor(&route, tz, off));
        self.rebuild_masked(); // 掩码重建 (hidden 不变) + 选中锚定
    }

    /// (src,line) → 可见位置 (书签跳转用; 线性扫 —— 低频用户动作, 千万行数十 ms,
    /// 不为它建反向索引)。
    pub fn position_of(&self, src: u32, line: u32) -> Option<u64> {
        match &self.filtered {
            Some(f) => f
                .iter()
                .position(|&r| {
                    let row = self.index.row(r as usize);
                    row.src == src && row.line == line
                })
                .map(|p| p as u64),
            None => self
                .index
                .rows()
                .iter()
                .position(|r| r.src == src && r.line == line)
                .map(|p| p as u64),
        }
    }

    /// 书签开关 (合并空间; pack 键)。返回 Some(true)=新增 / Some(false)=去掉 /
    /// None=越界行。上限与单文件书签同一口径 (MAX_BOOKMARKS 在 columns 仓)。
    pub fn toggle_bookmark(&mut self, src: u32, line: u32, cap: usize) -> Option<bool> {
        let key = pack_key(src, line);
        if self.bookmarks.remove(&key) {
            return Some(false);
        }
        if self.bookmarks.len() >= cap {
            return None;
        }
        self.bookmarks.insert(key);
        Some(true)
    }

    /// 按时间线位置序找「下一个」书签位置 (环绕)。返回 (位置, 位次 1-based, 总数)。
    pub fn next_bookmark_pos(&self) -> Option<(u64, usize, usize)> {
        let mut positions: Vec<u64> = self
            .bookmarks
            .iter()
            .filter_map(|&k| {
                let (s, l) = unpack_key(k);
                self.position_of(s, l)
            })
            .collect();
        positions.sort_unstable();
        let total = positions.len();
        let cur = self.selected;
        let next = positions
            .iter()
            .find(|&&p| p > cur)
            .or_else(|| positions.first())?;
        let rank = positions.iter().position(|p| p == next).unwrap() + 1;
        Some((*next, rank, total))
    }

    /// 可见窗口 [from, from+count) 的行视图 (sync 每帧拷这段, 纪律见模块头)。
    pub fn window(&self, from: u64, count: u64) -> Vec<MergeRowView> {
        let end = (from + count).min(self.row_count());
        let mut out = Vec::with_capacity((end - from) as usize);
        for pos in from..end {
            let Some(row) = self.row_at(pos) else { break };
            let src = &self.sources[row.src as usize];
            let text = src.file.line_lossy(u64::from(row.line)).into_owned();
            out.push(MergeRowView {
                ts: row.ts,
                src: row.src,
                line: row.line,
                text,
            });
        }
        out
    }
}

/// sync 拷给视图的行 (窗口元素)。
pub struct MergeRowView {
    pub ts: i64,
    pub src: u32,
    pub line: u32,
    /// 解码后行文本 (line_lossy, 与单文件 Raw 模式同解码口径)。
    pub text: String,
}

/// 归并作业交卷 (AsyncJob 载荷)。
pub struct MergeOutcome {
    pub sources: Vec<MergeSource>,
    pub ts: Vec<Vec<i64>>,
    pub index: MergeIndex,
    /// 被拒源 (探测失败 + 明示原因, SPEC D2)。
    pub rejected: Vec<(PathBuf, &'static str)>,
}

/// 归并 worker (后台线程): 逐源 打开 → 探测 → schema → 提取, 最后归并。
/// `cancel` 在源间检查 (归并本身 ~200ms 不插桩; AsyncJob invalidate 同款惯例:
/// 作废的交卷 UI 侧丢弃)。
pub fn build_merge(paths: &[PathBuf], cancel: &AtomicBool) -> MergeOutcome {
    let mut sources: Vec<MergeSource> = Vec::new();
    let mut rejected: Vec<(PathBuf, &'static str)> = Vec::new();
    for p in paths {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        let file = match LogFile::open(p) {
            Ok(f) => f,
            Err(_) => {
                rejected.push((p.clone(), "打开失败"));
                continue;
            }
        };
        let route = match timestamp::detect_route(&file) {
            Ok(r) => r,
            Err(reason) => {
                rejected.push((p.clone(), reason.label()));
                continue;
            }
        };
        let schema = if matches!(route, TsRoute::JsonlField(_)) {
            jsonl::discover_schema(&file).map(Arc::new)
        } else {
            None
        };
        sources.push(MergeSource {
            path: p.clone(),
            file: Arc::new(file),
            schema,
            route,
            offset_ms: 0,
            // 源时区默认**本地** (腿 D: 无 tz 格式必填, 默认本地) —— 只影响
            // 无 tz 时间戳的解释; 显式 Z/±hh:mm 的行不受它动。
            tz_offset_ms: local_tz_offset_ms(),
            hidden: false,
        });
    }
    // 提取 + 归并 (取消只挡源间, 提取段内不插桩 —— 单源提取实测 ≤0.4s/GB)。
    let mut ts: Vec<Vec<i64>> = Vec::with_capacity(sources.len());
    for s in &sources {
        let ts_vec =
            merge::extract_timeline(&s.file, extractor(&s.route, s.tz_offset_ms, s.offset_ms));
        ts.push(ts_vec);
    }
    let tls: Vec<SourceTimeline<'_>> = sources
        .iter()
        .zip(ts.iter())
        .map(|(s, v)| SourceTimeline {
            file: &s.file,
            ts: v.clone(),
        })
        .collect();
    let index = merge::build_index(&tls);
    MergeOutcome {
        sources,
        ts,
        index,
        rejected,
    }
}

/// 提取闭包: 通路 + 源时区 + 时钟偏移单源施加 (SPEC 腿 D —— 排序与显示同源,
/// 不许两处各算一遍)。
fn extractor(route: &TsRoute, tz_ms: i64, offset_ms: i64) -> impl Fn(&[u8]) -> Option<i64> + '_ {
    let route = route.clone();
    move |line| {
        let t = match &route {
            TsRoute::LogPrefix(fmt) => timestamp::parse_prefix(line, *fmt, tz_ms).map(|(v, _)| v),
            TsRoute::JsonlField(name) => timestamp::parse_jsonl_field(line, name, tz_ms),
        }?;
        Some(t + offset_ms)
    }
}

/// 时间列显示: epoch 毫秒 → `HH:MM:SS.mmm`。
/// **排序与显示同源**: 提取时已含源时区/时钟偏移, 这里不再加任何时区。
/// 已知边界 (T3 明言): 跨天合并的日期不进时间列 —— 观感若打架, 验收时裁
/// (备选: 日期变化处插分隔行, 那是新行种, 不为它动行锚定数学)。
pub fn fmt_time_of_day(ts_ms: i64) -> String {
    let rem = ts_ms.rem_euclid(86_400_000);
    let h = rem / 3_600_000;
    let m = rem % 3_600_000 / 60_000;
    let s = rem % 60_000 / 1_000;
    let ms = rem % 1_000;
    format!("{h:02}:{m:02}:{s:02}.{ms:03}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 临时 fixture (每用例独立名, 收尾删)。
    fn temp_log(name: &str, content: &[u8]) -> PathBuf {
        let p = std::env::temp_dir().join(format!("danqing-mv-test-{name}.log"));
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(content).unwrap();
        p
    }

    fn build(paths: &[PathBuf]) -> MergeOutcome {
        build_merge(paths, &AtomicBool::new(false))
    }

    fn state_of(out: MergeOutcome) -> MergeState {
        MergeState {
            sources: out.sources,
            ts: out.ts,
            index: out.index,
            filtered: None,
            top_row: 0.0,
            selected: 0,
            bookmarks: BTreeSet::new(),
            expanded: ExpandMap::new(),
            follow: false,
        }
    }

    #[test]
    fn pack_unpack_roundtrip() {
        assert_eq!(unpack_key(pack_key(7, 42)), (7, 42));
        assert_eq!(unpack_key(pack_key(0, u32::MAX)), (0, u32::MAX));
        // 不同源同行号不撞键
        assert_ne!(pack_key(1, 5), pack_key(2, 5));
    }

    #[test]
    fn build_merge_two_sources_interleaved_end_to_end() {
        let pj = temp_log(
            "e2e-a.jsonl",
            br#"{"ts":"2026-09-27T00:00:01Z","level":"INFO","msg":"a1"}
{"ts":"2026-09-27T00:00:03Z","level":"ERROR","msg":"a2"}
{"ts":"2026-09-27T00:00:05Z","level":"INFO","msg":"a3"}
"#,
        );
        let pl = temp_log(
            "e2e-b",
            b"2026-09-27T00:00:00Z INFO l0\n2026-09-27T00:00:02Z INFO l1\n2026-09-27T00:00:04Z INFO l2\n",
        );
        let out = build(&[pj.clone(), pl.clone()]);
        assert!(out.rejected.is_empty(), "拒收: {:?}", out.rejected);
        assert_eq!(out.sources.len(), 2);
        assert!(out.sources[0].schema.is_some(), "JSONL 源有 schema");
        assert!(out.sources[1].schema.is_none());
        let st = state_of(out);
        let seq: Vec<(u32, u32)> = (0..st.row_count())
            .map(|p| {
                let r = st.row_at(p).unwrap();
                (r.src, r.line)
            })
            .collect();
        assert_eq!(seq, vec![(1, 0), (0, 0), (1, 1), (0, 1), (1, 2), (0, 2)]);
        std::fs::remove_file(&pj).ok();
        std::fs::remove_file(&pl).ok();
    }

    #[test]
    fn build_merge_rejects_unprobeable_with_reason() {
        let good = temp_log("rej-good", b"2026-09-27T00:00:00Z INFO a\n2026-09-27T00:00:01Z INFO b\n2026-09-27T00:00:02Z INFO c\n");
        let bad = temp_log(
            "rej-bad",
            b"no timestamp here\nplain text\nstill plain\nmore plain\n",
        );
        let out = build(&[good.clone(), bad.clone()]);
        assert_eq!(out.sources.len(), 1);
        assert_eq!(out.rejected.len(), 1);
        assert_eq!(out.rejected[0].0, bad);
        assert!(!out.rejected[0].1.is_empty(), "明示原因非空 (SPEC D2)");
        std::fs::remove_file(&good).ok();
    }

    #[test]
    fn merge_state_filtered_row_at_maps_positions() {
        let pa = temp_log(
            "filt-a",
            b"2026-09-27T00:00:00Z a\n2026-09-27T00:00:01Z b\n2026-09-27T00:00:02Z c\n",
        );
        let out = build(std::slice::from_ref(&pa));
        let mut st = state_of(out);
        assert_eq!(st.row_count(), 3);
        st.filtered = Some(vec![0, 2]);
        assert_eq!(st.row_count(), 2);
        assert_eq!(st.row_at(1).unwrap().line, 2, "过滤位 1 → 合并行 2");
        assert!(st.row_at(2).is_none());
        std::fs::remove_file(&pa).ok();
    }

    #[test]
    fn rebuild_masked_hides_source_and_keeps_ids() {
        let pa = temp_log(
            "mask-a",
            b"2026-09-27T00:00:00Z a0\n2026-09-27T00:00:10Z a1\n2026-09-27T00:00:20Z a2\n",
        );
        let pb = temp_log(
            "mask-b",
            b"2026-09-27T00:00:01Z b0\n2026-09-27T00:00:11Z b1\n2026-09-27T00:00:21Z b2\n",
        );
        let pc = temp_log(
            "mask-c",
            b"2026-09-27T00:00:02Z c0\n2026-09-27T00:00:12Z c1\n2026-09-27T00:00:22Z c2\n",
        );
        let out = build(&[pa.clone(), pb.clone(), pc.clone()]);
        let mut st = state_of(out);
        assert_eq!(st.row_count(), 9, "三源归并总行数");
        st.filtered = Some(vec![0]);
        st.sources[1].hidden = true;
        st.rebuild_masked();
        assert_eq!(st.row_count(), 6, "藏源 1 后剩 6 行");
        assert!(
            st.filtered.is_none(),
            "重建作废过滤 (filter_landed 同族纪律)"
        );
        let srcs: Vec<u32> = (0..st.row_count())
            .map(|p| st.row_at(p).unwrap().src)
            .collect();
        assert_eq!(srcs, vec![0, 2, 0, 2, 0, 2], "源序号不漂移且时间交错");
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
        std::fs::remove_file(&pc).ok();
    }

    #[test]
    fn window_decodes_visible_rows_only() {
        let pa = temp_log(
            "win-a",
            b"2026-09-27T00:00:00Z alpha\n2026-09-27T00:00:01Z beta\n2026-09-27T00:00:02Z gamma\n",
        );
        let out = build(std::slice::from_ref(&pa));
        let st = state_of(out);
        let w = st.window(1, 2);
        assert_eq!(w.len(), 2);
        assert!(w[0].text.contains("beta"));
        assert!(w[1].text.contains("gamma"));
        assert!(st.window(2, WINDOW_ROWS).len() == 1, "窗口尾钳到行数");
        std::fs::remove_file(&pa).ok();
    }

    #[test]
    fn fmt_time_of_day_renders_hhmmssmmm() {
        assert_eq!(fmt_time_of_day(0), "00:00:00.000");
        assert_eq!(fmt_time_of_day(3_723_456), "01:02:03.456");
        // 负值 (1970 前) 环绕而非负数 —— rem_euclid 口径
        assert_eq!(fmt_time_of_day(-1), "23:59:59.999");
    }

    // ---- T4: 并集列断言锁 (D9/Q3) ----

    fn schema_of(names: &[&str]) -> jsonl::Schema {
        jsonl::Schema {
            columns: names
                .iter()
                .map(|n| jsonl::Column {
                    name: (*n).to_string(),
                    width_chars: 8,
                })
                .collect(),
        }
    }

    #[test]
    fn union_columns_first_seen_order_dedup_and_cap() {
        let a = schema_of(&["ts", "level", "msg"]);
        let b = schema_of(&["level", "req", "msg"]);
        let u = union_columns([Some(&a), Some(&b)]);
        assert_eq!(
            u.names,
            vec!["ts", "level", "msg", "req"],
            "首见序 + 跨源去重"
        );
        assert_eq!(u.truncated, 0);
        // 截断: 超 24 按首见截, 计数进 truncated
        let wide: Vec<String> = (0..30).map(|i| format!("c{i:02}")).collect();
        let wide_refs: Vec<&str> = wide.iter().map(|s| s.as_str()).collect();
        let w = schema_of(&wide_refs);
        let u = union_columns([Some(&w)]);
        assert_eq!(u.names.len(), UNION_COL_CAP, "上限 24");
        assert_eq!(u.names[0], "c00", "首见序保头");
        assert_eq!(u.truncated, 6, "超上限的列数如实计");
    }

    #[test]
    fn union_columns_mixed_sources_log_contributes_nothing() {
        let a = schema_of(&["ts", "msg"]);
        // None = .log 源 (无 schema): 不贡献列, 也不打断序
        let u = union_columns([Some(&a), None, Some(&schema_of(&["extra"]))]);
        assert_eq!(u.names, vec!["ts", "msg", "extra"]);
    }

    #[test]
    fn union_cells_blank_for_missing_columns_and_log_rows() {
        let a = schema_of(&["ts", "level"]);
        let b = schema_of(&["ts", "req"]);
        let u = union_columns([Some(&a), Some(&b)]);
        assert_eq!(u.names, vec!["ts", "level", "req"]);
        // 源 a 的行: req 列缺 → 空串 (异源缺列留空, D9)
        let cells = union_cells(&u, Some(&a), br#"{"ts":"1","level":"INFO"}"#);
        assert_eq!(cells, vec!["1", "INFO", ""]);
        // 源 b 的行: level 列缺 → 空串
        let cells = union_cells(&u, Some(&b), br#"{"ts":"2","req":"r1"}"#);
        assert_eq!(cells, vec!["2", "", "r1"]);
        // .log 源: 全空 (原文走消息列)
        let cells = union_cells(&u, None, b"2026-09-27T00:00:00Z INFO hi");
        assert_eq!(cells, vec!["", "", ""]);
    }

    #[test]
    fn union_cells_schema_gates_membership_not_stray_fields() {
        // 行里碰巧有源 schema 不认识的同名字段 —— 不填 (判据是 schema, 不是行)。
        let a = schema_of(&["ts"]);
        let b = schema_of(&["req"]);
        let u = union_columns([Some(&a), Some(&b)]);
        let cells = union_cells(&u, Some(&a), br#"{"ts":"1","req":"stray"}"#);
        assert_eq!(cells, vec!["1", ""], "req 不在源 a 的 schema → 空");
    }

    // ---- T5: 时钟偏移/时区 (腿 D) ----

    /// **主锁** `offset_applied_at_parse_boundary` (腿 D): 偏移只在**解析边界**
    /// 施加一次 —— 排序 (归并序) 与显示 (时间列) 读的是**同一份**已施加 ts。
    /// 断言两侧同时对: 序随偏移动 + 显示字面量恰好回位一次; 施加两遍必红
    /// (10s 拨 -7s → 显示 03.000; 双重施加 → 23:59:56.000)。
    #[test]
    fn offset_applied_at_parse_boundary() {
        // 源0 (log4j, 无 tz): 10/20/30s; 源1 (JSONL, 显式 Z): 3/25/35s。
        let pa = temp_log(
            "t5-a.log",
            b"2026-09-28 00:00:10,000 INFO a0
2026-09-28 00:00:20,000 INFO a1
2026-09-28 00:00:30,000 INFO a2
",
        );
        let pb = temp_log(
            "t5-b.jsonl",
            br#"{"ts":"2026-09-28T00:00:03Z","msg":"b0"}
{"ts":"2026-09-28T00:00:25Z","msg":"b1"}
{"ts":"2026-09-28T00:00:35Z","msg":"b2"}
"#,
        );
        let out = build(&[pa.clone(), pb.clone()]);
        let mut st = state_of(out);
        // 归一: 时区钳 0 (默认本地是 spec 语义, 但测试不许依赖机器时区)
        st.set_time_params(0, 0, 0);
        let seq = |st: &MergeState| -> Vec<(u32, u32)> {
            (0..st.row_count())
                .map(|p| {
                    let r = st.row_at(p).unwrap();
                    (r.src, r.line)
                })
                .collect()
        };
        // 未拨: 3(b0) 10(a0) 20(a1) 25(b1) 30(a2) 35(b2)
        assert_eq!(
            seq(&st),
            vec![(1, 0), (0, 0), (0, 1), (1, 1), (0, 2), (1, 2)]
        );
        let a0_pos = st.position_of(0, 0).unwrap();
        assert_eq!(
            fmt_time_of_day(st.row_at(a0_pos).unwrap().ts),
            "00:00:10.000"
        );
        // a0 拨 -7s → 3s, 与 b0(3s) 等时刻 → tie-break 源序号小者先 (源0)
        st.set_time_params(0, -7_000, 0);
        assert_eq!(
            seq(&st),
            vec![(0, 0), (1, 0), (0, 1), (0, 2), (1, 1), (1, 2)],
            "序随偏移重排 (a0 的 3s 与 b0 的 3s 等时刻, 源序号小者先)"
        );
        // 显示侧: 恰好施加一次 (两处各算必红 —— 双重施加会得 23:59:56.000)
        let a0_pos = st.position_of(0, 0).unwrap();
        assert_eq!(
            fmt_time_of_day(st.row_at(a0_pos).unwrap().ts),
            "00:00:03.000",
            "显示与排序同一份 ts, 偏移只施加一次"
        );
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
    }

    /// 时区只动**无 tz** 行 (腿 D): log4j 行随 tz 解释, 显式 Z 行一个字节不动。
    #[test]
    fn tz_interprets_tzless_rows_only() {
        let pa = temp_log(
            "t5-tz.log",
            b"2026-09-28 00:00:10,000 INFO a0
2026-09-28 00:00:20,000 INFO a1
2026-09-28 00:00:30,000 INFO a2
",
        );
        let pb = temp_log(
            "t5-tz-b.jsonl",
            br#"{"ts":"2026-09-28T00:00:03Z","msg":"b0"}
{"ts":"2026-09-28T00:00:25Z","msg":"b1"}
{"ts":"2026-09-28T00:00:35Z","msg":"b2"}
"#,
        );
        let out = build(&[pa.clone(), pb.clone()]);
        let mut st = state_of(out);
        st.set_time_params(0, 0, 0); // 归一 (测试不依赖机器时区)
        st.set_time_params(1, 0, 0);
        let base_a = st.row_at(st.position_of(0, 0).unwrap()).unwrap().ts;
        let base_b = st.row_at(st.position_of(1, 0).unwrap()).unwrap().ts;
        // tz = +8h: 无 tz 的 log4j 行 epoch 减 8h
        st.set_time_params(0, 0, 8 * 3_600_000);
        let new_a = st.row_at(st.position_of(0, 0).unwrap()).unwrap().ts;
        assert_eq!(new_a, base_a - 8 * 3_600_000, "无 tz 行随 tz 解释");
        // 显式 Z 的 JSONL 行一动不动 (只改了源0, 源1 本就没动 —— 补验: 源1 也拨同 tz)
        st.set_time_params(1, 0, 8 * 3_600_000);
        let new_b = st.row_at(st.position_of(1, 0).unwrap()).unwrap().ts;
        assert_eq!(new_b, base_b, "显式 Z 行不受 tz 动");
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
    }

    #[test]
    fn parse_tz_ms_accepts_hours_and_hhmm() {
        assert_eq!(parse_tz_ms("+8"), Some(8 * 3_600_000));
        assert_eq!(parse_tz_ms("-5.5"), Some(-5 * 3_600_000 - 1_800_000));
        assert_eq!(parse_tz_ms("+05:30"), Some(5 * 3_600_000 + 1_800_000));
        assert_eq!(parse_tz_ms("0"), Some(0));
        assert_eq!(parse_tz_ms("23"), Some(23 * 3_600_000));
        // 拒收: 空 / 双符号 / 越界 / 非数
        assert_eq!(parse_tz_ms(""), None);
        assert_eq!(parse_tz_ms("+-8"), None);
        assert_eq!(parse_tz_ms("+24"), None);
        assert_eq!(parse_tz_ms("abc"), None);
        assert_eq!(parse_tz_ms("+1:99"), None);
    }

    #[test]
    fn carry_view_state_remaps_keys_by_path_after_reorder() {
        let pa = temp_log(
            "carry-a",
            b"2026-09-27T00:00:00Z a0
2026-09-27T00:00:10Z a1
2026-09-27T00:00:20Z a2
",
        );
        let pb = temp_log(
            "carry-b",
            b"2026-09-27T00:00:01Z b0
2026-09-27T00:00:11Z b1
2026-09-27T00:00:21Z b2
",
        );
        let out = build(&[pa.clone(), pb.clone()]);
        let mut old = state_of(out);
        // 书签两枚 (源 0 行 1 / 源 1 行 2), 源 1 隐藏, 选中停在 (0,1)
        old.toggle_bookmark(0, 1, 256);
        old.toggle_bookmark(1, 2, 256);
        old.sources[1].hidden = true;
        old.selected = old.position_of(0, 1).unwrap();
        // 重建换入: **减源 0** (旧源 1 → 新源 0, 序号漂移)
        let out = build(std::slice::from_ref(&pb));
        let mut fresh = state_of(out);
        carry_view_state(&old, &mut fresh);
        assert_eq!(fresh.sources.len(), 1);
        assert!(fresh.sources[0].hidden, "显隐按路径继承");
        assert_eq!(
            fresh.bookmarks.iter().copied().collect::<Vec<u64>>(),
            vec![pack_key(0, 2)],
            "源 0 的书签随源消失丢弃; 源 1 的书签按路径重映射到新序号 0"
        );
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
    }

    #[test]
    fn position_of_and_next_bookmark_pos() {
        let pa = temp_log(
            "pos-a",
            b"2026-09-27T00:00:00Z a0\n2026-09-27T00:00:01Z a1\n2026-09-27T00:00:02Z a2\n",
        );
        let out = build(std::slice::from_ref(&pa));
        let mut st = state_of(out);
        assert_eq!(st.position_of(0, 1), Some(1));
        assert_eq!(st.position_of(3, 0), None, "不存在的源 → None");
        // 书签跳转: 位置序下一个, 环绕。
        st.toggle_bookmark(0, 0, 256);
        st.toggle_bookmark(0, 2, 256);
        st.selected = 0;
        assert_eq!(st.next_bookmark_pos(), Some((2, 2, 2)), "向后找下一个");
        st.selected = 2;
        assert_eq!(st.next_bookmark_pos(), Some((0, 1, 2)), "环绕回首个");
        assert_eq!(st.toggle_bookmark(0, 1, 1), None, "容量帽拒绝");
        std::fs::remove_file(&pa).ok();
    }
}
