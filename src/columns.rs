//! @author 十四叔
//! @date 2026/09/23
//!
//! 列配置三件套 (SPEC-v1x-table-column-config D1/D6): 用户态列宽/显隐/列序模型 + 对账。
//!
//! 引擎 `jsonl::Column.width_chars` 是发现期采样**建议宽**, 只读 —— 用户摆法的真身在
//! 这里, 不回写引擎字段 (`discover_schema` 重跑/换文件零互踩)。列名 exact 匹配
//! (JSON 键名大小写敏感; 同名 key 在 `discover_schema` find-or-push 里收敛进同列,
//! **列名唯一是构造保证**, 故列名可作 key)。

use std::collections::HashMap;

/// 手动宽 clamp (px, D6): event 无 `TextBatch` 量不了字符宽, 常量夹取「别拖没/别拖爆」。
pub const MIN_COL_W: f32 = 40.0;
/// 同上, 上限。
pub const MAX_COL_W: f32 = 512.0;

/// 一个文件的列摆法 (D1): 三件套 = 序 + 隐 + 宽, 列名 exact key。
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnConfig {
    /// 全列显示序 (**含隐藏列**)。对账后必含全部 schema 列名。
    pub order: Vec<String>,
    /// 隐藏列名 (order 的子集)。
    pub hidden: Vec<String>,
    /// 手动宽 px; 无条目 = 采样建议宽。
    pub widths: HashMap<String, f32>,
}

impl Default for ColumnConfig {
    /// 空摆法 (未载入/无 schema 态); 有 schema 时用 [`Self::from_schema`]。
    fn default() -> Self {
        Self {
            order: Vec::new(),
            hidden: Vec::new(),
            widths: HashMap::new(),
        }
    }
}

impl ColumnConfig {
    /// 缺省摆法 = schema 首见序 + 全显 + 采样宽 (D1)。
    pub fn from_schema(schema_names: &[String]) -> Self {
        Self {
            order: schema_names.to_vec(),
            hidden: Vec::new(),
            widths: HashMap::new(),
        }
    }

    /// 显示列序 = `order` 过滤隐藏 (D1)。调用前提: 已与当前 schema 对账。
    pub fn visible_names(&self) -> impl Iterator<Item = &str> {
        self.order
            .iter()
            .filter(|n| !self.hidden.contains(n))
            .map(String::as_str)
    }

    pub fn is_hidden(&self, name: &str) -> bool {
        self.hidden.iter().any(|n| n == name)
    }

    /// 有效宽 = 手动宽 (已夹取) 否则采样建议宽 (D1)。
    pub fn width_of(&self, name: &str, sampled: f32) -> f32 {
        match self.widths.get(name) {
            Some(w) => clamp_w(*w),
            None => sampled,
        }
    }

    /// 拖宽提交/弹层改宽。仅收已知列名; 非有限值拒收。返回是否落账。
    pub fn set_width(&mut self, name: &str, w: f32) -> bool {
        if !w.is_finite() || !self.order.iter().any(|n| n == name) {
            return false;
        }
        self.widths.insert(name.to_string(), clamp_w(w));
        true
    }

    /// 显隐切换的定向版。**≥1 可见列守卫** (D6): 关最后一可见列拒绝且零变更。
    /// 未知列名 / 幂等重复设置同样零变更地如实返回。
    pub fn set_hidden(&mut self, name: &str, hide: bool) -> bool {
        if !self.order.iter().any(|n| n == name) {
            return false;
        }
        let currently_hidden = self.is_hidden(name);
        if currently_hidden == hide {
            return true;
        }
        if hide {
            // 可见数按**过滤计数** (评审 C1): 脏 `hidden` 可比 `order` 长
            // (外部 columns.json 手造重复/失配), 相减会 usize 下溢 ——
            // debug 崩 / release 回绕成极大数使 `<= 1` 失效, 绕过 D6 守卫。
            let visible = self
                .order
                .iter()
                .filter(|n| !self.hidden.contains(n))
                .count();
            if visible <= 1 {
                return false; // 最后一可见列不可藏 (D6)
            }
            self.hidden.push(name.to_string());
        } else {
            self.hidden.retain(|n| n != name);
        }
        true
    }

    /// 弹层开关行用: 可见 → 隐藏, 隐藏 → 可见 (最后一可见列守卫同 `set_hidden`)。
    pub fn toggle_hidden(&mut self, name: &str) -> bool {
        self.set_hidden(name, !self.is_hidden(name))
    }

    /// 拖拽换位落点 (裁定②): `order` 内 from → to (to 按移除后下标解释,
    /// 与「把列插到第 to 个可见缝隙」的手势语义一致)。越界拒收。
    pub fn move_column(&mut self, from: usize, to: usize) -> bool {
        if from >= self.order.len() || to >= self.order.len() {
            return false;
        }
        let name = self.order.remove(from);
        self.order.insert(to, name);
        true
    }

    /// 拖拽换位落点 (T4/裁定②): 把 `name` 移到 `before` 之前; `before` 为
    /// `None` 或失配 = 排尾。`before == name` = 落点即原位 (缝隙语义 no-op);
    /// 未知名拒收如实返回。
    pub fn move_name_before(&mut self, name: &str, before: Option<&str>) -> bool {
        if before == Some(name) {
            return true;
        }
        let Some(from) = self.order.iter().position(|n| n == name) else {
            return false;
        };
        let to = match before {
            Some(b) => self
                .order
                .iter()
                .position(|n| n == b)
                .unwrap_or(self.order.len()),
            None => self.order.len(),
        };
        let name_s = self.order.remove(from);
        let to = if to > from { to - 1 } else { to };
        self.order.insert(to.min(self.order.len()), name_s);
        true
    }

    /// 「恢复默认」(D3): 整体回首见序 + 全显 + 采样宽。
    pub fn reset(&mut self, schema_names: &[String]) {
        *self = Self::from_schema(schema_names);
    }

    /// 对账 merge (D4): schema 是采样产物, 同文件重开列集合可能变 ——
    /// 增列按 schema 首见序补尾, 减列剔除, 失配 key **静默丢弃**; 存活列的
    /// 记忆序保持。空表 = 清空。
    ///
    /// 同时是**不变量收口点** (评审 C1/C2/C3): order/hidden 去重 (手造重复
    /// 不画两遍)、hidden/widths ⊆ order、merge 后保 ≥1 可见列 (D6)。
    pub fn merge_with_schema(&mut self, schema_names: &[String]) {
        let mut order: Vec<String> = Vec::new();
        for n in std::mem::take(&mut self.order)
            .into_iter()
            .chain(schema_names.iter().cloned())
        {
            if schema_names.contains(&n) && !order.contains(&n) {
                order.push(n);
            }
        }
        let mut hidden: Vec<String> = Vec::new();
        for n in std::mem::take(&mut self.hidden) {
            if order.contains(&n) && !hidden.contains(&n) {
                hidden.push(n);
            }
        }
        self.hidden = hidden;
        self.widths.retain(|n, _| order.contains(n));
        // ≥1 可见兜底 (评审 C2): 唯一可见列被 schema 剔除 → 取消隐藏首列
        if !order.is_empty() && order.iter().all(|n| self.hidden.contains(n)) {
            self.hidden.retain(|n| *n != order[0]);
        }
        self.order = order;
    }
}

