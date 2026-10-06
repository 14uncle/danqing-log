//! @author 十四叔
//! @date 2026/09/29
//! LogApp · 打开与 live-tail 簇: 增长轮询、轮转重建、追平、打开作业拾取。
use super::*;

impl LogApp {
    /// 增长检测 (live-tail): 文件变长 → `append_from` 增量; 缩容/轮转 → 全量重建。
    pub(crate) fn poll_growth(&mut self) {
        if self.workspace == Workspace::Merge {
            // T7 (腿 F): 合并期间单文件 tail 仍冻结 (app.file 不动，退出后追平
            // 零残留), 但**合并源各自轮询合流** —— 见 poll_growth_merge。
            self.poll_growth_merge();
            return;
        }
        if !self.has_file {
            return; // 空态无文件可轮询
        }
        if self.open_job.is_some() {
            return; // 打开/重建/追平在途: 不叠加 tail 动作 (async-open plan D3)
        }
        let Ok(cur) = FileStat::of(&self.path) else {
            return; // 文件暂不可读 (轮转间隙), 下轮再试
        };
        let known = self.file.stat_snapshot();
        if cur == known {
            return; // 未变化
        }
        if cur.head != known.head {
            // 首块变 = 轮转/覆写 (内容换了), 即便新文件更大也全量重建
            self.rebuild_file();
        } else if cur.len > known.len {
            let delta = cur.len - known.len;
            if delta >= APPEND_SYNC_MAX_BYTES {
                // 巨量追平 (久未轮询后的追平，如隐藏期间暴涨): 转 worker,
                // 旧快照保持可见可滚 + 底栏「追平中」; 在途期间本函数被门禁 (D3)。
                // 过滤激活时子句随行 (review R2): 增量过滤随 worker 下沉，
                // 落点只合并不扫描 —— 否则 GB 级追平的过滤成本回到 UI 线程。
                let old = Arc::clone(&self.file);
                let filter = if !self.filter_applied.is_empty() && self.filtered.is_some() {
                    Some((self.parse_filter(&self.filter_applied), old.line_count()))
                } else {
                    None
                };
                self.open_job = Some(OpenJob::launch_append(
                    &self.path,
                    old,
                    delta,
                    filter,
                    self.level_column.clone(),
                ));
                self.refresh_status();
                return;
            }
            // 同文件常态增长：同步增量追加 (新字节在页缓存，毫秒级)
            match LogFile::append_from(&self.file, &self.path) {
                Ok(new) => self.apply_appended(new, None),
                Err(e) => log::warn!("tail 追加失败：{e:#}"),
            }
        } else {
            // 同文件缩容 (截断): 全量重建
            self.rebuild_file();
        }
    }

