//! @author 十四叔
//! @date 2026/09/29
//! LogApp · 持久化簇: config.toml / state.json 读写、命名会话、损坏备份。
use super::*;

impl LogApp {
    /// 把当前设置写回 `config.toml`。
    ///
    /// 必须走整文件写入 —— [`config::Config`] 的两个键同源，分头写会让
    /// 「改主题」顺手抹掉侧栏开关 (config.rs 的 `round_trip_preserves_both_keys`
    /// 钉着这条)。
    pub(crate) fn save_config(&self) {
        let cfg = config::Config {
            theme: self.theme,
            histogram: self.histogram_visible,
        };
        match &self.cfg_path {
            Some(p) => cfg.save_to(p),
            // **测试里不许落到真实配置**: 这条不是洁癖，是实测过的坑 ——
            // 本批的 T20 单测走 `update(Msg::ToggleHistogram)` → 这里 → 真实路径，
            // 而 `save_to` 是**整文件覆盖写**、`load_from` 对认不出的 `mode` 取默认
            // (light): 一次 `cargo test` 就能把用户的主题改掉、手写注释抹掉。
            // 与其靠「下一个写测试的人记得」, 不如让它**写不出去**。
            None => {
                #[cfg(test)]
                panic!("测试不得写真实配置 —— 请用 LogApp::new_empty_at(临时路径)");
                #[cfg(not(test))]
                cfg.save();
            }
        }
    }

    /// license 文件路径：生产 = 用户真实路径; 测试 = 注入配置路径的同名邻居
    /// (与 cfg_path 同源注入 —— 测试永不碰真实 license, 与 save_config 同规：
    /// 与其靠「下一个写测试的人记得」, 不如让它**写不出去**)。
    pub(crate) fn license_path(&self) -> std::path::PathBuf {
        match &self.cfg_path {
            Some(p) => p.with_extension("license.key"),
            None => {
                #[cfg(test)]
                panic!("测试不得读写真实 license —— 请用 LogApp::new_empty_at(临时路径)");
                #[cfg(not(test))]
                license::default_license_path()
            }
        }
    }

    /// 状态账本路径 (`state.json`): 生产 = config 目录独立文件 (license.key 同目录
    /// 先例，D4); 测试 = 注入配置路径的邻居 + 无注入 panic (与 cfg_path/license_path
    /// 同规：「测试不得写真实配置」家法的封法)。2026-09-24 随账本改名一次到位
    /// (bookmark-persist 既定裁定)。
    fn state_path(&self) -> std::path::PathBuf {
        match &self.cfg_path {
            Some(p) => p.with_extension("state.json"),
            None => {
                #[cfg(test)]
                panic!("测试不得读写真实 state.json —— 请用 LogApp::new_empty_at(临时路径)");
                #[cfg(not(test))]
                danqing_log::columns::default_path()
            }
        }
    }

    /// 旧名账本路径 (只读迁移源，SPEC-v1x-workspace-sessions D3): 与
    /// [`Self::state_path`] **同分支派生** —— 测试注入是 `with_extension` 邻居派生
    /// (`x.state.json` ↔ `x.columns.json`), 生产是整名兄弟 (`state.json` ↔
    /// `columns.json`), 两种方案没有统一表达式，分支配对是唯一不歪的写法。
    fn legacy_state_path(&self) -> std::path::PathBuf {
        match &self.cfg_path {
            Some(p) => p.with_extension("columns.json"),
            None => {
                #[cfg(test)]
                panic!("测试不得读写真实 columns.json —— 请用 LogApp::new_empty_at(临时路径)");
                #[cfg(not(test))]
                danqing_log::columns::default_path().with_file_name("columns.json")
            }
        }
    }