/// 宽夹取 (D6)。非有限值在入口拒收, 不会流到这里。
fn clamp_w(w: f32) -> f32 {
    w.clamp(MIN_COL_W, MAX_COL_W)
}

/// per-文件条目 LRU 上限 (D4): 条目极小, 但配置文件不无限长。
pub const FILE_CAP: usize = 64;

/// per-文件书签上限 (SPEC-v1x-bookmark-persist D3): 正常使用永远碰不到, 刷屏有闸。
/// 满员拒绝新增并提示 (「至少保留一列可见」同款守卫风格); 存量超限手造数据在
/// 归一时截断保升序前 [`MAX_BOOKMARKS`]。
pub const MAX_BOOKMARKS: usize = 256;

/// per-路径命名会话上限 (SPEC-v1x-workspace-sessions D6): 同款「碰不到、有闸」;
/// 新增满员拒绝由调用方提示, 存量超限手造数据归一时截断保前 N。
/// **取 12 = 与弹层可视封顶 `POPOVER_ROWS_MAX` 取齐**（评审 M3: 上限 > 可视
/// 会让超出部分静默不可达 —— 可达集恒等于允许集, 才配叫「有闸」）。
pub const MAX_SESSIONS_PER_PATH: usize = 12;

/// 会话展开行号上限 (收编截断保前 N; 防手造天文数组, 书签同哲学)。
/// 正常使用 (逐个点开子行) 永远碰不到。
pub const MAX_EXPANDS: usize = 1024;

/// sessions 数组收集硬顶 (防手造天文数组, 书签先例: 超顶按收集序放弃)。
const SESSIONS_HARD_CAP: usize = MAX_SESSIONS_PER_PATH * FILE_CAP * 4;

/// 生产路径: config 目录下 `state.json` (`license.key` 同目录独立文件先例)。
/// **2026-09-24 改名一次到位**（bookmark-persist 既定裁定: 腿四扩多套会话时
/// 统一改名）——账本自此 = 列摆法 + 书签 + 命名会话, 旧名 `columns.json` 只
/// 读不写（迁移见 `LogApp::load_state_account`）。
pub fn default_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_default()
        .join("danqing-log")
        .join("state.json")
}

/// per-文件记忆的持久化集合 (D4 + SPEC-v1x-workspace-sessions D3): 独立账本
/// `state.json`, per-**路径** key, files 段 LRU [`FILE_CAP`] 条 + sessions 段
/// 命名会话 (用户自管, 不吃 LRU)。
///
/// 磁盘形状 (serde_json::Value 手拼 —— serde derive 不在依赖, 零新依赖红线):
/// `{ "files": [ { "path", "updated", "order", "hidden", "widths", "bookmarks" } ],
///    "sessions": [ { "path", "name", "filter", "search", "order", "hidden",
///                    "widths", "expands", "updated" } ] }`。
/// 两段**恒写** (空 = 空数组, roundtrip 全等好判)。
/// **损坏/缺失 → 空集合且不写回**（下次保存才写好, load 永不覆盖用户文件）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColumnFiles {
    pub entries: Vec<FileEntry>,
    /// 命名工作台会话 (平铺含 `path`; 收编态见 [`normalize_sessions`])。
    pub sessions: Vec<SessionEntry>,
}

/// 一个文件的记忆状态 (列摆法 + 书签, SPEC-v1x-bookmark-persist D1)。
/// **腿四接缝已兑现** (2026-09-24): 命名会话由 [`SessionEntry`] 承载, 其结构
/// **无** `bookmarks` 字段 —— 书签不随会话导出/切换 (Open Q1) 是结构保证,
/// 不靠运行时 strip。
#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    /// 文件路径 (exact key)。
    pub path: String,
    /// 摆法。
    pub config: ColumnConfig,
    /// 书签文件行号 (**升序去重**收编态, 见 [`normalize_bookmarks`])。
    pub bookmarks: Vec<u64>,
    /// 最近使用时刻 (调用方给 epoch 秒), LRU 依据。
    pub updated: u64,
}

/// 命名工作台会话 (SPEC-v1x-workspace-sessions D1/D2): per-路径命名快照,
/// 载荷四样 = 过滤查询串 + 搜索查询串 + 列摆法 + 展开行号表 (**应用时重建真相**,
/// 不存行集/子行内容)。**无书签字段** —— 见 [`FileEntry`] 腿四接缝注。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionEntry {
    /// 文件路径 (exact key, 与 [`FileEntry`] 同粒度; `put_sessions_for_path` 归参)。
    pub path: String,
    /// 会话名 (路径内唯一; 收编同名保首见)。
    pub name: String,
    /// 过滤查询串 (`filter_applied` 口径; 应用时走全链重跑)。
    pub filter: String,
    /// 搜索查询串 (Open Q2: 当前查询, 无多条历史)。
    pub search: String,
    /// 列摆法快照 (复用 [`ColumnConfig`]; 应用 = 写穿 per-file 条目)。
    pub config: ColumnConfig,
    /// 已展开的**文件行号** (**升序去重**收编态, 见 [`normalize_expands`])。
    pub expands: Vec<u64>,
    /// 保存时刻 (调用方给 epoch 秒)。
    pub updated: u64,
}

impl ColumnFiles {
    /// 读盘: 缺失/坏 JSON/坏形状一律回空集合, **零写盘**。
    pub fn load_from(path: &std::path::Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        Self::from_json(&bytes)
    }