    /// 合并工作区的 live-tail (腿 F/T7): **per-source** stat 轮询 ——
    ///
    /// - 未变：跳过; 读取失败：断流标记 (单源断流不拖垮全局，弹层/底栏明示);
    /// - 增长：小增量同步合流 (`MergeState::append_source`); 巨量追平 /
    ///   UTF-16 副本增量不适用 → 全量重归并 (worker, 旧 bundle 保持可见);
    /// - 轮转/缩容 (head 变 / len 缩): 全量重归并 (单文件 rebuild_file 同族)。
    ///
    /// 门禁与单文件同款：重归并在途不叠加 (D3 同族); trace 在途不挡追加
    /// (落地时缺口补滤对账，见 apply_trace_outcome)。
    fn poll_growth_merge(&mut self) {
        if self.merge_job_live {
            return; // 重归并在途不叠加 (打开/重建期间 stat 必过期，下轮再来)
        }
        enum MergeTailAct {
            Append(u32, LogFile),
            Rotate(String),
        }
        let mut acts: Vec<MergeTailAct> = Vec::new();
        let mut dirty = false;
        {
            let Some(m) = self.merge_active_mut() else {
                return;
            };
            for (i, s) in m.sources.iter_mut().enumerate() {
                let Ok(cur) = FileStat::of(&s.path) else {
                    if !s.stale {
                        s.stale = true;
                        dirty = true;
                    }
                    continue;
                };
                // 恢复可读即清断流 (内容是否过期由下面 stat 比对管 —— 能 stat 到
                // 就有资格再试; 追加失败会重新标记)。
                if s.stale {
                    s.stale = false;
                    dirty = true;
                }
                let known = s.file.stat_snapshot();
                if cur == known {
                    continue; // 未变化
                }
                if cur.head != known.head || cur.len < known.len {
                    acts.push(MergeTailAct::Rotate(s.name().to_string()));
                    break; // 一次重建覆盖全部，不用再扫
                }
                let delta = cur.len - known.len;
                // 巨量追平 / UTF-16 转码副本 (增量不适用，append_from 会退全量
                // 且转码整文件 —— 那是 worker 的活，不在 UI 线程付): 全量重归并
                let utf16 = matches!(
                    s.file.encoding(),
                    danqing::encoding::Encoding::Utf16Le | danqing::encoding::Encoding::Utf16Be
                );
                if delta >= APPEND_SYNC_MAX_BYTES || utf16 {
                    acts.push(MergeTailAct::Rotate(s.name().to_string()));
                    break;
                }
                match LogFile::append_from(&s.file, &s.path) {
                    Ok(new) => {
                        let grew = new.line_count() > s.file.line_count();
                        // 行数没长也可能有事：旧末行被追加**补全** (写了一半的行
                        // 续完) —— 内容/ts 都改判，不落地会一直显示残行。
                        let tail_completed = !grew && {
                            let oc = s.file.line_count();
                            oc > 0 && new.line(oc - 1) != s.file.line(oc - 1)
                        };
                        let new_rows = new.line_count().saturating_sub(s.file.line_count());
                        if new_rows > MERGE_SYNC_MAX_ROWS {
                            // 大批：交 worker 全量重归并 (加数 + 一键) —— 增量合流
                            // 的代价 ∝ 批行数 + 回找深度，大批不该在 UI 线程付
                            // (T9 实测：引擎整批单遍后仍随批行数线性长)。
                            acts.push(MergeTailAct::Rotate(s.name().to_string()));
                            break;
                        }
                        if grew || tail_completed {
                            acts.push(MergeTailAct::Append(i as u32, new));
                        }
                        // 否则：半行在写中，下轮再说
                    }
                    Err(_) => {
                        if !s.stale {
                            s.stale = true;
                            dirty = true;
                        }
                    }
                }
            }
        }
        for act in acts {
            match act {
                MergeTailAct::Append(src, new) => {
                    if let Some(m) = self.merge_active_mut() {
                        m.append_source(src, new);
                        dirty = true;
                    }
                }
                MergeTailAct::Rotate(name) => {
                    // 轮转/截断/巨量/UTF-16: 全量重归并 (旧 bundle 保持可见至交卷)
                    let paths: Vec<PathBuf> = self
                        .merge
                        .as_ref()
                        .map(|m| m.sources.iter().map(|s| s.path.clone()).collect())
                        .unwrap_or_default();
                    self.rebuild_merge(paths);
                    self.set_notice(
                        format!("源 {name} 已轮转/截断, 重归并中…"),
                        NoticeKind::Warn,
                    );
                    return; // rebuild_merge 自带提示与状态，余下动作下轮再扫
                }
            }
        }
        if dirty {
            self.refresh_status();
        }
    }

    /// 缩容/轮转: 异步全量重建 (旧快照保持可见可滚，spec 裁决);
    /// pickup 走 [`Self::apply_rebuild`] 重置链。
    fn rebuild_file(&mut self) {
        let path = self.path.clone();
        self.open_job = Some(OpenJob::launch(OpenKind::Rebuild, &path));
        self.refresh_status();
    }

    /// Rebuild 换入 (worker 交卷): 清失效状态 (书签越界丢弃) + 状态提示
    /// (原 rebuild_file 重置链; base_status 换新打开统计 —— 同步时代留旧串，
    /// 轮转后底栏数字失真，异步化顺带修正)。
    pub(crate) fn apply_rebuild(&mut self, path: &Path, out: OpenOutcome) {
        // review C1: 旧内容上的在途 filter/search 结果不得贴到新内容
        self.filter_job.invalidate();
        self.search_job.invalidate();
        let OpenOutcome {
            file,
            schema,
            level_column,
            ..
        } = out;
        let base_status = status_text(path, &file);
        let new_count = file.line_count();
        self.file = Arc::new(file);
        // 格式可能整体换了 → 列名与子句表跟着换 (不沿用旧的); 计数重新后台算
        self.adopt_level_column(level_column);
        self.launch_levels_job();
        // review R4: 轮转后格式可能变了 (JSONL↔明文), schema/mode 用 worker 新发现
        self.schema = schema.map(Arc::new);
        self.mode = if self.schema.is_some() {
            ViewMode::Table
        } else {
            ViewMode::Raw
        };
        // 弹层是旧内容语境 (评审 R4: 列集可能整体换) —— 与 apply_fresh 同纪律关尽
        self.close_popovers();
        // 列配置对账 (评审 C4, apply_fresh 同纪律): 同路径不重读盘，但 schema 可能
        // 换了列集 —— merge 收敛失配序/隐/宽 + ≥1 可见兜底 (C2 在 merge 内收口)。
        self.merge_columns();
        self.base_status = base_status;
        self.bookmarks.retain(|&l| l < new_count);
        self.filtered = None;
        self.filter_applied.clear();
        self.filter_landed.clear();
        self.filter_pending = false;
        // 换文件/重建 = 分析结果作废 (D8 后半句：文件语境没了);
        // 在途作业作废 —— 旧文件的分析结果不得贴到新文件 (async-open C1 同款纪律)
        self.analysis_result = None;
        self.analysis_filter_src.clear();
        self.analysis_running = false;
        self.analysis_job.invalidate();
        self.filter_elapsed = None;
        self.search = None;
        self.search_query.clear();
        self.search_pattern = None;
        self.search_elapsed = None;
        self.expanded = ExpandMap::new();
        self.sub_rows.clear();
        self.set_top(0.0);
        self.set_selected(0);
        self.set_notice("文件已截断/轮转".into(), NoticeKind::Warn);
    }