    /// 载入状态账本 (**读旧写新迁移**, SPEC-v1x-workspace-sessions D3): 新名
    /// `state.json` **可辨**才新名优先; 新名缺失/空/坏/全废 → 回落旧名
    /// `columns.json`（与 [`Self::backup_if_corrupt`] 同一「可辨」谓词——评审
    /// M2: 读/备判据不对称会让坏新名 + 旧名有货时，save_state 先挪新名、再读
    /// 旧名、再拿空内存覆盖旧名本路径切片 = 家族 Critical 复发）。落盘只写新名，
    /// 成功后旧名退役（一次性，见 [`Self::save_state`]）。
    fn load_state_account(&self) -> danqing_log::columns::ColumnFiles {
        let new_acc = danqing_log::columns::ColumnFiles::load_from(&self.state_path());
        if new_acc.is_recognizable() {
            return new_acc;
        }
        danqing_log::columns::ColumnFiles::load_from(&self.legacy_state_path())
    }

    /// 换文件载入该路径的记忆状态 (D2, SPEC-v1x-bookmark-persist): 列摆法 + 书签
    /// **一次读盘同取**; 书签按当前行数越界剔除**不写回** (损坏零写回同哲学);
    /// 无记忆 = 默认摆法 + 空书签 (替换语义 —— 旧文件的手势/书签不带进新文件)。
    pub(crate) fn load_state_for_current_file(&mut self) {
        let files = self.load_state_account();
        match files.get_entry(self.path.to_string_lossy().as_ref()) {
            Some(e) => {
                // 通路段「不读」(gate-trio G2/G3): 免费态列配置段/书签段**不读**
                // —— 默认列摆法 (v1.0 行为) / 书签不跨重启恢复 (会话内照用)。
                // 读侧拦住，磁盘上的付费期数据原样躺着 (数据永在，不删不改)。
                if self.entitlement.allows(Feature::ColumnConfig) {
                    self.columns = e.config.clone();
                } else {
                    self.columns = danqing_log::columns::ColumnConfig::default();
                }
                let total = self.file.line_count();
                if self.entitlement.allows(Feature::BookmarkPersist) {
                    self.bookmarks = e.bookmarks.iter().copied().filter(|&l| l < total).collect();
                } else {
                    self.bookmarks.clear();
                }
            }
            None => {
                self.columns = danqing_log::columns::ColumnConfig::default();
                self.bookmarks.clear();
            }
        }
        // 命名会话 (T2): 该路径切片整片替换 + 选中清
        // (Open Q3: 「会话」列表只显本路径，换文件不带旧选中)。
        self.sessions = files.sessions_for_path(self.path.to_string_lossy().as_ref());
        self.session_selected = None;
        self.merge_columns();
    }

    /// 保存/覆盖命名会话 (D1/D2/D6): 快照四样 = 过滤查询串 + 搜索查询串 +
    /// 列摆法 + 展开行号表 (**应用时重建真相**, 不存行集)。同名覆盖 +「已更新」;
    /// 空名 / 满 [`MAX_SESSIONS_PER_PATH`] (新名才计数) 拒绝并说清。
    /// **书签零触碰** (Open Q1)。
    pub(crate) fn save_session(&mut self, name: &str) {
        use danqing_log::columns::{MAX_SESSIONS_PER_PATH, SessionEntry};
        let name = danqing_log::columns::clean_session_name(name);
        if name.is_empty() {
            self.set_notice("先命名再保存会话".into(), NoticeKind::Warn);
            return;
        }
        let exists = self.sessions.iter().any(|s| s.name == name);
        if !exists && self.sessions.len() >= MAX_SESSIONS_PER_PATH {
            self.set_notice(
                format!("会话已满 ({MAX_SESSIONS_PER_PATH} 个), 先删再存"),
                NoticeKind::Warn,
            );
            return;
        }
        let updated = now_secs();
        let entry = SessionEntry {
            path: self.path.to_string_lossy().into_owned(),
            name: name.to_string(),
            filter: self.filter_applied.clone(),
            search: self.search_query.clone(),
            config: self.columns.clone(),
            expands: self.expanded.lines(),
            // T8 (SPEC-v1x-merge-timeline D4): 合并工作区保存 = 单文件侧四样
            // (冻结现场，属 self.path) + 合并组快照 (源/时间参数/显隐);
            // 追踪过滤串首版不落盘 (MergeGroup 注释)。
            merge: if self.workspace == Workspace::Merge {
                self.merge.as_ref().map(|m| m.snapshot_group())
            } else {
                None
            },
            updated,
        };
        if let Some(slot) = self.sessions.iter_mut().find(|s| s.name == name) {
            *slot = entry;
        } else {
            self.sessions.push(entry);
        }
        let ok = self.save_state();
        let verb = if exists { "已更新" } else { "已保存" };
        let tail = if ok { "" } else { " (未落盘)" };
        self.set_notice(format!("会话「{name}」{verb}{tail}"), NoticeKind::Info);
    }

