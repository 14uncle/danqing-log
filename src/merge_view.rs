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
    /// live-tail 断流标记 (腿 F/T7): stat/追加读取失败 = true (弹层标「断流」,
    /// 底栏报数), 恢复可读即清。**单源断流不拖垮全局** —— 其余源照常合流,
    /// 该源行集保持旧快照 (内容诚实, 标记说清它不再更新)。
    pub stale: bool,
}

/// 旧快照释放**交一次性线程** (T9 D3): 1 GiB 映射的解映射实测 50-160 ms,
/// 每次 live-tail 追加都要换一次快照 —— 压在 UI 线程就是每轮一次可见卡顿。
/// 只为释放, 无结果无通知; Arc 若仍被别处持有, 这里只是减引用。
/// **释放策略的唯一 spawn 点** —— 两种载荷 (单/批) 都走它:
/// `Builder` 而非 `thread::spawn` (建线程失败**回退就地 drop**: `spawn` 会
/// panic, release 档 = abort, 为一个释放动作付进程代价不值)。
fn dispose_offthread<T: Send + 'static>(payload: T) {
    let _ = std::thread::Builder::new().spawn(move || drop(payload));
}

/// 单个旧快照 (归并侧换快照时调用)。
pub fn dispose_snapshot(old: std::sync::Arc<LogFile>) {
    dispose_offthread(old);
}