    /// 解析 bytes; 顶层不可辨 → 空; 段/条目坏 → **丢段丢条不丢账**
    /// (D4 失配同哲学: `files`/`sessions` 互不牵连)。
    pub fn from_json(bytes: &[u8]) -> Self {
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            return Self::default();
        };
        let mut files = Self::default();
        if let Some(arr) = v.get("files").and_then(|f| f.as_array()) {
            for item in arr {
                if let Some(entry) = entry_from_value(item) {
                    files.entries.push(entry);
                }
            }
        }
        if let Some(arr) = v.get("sessions").and_then(|s| s.as_array()) {
            for (i, item) in arr.iter().enumerate() {
                if i >= SESSIONS_HARD_CAP {
                    break; // 超顶按收集序放弃 (书签先例)
                }
                if let Some(s) = session_from_value(item) {
                    files.sessions.push(s);
                }
            }
            files.sessions = normalize_sessions(std::mem::take(&mut files.sessions));
        }
        files
    }

    /// 写盘 (整文件覆盖写, **temp + rename 原子落盘**; 同源一次写,
    /// `config::Config::save_to` 同哲学)。评审 Critical: `fs::write` 直写
    /// 中途被杀会留半截坏 JSON —— 再触发保存侧覆盖即抹掉其余全部记忆;
    /// rename 没有半截窗口。
    pub fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        let arr: Vec<serde_json::Value> = self.entries.iter().map(entry_to_value).collect();
        let sess: Vec<serde_json::Value> = self.sessions.iter().map(session_to_value).collect();
        let v = serde_json::json!({ "files": arr, "sessions": sess });
        let bytes = serde_json::to_vec_pretty(&v).expect("Value 序列化不会失败");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, path)
    }

    pub fn get(&self, path: &str) -> Option<&ColumnConfig> {
        self.get_entry(path).map(|e| &e.config)
    }

    /// 整条目访问 (书签持久化后载入要拿列摆法**和**书签, 只给 config 不够)。
    pub fn get_entry(&self, path: &str) -> Option<&FileEntry> {
        self.entries.iter().find(|e| e.path == path)
    }

    /// 记入/更新一条并按 `updated` 保 LRU [`FILE_CAP`] (超限挤掉最旧)。
    /// 书签入库前过 [`normalize_bookmarks`] (升序去重截断, 收编态见 D1)。
    /// **淘汰保护** (评审 R2): 书签是用户内容不是偏好 —— 无书签条目先挤,
    /// 带书签条目最后挤 (同级再按 `updated` 最旧); 全带书签才按纯 LRU。
    /// 被挤条目**原样返回** (调用方可提示留痕)。
    /// 同秒并列时**本次条目**视为最新 (排序次键), 否则稳定排序把它排在同秒组尾、
    /// `truncate` 反而挤掉刚 put 的自己 (评审 C5)。
    pub fn put(&mut self, mut entry: FileEntry) -> Vec<FileEntry> {
        entry.bookmarks = normalize_bookmarks(std::mem::take(&mut entry.bookmarks));
        let path = entry.path.clone();
        self.entries.retain(|e| e.path != path);
        self.entries.push(entry);
        self.entries.sort_by_key(|e| {
            (
                e.bookmarks.is_empty(),
                std::cmp::Reverse(e.updated),
                e.path != path,
            )
        });
        if self.entries.len() > FILE_CAP {
            self.entries.split_off(FILE_CAP)
        } else {
            Vec::new()
        }
    }

    /// 该路径的命名会话 (载入收编态原样返回副本)。
    pub fn sessions_for_path(&self, path: &str) -> Vec<SessionEntry> {
        self.sessions
            .iter()
            .filter(|s| s.path == path)
            .cloned()
            .collect()
    }

    /// **可辨账本**谓词 (评审 M1/M2 单一收口): 两段至少一段有有效条目。
    /// 「files 坏条 + sessions 完好」是合法容错结果 (丢段不丢账) —— 不算损坏;
    /// 损坏备份与迁移回落**共用本判据**, 不许各算一份。
    /// 已知残留: 合法空账 `{"files":[],"sessions":[]}` 会被误判不可辨 ——
    /// 备份侧零数据损失 (顶多多一个 .bak), 回落侧会被旧名接住 (正是迁移语义)。
    pub fn is_recognizable(&self) -> bool {
        !self.entries.is_empty() || !self.sessions.is_empty()
    }

    /// 替换**该路径**的命名会话切片 (他路径不动 —— 保存会话不许抹掉别处会话,
    /// bookmark-persist Critical「读改写覆盖抹全记忆」同族面的会话版封口)。
    /// 条目 `path` 归本参数; 入库过 [`normalize_sessions`] (收编态进收编态出)。
    pub fn put_sessions_for_path(&mut self, path: &str, sessions: Vec<SessionEntry>) {
        self.sessions.retain(|s| s.path != path);
        for mut s in sessions {
            s.path = path.to_string();
            self.sessions.push(s);
        }
        self.sessions = normalize_sessions(std::mem::take(&mut self.sessions));
    }
}

/// 书签收编 (D1): 去重 + 升序 + 超 [`MAX_BOOKMARKS`] 截断保前 N。
/// 磁盘形状的**唯一**归一入口 (`put` / `entry_from_value` 同用) —— 收编态进,
/// 收编态出, roundtrip 全等好判。
fn normalize_bookmarks(mut raw: Vec<u64>) -> Vec<u64> {
    raw.sort_unstable();
    raw.dedup();
    raw.truncate(MAX_BOOKMARKS);
    raw
}

/// 展开行号收编 (D2): 升序去重 + 超 [`MAX_EXPANDS`] 截断保前 N (书签同哲学)。
fn normalize_expands(mut raw: Vec<u64>) -> Vec<u64> {
    raw.sort_unstable();
    raw.dedup();
    raw.truncate(MAX_EXPANDS);
    raw
}

/// 会话名长度上限 (正常使用碰不到; 超长截断 = 收编语义)。
pub const MAX_SESSION_NAME_LEN: usize = 64;

/// 会话名归一 (评审 M10, 保存/载入同式): trim + 剔 ASCII 控制字符（换行会打进
/// 底栏 notice / 列表行）+ 截 [`MAX_SESSION_NAME_LEN`] 字符（Unicode 安全）。
/// 空结果 = 不可用名（调用方丢条/拒绝）。
pub fn clean_session_name(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_SESSION_NAME_LEN)
        .collect()
}

/// 会话收编 (D6, 磁盘形状唯一归一入口): 名归一后空名丢条; (path, name) 去重
/// 保首见; 每路径超 [`MAX_SESSIONS_PER_PATH`] 截断保前 N (存量手造超限 =
/// 收编语义); 行号走 [`normalize_expands`]。收编态进收编态出。
fn normalize_sessions(raw: Vec<SessionEntry>) -> Vec<SessionEntry> {
    let mut seen: HashMap<(String, String), ()> = HashMap::new();
    let mut per_path: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<SessionEntry> = Vec::new();
    for mut s in raw {
        s.name = clean_session_name(&s.name);
        if s.name.is_empty() {
            continue;
        }
        if seen.insert((s.path.clone(), s.name.clone()), ()).is_some() {
            continue;
        }
        let count = per_path.entry(s.path.clone()).or_insert(0);
        if *count >= MAX_SESSIONS_PER_PATH {
            continue;
        }
        *count += 1;
        s.expands = normalize_expands(std::mem::take(&mut s.expands));
        out.push(s);
    }
    out
}

/// 条目 → Value (手拼; 与 [`entry_from_value`] 互为往返)。
/// `widths` 键**排序后写入** (评审 Nit: HashMap 迭代序随进程变, 不排的话
/// 同一摆法两次落盘字节面就不同 —— 用户 diff/sync columns.json 徒增噪音)。
/// `bookmarks` 恒写 (空 = 空数组, 不省略键 —— roundtrip 全等好判, D1)。
/// widths → Value (键**排序后写入** —— HashMap 迭代序随进程变, 不排则同摆法
/// 两次落盘字节面就不同, 评审 Nit 先例)。`entry_to_value`/`session_to_value`
/// 同用 (列三字段同模型)。
fn widths_to_value(widths: &HashMap<String, f32>) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    let mut keys: Vec<&String> = widths.keys().collect();
    keys.sort();
    for k in keys {
        map.insert(k.clone(), serde_json::json!(widths[k] as f64));
    }
    serde_json::Value::Object(map)
}