    /// 应用命名会话 (D1/D2): 单文件会话 = 四样全链重跑; 合并组会话 (T8) =
    /// 先验源 (缺失明示跳过，不足两源整体不动) → 单文件侧四样照旧 → 后台
    /// 重开源组重建合并，保存的源参数交卷时套回 (apply_merge_outcome)。
    /// 应用即记选中 (「删除」指针，T3)。**书签零触碰** (Open Q1)。
    /// 返回是否已应用 (未知名/源不足不动账，留弹层重选)。
    pub(crate) fn apply_session(&mut self, name: &str) -> bool {
        let Some(s) = self.sessions.iter().find(|s| s.name == name).cloned() else {
            self.set_notice("会话不存在".into(), NoticeKind::Warn);
            return false;
        };
        if let Some(group) = s.merge.clone() {
            // 先验源 (T8 验收 g): 缺失/暂不可读明示跳过; 现存不足两个 =
            // 整体不应用 (零副作用，会话留着，修源后重试)。
            let mut paths: Vec<PathBuf> = Vec::new();
            let mut missing: Vec<String> = Vec::new();
            let mut refused = 0usize;
            for src in &group.sources {
                let p = PathBuf::from(&src.path);
                if paths.contains(&p) {
                    continue; // 手造账本重复源 (载入侧已收编，双保险)
                }
                // 设备命名空间 (\\.\PhysicalDrive0 之类) 不是日志：拒收明示。
                // 账本路径本身是「用户自己的文件」= 已信任假设 (含 UNC 网络盘：
                // 那可能是用户真在看的共享日志), 但设备命名空间会把原始设备
                // 当普通文件整个映射 —— 行为不可预期，一律不认 (安全审计同条)。
                if src.path.starts_with(r"\\.\") {
                    refused += 1;
                    continue;
                }
                if FileStat::of(&p).is_ok() {
                    paths.push(p);
                } else {
                    missing.push(src.path.clone());
                }
            }
            if paths.len() < 2 {
                self.set_notice(
                    format!(
                        "会话「{name}」的合并源现存不足两个 (缺失 {} 个), 未应用",
                        missing.len()
                    ),
                    NoticeKind::Warn,
                );
                return false;
            }
            self.workspace = Workspace::Single; // 单文件侧载荷在 Single 路由下应用
            self.apply_session_payload(&s);
            // 合并组重开 (后台归并); 源集合换了 → 在途追踪作废。
            self.discard_trace_job();
            self.pending_merge_apply = Some(group.clone());
            self.merge_job_live = true;
            // 保存的源参数**带进 worker**: 偏移/时区在提取时就施加 (评审 R3 ——
            // 原先交卷后在 UI 线程重提 3×1GiB ≈1s, 与「重活在 worker」自相矛盾),
            // 显隐在建索引时就掩码 (落点零重建)。
            let saved_sources = group.sources.clone();
            self.merge_job.launch(move || {
                danqing_log::merge_view::build_merge(
                    &paths,
                    &saved_sources,
                    &std::sync::atomic::AtomicBool::new(false),
                )
            });
            let mut skip = if missing.is_empty() {
                String::new()
            } else {
                format!(" (跳过 {} 个缺失源)", missing.len())
            };
            if refused > 0 {
                skip.push_str(&format!(" (忽略 {refused} 个设备路径)"));
            }
            self.set_notice(format!("恢复合并会话「{name}」…{skip}"), NoticeKind::Info);
            return true;
        }
        // 单文件会话：合并中应用 = 先切回 Single (合并 bundle 保留，D4 不丢)。
        // 离场同样作废 (评审 C1: 否则旧命中表会在下次进合并时落地)。
        self.discard_trace_job();
        self.workspace = Workspace::Single;
        self.apply_session_payload(&s);
        true
    }