/// 一批快照的交接收口 (评审 R2): `LogView.merge_files` 与 `MergeState.sources`
/// 同持 Arc —— 只释放 state 侧那一个引用会让 refcount 停在 1, **真正的
/// munmap 落在同帧稍后 view 的 sync 里 (UI 线程)**。故 view 侧退役的整表也走
/// 这条通道, 保证**最后一次** drop 不在 UI 线程上。
pub fn dispose_snapshots(old: Vec<std::sync::Arc<LogFile>>) {
    if !old.is_empty() {
        dispose_offthread(old);
    }
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

// ─── T8: 合并组会话载荷 (SPEC-v1x-merge-timeline D4) ───────────────────

/// 会话载荷里单个源的快照 (T8)。存路径 + 时间参数 + 显隐; **探测结论
/// (route) 不落盘** —— 恢复时重新探测 (文件内容可能已变; 探测失败/文件缺失
/// = 明示跳过该源, 不拒全体会话)。
#[derive(Debug, Clone, PartialEq)]
pub struct MergeSourceState {
    /// 源文件路径 (恢复按路径对源 —— 加/减源造成的序号漂移免疫)。
    pub path: String,
    /// 时钟偏移 ms (T5 弹层编辑值)。
    pub offset_ms: i64,
    /// 无 tz 格式源时区偏移 ms (有 tz 格式不受它影响, 套回无害)。
    pub tz_offset_ms: i64,
    /// 按源显隐。
    pub hidden: bool,
}

/// 合并组会话载荷 (D4「merge group 进 sessions 载荷 = 首版目标」兑现):
/// 源组快照, **保序 = 源序号**。追踪过滤串/书签/展开**不落盘** —— 追踪是
/// 可重跑的查询但首版不存 (spec 实现记); 书签/展开是 (src,line) 键现场,
/// 会话语义不携 (Open Q1 同哲学: 书签不随会话)。
#[derive(Debug, Clone, PartialEq)]
pub struct MergeGroup {
    pub sources: Vec<MergeSourceState>,
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
    // 源集合变了 (本函数唯一调用场景) → 合并选区代次 +1 (序号漂移, 旧选区键失效)。
    new.sel_rev = old.sel_rev.wrapping_add(1);
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
    /// 追踪过滤的值 (腿 E/T6): 底栏常驻显示 + (T7) 增量追加复用; 与 filtered 同生同灭。
    pub trace: Option<String>,
    /// 追踪命中缓存 (T7): per-source 命中行号, 与 trace 同生同灭 —— live-tail
    /// 追加时增量补滤 (run_filter_from 退一行) 全靠它, 不重扫全源。
    pub trace_hits: Option<Vec<Vec<u64>>>,
    /// 合并选区失效代次 (T6): 仅**源集合变化** (carry_view_state) 递增 —— (源序号,行)
    /// 键在显隐/偏移/追踪过滤下内容稳定, 唯有加/减源序号漂移 (视图据此清合并选区)。
    pub sel_rev: u64,
    pub top_row: f64,
    pub selected: u64,
    /// 书签 (pack 键集)。
    pub bookmarks: BTreeSet<u64>,
    /// 展开态: ExpandMap 键 = pack_key(src, file_line)。
    pub expanded: ExpandMap,
    pub follow: bool,
}

impl MergeState {
    /// 交卷 → 视图现场 (**首次合并的全新状态**; carry / 会话恢复载荷由应用层
    /// 另行套用 —— 本函数不碰任何旧现场)。T9 收口: 此前「交卷换入」「测试
    /// 夹具」「基准」三处各抄一份同形字面量。
    pub fn from_outcome(out: MergeOutcome) -> Self {
        Self {
            sources: out.sources,
            ts: out.ts,
            index: out.index,
            filtered: None,
            trace: None,
            trace_hits: None,
            sel_rev: 0,
            top_row: 0.0,
            selected: 0,
            bookmarks: BTreeSet::new(),
            expanded: ExpandMap::new(),
            follow: false,
        }
    }

    /// 时间线行数 (显示口径: 过滤后)。
    pub fn row_count(&self) -> u64 {
        match &self.filtered {
            Some(f) => f.len() as u64,
            None => self.index.len() as u64,
        }
    }

    /// 第 pos 个可见行的**行身份** = `(源序号, 文件行号)` —— 全模块的锚定键
    /// (键不漂原则: 重建/过滤/追加后按它找回, 位置值 `selected` 只是快照)。
    /// 收口「row_at + 取二元组」这一形状 (重建/清追踪/追加三处共用一份语义)。
    pub fn anchor_at(&self, pos: u64) -> Option<(u32, u32)> {
        self.row_at(pos).map(|r| (r.src, r.line))
    }

    /// 各源的文件句柄表 (复制链/追踪作业要按源序号取原文)。
    pub fn file_handles(&self) -> Vec<Arc<LogFile>> {
        self.sources.iter().map(|s| Arc::clone(&s.file)).collect()
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
        let anchor = self.anchor_at(self.selected);
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
        self.trace = None; // 与 filtered 同生同灭 (字段注释)
        self.trace_hits = None;
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
            // 钳制收口 (评审 Optional 同源): UI 手输与账本载入走同一条界 ——
            // 否则天文值经 extractor 的裸加法回绕 (release 静默错序)。
            let s = &mut self.sources[src];
            s.offset_ms = clamp_offset_ms(offset_ms);
            s.tz_offset_ms = clamp_tz_ms(tz_offset_ms);
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

    /// 合并组快照 (T8 会话保存): 源路径 + 时间参数 + 显隐, 保序 = 源序号。
    pub fn snapshot_group(&self) -> MergeGroup {
        MergeGroup {
            sources: self
                .sources
                .iter()
                .map(|s| MergeSourceState {
                    path: s.path.to_string_lossy().into_owned(),
                    offset_ms: s.offset_ms,
                    tz_offset_ms: s.tz_offset_ms,
                    hidden: s.hidden,
                })
                .collect(),
        }
    }

    /// 会话恢复 (T8): 把保存的源参数 (偏移/时区/显隐) 按**路径**套回当前
    /// 源组 —— 序号漂移免疫。时间参数变了才重提该源 ts (set_time_params
    /// 同边界: 文件行索引不动), 全部套完**一次**重归并 (N 源不 N 次重建)。
    /// 返回有实际变化的源数 (notice/锁用)。
    ///
    /// 书签/展开/选中不直接触碰 —— 调用方 (交卷) 先跑 carry_view_state,
    /// 本函数殿后 (显隐以保存值为准); rebuild_masked 自带选中 (源,行) 锚定。
    pub fn apply_saved_params(&mut self, saved: &MergeGroup) -> usize {
        // T9 评审 R3 后: 本函数是**网**不是主通路 —— `build_merge` 已按会话参数
        // 提取 (worker 里做完, 零 UI 线程重提); 这里只收拾「source 集合之后又变过」
        // 的差额。参数一致时 applied = 0, 零重提零重建。
        let mut applied = 0usize;
        for i in 0..self.sources.len() {
            let Some(sv) = saved
                .sources
                .iter()
                .find(|x| x.path == self.sources[i].path.to_string_lossy())
            else {
                continue; // 载荷外的新源 (恢复后又加的) 不动
            };
            let mut dirty = false;
            if self.sources[i].offset_ms != sv.offset_ms
                || self.sources[i].tz_offset_ms != sv.tz_offset_ms
            {
                self.sources[i].offset_ms = sv.offset_ms;
                self.sources[i].tz_offset_ms = sv.tz_offset_ms;
                let (file, route, tz, off) = {
                    let s = &self.sources[i];
                    (
                        Arc::clone(&s.file),
                        s.route.clone(),
                        s.tz_offset_ms,
                        s.offset_ms,
                    )
                };
                self.ts[i] = merge::extract_timeline(&file, extractor(&route, tz, off));
                dirty = true;
            }
            if self.sources[i].hidden != sv.hidden {
                self.sources[i].hidden = sv.hidden;
                dirty = true;
            }
            if dirty {
                applied += 1;
            }
        }
        if applied > 0 {
            self.rebuild_masked();
        }
        applied
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

    /// 追踪过滤落地 (腿 E/T6): per-source 命中行集 → 合并行号集 (升序)。
    /// 命中表**收归持有** (trace_hits 缓存) —— T7 live-tail 追加的增量补滤
    /// 全靠它, 不重扫全源。
    /// 选中按 (源,行) 锚定 (rebuild_masked 同规); anchor 必在命中集 (发起行
    /// 本行含该值), 仍 fallback 0 防御。返回命中行数。
    pub fn apply_trace(&mut self, hits: Vec<Vec<u64>>, anchor: (u32, u32)) -> usize {
        // **运行期**同源集校验 (评审 C1): 命中表是「某时刻源集合」的产物, 若
        // 交卷时源集合已换 (在途追踪 + 换源), 按 src 索引命中表会越界 panic
        // (release = abort) —— 宁丢一次追踪, 不许崩。调用方应同时作废在途作业
        // (应用层), 这里是最后一道闸。
        if hits.len() != self.sources.len() {
            self.filtered = None;
            self.trace = None;
            self.trace_hits = None;
            return 0;
        }
        let out = self.filtered_from_hits(&hits);
        let n = out.len();
        self.filtered = Some(out);
        self.trace_hits = Some(hits);
        self.selected = self.position_of(anchor.0, anchor.1).unwrap_or(0);
        n
    }

    /// 两指针走查 (apply_trace 与 T7 追加重推共用单点): 归并**保持文件内行序**
    /// (D1/CP0 钉住语义), 故每源的行在索引里升序出现, 命中表 (run_filter 产出
    /// 即升序) 可单调指针比对, 全程 O(合并行数 + 命中数), 无哈希无分配放大。
    fn filtered_from_hits(&self, hits: &[Vec<u64>]) -> Vec<u32> {
        debug_assert_eq!(hits.len(), self.sources.len(), "hits 与 sources 同序等长");
        let mut cursors = vec![0usize; hits.len()];
        let mut out: Vec<u32> = Vec::new();
        for (i, r) in self.index.rows().iter().enumerate() {
            // `get` 而非下标: 索引行可能引用超出命中表长度的源 (源集合被换过) ——
            // 该行按「未命中」处理, 不 panic (入口 apply_trace 已挡, 此为纵深)。
            let Some(h) = hits.get(r.src as usize) else {
                continue;
            };
            let c = &mut cursors[r.src as usize];
            while *c < h.len() && h[*c] < u64::from(r.line) {
                *c += 1;
            }
            if *c < h.len() && h[*c] == u64::from(r.line) {
                out.push(i as u32);
            }
        }
        out
    }

    /// 清除追踪过滤 (Esc): 回全量; 选中按 (源,行) 锚定 (位置空间变大, 锚行仍在)。
    pub fn clear_trace(&mut self) {
        if self.filtered.is_none() {
            return;
        }
        let anchor = self.anchor_at(self.selected);
        self.filtered = None;
        self.trace = None;
        self.trace_hits = None;
        self.selected = anchor
            .and_then(|(s, l)| self.position_of(s, l))
            .unwrap_or(0);
    }

    /// 跟随钉尾 (腿 F/T7): 合并时间线尾部 = 最新事件处 (单文件 toggle_follow 同款)。
    pub fn pin_tail(&mut self) {
        let last = self.row_count().saturating_sub(1);
        self.selected = last;
        self.top_row = last as f64;
    }

    /// 单源增量合流 (腿 F/T7): 该源新行进 ts 向量 (extract_append) + 索引插入
    /// (insert_rows 尾端回找)。返回新增行数 (0 = 无可合流)。
    ///
    /// - **末行补全** (无换行结尾被追加补全, 内容改判): 退一行重提 —— ts 先 pop
    ///   再 extract_append; 索引里旧末行条目**先摘后插** (remove_row, 否则同一
    ///   (src,line) 两条 = 旧 ts 幽灵)。摘不到 (超回找帽) → 掩码全量重建兜底
    ///   (诚实, 不静默留幽灵; 罕遇见底注)。
    /// - **隐藏源**: ts 照长 (再显示时掩码重建同源), 索引不插 (掩码语义不破)。
    /// - **选中锚定**: 插入会让位置漂移, 全程按 (源,行) 锚 (书签同原则);
    ///   follow 态钉尾覆盖锚定。
    /// - **追踪过滤随行**: trace 激活时新行增量补滤 (run_filter_from **退一行**
    ///   重叠 —— 末行补全改判同区间, 摘/补对称同单文件 apply_appended 纪律),
    ///   过滤行集全量重推 (插入漂移后唯一诚实形态)。
    ///
    /// 调用方纪律: `new_file` 必须是 `append_from` 的追加产物 (同文件更长);
    /// 轮转/缩容/UTF-16 副本不走这里 (应用层分流 rebuild_merge)。
    pub fn append_source(&mut self, src: u32, new_file: LogFile) -> u64 {
        let (old_count, new_count, hidden, extract) = {
            let s = &self.sources[src as usize];
            (
                s.file.line_count(),
                new_file.line_count(),
                s.hidden,
                extractor(&s.route, s.tz_offset_ms, s.offset_ms),
            )
        };
        // 末行补全判定: 旧快照以 \n 结尾 = 末行完整, 追加不可能改判它;
        // 空文件 (old_count=0) 无末行可补全, 同走完整档 (否则 0-1 下溢)。
        let tail_complete =
            old_count == 0 || self.sources[src as usize].file.bytes().last() == Some(&b'\n');
        // 缩容不在此通路 (应用层分流重建); 「行数不变 + 末行完整」= 空转 —
        // 但**行数不变 + 末行曾被截断** = 补全改判 (字节长了行数没长), 要走。
        if new_count < old_count || (new_count == old_count && tail_complete) {
            return 0;
        }
        // 选中锚 (手术前): 插入漂移后找回
        let anchor = self.anchor_at(self.selected);

        // ① ts 增量提取 (退一行口径见上)
        let from = if tail_complete {
            old_count
        } else {
            old_count.saturating_sub(1)
        };
        {
            let ts = &mut self.ts[src as usize];
            debug_assert_eq!(ts.len() as u64, old_count, "ts 与行数同长 (构建不变量)");
            for _ in from..old_count {
                ts.pop();
            }
            merge::extract_append(&new_file, ts, from, extract);
            debug_assert_eq!(ts.len() as u64, new_count);
        }
        let new_rows: Vec<(i64, u32)> = (from..new_count)
            .map(|l| (self.ts[src as usize][l as usize], l as u32))
            .collect();

        // ② 追踪增量补滤 (摘/补同区间 from; 须在换入文件前用 new_file 跑)
        let trace_live = if let (Some(value), Some(hits)) = (&self.trace, &mut self.trace_hits) {
            let fresh = jsonl::run_filter_from(&new_file, &[trace_clause(value)], from);
            let h = &mut hits[src as usize];
            let keep = h.partition_point(|&l| l < from);
            h.truncate(keep);
            h.extend(fresh); // 升序接升序 (from 分界)
            true
        } else {
            false
        };

        // ③ 索引手术 (可见源才动; 隐藏源行不在掩码索引里)
        let mut rebuilt_fallback = false;
        if !hidden {
            if !tail_complete {
                // 旧末行条目先摘 (内容改判, ts 可能变); 摘不到 = 诚实兜底重建
                if !self.index.remove_row(src, (old_count - 1) as u32) {
                    rebuilt_fallback = true;
                }
            }
            if !rebuilt_fallback {
                self.index.insert_rows(src, &new_rows);
            }
        }

        // ④ 换入新文件快照 (行内容读取源); 旧快照**交线程释放** (T9 D3: 1 GiB
        // 映射解映射实测 50-160 ms, 每次追加都付一次 —— 不许压在 UI 线程上)。
        let old = std::mem::replace(&mut self.sources[src as usize].file, Arc::new(new_file));
        dispose_snapshot(old);

        if rebuilt_fallback {
            // 索引/ts 已一致 (ts 已重提), 掩码重建索引即可; trace 随重建作废
            // (rebuild_masked 同族纪律: 过滤串与行集不许跨重建漂)。
            self.rebuild_masked();
            return new_count - old_count;
        }

        // ⑤ 过滤行集重推 (插入漂移后唯一诚实形态) + 选中锚定/钉尾
        if trace_live {
            let filtered = {
                let hits = self.trace_hits.as_ref().expect("trace_live 保证缓存在");
                self.filtered_from_hits(hits)
            };
            self.filtered = Some(filtered);
        }
        if self.follow {
            self.pin_tail();
        } else {
            // 选中锚定新位置 (T9 D4 快路 + 评审 C2 修正): 位移模型 = 原位 −
            // (原位之前的摘除数, 至多 1) + (原位之前的插入数, ≤ k) ⇒ **旁观锚行**
            // 的新位置必落在 [原位−1, 原位+k] 里, 扫这一窗即可, 省掉 position_of
            // 的全表线性扫 (17M 行 158-250 ms/次, 每次追加都付)。
            //
            // **例外 (C2 实测反例)**: 锚行**本身**就是那条被补全改判的残行 ——
            // 它的键 (ts) 变了, 新位置可任意远 (窗口搜不到)。此时回落到
            // `position_of` 精确定位 (罕遇: 需用户正好选中残行且它被补全)。
            let removed = u64::from(!tail_complete && !hidden);
            let k = if hidden { 0 } else { new_rows.len() as u64 };
            let lo = self.selected.saturating_sub(removed);
            let hi = (self.selected + k).min(self.row_count().saturating_sub(1));
            self.selected = match anchor {
                Some(a) => (lo..=hi)
                    .find(|&p| self.row_at(p).is_some_and(|r| (r.src, r.line) == a))
                    .or_else(|| self.position_of(a.0, a.1))
                    .unwrap_or(lo),
                None => self.row_count().saturating_sub(1),
            };
        }
        new_count - old_count
    }

    /// 可见窗口 [from, from+count) 的行视图 (sync 每帧拷这段, 纪律见模块头)。
    pub fn window(&self, from: u64, count: u64) -> Vec<MergeRowView> {
        let end = (from + count).min(self.row_count());
        let mut out = Vec::with_capacity((end - from) as usize);
        for pos in from..end {
            let Some(row) = self.row_at(pos) else { break };
            let src = &self.sources[row.src as usize];
            out.push(MergeRowView {
                ts: row.ts,
                src: row.src,
                line: row.line,
                text: row_text(&src.file, row.line),
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
    /// 显示文本 (row_text: 解码 + trim_end; 选区字节偏移以它为准)。
    pub text: String,
}

/// 单源流水线的产出 (并行的结果槽)。
enum BuiltSource {
    /// 打开/探测/提取全通: 源 + 其时间戳向量。
    Ready(MergeSource, Vec<i64>),
    /// 拒收 (SPEC D2: 明示原因)。
    Rejected(PathBuf, &'static str),
    /// 开工前已被取消 (不计入源, 也不进拒收 —— 旧「源间 break」语义)。
    Skipped,
}

/// 单源流水线: 打开 → 探测通路 → (JSONL) 列发现 → 逐行提取时间戳。
/// 纯函数式 (只读文件), 线程安全 —— [`build_merge`] 逐源并行调它。
/// `saved` = 该源已知的会话参数 (按路径匹配); 无/不匹配 = 全默认
/// (偏移 0 / 时区本地 / 不隐藏)。
fn build_one_source(
    p: &std::path::Path,
    saved: Option<&MergeSourceState>,
    cancel: &AtomicBool,
) -> BuiltSource {
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return BuiltSource::Skipped;
    }
    let file = match LogFile::open(p) {
        Ok(f) => f,
        Err(_) => return BuiltSource::Rejected(p.to_path_buf(), "打开失败"),
    };
    let route = match timestamp::detect_route(&file) {
        Ok(r) => r,
        Err(reason) => return BuiltSource::Rejected(p.to_path_buf(), reason.label()),
    };
    let schema = if matches!(route, TsRoute::JsonlField(_)) {
        jsonl::discover_schema(&file).map(Arc::new)
    } else {
        None
    };
    let source = MergeSource {
        path: p.to_path_buf(),
        file: Arc::new(file),
        schema,
        route,
        offset_ms: saved.map_or(0, |s| clamp_offset_ms(s.offset_ms)),
        // 源时区: 有存值用存值, 否则**本地** (腿 D: 无 tz 格式必填, 默认本地)
        // —— 只影响无 tz 时间戳的解释; 显式 Z/±hh:mm 的行不受它动。
        tz_offset_ms: saved.map_or_else(local_tz_offset_ms, |s| clamp_tz_ms(s.tz_offset_ms)),
        hidden: saved.is_some_and(|s| s.hidden),
        stale: false,
    };
    let ts = merge::extract_timeline(
        &source.file,
        extractor(&source.route, source.tz_offset_ms, source.offset_ms),
    );
    BuiltSource::Ready(source, ts)
}

/// 时间参数钳制 (账本外部数据家规的**唯一真身**): 手造账本与 UI 手输同一条界。
/// ±10 年 (ms) —— 真用途是机器间钟差 (秒~小时级); `extractor` 是裸加法,
/// 不钳则天文值回绕 (release 静默错序 / debug panic)。
pub const MAX_ABS_OFFSET_MS: i64 = 10 * 365 * 24 * 3_600_000;
/// tz 钳制 = [`parse_tz_ms`] 同界 (|h| ≤ 23)。
pub const MAX_ABS_TZ_MS: i64 = 23 * 3_600_000;

/// 偏移钳制 (见 [`MAX_ABS_OFFSET_MS`])。
pub fn clamp_offset_ms(v: i64) -> i64 {
    v.clamp(-MAX_ABS_OFFSET_MS, MAX_ABS_OFFSET_MS)
}

/// 时区钳制 (见 [`MAX_ABS_TZ_MS`])。
pub fn clamp_tz_ms(v: i64) -> i64 {
    v.clamp(-MAX_ABS_TZ_MS, MAX_ABS_TZ_MS)
}

/// 归并作业交卷 (AsyncJob 载荷)。
pub struct MergeOutcome {
    pub sources: Vec<MergeSource>,
    pub ts: Vec<Vec<i64>>,
    pub index: MergeIndex,
    /// 被拒源 (探测失败 + 明示原因, SPEC D2)。
    pub rejected: Vec<(PathBuf, &'static str)>,
}

/// 归并 worker (后台线程): **逐源流水线并行** (打开 → 探测 → schema → 提取),
/// 最后串行归并。
///
/// **为什么并行** (T9 实测): 串行时 3×1GiB 的 1525ms 里提取占 1025ms —— 源之间
/// 零依赖, 各自读自己的文件、产自己的 ts 向量; 并行后总时间 ≈ max(单源流水线)。
/// 归并本身 (k-way min-head) 天然串行, 17M 行 ~215ms 不插桩。
/// 结果**按源序回填** (源序号是 tie-break 依据, 不许按完成序排)。
/// `cancel` 在**每源开工前**检查 (已在跑的源不中断, 同旧「源间」语义 ——
/// AsyncJob invalidate 同款惯例: 作废的交卷 UI 侧丢弃)。
pub fn build_merge(
    paths: &[PathBuf],
    saved: &[MergeSourceState],
    cancel: &AtomicBool,
) -> MergeOutcome {
    // 每源一条独立结果槽 (索引 = 源序); 线程数 = 源数 (源上限 8, 各持一个 mmap)。
    let mut built: Vec<Option<BuiltSource>> = Vec::with_capacity(paths.len());
    built.resize_with(paths.len(), || None);
    std::thread::scope(|scope| {
        let handles: Vec<_> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                // 会话参数按**路径**查 (加/减源后的序号漂移免疫)
                let s = saved.iter().find(|s| std::path::Path::new(&s.path) == p);
                scope.spawn(move || {
                    // 源线程 panic 不连坐: 该源记拒收 (诚实出声), 其余源照常交卷。
                    // (release 档 panic = abort, 本兜底只在 unwind 档生效 —— 有胜于无。)
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        build_one_source(p, s, cancel)
                    }))
                    .unwrap_or(BuiltSource::Rejected(p.to_path_buf(), "构建异常"));
                    (i, r)
                })
            })
            .collect();
        for h in handles {
            let (i, r) = h
                .join()
                .expect("源构建线程只可能正常返回 (panic 已在内部兜住)");
            built[i] = Some(r);
        }
    });
    let mut sources: Vec<MergeSource> = Vec::new();
    let mut ts: Vec<Vec<i64>> = Vec::new();
    let mut rejected: Vec<(PathBuf, &'static str)> = Vec::new();
    for slot in built.into_iter().flatten() {
        match slot {
            BuiltSource::Ready(s, v) => {
                sources.push(s);
                ts.push(v);
            }
            BuiltSource::Rejected(p, why) => rejected.push((p, why)),
            BuiltSource::Skipped => {} // 取消: 不收不拒 (旧 break 语义)
        }
    }
    let tls: Vec<SourceTimeline<'_>> = sources
        .iter()
        .zip(ts.iter())
        .map(|(s, v)| SourceTimeline {
            file: &s.file,
            ts: v.clone(),
        })
        .collect();
    // 有隐藏源 → 掩码建索引 (隐藏行不进时间线, 源序号不漂移; 会话恢复
    // 直接以保存的显隐建场, 落点零重建)。
    let index = match sources.iter().any(|s| s.hidden) {
        true => {
            let visible: Vec<bool> = sources.iter().map(|s| !s.hidden).collect();
            merge::build_index_masked(&tls, &visible)
        }
        false => merge::build_index(&tls),
    };
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
        // saturating: 调用方已钳制偏移 (set_time_params / build_one_source),
        // 此处再兜一层 —— 裸加法回绕是 release 静默错序。
        Some(t.saturating_add(offset_ms))
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

/// 合并行显示文本: 解码 (line_lossy, 与单文件 Raw 模式同口径) + `trim_end`。
/// **paint / 命中 / 复制 / 追踪值提取四处共用这一个口径** —— 选区字节偏移只对
/// 同一串文本自洽 (单点纪律, 不许各处各 trim)。
pub fn row_text(file: &LogFile, line: u32) -> String {
    file.line_lossy(u64::from(line)).trim_end().to_string()
}

// ---- 腿 E (T6): req_id 追踪 (选中值 → 跨源过滤) ----

/// 追踪作业交卷 (AsyncJob 载荷)。
pub struct TraceOutcome {
    /// per-source 命中文件行号 (与 sources 同序, 各自升序 —— run_filter 产出即有序)。
    pub hits: Vec<Vec<u64>>,
    /// per-source **扫描快照的行数** (Arc 快照行数恒定) —— 落地时与当前行数对账:
    /// 在途窗口内追加进来的行不在命中集里, 缺口须增量补滤 (T7 缝, R5 族),
    /// 否则追踪永久缺那段时间窗里进来的行。
    pub scanned: Vec<u64>,
    /// 追踪值 (底栏常驻显示 + T7 增量追加复用)。
    pub value: String,
    /// 发起行 (源, 行) —— 落地后选中锚回它 (本行含该值, 必在命中集)。
    pub anchor: (u32, u32),
    /// 全源过滤墙钟。
    pub elapsed: std::time::Duration,
}

/// 追踪值 → 过滤子句 (SPEC 腿 E/D3): **单条 Bare 字面子串**, 不经 parse_query ——
/// 值里的空白会被 split_whitespace 切碎、算子字符 (`=`/`>`/`<`) 会被误读成字段
/// 子句 (.log 行没有 `"key":` 形态, 字段子句在 .log 源零命中 = 追踪静默落空)。
/// 追踪语义 = 「这串字面量在整行里出现」, 不是查询语法。
pub fn trace_clause(value: &str) -> jsonl::Clause {
    jsonl::Clause::Bare(value.to_string())
}

/// JSONL 行的「取字段值」放大 (SPEC 腿 E): 选区 `[lo,hi)` **整体落在同一个字符串
/// 字面量内部** → 放大到整个字面量内容 (不含引号) —— 双击落在带空格的值里也能
/// 追到全值。转义序列保持原文: 过滤在原始行文本上做子串, 转义形态才是行里的
/// 真实字节, 不许 unescape。选区为空/贴引号边/跨字面量/不在任何字面量 → None
/// (调用方回落选区原文, 与 .log 同款)。
pub fn snap_jsonl_string(text: &str, lo: usize, hi: usize) -> Option<(usize, usize)> {
    if lo >= hi || hi > text.len() {
        return None;
    }
    let bytes = text.as_bytes();
    let mut content_start: Option<usize> = None; // 当前字面量内容起点 (开引号后)
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if content_start.is_some() => i += 1, // 转义对: 跳过下一字节
            b'"' => match content_start {
                None => content_start = Some(i + 1),
                Some(s) => {
                    if s <= lo && hi <= i {
                        return Some((s, i)); // [lo,hi) ⊆ 内容 [s,i)
                    }
                    content_start = None;
                }
            },
            _ => {}
        }
        i += 1;
    }
    None
}

/// 追踪 worker 体 (AsyncJob 任务闭包与测试注入共用 —— build_merge/apply_merge_sync
/// 同先例): per-source `run_filter` 各跑一遍 (腿 B「per-source 执行 + 重归并」)。
/// 返回 (命中表, **扫描快照行数**) —— 后者供落地时对账在途窗口内的新行 (T7 缝)。
pub fn trace_hits(files: &[Arc<LogFile>], clause: &jsonl::Clause) -> (Vec<Vec<u64>>, Vec<u64>) {
    let hits: Vec<Vec<u64>> = files
        .iter()
        .map(|f| jsonl::run_filter(f, std::slice::from_ref(clause)))
        .collect();
    let scanned = files.iter().map(|f| f.line_count()).collect();
    (hits, scanned)
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
        build_merge(paths, &[], &AtomicBool::new(false))
    }

    fn state_of(out: MergeOutcome) -> MergeState {
        MergeState::from_outcome(out)
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

    /// T9 并行流水线的**保序**锁 (最容易踩的坑): 结果按**源序**回填, 与线程
    /// 完成序无关 —— 源序号是归并 tie-break 的依据, 按完成序排 = 等 ts 行乱序。
    /// 构造: 首源大 (慢) / 次源小 (快), 完成序与源序相反。
    #[test]
    fn build_merge_keeps_source_order_under_parallel_build() {
        let big = temp_log("par-big", &b"l\n".repeat(50_000));
        // 大源得能探测出时间戳: 重写为带 ts 的行 (仍是大文件)
        let mut content = String::with_capacity(50_000 * 30);
        for i in 0..50_000 {
            let ts = if i == 0 {
                // 首行与次源首行**等 ts** (00:00.500) —— tie-break 判据
                "2026-09-27T00:00:00.500".to_string()
            } else {
                format!("2026-09-27T00:00:{:02}.{:03}", i % 60, i % 1000)
            };
            content.push_str(&format!("{ts}Z INFO big {i}\n"));
        }
        std::fs::write(&big, content.as_bytes()).unwrap();
        let small = temp_log(
            "par-small",
            b"2026-09-27T00:00:00.500Z INFO small0\n2026-09-27T00:00:01.500Z INFO small1\n2026-09-27T00:00:02.500Z INFO small2\n",
        );
        let out = build(&[big.clone(), small.clone()]);
        assert_eq!(out.sources.len(), 2);
        assert_eq!(out.sources[0].path, big, "首源仍是首源 (不按完成序排)");
        assert_eq!(out.sources[1].path, small);
        assert_eq!(out.ts[0].len(), 50_000, "ts 向量与源同序对齐");
        assert_eq!(out.ts[1].len(), 3);
        // 等 ts tie-break 用**源序号**: 两源首行同为 00:00.500, 索引前两位必是
        // 源0 → 源1 (保序错了这里就炸)
        assert_eq!(
            (out.index.row(0).src, out.index.row(1).src),
            (0, 1),
            "等 ts 行按源序号先小后大 (并行回填保序实证)"
        );
        std::fs::remove_file(&big).ok();
        std::fs::remove_file(&small).ok();
    }

    /// 取消语义 (T9 并行化后): 开工前已取消的源**不收也不拒** —— 拒收会走
    /// 「N 个源未加入 (探测失败)」告示, 取消不是那个意思; 交卷由 AsyncJob 代次丢弃。
    #[test]
    fn build_merge_cancel_skips_without_rejecting() {
        let a = temp_log(
            "can-a",
            b"2026-09-27T00:00:00Z a\n2026-09-27T00:00:01Z b\n2026-09-27T00:00:02Z c\n",
        );
        let b = temp_log(
            "can-b",
            b"2026-09-27T00:00:00Z a\n2026-09-27T00:00:01Z b\n2026-09-27T00:00:02Z c\n",
        );
        let out = build_merge(&[a.clone(), b.clone()], &[], &AtomicBool::new(true));
        assert!(out.sources.is_empty(), "取消: 源不收");
        assert!(out.rejected.is_empty(), "取消 ≠ 拒收 (不报探测失败)");
        assert_eq!(out.index.len(), 0);
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();
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

    // ---- 腿 E (T6): req_id 追踪 ----

    /// SPEC §5 指定锁 (两段): 串生成 (值 → 子句, 不经 parse_query 切碎) +
    /// 过滤链 (per-source 命中 → 合并行集 + 锚定 + 清除回全量)。
    #[test]
    fn trace_field_value_builds_filter() {
        // —— 段①: 串生成 —— 值含空白/算子也必须是**一条 Bare 字面量**;
        // 同一串过 parse_query 会碎成 字段子句+Bare (正是要避开的行为, 对照断言)。
        let c = trace_clause("req=a b>c");
        assert_eq!(c, jsonl::Clause::Bare("req=a b>c".to_string()));
        assert_ne!(jsonl::parse_query("req=a b>c"), vec![c.clone()]);

        // —— 段②: 过滤链 ——
        // cur.log (源0): 行1/行2 含 req_id=aaa111 (**单源双命中**, 逼两指针推进 —
        // 单命中时摘掉指针推进也能蒙对); a.log (源1): 无命中;
        // b.jsonl (源2): 行1 含 "req_id":"aaa111"。(探测 ≥3 行才收, fixture 各 3 行)
        let cur = temp_log(
            "trace-cur",
            b"2026-09-27T00:00:00Z INFO boot\n2026-09-27T00:00:02Z INFO request done req_id=aaa111\n2026-09-27T00:00:04Z INFO retry req_id=aaa111 again\n",
        );
        let a = temp_log(
            "trace-a",
            b"2026-09-27T00:00:01Z INFO other\n2026-09-27T00:00:03Z INFO noise\n2026-09-27T00:00:05Z INFO tail\n",
        );
        let b = temp_log(
            "trace-b.jsonl",
            br#"{"ts":"2026-09-27T00:00:01.500Z","msg":"enter"}
{"ts":"2026-09-27T00:00:02.500Z","msg":"handled","req_id":"aaa111"}
{"ts":"2026-09-27T00:00:06.000Z","msg":"end"}
"#,
        );
        let out = build(&[cur.clone(), a.clone(), b.clone()]);
        assert!(out.rejected.is_empty(), "拒收: {:?}", out.rejected);
        let mut st = state_of(out);
        assert_eq!(st.row_count(), 9);

        let files: Vec<Arc<LogFile>> = st.sources.iter().map(|s| Arc::clone(&s.file)).collect();
        let (hits, scanned) = trace_hits(&files, &trace_clause("aaa111"));
        assert_eq!(
            hits,
            vec![vec![1, 2], vec![], vec![1]],
            "per-source 命中行号"
        );
        assert_eq!(scanned, vec![3, 3, 3], "扫描快照行数随卷 (T7 缝对账原料)");

        let n = st.apply_trace(hits, (0, 1));
        assert_eq!(n, 3, "跨源一次滤出 (源0 双命中)");
        assert_eq!(st.row_count(), 3);
        let seq: Vec<(u32, u32)> = (0..st.row_count())
            .map(|p| st.row_at(p).map(|r| (r.src, r.line)).unwrap())
            .collect();
        assert_eq!(
            seq,
            vec![(0, 1), (2, 1), (0, 2)],
            "命中集按时间线序 (00:00:02 / 02.5 / 04)"
        );
        // 锚定: 选中回到发起行 (源0,行1) 在过滤空间的位置 = 0
        assert_eq!(st.selected, 0);
        assert_eq!(
            st.row_at(st.selected).map(|r| (r.src, r.line)),
            Some((0, 1))
        );

        // 清除: 回全量且选中锚行不动 ((0,1) 在全量空间仍在)
        st.clear_trace();
        assert_eq!(st.row_count(), 9);
        assert!(st.trace.is_none());
        assert_eq!(
            st.row_at(st.selected).map(|r| (r.src, r.line)),
            Some((0, 1)),
            "清除后选中锚行不漂"
        );
        std::fs::remove_file(&cur).ok();
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();
    }

    /// JSONL「取字段值」放大矩阵 (腿 E): 落在字符串字面量内部 → 整个内容;
    /// 引号边/跨字面量/无串区/空选区 → None (回落选区原文)。
    #[test]
    fn snap_jsonl_string_matrix() {
        let line = r#"{"msg":"request timeout after 3s","req_id":"abc","n":42}"#;
        let val = |s: &str| {
            let lo = line.find(s).unwrap();
            (lo, lo + s.len())
        };
        // 双击落在带空格值内部 → 放大到整个字段值
        let (lo, hi) = val("timeout");
        let (a, b) = snap_jsonl_string(line, lo, hi).expect("字面量内部应放大");
        assert_eq!(&line[a..b], "request timeout after 3s");
        // 落在 req_id 值上 (无空格) → 值本体 (与选区相同, 幂等)
        let (lo, hi) = val("abc");
        let (a, b) = snap_jsonl_string(line, lo, hi).unwrap();
        assert_eq!(&line[a..b], "abc");
        // 落在 key 上 → key 内容 (诚实: 选啥追啥, 不猜意图)
        let (lo, hi) = val("req_id");
        let (a, b) = snap_jsonl_string(line, lo, hi).unwrap();
        assert_eq!(&line[a..b], "req_id");
        // 数字值 (无引号) → None
        let (lo, hi) = val("42");
        assert_eq!(snap_jsonl_string(line, lo, hi), None);
        // 跨两个字面量 (从 msg 值拖到 req_id 值) → None
        let (lo, _) = val("timeout");
        let (_, hi) = val("abc");
        assert_eq!(snap_jsonl_string(line, lo, hi), None);
        // 空选区 / 越界 → None
        assert_eq!(snap_jsonl_string(line, 5, 5), None);
        assert_eq!(snap_jsonl_string(line, 0, line.len() + 1), None);
        // 转义引号: 字面量 `"a\"b"` 的内部选择 → 内容保持转义原文
        let esc = r#"{"msg":"a\"b"} "#;
        let lo = esc.find('a').unwrap();
        let (a, b) = snap_jsonl_string(esc, lo, lo + 1).expect("转义引号不终结字面量");
        assert_eq!(&esc[a..b], r#"a\"b"#, "转义序列保持原文 (子串过滤口径)");
    }

    /// 重建 (显隐/偏移) 作废追踪过滤: 过滤串与行集的对应不许跨重建漂。
    #[test]
    fn rebuild_masked_clears_trace() {
        let cur = temp_log(
            "traceclr-cur",
            b"2026-09-27T00:00:00Z INFO req_id=zz\n2026-09-27T00:00:01Z INFO other\n2026-09-27T00:00:03Z INFO more\n",
        );
        let a = temp_log(
            "traceclr-a",
            b"2026-09-27T00:00:02Z INFO tail\n2026-09-27T00:00:04Z INFO tail2\n2026-09-27T00:00:05Z INFO tail3\n",
        );
        let out = build(&[cur.clone(), a.clone()]);
        let mut st = state_of(out);
        let files: Vec<Arc<LogFile>> = st.sources.iter().map(|s| Arc::clone(&s.file)).collect();
        let (hits, _) = trace_hits(&files, &trace_clause("zz"));
        st.trace = Some("zz".to_string());
        st.apply_trace(hits, (0, 0));
        assert_eq!(st.row_count(), 1);
        st.rebuild_masked();
        assert!(
            st.filtered.is_none() && st.trace.is_none(),
            "重建即作废 (同族纪律)"
        );
        assert_eq!(st.row_count(), 6);
        std::fs::remove_file(&cur).ok();
        std::fs::remove_file(&a).ok();
    }

    /// 选区失效代次: 显隐重建**不**升 sel_rev ((源,行) 键稳定, 选区留着);
    /// carry (加/减源) 升 sel_rev (序号漂移)。
    #[test]
    fn sel_rev_only_bumps_on_source_set_change() {
        let cur = temp_log(
            "selrev-cur",
            b"2026-09-27T00:00:00Z INFO c0\n2026-09-27T00:00:02Z INFO c1\n2026-09-27T00:00:04Z INFO c2\n",
        );
        let a = temp_log(
            "selrev-a",
            b"2026-09-27T00:00:01Z INFO a0\n2026-09-27T00:00:03Z INFO a1\n2026-09-27T00:00:05Z INFO a2\n",
        );
        let out = build(&[cur.clone(), a.clone()]);
        assert_eq!(out.sources.len(), 2, "fixture 两源都在 (防空转)");
        let mut st = state_of(out);
        let rev0 = st.sel_rev;
        st.rebuild_masked();
        assert_eq!(st.sel_rev, rev0, "显隐/偏移重建不动选区代次");
        let out2 = build(std::slice::from_ref(&cur)); // 减源 → carry
        assert_eq!(out2.sources.len(), 1);
        let mut st2 = state_of(out2);
        carry_view_state(&st, &mut st2);
        assert_eq!(st2.sel_rev, rev0 + 1, "源集合变化 → 选区失效");
        std::fs::remove_file(&cur).ok();
        std::fs::remove_file(&a).ok();
    }

    // ---- 腿 F (T7): live-tail 合流 ----

    /// 追加字节到文件 (live 写入模拟; 收尾由调用方删)。
    fn append_bytes(p: &PathBuf, bytes: &[u8]) {
        let mut w = std::fs::OpenOptions::new().append(true).open(p).unwrap();
        std::io::Write::write_all(&mut w, bytes).unwrap();
    }

    /// 增量合流: 新行按 ts 进时间线 (尾追加 + 慢时钟回找两形态), 选中锚定不漂。
    #[test]
    fn append_source_merges_new_lines_in_order() {
        let pa = temp_log(
            "app-a",
            b"2026-09-27T00:00:00Z INFO a0\n2026-09-27T00:00:02Z INFO a1\n2026-09-27T00:00:04Z INFO a2\n",
        );
        let pb = temp_log(
            "app-b",
            b"2026-09-27T00:00:01Z INFO b0\n2026-09-27T00:00:03Z INFO b1\n2026-09-27T00:00:05Z INFO b2\n",
        );
        let out = build(&[pa.clone(), pb.clone()]);
        let mut st = state_of(out);
        assert_eq!(st.row_count(), 6);
        st.selected = 1; // (源1,行0) —— 锚定目标
        // 源0 追加两行: 一行尾追加 (06), 一行慢时钟 (01.500, 回找插入)
        append_bytes(
            &pa,
            b"2026-09-27T00:00:06Z INFO a3\n2026-09-27T00:00:01.500Z INFO late\n",
        );
        let new = LogFile::append_from(&st.sources[0].file, &pa).unwrap();
        let added = st.append_source(0, new);
        assert_eq!(added, 2);
        assert_eq!(st.row_count(), 8);
        let seq: Vec<(u32, u32)> = (0..st.row_count())
            .map(|p| st.row_at(p).map(|r| (r.src, r.line)).unwrap())
            .collect();
        assert_eq!(
            seq,
            vec![
                (0, 0),
                (1, 0),
                (0, 4), // late (ts 1500) 回找插在 b0 之后 a1 之前
                (0, 1),
                (1, 1),
                (0, 2),
                (1, 2),
                (0, 3), // 尾追加 (ts 6000 在 b2@5000 后)
            ],
            "新行按 ts 进时间线"
        );
        assert_eq!(
            st.row_at(st.selected).map(|r| (r.src, r.line)),
            Some((1, 0)),
            "插入漂移后选中锚行不漂"
        );
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
    }

    /// 末行补全 (无换行残行): 残行 ts 曾继承上一行 → 补全后改判真 ts;
    /// 索引先摘后插, (src,line) 无幽灵双条; 行数不变也落地 (added=0)。
    #[test]
    fn append_source_completes_partial_tail() {
        let pa = temp_log(
            "part-a",
            b"2026-09-27T00:00:00Z INFO a0\n2026-09-27T00:00:02Z INFO a1\n2026-09-27T00:00:0",
        );
        let pb = temp_log(
            "part-b",
            b"2026-09-27T00:00:01Z INFO b0\n2026-09-27T00:00:03Z INFO b1\n2026-09-27T00:00:05Z INFO b2\n",
        );
        let out = build(&[pa.clone(), pb.clone()]);
        assert!(out.rejected.is_empty());
        let mut st = state_of(out);
        // 先挂一条追踪: 常态补全必须走「摘旧插新」小路 —— 若退化成掩码重建
        // 兜底, trace 随重建作废 (rebuild_masked 同族纪律), 下面的存活断言红。
        // (A/B 实证: 摘除目标改错行, 结果经兜底路径仍对, 纯结果锁辨不出路径。)
        let files: Vec<Arc<LogFile>> = st.sources.iter().map(|s| Arc::clone(&s.file)).collect();
        let (hits, _) = trace_hits(&files, &trace_clause("a0"));
        st.trace = Some("a0".to_string());
        assert_eq!(st.apply_trace(hits, (0, 0)), 1);
        // 残行 (源0,行2) 继承 a1 的 ts=2000 —— 排在 b1@3000 前
        let pre: Vec<(u32, u32)> = (0..st.row_count())
            .map(|p| st.row_at(p).map(|r| (r.src, r.line)).unwrap())
            .collect();
        assert_eq!(pre, vec![(0, 0)], "追踪态过滤集只含 a0 行");
        st.clear_trace();
        // 看清裸序: 残行继承 a1 的 ts=2000 参与排序
        let pre: Vec<(u32, u32)> = (0..st.row_count())
            .map(|p| st.row_at(p).map(|r| (r.src, r.line)).unwrap())
            .collect();
        assert_eq!(
            pre,
            vec![(0, 0), (1, 0), (0, 1), (0, 2), (1, 1), (1, 2)],
            "残行继承 ts 参与排序"
        );
        // 再挂追踪 (补全落地时须在场, 充当路径判官)
        let (hits, _) = trace_hits(&files, &trace_clause("a0"));
        st.trace = Some("a0".to_string());
        st.apply_trace(hits, (0, 0));
        // 补全: 残行续完为真 ts 09 (无新增行)
        append_bytes(&pa, b"9Z INFO a2\n");
        let new = LogFile::append_from(&st.sources[0].file, &pa).unwrap();
        let added = st.append_source(0, new);
        assert_eq!(added, 0, "补全不加行");
        assert!(st.trace.is_some(), "常态补全走摘旧插新, 追踪存活 (路径锁)");
        assert_eq!(st.row_count(), 1, "追踪过滤集仍只含 a0 行");
        st.clear_trace();
        assert_eq!(st.row_count(), 6, "无幽灵双条 (摘旧插新)");
        let post: Vec<(u32, u32)> = (0..st.row_count())
            .map(|p| st.row_at(p).map(|r| (r.src, r.line)).unwrap())
            .collect();
        assert_eq!(
            post,
            vec![(0, 0), (1, 0), (0, 1), (1, 1), (1, 2), (0, 2)],
            "补全后 ts 改判 9000, 挪到尾位"
        );
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
    }

    /// 隐藏源: ts 照长 (再显示时掩码重建同源), 索引不插 (掩码语义不破)。
    #[test]
    fn append_source_hidden_grows_ts_but_not_index() {
        let pa = temp_log(
            "hid-a",
            b"2026-09-27T00:00:00Z INFO a0\n2026-09-27T00:00:02Z INFO a1\n2026-09-27T00:00:04Z INFO a2\n",
        );
        let pb = temp_log(
            "hid-b",
            b"2026-09-27T00:00:01Z INFO b0\n2026-09-27T00:00:03Z INFO b1\n2026-09-27T00:00:05Z INFO b2\n",
        );
        let out = build(&[pa.clone(), pb.clone()]);
        let mut st = state_of(out);
        st.sources[0].hidden = true;
        st.rebuild_masked();
        assert_eq!(st.row_count(), 3, "掩码后只剩源1");
        append_bytes(&pa, b"2026-09-27T00:00:06Z INFO a3\n");
        let new = LogFile::append_from(&st.sources[0].file, &pa).unwrap();
        st.append_source(0, new);
        assert_eq!(st.row_count(), 3, "隐藏源不进索引");
        assert_eq!(st.ts[0].len(), 4, "ts 照长 (再显示同源)");
        st.sources[0].hidden = false;
        st.rebuild_masked();
        assert_eq!(st.row_count(), 7, "恢复显示 = 掩码重建带上新行");
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
    }

    /// 追踪过滤随行 (T6×T7 组合): 追加的命中行进过滤集, 非命中行不进;
    /// follow 钉尾覆盖锚定。
    #[test]
    fn append_source_extends_trace_filter_and_pins_tail() {
        let pa = temp_log(
            "tapp-a",
            b"2026-09-27T00:00:00Z INFO boot\n2026-09-27T00:00:02Z INFO req_id=zz first\n2026-09-27T00:00:04Z INFO idle\n",
        );
        let pb = temp_log(
            "tapp-b",
            b"2026-09-27T00:00:01Z INFO b0\n2026-09-27T00:00:03Z INFO b1\n2026-09-27T00:00:05Z INFO b2\n",
        );
        let out = build(&[pa.clone(), pb.clone()]);
        let mut st = state_of(out);
        let files: Vec<Arc<LogFile>> = st.sources.iter().map(|s| Arc::clone(&s.file)).collect();
        let (hits, _) = trace_hits(&files, &trace_clause("zz"));
        st.trace = Some("zz".to_string());
        let n = st.apply_trace(hits, (0, 1));
        assert_eq!(n, 1);
        assert_eq!(st.row_count(), 1);
        // 追加: 一行命中 (zz), 一行不命中
        append_bytes(
            &pa,
            b"2026-09-27T00:00:06Z INFO req_id=zz second\n2026-09-27T00:00:07Z INFO noise\n",
        );
        let new = LogFile::append_from(&st.sources[0].file, &pa).unwrap();
        st.append_source(0, new);
        assert_eq!(st.row_count(), 2, "新命中行进过滤集");
        let last = st.row_at(1).unwrap();
        assert_eq!((last.src, last.line), (0, 3), "命中的是新行 (源0,行3)");
        // follow 钉尾
        st.follow = true;
        append_bytes(&pa, b"2026-09-27T00:00:08Z INFO req_id=zz third\n");
        let new = LogFile::append_from(&st.sources[0].file, &pa).unwrap();
        st.append_source(0, new);
        assert_eq!(st.row_count(), 3);
        assert_eq!(st.selected, st.row_count() - 1, "follow 钉时间线尾");
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
    }

    // ─── T8: 合并组会话载荷 (SPEC D4) ─────────────────────────────────

    /// 评审 C1 (安全审计 Required 同源): 命中表按**某一时刻的源集合**编号 ——
    /// 源集合换过之后按 src 索引会越界 (release = abort)。运行期闸必须把它
    /// 降级成「丢弃追踪」而不是 panic; 合法表不许误伤。
    #[test]
    fn apply_trace_rejects_mismatched_hits_without_panic() {
        let pa = temp_log(
            "c1g-a",
            b"2026-09-27T00:00:00Z a0\n2026-09-27T00:00:01Z a1\n2026-09-27T00:00:02Z a2\n",
        );
        let pb = temp_log(
            "c1g-b",
            b"2026-09-27T00:00:03Z b0\n2026-09-27T00:00:04Z b1\n2026-09-27T00:00:05Z b2\n",
        );
        let out = build(&[pa.clone(), pb.clone()]);
        let mut st = state_of(out);
        assert_eq!(st.sources.len(), 2);
        // 3 条命中表 (旧的三源集合产物) → 丢弃, 不越界
        let stale = vec![vec![0u64], vec![1u64], vec![2u64]];
        assert_eq!(st.apply_trace(stale, (0, 0)), 0, "源集合不符 → 零命中");
        assert!(
            st.trace.is_none() && st.filtered.is_none() && st.trace_hits.is_none(),
            "不符时清追踪态"
        );
        // 合法表照旧工作 (不误伤): 每源各命中 1 行 → 合并行 2 条
        let ok = vec![vec![0u64], vec![2u64]];
        assert_eq!(st.apply_trace(ok, (0, 0)), 2, "同源集的表照常落地");
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
    }

    /// 评审 C2 (代码评审 Critical): 锚行**自身**是那条被补全改判的残行时,
    /// 它的键变了、位置可任意远 —— 有界窗口搜不到, 必须回落精确定位,
    /// 不许 `unwrap_or(lo)` 静默跳到别的行 (违背 (源,行) 锚定不变式)。
    #[test]
    fn append_source_keeps_selection_when_anchor_is_rejudged_tail() {
        // 源0: A0@0, A1@10, 末行**无换行残件** (解析失败 → 继承 A1 的 ts=10)
        let pa = temp_log(
            "c2-a",
            b"2026-09-27T00:00:00Z A0\n2026-09-27T00:00:10Z A1\n2026-09-27T00:01:4",
        );
        // 源1: ts 5,15,...,75 (8 行) —— 交错出 A0 B0 A1 A2 B1 … 的序
        let mut b = Vec::new();
        for i in 0..8 {
            b.extend_from_slice(format!("2026-09-27T00:00:{:02}Z B{i}\n", 5 + i * 10).as_bytes());
        }
        let pb = temp_log("c2-b", &b);
        let out = build(&[pa.clone(), pb.clone()]);
        let mut st = state_of(out);
        // (0,2) 此刻继承 ts=10, 落在位 3
        let at3 = st.row_at(3).unwrap();
        assert_eq!((at3.src, at3.line), (0, 2), "残件行在位 3 (继承 ts)");
        st.selected = 3;
        // 补全残件 (真 ts=00:01:40) + 追加一行 (00:01:50) —— 锚行跳到时间线尾
        append_bytes(&pa, b"0.000Z A2done\n2026-09-27T00:01:50Z A3\n");
        let new = LogFile::append_from(&st.sources[0].file, &pa).unwrap();
        st.append_source(0, new);
        let r = st.row_at(st.selected).unwrap();
        assert_eq!(
            (r.src, r.line),
            (0, 2),
            "选中仍钉 (0,2) (窗口外 → 回落精确定位), 实得位 {} 的 {:?}",
            st.selected,
            (r.src, r.line)
        );
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
    }

    /// 快照保序保值 (保存侧): 源路径/时间参数/显隐逐项对得上。
    #[test]
    fn snapshot_group_captures_sources_in_order() {
        let pa = temp_log(
            "snap-a",
            b"2026-09-27T00:00:00Z a0\n2026-09-27T00:00:02Z a1\n2026-09-27T00:00:04Z a2\n",
        );
        let pb = temp_log(
            "snap-b",
            b"2026-09-27T00:00:01Z b0\n2026-09-27T00:00:03Z b1\n2026-09-27T00:00:05Z b2\n",
        );
        let out = build(&[pa.clone(), pb.clone()]);
        let mut st = state_of(out);
        st.set_time_params(1, -3000, 0);
        st.sources[0].hidden = true;
        let g = st.snapshot_group();
        assert_eq!(g.sources.len(), 2);
        assert_eq!(g.sources[0].path, pa.to_string_lossy());
        assert_eq!(g.sources[1].path, pb.to_string_lossy());
        assert!(g.sources[0].hidden);
        assert!(!g.sources[1].hidden);
        assert_eq!(
            (g.sources[1].offset_ms, g.sources[1].tz_offset_ms),
            (-3000, 0)
        );
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
    }

    /// 套回 (恢复侧): 保存的偏移改变排序、显隐改变行集, 一次重归并;
    /// 返回值 = 有变化的源数。无变化载荷 = 零重建 (applied = 0)。
    #[test]
    fn apply_saved_params_replays_offsets_and_hidden() {
        let pa = temp_log(
            "replay-a",
            b"2026-09-27T00:00:00Z a0\n2026-09-27T00:00:02Z a1\n2026-09-27T00:00:04Z a2\n",
        );
        let pb = temp_log(
            "replay-b",
            b"2026-09-27T00:00:01Z b0\n2026-09-27T00:00:03Z b1\n2026-09-27T00:00:05Z b2\n",
        );
        let out = build(&[pa.clone(), pb.clone()]);
        let mut st = state_of(out);
        assert_eq!(st.row_count(), 6);
        let g = MergeGroup {
            sources: vec![
                MergeSourceState {
                    path: pa.to_string_lossy().into_owned(),
                    offset_ms: 0,
                    tz_offset_ms: st.sources[0].tz_offset_ms, // 不变 (归一本机 tz)
                    hidden: false,
                },
                MergeSourceState {
                    path: pb.to_string_lossy().into_owned(),
                    offset_ms: -3_000, // b 源拨快 3s: b0@01-3s 排到最前
                    tz_offset_ms: st.sources[1].tz_offset_ms,
                    hidden: true, // 且隐藏 → 行集只剩 a 源
                },
            ],
        };
        let applied = st.apply_saved_params(&g);
        assert_eq!(applied, 1, "只有源 1 有变化");
        assert_eq!(st.row_count(), 3, "隐藏源行不进索引");
        assert!(st.sources[1].hidden);
        assert_eq!(st.sources[1].offset_ms, -3000);
        // 空转载荷 = 零重建 (不打扰现场)
        let g2 = st.snapshot_group();
        assert_eq!(st.apply_saved_params(&g2), 0);
        std::fs::remove_file(&pa).ok();
        std::fs::remove_file(&pb).ok();
    }
}
