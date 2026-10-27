//! @author 十四叔
//! @date 2026/09/29
//! LogApp · 合并时间线簇: 归并发起/重建/交卷、源管理、追踪过滤。
use super::*;

impl LogApp {
    /// 保证列配置与当前 schema 对账 (幂等，≤16 列): 提交/落盘的前置 (D4)。
    pub(crate) fn merge_columns(&mut self) {
        if let Some(s) = &self.schema {
            let names: Vec<String> = s.columns.iter().map(|c| c.name.clone()).collect();
            self.columns.merge_with_schema(&names);
        }
    }

    /// 启动合并 (T3 内部直驱通路; 可见入口 + Feature::MergeTimeline 门控在 T4)。
    /// 源列表 = 当前文件 (主源，D4) + 追加源 (去重); 归并进后台作业。
    pub(crate) fn start_merge(&mut self, extra: Vec<PathBuf>) {
        if self.workspace == Workspace::Merge {
            return; // 已在合并：源增删走源管理弹层 (T4), 本通路不叠加
        }
        let mut paths: Vec<PathBuf> = Vec::new();
        if self.has_file {
            paths.push(self.path.clone());
        }
        for p in extra {
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
        if paths.len() < 2 {
            self.set_notice(
                "合并至少需要两个源 (当前文件 + 追加)".into(),
                NoticeKind::Warn,
            );
            return;
        }
        if paths.len() > danqing_log::merge_view::MAX_SOURCES {
            self.set_notice(Self::merge_cap_notice(), NoticeKind::Warn);
            return;
        }
        // cancel 插桩在源间 (build_merge 内); UI 侧作废走 AsyncJob 代次，不设显式取消钮 (T3)。
        // T8: 用户另起归并 = 作废旧会话恢复载荷 (防错嫁到新归并)。
        self.pending_merge_apply = None;
        self.merge_job_live = true;
        self.merge_job.launch(move || {
            danqing_log::merge_view::build_merge(
                &paths,
                &[],
                &std::sync::atomic::AtomicBool::new(false),
            )
        });
        self.set_notice("合并中…".into(), NoticeKind::Info);
    }

    /// 归并作业拾取 (tick 每帧)。**交付才清在途标记** —— 摘「先清后 poll」:
    /// 首帧 poll 空转就把 live 清了 = 「重归并在途不叠加」门禁在飞行中提前
    /// 开门 (T7 遗留错形，T8 测试撞出：pump 首拾取即返回，作业还在飞)。
    pub(crate) fn pickup_merge_job(&mut self) {
        let Some(out) = self.merge_job.poll() else {
            return;
        };
        self.merge_job_live = false;
        self.apply_merge_outcome(out);
    }

    /// 加源 (弹层「加源…」/ 对话框选定): 未合并 = 以此起并 (当前文件主源);
    /// 已合并 = 源列表 + 本源**重归并** (carry 保书签/显隐/选中)。
    /// 上限/去重拒绝明示 (D7)。
    pub(crate) fn add_merge_source(&mut self, path: PathBuf) {
        if self.workspace == Workspace::Merge {
            let Some(m) = self.merge.as_ref() else {
                return;
            };
            let mut paths: Vec<PathBuf> = m.sources.iter().map(|s| s.path.clone()).collect();
            if paths.contains(&path) {
                self.set_notice("该源已在合并中".into(), NoticeKind::Warn);
                return;
            }
            if paths.len() >= danqing_log::merge_view::MAX_SOURCES {
                self.set_notice(Self::merge_cap_notice(), NoticeKind::Warn);
                return;
            }
            paths.push(path);
            self.rebuild_merge(paths);
        } else {
            self.update(Msg::StartMerge(vec![path]));
        }
    }

    /// 减源 (弹层「移除」作用选中源; 指针语义): 重归并。
    /// **D 闸 (2026-09-29 用户裁定, G 组验收缺陷②)**: 只剩两源时**不出手** ——
    /// 2 源删 1 = 合并解体, 那不是本钮承诺的事 (旧行为: 移除生效但弹层行列表
    /// 零可见变化, 用户读作「移除不了」); 拦下并指路「退出合并」。
    /// ≥3 源正常删 (删后恒 ≥2, 故不再有「不足两源」分支)。
    pub(crate) fn remove_selected_merge_source(&mut self) {
        let Some(sel) = self.merge_source_selected.clone() else {
            self.set_notice("先点选源再移除".into(), NoticeKind::Warn);
            return;
        };
        let Some(m) = self.merge.as_ref() else {
            self.set_notice("当前没有合并".into(), NoticeKind::Warn);
            return;
        };
        if m.sources.len() <= 2 {
            self.set_notice(
                "只剩两源 —— 要退出合并请用「退出合并」".into(),
                NoticeKind::Warn,
            );
            return;
        }
        let paths: Vec<PathBuf> = m
            .sources
            .iter()
            .map(|s| s.path.clone())
            .filter(|p| p.as_path() != std::path::Path::new(&sel))
            .collect();
        self.merge_source_selected = None;
        self.rebuild_merge(paths);
    }

    /// 改**选中**源的时间参数 (T5 腿 D): 偏移/时区在解析边界单源施加 ——
    /// 只重提该源时间戳 + 重归并 (文件行索引不动)。无选中 = 提示 (指针语义)。
    /// 变换给出 (新偏移，新时区); None 臂 = 该参数不动。
    pub(crate) fn edit_source_time(
        &mut self,
        f: impl FnOnce(i64, i64) -> (Option<i64>, Option<i64>),
    ) {
        let Some(sel) = self.merge_source_selected.clone() else {
            self.set_notice("先点选源再改时间参数".into(), NoticeKind::Warn);
            return;
        };
        let Some(m) = self.merge.as_mut() else {
            self.set_notice("当前没有合并".into(), NoticeKind::Warn);
            return;
        };
        let Some(src) = m
            .sources
            .iter()
            .position(|s| s.path.as_path() == std::path::Path::new(&sel))
        else {
            self.set_notice("选中源已不在合并中".into(), NoticeKind::Warn);
            return;
        };
        let (off, tz) = (m.sources[src].offset_ms, m.sources[src].tz_offset_ms);
        let (new_off, new_tz) = f(off, tz);
        m.set_time_params(src, new_off.unwrap_or(off), new_tz.unwrap_or(tz));
        self.discard_trace_job(); // 时间参数变了 = 过滤行集作废 (rebuild_masked)
        self.refresh_status();
    }

    /// 源列表变化后的重归并 (加/减源共用): 后台重跑 build_merge,
    /// 交卷时 [`merge_view::carry_view_state`] 按路径把书签/显隐/选中搬过来。
    /// **源参数按路径随行** (评审 T9 补): 时钟偏移/时区是用户调过的现场,
    /// 不加/减源就丢 —— 旧 `carry_view_state` 只搬显隐 (那是评审抓到的静默重置)。
    pub(crate) fn rebuild_merge(&mut self, paths: Vec<PathBuf>) {
        self.discard_trace_job(); // 源集合变了
        // T8: 改源归并 = 作废旧会话恢复载荷 (防错嫁到新归并)。
        self.pending_merge_apply = None;
        let saved = self
            .merge
            .as_ref()
            .map(|m| m.snapshot_group().sources)
            .unwrap_or_default();
        self.merge_job_live = true;
        self.merge_job.launch(move || {
            danqing_log::merge_view::build_merge(
                &paths,
                &saved,
                &std::sync::atomic::AtomicBool::new(false),
            )
        });
        self.set_notice("重归并中…".into(), NoticeKind::Info);
    }

    // ---- 腿 E (T6): req_id 追踪 ----
    /// 追踪选中值：值提取 (JSONL 行落在字符串字面量内 → 放大整个字段值;
    /// 否则选区原文，.log 同款) → **单条 Bare 子句** (不经 parse_query, 见
    /// `trace_clause` 注释) → per-source run_filter 后台作业。
    pub(crate) fn start_trace(&mut self, src: u32, line: u32, lo: usize, hi: usize) {
        let (value, files) = {
            let Some(m) = self.merge_active() else {
                self.set_notice("追踪只在合并视图内可用".into(), NoticeKind::Warn);
                return;
            };
            // 下面两条 = 拓扑在「手势 → 落地」之间变过 (加/减源/换文件)。**出声**
            // (P24: 不许静默吞动作), 不猜用户想追哪一行。
            let Some(source) = m.sources.get(src as usize) else {
                self.set_notice("合并源已变化，请重新选中再追踪".into(), NoticeKind::Warn);
                return;
            };
            let text = danqing_log::merge_view::row_text(&source.file, line);
            let Some(sel) = text.get(lo..hi) else {
                self.set_notice("该行内容已变化，请重新选中再追踪".into(), NoticeKind::Warn);
                return;
            };
            let value: &str =
                if matches!(source.route, danqing_log::timestamp::TsRoute::JsonlField(_)) {
                    match danqing_log::merge_view::snap_jsonl_string(&text, lo, hi) {
                        Some((a, b)) => &text[a..b],
                        None => sel,
                    }
                } else {
                    sel
                };
            if value.trim().is_empty() {
                self.set_notice(
                    "追踪值为空 —— 双击或框选消息里的追踪值 (如 req_id)".into(),
                    NoticeKind::Warn,
                );
                return;
            }
            let files: Vec<Arc<LogFile>> = m.file_handles();
            (value.to_string(), files)
        };
        let clause = danqing_log::merge_view::trace_clause(&value);
        let v = value.clone();
        self.trace_job.launch(move || {
            let t = Instant::now();
            let (hits, scanned) = danqing_log::merge_view::trace_hits(&files, &clause);
            danqing_log::merge_view::TraceOutcome {
                hits,
                scanned,
                value: v,
                anchor: (src, line),
                elapsed: t.elapsed(),
            }
        });
        self.set_notice(format!("追踪 \"{value}\" 中…"), NoticeKind::Info);
    }

    /// 源上限拒绝文案 (D7 同一句话，两个入口：起并 / 加源) —— 收口一处，
    /// 免得上限改了只改一处; 上限值本身仍取 `merge_view::MAX_SOURCES` 真身。
    fn merge_cap_notice() -> String {
        format!("合并源上限 {} 个", danqing_log::merge_view::MAX_SOURCES)
    }

    /// **在途追踪作废** (R5 族唯一收口): 命中表按「某时刻的源集合」编号 ——
    /// 源集合变 (加/减源/换工作区/显隐/时间参数) 或工作区离场后，旧表对新状态
    /// 就是错账：轻则把命中贴到别源，重则按 `src` 索引越界 (release=abort)。
    /// 所有「集合/参数/工作区变了」的路径都必须调它; `apply_trace_outcome` 另有
    /// 到点校验兜底 (评审 C1 双保险)。
    pub(crate) fn discard_trace_job(&mut self) {
        self.trace_job.invalidate();
    }

    /// 追踪交卷落地 (tick 拾取 / 测试同步注入共用 —— apply_merge_sync 先例):
    /// 过滤行集进 MergeState + 选中锚回发起行 + 视口跟上 + 底栏两通道反馈
    /// (notice = 对动作的回答; status = 常驻「追踪中」态)。
    pub(crate) fn apply_trace_outcome(&mut self, out: danqing_log::merge_view::TraceOutcome) {
        let Some(m) = self.merge_active_mut() else {
            return; // 已退出合并 → 丢 (R5 族)
        };
        // 源集合不符 = 换过源 (在途窗口里加/减源或换过工作区) → 命中表按旧序号
        // 编号，用了就是越界/错贴。丢弃并**出声** (评审 C1; P24: 不许静默)。
        if out.hits.len() != m.sources.len() {
            self.set_notice(
                "合并源已变化，本次追踪作废 (请重新追踪)".into(),
                NoticeKind::Warn,
            );
            return;
        }
        // T7 缝：在途窗口内源可能已追加 —— 扫描快照行数 < 当前行数的源，
        // 缺口 [scanned-1, 当前) 增量补滤 (退一行 = 末行补全改判同区间，
        // 摘/补对称同 apply_appended 纪律), 不许追踪永久缺那窗里进来的行。
        let mut hits = out.hits;
        for (i, s) in m.sources.iter().enumerate() {
            let cur = s.file.line_count();
            let scanned = out.scanned.get(i).copied().unwrap_or(cur);
            if cur > scanned {
                let from = scanned.saturating_sub(1);
                let gap = jsonl::run_filter_from(
                    &s.file,
                    &[danqing_log::merge_view::trace_clause(&out.value)],
                    from,
                );
                let h = &mut hits[i];
                let keep = h.partition_point(|&l| l < from);
                h.truncate(keep);
                h.extend(gap); // 升序接升序 (from 分界)
            }
        }
        let n = m.apply_trace(hits, out.anchor);
        m.trace = Some(out.value.clone());
        m.top_row = m.selected.saturating_sub(3) as f64; // 锚行上方留三行上下文
        self.set_notice(
            format!(
                "追踪 \"{}\" · 命中 {n} 行 ({} ms)",
                out.value,
                out.elapsed.as_millis()
            ),
            NoticeKind::Info,
        );
        self.refresh_status();
    }

    /// 归并交卷换入：建 MergeState + 切 Merge 工作区 + 拒收源明示 (SPEC D2);
    /// 旧 bundle 在场 = carry 保书签/显隐/选中 (加减源重建不丢，D4)。
    pub(crate) fn apply_merge_outcome(&mut self, out: danqing_log::merge_view::MergeOutcome) {
        // 源集合整体换入的唯一落地点 (评审 C1: 不在此作废，旧命中表必越界)
        self.discard_trace_job();
        let rejected = out.rejected.len();
        let n_sources = out.sources.len();
        let rows = out.index.len();
        let mut fresh = danqing_log::merge_view::MergeState::from_outcome(out);
        if let Some(old) = self.merge.as_ref() {
            // 加/减源重建: 旧 bundle 在场 → 书签/显隐/选中按路径搬 (D4 不丢)。
            // 首次合并 (old = None) 不走 —— 全新状态。
            danqing_log::merge_view::carry_view_state(old, &mut fresh);
        }
        // T8: 会话恢复载荷殿后 (carry 先跑 = 书签/展开零触碰, 载荷后跑 =
        // 显隐以保存值为准)。返回有变化的源数 (0 = 全同，零打扰)。
        let restored = match self.pending_merge_apply.take() {
            Some(saved) => fresh.apply_saved_params(&saved),
            None => 0,
        };
        self.merge = Some(fresh);
        self.workspace = Workspace::Merge;
        if rejected > 0 {
            self.set_notice(
                format!("{rejected} 个源未加入 (探测失败), 已明示; 合并 {n_sources} 源 {rows} 行"),
                NoticeKind::Warn,
            );
        } else if restored > 0 {
            self.set_notice(
                format!("合并会话参数已套回 ({restored} 源，{n_sources} 源 {rows} 行)"),
                NoticeKind::Info,
            );
        }
        self.focus_target = Some("log-view");
        self.refresh_status();
    }
}