    /// 单文件侧四样应用 (D1/D2): ①列换入**写穿** per-file 条目 ②过滤/搜索走
    /// 既有全链重跑 (查询串 = 真相重建) ③展开重建 (越界/不可展开剔除) ④回顶。
    /// 单文件会话与合并组会话共用 (T8 拆出，行为零变化)。
    fn apply_session_payload(&mut self, s: &danqing_log::columns::SessionEntry) {
        self.columns = s.config.clone();
        self.merge_columns();
        self.save_state(); // 写穿 (接缝定案：会话应用 = 写穿 per-file 条目)
        self.apply_filter(s.filter.clone());
        // 搜索串先验正则 (评审 M8): 跨编码/手造会话的搜索串可能在当前文件上
        // 正则无效 (如 GBK 下合法的 `(` 到 UTF-8 是残括号) —— 失败则**显式清空**
        // 并说清，四样必须「已应用或已显式清空」, 不许 3/4 写穿后谎称成功。
        let search_usable = s.search.is_empty()
            || regex::bytes::Regex::new(&build_search_pattern(self.file.encoding(), &s.search))
                .is_ok();
        if search_usable && !s.search.is_empty() {
            self.apply_search(s.search.clone());
        } else {
            self.clear_search();
            if !s.search.is_empty() {
                self.set_notice("会话搜索串正则无效，已清空搜索".into(), NoticeKind::Warn);
            }
        }
        self.rebuild_expands(&s.expands);
        self.set_top(0.0);
        self.set_selected(0);
        self.session_selected = Some(s.name.clone());
    }

    /// 删除命名会话 (Open Q4: 无确认，说清即走 —— 快照非唯一记忆，重存即可)。
    pub(crate) fn delete_session(&mut self, name: &str) {
        // 指针随名清 (评审 M9): 名不存在 (外部改账/收编丢条) 也清 —— 免得
        // 「删除」一直对着幽灵名反复「会话不存在」。
        if self.session_selected.as_deref() == Some(name) {
            self.session_selected = None;
        }
        let before = self.sessions.len();
        self.sessions.retain(|s| s.name != name);
        if self.sessions.len() == before {
            self.set_notice("会话不存在".into(), NoticeKind::Warn);
            return;
        }
        let ok = self.save_state();
        let tail = if ok { "" } else { " (未落盘)" };
        self.set_notice(format!("会话已删除{tail}"), NoticeKind::Info);
    }

    /// 展开态整体换入 (D2/D6): 会话行号表逐行现算子行 —— 越界 / 不可展开
    /// **静默剔除** (书签越界剔除同哲学); `expand_rev` 保守 +1 作废旧选区
    /// (整体换入不逐次判「有无真变化」)。
    fn rebuild_expands(&mut self, lines: &[u64]) {
        self.expanded = ExpandMap::new();
        self.sub_rows.clear();
        let total = self.file.line_count();
        for &line in lines {
            if line >= total {
                continue;
            }
            let raw = self.file.line(line);
            let Some(parsed) = jsonl::parse_line(raw) else {
                continue;
            };
            let rows = jsonl::flatten(&parsed);
            if rows.is_empty() {
                continue;
            }
            self.expanded.expand(line, rows.len());
            self.sub_rows.insert(line, rows);
        }
        self.expand_rev += 1;
    }