/// Value → widths (逐键容错: 非数值 / 非有限 (如 1e308 → f32 inf) / 失配键 →
/// **丢键不丢条**, 评审 C1 先例)。`entry_from_value`/`session_from_value` 同用。
fn widths_from_value(
    obj: &serde_json::Map<String, serde_json::Value>,
    order: &[String],
) -> HashMap<String, f32> {
    let mut widths = HashMap::new();
    if let Some(w) = obj.get("widths").and_then(|w| w.as_object()) {
        for (k, num) in w {
            let Some(w) = num.as_f64().map(|f| f as f32).filter(|f| f.is_finite()) else {
                continue;
            };
            if order.contains(k) {
                widths.insert(k.clone(), w);
            }
        }
    }
    widths
}

fn entry_to_value(e: &FileEntry) -> serde_json::Value {
    serde_json::json!({
        "path": e.path,
        "updated": e.updated,
        "order": e.config.order,
        "hidden": e.config.hidden,
        "widths": widths_to_value(&e.config.widths),
        "bookmarks": e.bookmarks,
    })
}

/// Value → 条目; 坏条目 (缺 path/order、形状不可辨) → None 跳过。
/// `hidden`/`widths` 容缺省 (空 = 全显/采样宽, 语义自洽)。
///
/// **载入侧归一** (评审 C1/C3/C6, 外部 `columns.json` 按不可信数据对待):
/// order/hidden 去重保首见、hidden/widths 键 ⊆ order、widths 逐键容错 ——
/// 坏键/非有限值/失配键**丢键不丢条** (列名级静默剔除, D4 同哲学)。
fn entry_from_value(v: &serde_json::Value) -> Option<FileEntry> {
    let obj = v.as_object()?;
    let path = obj.get("path")?.as_str()?.to_string();
    let updated = obj.get("updated")?.as_u64()?;
    let mut order: Vec<String> = Vec::new();
    for n in str_list(obj.get("order")?)? {
        if !order.contains(&n) {
            order.push(n);
        }
    }
    let mut hidden: Vec<String> = Vec::new();
    if let Some(h) = obj.get("hidden") {
        for n in str_list(h)? {
            if order.contains(&n) && !hidden.contains(&n) {
                hidden.push(n);
            }
        }
    }
    let widths = widths_from_value(obj, &order);
    // 书签 (D1): 可缺省 (空 = 无书签); 逐元素容错 (非 u64 元素丢弃),
    // 坏形状**丢字段不丢条** (C6 同粒度); 收编走 normalize_bookmarks。
    let mut bookmarks = Vec::new();
    if let Some(b) = obj.get("bookmarks").and_then(|b| b.as_array()) {
        for x in b {
            // 收集硬顶 (评审 Optional): 手造天文数组不必全量进 Vec 再排 ——
            // 超顶部分按收集序放弃 (极端手造数据的「升序前 256」近似, spec D3
            // 存量手造数据本就是收编语义)。
            if bookmarks.len() >= MAX_BOOKMARKS * 4 {
                break;
            }
            if let Some(n) = x.as_u64() {
                bookmarks.push(n);
            }
        }
    }
    Some(FileEntry {
        path,
        config: ColumnConfig {
            order,
            hidden,
            widths,
        },
        bookmarks: normalize_bookmarks(bookmarks),
        updated,
    })
}

/// 会话条目 → Value (手拼; 与 [`session_from_value`] 互为往返)。
/// 列三字段与 [`entry_to_value`] 同形状同排序 (widths 键排序写入);
/// **无 `bookmarks` 键** (Open Q1: 书签不随会话 = 结构保证)。
fn session_to_value(s: &SessionEntry) -> serde_json::Value {
    serde_json::json!({
        "path": s.path,
        "name": s.name,
        "filter": s.filter,
        "search": s.search,
        "order": s.config.order,
        "hidden": s.config.hidden,
        "widths": widths_to_value(&s.config.widths),
        "expands": s.expands,
        "updated": s.updated,
    })
}

/// Value → 会话条目; 坏条 (缺/空 name、缺 path/updated、形状不可辨) → None 条废。
/// filter/search/order/hidden/widths/expands **容缺省** —— 坏字段丢字段不丢条
/// (C6 同粒度; order 坏形状 = 空表, 应用时 merge 兜底)。
fn session_from_value(v: &serde_json::Value) -> Option<SessionEntry> {
    let obj = v.as_object()?;
    let path = obj.get("path")?.as_str()?.to_string();
    let name = obj.get("name")?.as_str()?.to_string();
    if name.is_empty() {
        return None;
    }
    let updated = obj.get("updated")?.as_u64()?;
    let filter = obj
        .get("filter")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let search = obj
        .get("search")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let mut order: Vec<String> = Vec::new();
    if let Some(list) = obj.get("order").and_then(str_list) {
        for n in list {
            if !order.contains(&n) {
                order.push(n);
            }
        }
    }
    let mut hidden: Vec<String> = Vec::new();
    if let Some(list) = obj.get("hidden").and_then(str_list) {
        for n in list {
            if order.contains(&n) && !hidden.contains(&n) {
                hidden.push(n);
            }
        }
    }
    let widths = widths_from_value(obj, &order);
    let mut expands = Vec::new();
    if let Some(e) = obj.get("expands").and_then(|e| e.as_array()) {
        for x in e {
            // 收集硬顶 (书签先例: 天文数组不必全量进 Vec 再排)
            if expands.len() >= MAX_EXPANDS * 4 {
                break;
            }
            if let Some(n) = x.as_u64() {
                expands.push(n);
            }
        }
    }
    Some(SessionEntry {
        path,
        name,
        filter,
        search,
        config: ColumnConfig {
            order,
            hidden,
            widths,
        },
        expands: normalize_expands(expands),
        updated,
    })
}