    /// 热替换文件 (Ctrl+O / 拖拽): 异步管道发起 (在途旧 job 被 drop = 取消);
    /// 旧视图保持至 worker 交卷 (spec 裁决 A), 换入走 [`Self::apply_fresh`]。
    pub(crate) fn reload_file(&mut self, new_path: PathBuf) {
        // D4 切换语义：打开新文件 = 回单文件工作区 (合并 bundle 保留，不丢)。
        self.workspace = Workspace::Single;
        self.open_job = Some(OpenJob::launch(OpenKind::Fresh, &new_path));
        self.refresh_status();
    }

    /// Fresh 换入 (worker 交卷): 全部状态重建，窗口不重建 (原 reload_file 重置链)。
    pub(crate) fn apply_fresh(&mut self, new_path: PathBuf, out: OpenOutcome) {
        // review C1: 旧文件上的在途 filter/search 结果不得贴到新文件
        self.filter_job.invalidate();
        self.search_job.invalidate();
        // SPEC-v1x-export D2: 换文件同纪律 —— 在途导出作废 (worker 删半成品收尾)
        self.export_job.invalidate();
        // 弹层族同灭 (review R4 + D1): 弹层是旧文件语境的 —— async-open 在途
        // 开着菜单，落地后点格式会导出的是**新**文件，一起作废。
        self.close_popovers();
        let OpenOutcome {
            file: new_file,
            schema,
            level_column,
            ..
        } = out;
        let base_status = status_text(&new_path, &new_file);
        log::info!("{base_status}");
        if let Some(s) = &schema {
            log::info!(
                "JSONL 检出，列化 {} 列：{}",
                s.columns.len(),
                s.columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        let new_file = Arc::new(new_file);
        let schema = schema.map(Arc::new);
        let mode = if schema.is_some() {
            ViewMode::Table
        } else {
            ViewMode::Raw
        };
        self.file = new_file;
        self.has_file = true;
        self.path = new_path;
        self.base_status = base_status;
        self.mode = mode;
        self.schema = schema;
        // 列配置载入 (T5/D4): schema 就位后取该路径记忆摆法并对账 (先 schema 才能 merge)
        self.load_state_for_current_file();
        self.adopt_level_column(level_column);
        self.launch_levels_job();
        self.set_top(0.0);
        self.set_selected(0);
        self.filtered = None;
        self.filter_applied.clear();
        self.filter_landed.clear();
        self.filter_pending = false;
        self.analysis_result = None;
        self.analysis_filter_src.clear();
        self.analysis_running = false;
        self.analysis_job.invalidate();
        self.filter_clear_rev += 1;
        self.filter_elapsed = None;
        self.search_clear_rev += 1;
        self.search = None;
        self.search_query.clear();
        self.search_pattern = None;
        self.search_elapsed = None;
        // 书签不在这清：`load_state_for_current_file` 是**替换**语义 (D2) ——
        // 记忆恢复或清空都在那一处发生; 这里再 clear 会把刚载入的记忆清掉
        // (plan 核实⑤次序陷阱，锁 `bookmarks_are_per_path_and_apply_fresh_replaces_with_memory`)。
        self.expanded = ExpandMap::new();
        self.sub_rows.clear();
        self.follow = false;
        self.notice = None;
        self.notice_until = None;
        // **把焦点送进列表** (T14 之后高亮只在持焦时画): 不送的话，打开文件看到的
        // 是「一行都没选中」, 而按 ↑↓ 只动底栏行号、屏上什么都不动 —— 正是本模块
        // 自己那条判据要消灭的「按了没反应」。只在 **Fresh** (换了文件) 时送：
        // rebuild/append 走的是 `apply_rebuild`/`apply_appended`, 不动焦点，免得
        // 轮转或追长时把正在栏里打字的用户拽走。
        self.focus_target = Some("log-view");
        self.refresh_status();
    }

    /// Append 换入 (同步小追加 / 追平 worker 交卷同链): 增量过滤 + follow 滚底。
    /// `worker_hits` = worker 已算好的增量命中 (review R2: 巨量追平过滤下沉);
    /// None = 本地扫 (同步小追加，毫秒级)。
    pub(crate) fn apply_appended(&mut self, new: LogFile, worker_hits: Option<Vec<u64>>) {
        let old_line_count = self.file.line_count();
        // 重算起点**退一行**: 旧快照末行可能以无换行结尾、被本次追加补全改判
        // (review R1)。计数与过滤必须同起点同区间，否则柱条数字与筛选结果
        // 当场分岔 —— 即 D2 红线破裂，而这正是 review 前两侧同步漂移掩盖掉的那个形态。
        //
        // 这里与 worker 各自独立算出同一个 `from` (worker 用发起时的旧行数，这里用
        // 落地时的) —— 二者能相等，靠的是 `poll_growth` 开头的 `open_job.is_some()`
        // 门禁：在途期间不叠加任何 tail 动作，故 `self.file` 不会在 launch 与落地
        // 之间被别的追加换掉。**若将来允许并发追加，这个摘/补对称会静默失效**
        // (摘多了漏行、摘少了重计), 届时须把 `from` 随产物一起交回来。
        let from = old_line_count.saturating_sub(1);
        // 计数：已就绪 → 在 UI 线程做「重叠一行」的绝对量更新 (KB 级增量，便宜),
        // **先算再换入** (update_for_append 需要旧快照)。
        //
        // 未就绪 → **什么都不做**: 计数作业交付时会拿它自己的快照与当时的文件
        // 对账 (见 `pickup_levels_job`)。这里若重起作业，一个持续增长的 tail
        // 会把计数一遍遍从头来过 —— 永远算不完，侧栏永远挂在「…」。
        let recomputed = (!self.levels_pending).then(|| {
            levels::update_for_append(
                &self.file,
                &new,
                *self.level_counts,
                self.level_column.as_deref(),
            )
        });
        self.file = Arc::new(new);
        if let Some(c) = recomputed {
            self.level_counts = Arc::new(c);
        }
        // 过滤：先摘掉将被重算区间的旧命中，再合并新命中 (否则重叠行出现两次)
        self.drop_filter_hits_from(from);
        match worker_hits {
            Some(hits) => self.merge_filter_hits(hits),
            None => self.append_filter_hits(from),
        }
        if self.follow {
            self.set_top(self.max_top());
            self.set_selected(self.display_count().saturating_sub(1));
        }
        self.refresh_status();
    }

    /// 打开管道拾取 (async-open): 完成/失败都收摊 (take → drop 旧 job 语义),
    /// 按 kind 分派换入链; 失败按 kind 分流 (Fresh notice / 余静默待重试)。
    pub(crate) fn pickup_open_job(&mut self) {
        let Some(res) = self.open_job.as_mut().and_then(OpenJob::poll) else {
            return;
        };
        let job = self.open_job.take().expect("在途 job");
        match res {
            Ok(out) => {
                // 落地耗时 (自发起): 与 worker 的 `perf open_phases` 对照 ——
                // 两者相减即「交付 + 拾取」的延迟; 若落地很快而用户仍等很久，
                // 瓶颈就在落地之后的渲染，不在这条管道。
                log::info!(
                    "perf open_landed: {:?} 自发起 (kind={:?})",
                    job.elapsed_since_launch(),
                    job.kind()
                );
                match job.kind() {
                    OpenKind::Fresh => self.apply_fresh(job.path().to_path_buf(), out),
                    OpenKind::Rebuild => self.apply_rebuild(job.path(), out),
                    OpenKind::Append => {
                        if out.rebuilt {
                            // 追加退化全量重建 (UTF-16/缩容，review R3): 走 rebuild 重置链
                            self.apply_rebuild(job.path(), out);
                        } else {
                            let OpenOutcome {
                                file,
                                incremental_hits,
                                ..
                            } = out;
                            self.apply_appended(file, incremental_hits);
                        }
                    }
                }
            }
            Err(e) => {
                // 「索引已取消」= 主动取消，静默; 失败语义按 kind 分流 (保旧行为):
                // Fresh 失败 notice + 留空态/旧视图; Rebuild/Append 静默，
                // 文件仍过期，下轮 250ms poll 自然驱动重试。
                if !e.to_string().contains(INDEX_CANCELLED) {
                    match job.kind() {
                        OpenKind::Fresh => {
                            log::warn!("打开失败：{e:#}");
                            self.set_notice(
                                format!(
                                    "无法打开：{}",
                                    job.path()
                                        .file_name()
                                        .map(|n| n.to_string_lossy())
                                        .unwrap_or_default()
                                ),
                                NoticeKind::Warn,
                            );
                        }
                        OpenKind::Rebuild => log::warn!("轮转重建失败：{e:#}"),
                        OpenKind::Append => log::warn!("tail 追加失败：{e:#}"),
                    }
                }
                self.refresh_status();
            }
        }
    }
}