    /// 损坏备份守卫 (评审 Critical, [`Self::save_state`] 前置): 文件存在且非空
    /// 但解析为空账 (坏 JSON / 形状不可辨 / 条目全废) → 先 rename `.bak` 再开
    /// 新账 —— 读改写覆盖不得把其余路径的记忆连同坏文件一起抹掉 (「下次保存
    /// 才写好」不许做成「才写坏」)。返回**是否可继续落盘** (备份失败 = 拒绝覆盖)。
    fn backup_if_corrupt(&mut self, path: &std::path::Path) -> bool {
        let Ok(bytes) = std::fs::read(path) else {
            return true;
        };
        // 损坏 = 非空文件却给不出**可辨**账本 (评审 M1: 判据必须认 sessions 段 ——
        // 「files 坏条 + sessions 完好」是丢段不丢账的合法容错，不许整账判损
        // 把他会话 rename 进 .bak 丢出活跃账本)。判据与 `load_state_account`
        // 回落同源 (`is_recognizable`)。
        if bytes.is_empty()
            || danqing_log::columns::ColumnFiles::from_json(&bytes).is_recognizable()
        {
            return true;
        }
        let bak = path.with_extension("json.bak");
        if std::fs::rename(path, &bak).is_err() {
            log::warn!("state.json 已损坏且备份失败 —— 拒绝覆盖以免抹掉其余记忆");
            return false;
        }
        self.set_notice(
            "state.json 已损坏，原文件已备份为 state.json.bak".into(),
            NoticeKind::Warn,
        );
        true
    }

    /// 记忆状态落盘 (D2/D4): `state.json` per-路径条目 (列摆法 + 书签 + 命名会话), 变更即写
    /// (save_config 同哲学)。路径 key = `to_string_lossy` exact (已知局限：同文件
    /// 不同路径写法算两条)。返回是否落盘成功 —— **toggle 据此不许说谎**
    /// (评审 R①: 落盘失败还报「已添加」= 成功判据 1 静默违约)。
    pub(crate) fn save_state(&mut self) -> bool {
        if !self.has_file {
            return true;
        }
        let path = self.state_path();
        if !self.backup_if_corrupt(&path) {
            return false;
        }
        // 读改写 (含旧名迁移读): sessions 段随 from_json/save_to 恒写自动保真
        let mut files = self.load_state_account();
        let now = now_secs();
        // 通路段「不写」(gate-trio G2/G3): 免费态两段**取磁盘原值**写回 ——
        // 免费期的运行态不上账，付费期已写的原样保留 (数据永在，读改写不许
        // 把这两段改掉)。列配置与书签分门各判，互不牵连。
        let prev = files
            .get_entry(self.path.to_string_lossy().as_ref())
            .cloned();
        let config = if self.entitlement.allows(Feature::ColumnConfig) {
            self.columns.clone()
        } else {
            prev.as_ref().map(|e| e.config.clone()).unwrap_or_default()
        };
        let bookmarks = if self.entitlement.allows(Feature::BookmarkPersist) {
            // 书签按当前行数过滤再落盘 (评审 R1: 与 load 同式，脏行号不得出内存)
            self.bookmarks
                .iter()
                .copied()
                .filter(|&l| l < self.file.line_count())
                .collect()
        } else {
            prev.map(|e| e.bookmarks).unwrap_or_default()
        };
        let evicted = files.put(danqing_log::columns::FileEntry {
            path: self.path.to_string_lossy().into_owned(),
            config,
            bookmarks,
            updated: now,
        });
        for e in evicted {
            if !e.bookmarks.is_empty() {
                log::warn!(
                    "LRU 清理了久未打开文件的记忆 (含 {} 条书签): {}",
                    e.bookmarks.len(),
                    e.path
                );
            }
        }
        // 命名会话整片写穿 (T2): 只动本路径切片 —— 保存不抹别处会话
        // (bookmark-persist Critical「读改写覆盖抹全记忆」同族面的会话版封口)。
        files.put_sessions_for_path(self.path.to_string_lossy().as_ref(), self.sessions.clone());
        match files.save_to(&path) {
            Ok(()) => {
                // 迁移**一次性** (D3, 评审 M2): 新账落成后旧名退役 —— rename
                // 不删 (数据不毁), 但从此读侧只可能命中可辨新账，双名稳态
                // 消灭 (双名并存 + 新名再坏 = 会把陈旧旧账当恢复源)。
                let legacy = self.legacy_state_path();
                if legacy.exists() {
                    let retired = legacy.with_extension("json.migrated");
                    if let Err(e) = std::fs::rename(&legacy, &retired) {
                        log::warn!("旧账本退役失败 (不影响本次落盘): {e}");
                    }
                }
                true
            }
            Err(e) => {
                log::warn!("记忆状态落盘失败：{e}");
                false
            }
        }
    }
}