/// Value 数组 → 字符串表; 非数组 → None, 含非字符串元素**跳过该元素**
/// (逐元素容错, 与 `entry_from_value` 的列名级粒度一致)。
fn str_list(v: &serde_json::Value) -> Option<Vec<String>> {
    Some(
        v.as_array()?
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn default_is_first_seen_order_all_visible_sampled_width() {
        let cfg = ColumnConfig::from_schema(&names(&["ts", "level", "msg"]));
        assert_eq!(cfg.order, names(&["ts", "level", "msg"]));
        assert!(cfg.hidden.is_empty() && cfg.widths.is_empty());
        assert_eq!(
            cfg.visible_names().collect::<Vec<_>>(),
            vec!["ts", "level", "msg"]
        );
        // 无手动宽 = 采样建议宽原样
        assert_eq!(cfg.width_of("msg", 21.0), 21.0);
    }

    #[test]
    fn set_width_overrides_and_clamps() {
        let mut cfg = ColumnConfig::from_schema(&names(&["a", "b"]));
        assert!(cfg.set_width("a", 39.0));
        assert_eq!(cfg.width_of("a", 99.0), MIN_COL_W);
        assert!(cfg.set_width("a", 513.0));
        assert_eq!(cfg.width_of("a", 99.0), MAX_COL_W);
        assert!(cfg.set_width("a", 120.0));
        assert_eq!(cfg.width_of("a", 99.0), 120.0);
        // 未知列名 / NaN 拒收零变更
        assert!(!cfg.set_width("ghost", 50.0));
        assert!(!cfg.set_width("a", f32::NAN));
        assert_eq!(cfg.width_of("a", 99.0), 120.0);
    }

    #[test]
    fn hide_last_visible_column_is_refused() {
        let mut cfg = ColumnConfig::from_schema(&names(&["a", "b"]));
        assert!(cfg.set_hidden("a", true));
        assert_eq!(cfg.visible_names().collect::<Vec<_>>(), vec!["b"]);
        // 最后一可见列: 拒绝且零变更
        assert!(!cfg.set_hidden("b", true));
        assert!(!cfg.toggle_hidden("b"));
        assert_eq!(cfg.visible_names().collect::<Vec<_>>(), vec!["b"]);
        // 取消隐藏后又可再藏
        assert!(cfg.set_hidden("a", false));
        assert!(cfg.set_hidden("b", true));
        assert_eq!(cfg.visible_names().collect::<Vec<_>>(), vec!["a"]);
    }

    #[test]
    fn toggle_hidden_flips_and_unknown_is_noop() {
        let mut cfg = ColumnConfig::from_schema(&names(&["a", "b"]));
        assert!(cfg.toggle_hidden("a"));
        assert!(cfg.is_hidden("a"));
        assert!(cfg.toggle_hidden("a"));
        assert!(!cfg.is_hidden("a"));
        assert!(!cfg.toggle_hidden("ghost"));
    }

    #[test]
    fn move_name_before_semantics() {
        let mut cfg = ColumnConfig::from_schema(&names(&["a", "b", "c"]));
        assert!(cfg.move_name_before("c", Some("a"))); // c 移到 a 前
        assert_eq!(cfg.order, names(&["c", "a", "b"]));
        assert!(cfg.move_name_before("c", None)); // 排尾
        assert_eq!(cfg.order, names(&["a", "b", "c"]));
        assert!(cfg.move_name_before("b", Some("b"))); // 落点即原位 no-op
        assert_eq!(cfg.order, names(&["a", "b", "c"]));
        assert!(cfg.move_name_before("a", Some("ghost"))); // before 失配 → 排尾
        assert_eq!(cfg.order, names(&["b", "c", "a"]));
        assert!(!cfg.move_name_before("ghost", None)); // 未知列拒收
        assert_eq!(cfg.order, names(&["b", "c", "a"]));
    }

    #[test]
    fn move_column_relocates_first_mid_last() {
        let mut cfg = ColumnConfig::from_schema(&names(&["a", "b", "c", "d"]));
        // 首 → 尾
        assert!(cfg.move_column(0, 3));
        assert_eq!(cfg.order, names(&["b", "c", "d", "a"]));
        // 中 → 中
        assert!(cfg.move_column(1, 2));
        assert_eq!(cfg.order, names(&["b", "d", "c", "a"]));
        // 尾 → 首
        assert!(cfg.move_column(3, 0));
        assert_eq!(cfg.order, names(&["a", "b", "d", "c"]));
        // 越界拒收零变更
        assert!(!cfg.move_column(4, 0));
        assert!(!cfg.move_column(0, 4));
        assert_eq!(cfg.order, names(&["a", "b", "d", "c"]));
    }

    #[test]
    fn merge_appends_new_columns_in_schema_order() {
        let mut cfg = ColumnConfig::from_schema(&names(&["a", "b"]));
        cfg.merge_with_schema(&names(&["a", "b", "c", "d"]));
        assert_eq!(cfg.order, names(&["a", "b", "c", "d"]));
    }

    #[test]
    fn merge_drops_missing_and_stale_keys() {
        let mut cfg = ColumnConfig::from_schema(&names(&["a", "b", "gone"]));
        assert!(cfg.set_hidden("gone", true));
        assert!(cfg.set_width("gone", 200.0));
        cfg.merge_with_schema(&names(&["a", "b"]));
        // 失配列静默剔除: 序/隐/宽三处都不残留
        assert_eq!(cfg.order, names(&["a", "b"]));
        assert!(cfg.hidden.is_empty());
        assert!(cfg.widths.is_empty());
    }

    #[test]
    fn merge_keeps_remembered_order_for_survivors() {
        let mut cfg = ColumnConfig::from_schema(&names(&["a", "b", "c"]));
        assert!(cfg.move_column(2, 0)); // c a b
        cfg.merge_with_schema(&names(&["a", "b", "c", "new"]));
        assert_eq!(cfg.order, names(&["c", "a", "b", "new"]));
    }

    #[test]
    fn merge_is_idempotent_on_same_schema() {
        let mut cfg = ColumnConfig::from_schema(&names(&["a", "b"]));
        assert!(cfg.set_width("a", 100.0));
        let snapshot = cfg.clone();
        cfg.merge_with_schema(&names(&["a", "b"]));
        cfg.merge_with_schema(&names(&["a", "b"]));
        assert_eq!(cfg, snapshot);
    }

    #[test]
    fn merge_empty_schema_clears_everything() {
        let mut cfg = ColumnConfig::from_schema(&names(&["a", "b"]));
        assert!(cfg.set_hidden("a", true));
        assert!(cfg.set_width("b", 100.0));
        cfg.merge_with_schema(&[]);
        assert_eq!(cfg.order, Vec::<String>::new());
        assert!(cfg.hidden.is_empty() && cfg.widths.is_empty());
    }

    #[test]
    fn reset_restores_defaults() {
        let names3 = names(&["a", "b", "c"]);
        let mut cfg = ColumnConfig::from_schema(&names3);
        assert!(cfg.move_column(0, 2));
        assert!(cfg.set_hidden("a", true));
        assert!(cfg.set_width("b", 300.0));
        cfg.reset(&names3);
        assert_eq!(cfg, ColumnConfig::from_schema(&names3));
    }

    // ---- T2: columns.json 持久化 (D4) ----

    fn temp_columns_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "danqing-log-columns-{}-{tag}.json",
            std::process::id()
        ))
    }

    fn sample_config() -> ColumnConfig {
        let mut cfg = ColumnConfig::from_schema(&names(&["ts", "level", "msg"]));
        assert!(cfg.move_column(2, 0));
        assert!(cfg.set_hidden("level", true));
        assert!(cfg.set_width("ts", 133.5));
        cfg
    }

    /// 测试构造器: 无书签条目 (书签用例显式构造 `FileEntry`)。
    fn entry(path: impl Into<String>, config: ColumnConfig, updated: u64) -> FileEntry {
        FileEntry {
            path: path.into(),
            config,
            bookmarks: Vec::new(),
            updated,
        }
    }

    #[test]
    fn roundtrip_preserves_order_hidden_widths() {
        let p = temp_columns_path("roundtrip");
        let mut files = ColumnFiles::default();
        files.put(entry("C:\\logs\\a.jsonl", sample_config(), 100));
        files.save_to(&p).unwrap();
        let loaded = ColumnFiles::load_from(&p);
        assert_eq!(loaded, files);
        assert_eq!(loaded.get("C:\\logs\\a.jsonl"), Some(&sample_config()));
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn lru_evicts_oldest_beyond_file_cap() {
        let mut files = ColumnFiles::default();
        for i in 0..FILE_CAP {
            files.put(entry(format!("p{i}"), ColumnConfig::default(), i as u64));
        }
        assert_eq!(files.entries.len(), FILE_CAP);
        // 第 65 条: 挤掉最旧 (updated=0 的 p0)
        files.put(entry("newest", ColumnConfig::default(), 999));
        assert_eq!(files.entries.len(), FILE_CAP);
        assert!(files.get("p0").is_none());
        assert!(files.get("newest").is_some());
        // 更新既有条目不涨数
        files.put(entry("p1", ColumnConfig::default(), 1000));
        assert_eq!(files.entries.len(), FILE_CAP);
    }

    #[test]
    fn corrupt_file_loads_empty_and_is_not_overwritten() {
        let p = temp_columns_path("corrupt");
        std::fs::write(&p, b"{ not json at all").unwrap();
        let before = std::fs::read(&p).unwrap();
        let files = ColumnFiles::load_from(&p);
        // 损坏 → 空; 且 load **零写盘** (不覆盖用户文件, D4)
        assert_eq!(files, ColumnFiles::default());
        assert_eq!(std::fs::read(&p).unwrap(), before);
        // 下次保存才写好
        let mut files = files;
        files.put(entry("p", sample_config(), 1));
        files.save_to(&p).unwrap();
        assert_eq!(ColumnFiles::load_from(&p), files);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn missing_or_bad_shape_loads_empty_and_bad_entries_skip() {
        assert_eq!(
            ColumnFiles::load_from(&temp_columns_path("missing")),
            ColumnFiles::default()
        );
        // 顶层可辨 + 一条坏条目 (缺 order) + 一条好条目: 坏跳好留
        let good = entry_to_value(&entry("ok", ColumnConfig::default(), 1));
        let mut bad = serde_json::Map::new();
        bad.insert("path".into(), serde_json::json!("bad"));
        bad.insert("updated".into(), serde_json::json!(2));
        let bytes = serde_json::to_vec(&serde_json::json!({
            "files": [serde_json::Value::Object(bad), good],
        }))
        .unwrap();
        let files = ColumnFiles::from_json(&bytes);
        assert_eq!(files.entries.len(), 1);
        assert_eq!(files.entries[0].path, "ok");
    }

    // ---- 评审修复锁 (2026-09-23 双路评审并账): 外部数据归一 / LRU 边界 ----

    /// 评审 C1: 脏 `hidden` (重复/失配) 不得打穿 ≥1 可见守卫 ——
    /// `order.len() - hidden.len()` 相减在 hidden 撑长时 usize 下溢
    /// (debug 崩 / release 回绕使 `visible <= 1` 失效, 可藏到 0 列)。
    #[test]
    fn dirty_hidden_cannot_break_min_visible_guard() {
        let mut cfg = ColumnConfig {
            order: names(&["a", "b", "c"]),
            // 4 个 hidden 条目 vs order 3 列: 相减必下溢; 真可见仅 "c"
            hidden: names(&["a", "a", "a", "b"]),
            widths: HashMap::new(),
        };
        // 藏 "c" = 关最后一可见列: 必拒 (旧式相减会 panic/回绕放行)
        assert!(!cfg.set_hidden("c", true));
        assert!(!cfg.is_hidden("c"), "零变更");
        // 取消隐藏照常可用
        assert!(cfg.set_hidden("a", false));
        assert!(cfg.set_hidden("c", true));
    }

    /// 评审 C3/C1: merge 归一 —— order/hidden 手造重复收敛 (同名列不画两遍,
    /// 失配 hidden 不残留), 记忆序保持。
    #[test]
    fn merge_dedups_order_and_hidden_drops_stale_names() {
        let mut cfg = ColumnConfig {
            order: names(&["msg", "level", "msg"]),
            hidden: names(&["level", "level", "ghost"]),
            widths: HashMap::from([("msg".to_string(), 120.0), ("ghost".to_string(), 50.0)]),
        };
        cfg.merge_with_schema(&names(&["msg", "level"]));
        assert_eq!(cfg.order, names(&["msg", "level"]));
        assert_eq!(cfg.hidden, names(&["level"]));
        assert_eq!(cfg.widths.len(), 1, "失配 width key 静默剔除");
        assert_eq!(cfg.widths.get("msg"), Some(&120.0));
    }

    /// 评审 C2: merge 后不得 0 列 —— 唯一可见列被 schema 剔除时,
    /// 取消隐藏首见序首列兜底 (D6 在 merge 路径同样成立)。
    #[test]
    fn merge_restores_min_visible_when_survivors_all_hidden() {
        let mut cfg = ColumnConfig::from_schema(&names(&["a", "b", "c"]));
        assert!(cfg.set_hidden("a", true));
        assert!(cfg.set_hidden("b", true));
        // "c" 被新 schema 剔除 → 存活列 a,b 全隐藏 → 必须兜回至少一列
        cfg.merge_with_schema(&names(&["a", "b"]));
        assert_eq!(cfg.order, names(&["a", "b"]));
        assert_eq!(
            cfg.visible_names().collect::<Vec<_>>(),
            vec!["a"],
            "取消隐藏 order 首列"
        );
    }

    /// 评审 C1/C3/C6 (载入侧归一): order/hidden 去重收敛、hidden ⊆ order、
    /// widths 逐键容错 —— 坏键/非有限值/失配键**丢键不丢条**, 其余照留。
    #[test]
    fn entry_from_json_normalizes_dirty_lists_and_widths() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "files": [{
                "path": "C:\\x.jsonl",
                "updated": 1,
                "order": ["msg", "level", "msg"],
                "hidden": ["level", "level", "ghost"],
                "widths": {
                    "msg": 1e308,          // f32 化 = inf → 丢键
                    "level": 80.0,          // 正常
                    "ghost": 50.0,          // 不在 order → 丢键
                    "bad": "120"            // 非数值 → 丢键 (C6: 不废整条)
                }
            }]
        }))
        .unwrap();
        let files = ColumnFiles::from_json(&bytes);
        assert_eq!(files.entries.len(), 1, "坏 width 键不得废掉整条 (C6)");
        let e = &files.entries[0];
        assert_eq!(e.config.order, names(&["msg", "level"]));
        assert_eq!(e.config.hidden, names(&["level"]));
        assert_eq!(e.config.widths.len(), 1);
        assert_eq!(e.config.widths.get("level"), Some(&80.0));
    }

    /// 评审 C5: LRU `put` 同秒并列时**本次条目**不得被 truncate 挤掉自己。
    #[test]
    fn lru_put_same_second_survives_truncation() {
        let mut files = ColumnFiles::default();
        for i in 0..FILE_CAP {
            files.put(entry(format!("p{i}"), ColumnConfig::default(), 7));
        }
        assert_eq!(files.entries.len(), FILE_CAP);
        // 第 65 条与全部旧条目同秒 (updated=7): 刚 put 的必须存活
        files.put(entry("newest", ColumnConfig::default(), 7));
        assert_eq!(files.entries.len(), FILE_CAP);
        assert!(files.get("newest").is_some(), "同秒并列不得挤掉本次条目");
    }

    /// 评审 R2: 书签是用户内容不是偏好 —— LRU 淘汰**无书签条目先挤**,
    /// 带书签条目最后挤; 全都带书签才按纯 LRU 丢最旧; 被挤者返回给调用方。
    #[test]
    fn lru_evicts_bookmarkless_before_bookmarked() {
        let mut files = ColumnFiles::default();
        // 1 条无书签最旧 + 63 条带书签 (满 64)
        files.put(entry("plain", ColumnConfig::default(), 0));
        for i in 0..FILE_CAP - 1 {
            files.put(FileEntry {
                path: format!("bm{i}"),
                config: ColumnConfig::default(),
                bookmarks: vec![1],
                updated: (i + 1) as u64,
            });
        }
        assert_eq!(files.entries.len(), FILE_CAP);
        // 第 65 条: 无书签的 "plain" 先挤, 带书签全留
        let evicted = files.put(entry("newest", ColumnConfig::default(), 999));
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].path, "plain", "无书签最旧先挤");
        assert!(files.get_entry("bm0").is_some(), "带书签条目全留");
        assert_eq!(files.entries.len(), FILE_CAP);
        // 无书签条目**即使更新**也先挤 (此时 "newest" 无书签 updated=999)
        let evicted = files.put(FileEntry {
            path: "newest2".into(),
            config: ColumnConfig::default(),
            bookmarks: vec![2],
            updated: 1000,
        });
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].path, "newest", "无书签即使更新也先挤");
        // 全带书签 → 纯 LRU: 最旧的带书签条目被挤并**原样返回**
        let evicted = files.put(FileEntry {
            path: "newest3".into(),
            config: ColumnConfig::default(),
            bookmarks: vec![3],
            updated: 1001,
        });
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].path, "bm0", "全都带书签 → 纯 LRU 丢最旧");
        assert!(
            !evicted[0].bookmarks.is_empty(),
            "被挤条目原样返回 (调用方可提示)"
        );
    }

    // ---- T1: 书签持久化磁盘模型 (SPEC-v1x-bookmark-persist D1) ----

    /// roundtrip 全等 (三形态: 有书签/空/满 [`MAX_BOOKMARKS`]); `put` 收编
    /// (乱序重复进 → 升序去重出); `get` 简写仍指列摆法, `get_entry` 拿整条。
    /// 摘 `entry_to_value` 的 bookmarks 写出 → `loaded != files` 精确红 (A/B)。
    #[test]
    fn roundtrip_preserves_bookmarks_in_three_forms() {
        let p = temp_columns_path("bm-roundtrip");
        let mut files = ColumnFiles::default();
        files.put(FileEntry {
            path: "C:\\logs\\a.jsonl".into(),
            config: ColumnConfig::default(),
            bookmarks: vec![10, 5, 5, 3], // 乱序 + 重复 → 收编升序去重
            updated: 100,
        });
        files.put(FileEntry {
            path: "C:\\logs\\b.jsonl".into(),
            config: ColumnConfig::default(),
            bookmarks: Vec::new(),
            updated: 99,
        });
        let full: Vec<u64> = (0..MAX_BOOKMARKS as u64).collect();
        files.put(FileEntry {
            path: "C:\\logs\\c.jsonl".into(),
            config: ColumnConfig::default(),
            bookmarks: full.clone(),
            updated: 98,
        });
        files.save_to(&p).unwrap();
        let loaded = ColumnFiles::load_from(&p);
        assert_eq!(loaded, files, "roundtrip 全等");
        assert_eq!(
            loaded
                .get_entry("C:\\logs\\a.jsonl")
                .map(|e| e.bookmarks.as_slice()),
            Some([3u64, 5, 10].as_slice()),
            "put 归一: 去重升序"
        );
        assert_eq!(
            loaded
                .get_entry("C:\\logs\\b.jsonl")
                .map(|e| e.bookmarks.len()),
            Some(0),
            "空书签原样"
        );
        assert_eq!(
            loaded
                .get_entry("C:\\logs\\c.jsonl")
                .map(|e| e.bookmarks.as_slice()),
            Some(full.as_slice()),
            "满 MAX_BOOKMARKS 原样保留 (不截)"
        );
        // get 简写 = config 半边
        assert_eq!(
            loaded.get("C:\\logs\\b.jsonl"),
            Some(&ColumnConfig::default())
        );
        std::fs::remove_file(&p).ok();
    }

    /// 载入归一 (D1/C6 同粒度): 逐元素容错 (非 u64 丢弃) / 去重 / 升序 /
    /// 超 [`MAX_BOOKMARKS`] 截断保前 N / 坏形状**丢字段不丢条** / 可缺省。
    #[test]
    fn entry_from_json_normalizes_bookmarks_leniently() {
        let mut shuffled: Vec<u64> = (0..(MAX_BOOKMARKS as u64 + 3)).collect();
        shuffled.reverse(); // 乱序 259 条
        let bytes = serde_json::to_vec(&serde_json::json!({
            "files": [
                {
                    "path": "a", "updated": 1, "order": ["x"],
                    "bookmarks": ["oops", 3.5, 7u64, 7u64, 2u64, -1]
                },
                {
                    "path": "b", "updated": 2, "order": ["x"],
                    "bookmarks": shuffled
                },
                {
                    "path": "c", "updated": 3, "order": ["x"],
                    "bookmarks": "not-a-list"
                },
                { "path": "d", "updated": 4, "order": ["x"] }
            ]
        }))
        .unwrap();
        let files = ColumnFiles::from_json(&bytes);
        assert_eq!(files.entries.len(), 4, "坏 bookmarks 不废条");
        assert_eq!(
            files.entries[0].bookmarks,
            vec![2, 7],
            "非数值/浮点/负数丢弃, 去重升序"
        );
        // 列摆法照留 (评审 R③: 只断言 bookmarks 空/条数会让「丢摆法留空壳」漏网)
        assert_eq!(files.entries[0].config.order, names(&["x"]));
        let b = &files.entries[1];
        assert_eq!(b.bookmarks.len(), MAX_BOOKMARKS, "超限截断保升序前 256");
        assert_eq!(b.bookmarks.first(), Some(&0));
        assert_eq!(b.bookmarks.last(), Some(&((MAX_BOOKMARKS - 1) as u64)));
        assert!(files.entries[2].bookmarks.is_empty(), "坏形状丢字段不丢条");
        assert_eq!(files.entries[2].config.order, names(&["x"]), "摆法照留");
        assert!(files.entries[3].bookmarks.is_empty(), "可缺省 = 空");
        assert_eq!(files.entries[3].config.order, names(&["x"]));
    }

    // ---- T1: sessions 段 (SPEC-v1x-workspace-sessions) ----

    fn session(path: &str, name: &str) -> SessionEntry {
        SessionEntry {
            path: path.into(),
            name: name.into(),
            filter: "level=ERROR status>=500".into(),
            search: "timeout".into(),
            config: sample_config(),
            expands: vec![7, 3, 3, 11], // 乱序 + 重复 → 收编升序去重
            updated: 42,
        }
    }

    /// 四样载荷 roundtrip 全等; `sessions` **恒写** (空也写键); 序列化面**无**
    /// `bookmarks` 键 (Open Q1: 书签不随会话 = 结构保证); files 段同账不互扰。
    #[test]
    fn session_roundtrip_preserves_four_parts_and_has_no_bookmarks_key() {
        let p = temp_columns_path("sess-rt");
        let mut files = ColumnFiles::default();
        files.put(entry("a", sample_config(), 1));
        files.put_sessions_for_path("a", vec![session("ignored", "排障A")]);
        files.put_sessions_for_path("b", vec![session("ignored2", "排障B")]);
        files.save_to(&p).unwrap();
        let loaded = ColumnFiles::load_from(&p);
        assert_eq!(loaded, files, "roundtrip 全等 (files + sessions 同账)");
        let sa = loaded.sessions_for_path("a");
        assert_eq!(sa.len(), 1);
        assert_eq!(sa[0].name, "排障A");
        assert_eq!(sa[0].path, "a", "path 归 put 参数定");
        assert_eq!(
            (sa[0].filter.as_str(), sa[0].search.as_str()),
            ("level=ERROR status>=500", "timeout")
        );
        assert_eq!(sa[0].config, sample_config());
        assert_eq!(sa[0].expands, vec![3, 7, 11], "乱序重复收编升序去重");
        // 恒写: 空 sessions 也写键 (roundtrip 全等好判, D1 同哲学)
        ColumnFiles::default().save_to(&p).unwrap();
        let txt = String::from_utf8(std::fs::read(&p).unwrap()).unwrap();
        assert!(txt.contains("\"sessions\""), "sessions 恒写");
        // Open Q1 结构 strip: 会话序列化面根本没有 bookmarks 键
        assert!(
            session_to_value(&session("a", "n"))
                .get("bookmarks")
                .is_none()
        );
        std::fs::remove_file(&p).ok();
    }

    /// 坏条丢条不丢账 (表驱动): 缺/空/错型 name、缺 path/updated → 条废;
    /// 同 (path,name) 去重保首见; filter/search/order/hidden/widths/expands
    /// 容缺省丢字段不丢条; expands 逐元素容错收编。
    #[test]
    fn session_from_json_drops_bad_keeps_good() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "sessions": [
                { "path": "a", "name": "好", "filter": "f", "search": "s",
                  "order": ["x", "x", "y"], "hidden": ["x", "ghost"],
                  "widths": {"x": 100.0, "ghost": 50.0, "y": "oops"},
                  "expands": ["oops", 5, 5, 2], "updated": 1 },
                { "path": "a", "name": "好", "filter": "dup", "updated": 2 },
                { "path": "a", "filter": "缺名", "updated": 3 },
                { "path": "a", "name": "", "updated": 4 },
                { "path": "a", "name": 5, "updated": 5 },
                { "name": "缺path", "updated": 6 },
                { "path": "b", "name": "缺updated" },
                { "path": "b", "name": "宽容", "order": "not-a-list", "updated": 9 }
            ]
        }))
        .unwrap();
        let files = ColumnFiles::from_json(&bytes);
        assert_eq!(files.sessions.len(), 2, "坏条丢条不丢账");
        let good = &files.sessions[0];
        assert_eq!((good.name.as_str(), good.filter.as_str()), ("好", "f"));
        assert_eq!(good.expands, vec![2, 5], "逐元素容错 + 收编");
        assert_eq!(good.config.order, names(&["x", "y"]), "order 去重保首见");
        assert_eq!(good.config.hidden, names(&["x"]), "hidden ⊆ order 归一");
        assert_eq!(good.config.widths.len(), 1, "失配/坏宽丢键不丢条");
        assert_eq!(good.config.widths["x"], 100.0);
        let lenient = &files.sessions[1];
        assert_eq!(lenient.name, "宽容");
        assert_eq!(
            (lenient.filter.as_str(), lenient.search.as_str()),
            ("", ""),
            "可缺省 = 空"
        );
        assert!(lenient.config.order.is_empty(), "坏形状丢字段不丢条");
    }

    /// `put_sessions_for_path` 只替换本路径切片 (他路径不动); 满
    /// [`MAX_SESSIONS_PER_PATH`] 收编截断保前 N (手造超限 = 收编语义)。
    #[test]
    fn put_sessions_replaces_only_that_path_slice() {
        let mut files = ColumnFiles::default();
        files.put_sessions_for_path("a", vec![session("x", "a1"), session("x", "a2")]);
        files.put_sessions_for_path("b", vec![session("x", "b1")]);
        files.put_sessions_for_path("a", vec![session("x", "a3")]);
        let sa = files.sessions_for_path("a");
        let sb = files.sessions_for_path("b");
        let a: Vec<&str> = sa.iter().map(|s| s.name.as_str()).collect();
        let b: Vec<&str> = sb.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(a, vec!["a3"], "本路径整片替换");
        assert_eq!(b, vec!["b1"], "他路径不动");
        let many: Vec<SessionEntry> = (0..40).map(|i| session("x", &format!("s{i:02}"))).collect();
        files.put_sessions_for_path("c", many);
        assert_eq!(
            files.sessions_for_path("c").len(),
            MAX_SESSIONS_PER_PATH,
            "满员截断保前 32"
        );
    }

    /// 评审 M10: 名归一 (trim / 剔控制字符 / 截 64) 保存载入同式;
    /// 归一后为空 = 丢条。
    #[test]
    fn clean_session_name_trims_strips_controls_and_clamps() {
        assert_eq!(clean_session_name("  排障 A  "), "排障 A");
        assert_eq!(
            clean_session_name("A\nB\tC"),
            "ABC",
            "换行/制表打进 notice 的面"
        );
        assert_eq!(clean_session_name("   "), "");
        let long = "x".repeat(100);
        assert_eq!(
            clean_session_name(&long).chars().count(),
            MAX_SESSION_NAME_LEN
        );
        // 载入侧同式: 纯空白/纯控制名丢条, 好名 trim
        let bytes = serde_json::to_vec(&serde_json::json!({
            "sessions": [
                { "path": "a", "name": "   ", "updated": 1 },
                { "path": "a", "name": "\n\t", "updated": 2 },
                { "path": "a", "name": " 好 ", "updated": 3 }
            ]
        }))
        .unwrap();
        let files = ColumnFiles::from_json(&bytes);
        assert_eq!(files.sessions.len(), 1, "坏名丢条不丢账");
        assert_eq!(files.sessions[0].name, "好");
    }

    /// 收集硬顶 (防手造天文数组, 书签先例: 超顶按收集序放弃)。
    #[test]
    fn session_collect_hard_cap_defends_astronomical_arrays() {
        let big = SESSIONS_HARD_CAP + 10;
        let arr: Vec<serde_json::Value> = (0..big)
            .map(|i| serde_json::json!({ "path": format!("p{i}"), "name": "n", "updated": i }))
            .collect();
        let bytes = serde_json::to_vec(&serde_json::json!({ "sessions": arr })).unwrap();
        let files = ColumnFiles::from_json(&bytes);
        assert_eq!(files.sessions.len(), SESSIONS_HARD_CAP, "超顶按收集序放弃");
    }
}
