//! @author 十四叔
//! @date 2026/09/29
//! LogApp · 导出簇: 导出入口/格式/行集冻结/作业发起与交卷。
use super::*;

impl LogApp {
    /// CSV 列序列集 (export D5): **schema 首见序全列** —— 显示配置不影响交付物
    /// (SPEC-v1x-table-column-config D5): 摆列/隐藏/拖宽都不改变导出列。
    pub(crate) fn export_csv_columns(&self) -> Vec<String> {
        self.schema
            .as_ref()
            .map(|s| s.columns.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }

    /// 导出入口 (SPEC-v1x-export D6/D7): 作业态点 = 取消 (单作业，同一按钮);
    /// 否则门控 —— 免费态弹统一升级提示，**保存对话框之前** (先让人选完路径再
    /// 告诉他不能存是最坏的顺序)。付费态开格式小菜单。
    pub(crate) fn export_entry_clicked(&mut self) {
        if self.export_job.is_running() {
            // worker 可能已收尾、只是 tick 还没拾取 (review Optional): 先拾取 ——
            // 否则用户先看到「正在取消」下一帧又弹「导出完成」, 双态拧巴。
            if let Some(r) = self.export_job.poll() {
                self.handle_export_result(r);
                return;
            }
            self.export_job.cancel();
            self.set_notice("正在取消导出…".into(), NoticeKind::Info);
            return;
        }
        // 两道在途闸开在**入口** (review B-R1/Optional; 与 D6「门控点位 = 入口」同点位):
        // 过滤计算中导出 = 静默拿到上一份行集 (首筛在途 = 全文件);
        // 搜索命中被导航表封顶 = 静默截断交付物 (对账事故) —— 都挡在格式菜单之前。
        if self.filter_pending {
            self.set_notice("过滤计算中，请稍候再导出".into(), NoticeKind::Warn);
            return;
        }
        if let Some(nav) = &self.search {
            if (nav.hits().len() as u64) < nav.total() {
                self.set_notice(
                    "搜索命中超过 100 万，导航表已封顶 —— 请收窄搜索后再导出".into(),
                    NoticeKind::Warn,
                );
                return;
            }
        }
        if !self.has_file {
            self.set_notice("尚未打开文件 (Ctrl+O 打开)".into(), NoticeKind::Warn);
            return;
        }
        if !self.entitlement.allows(Feature::Export) {
            self.update(Msg::ShowUpgradePrompt(Feature::Export));
            return;
        }
        self.close_popovers(); // 互斥 (D3/D1): 开一关二，双 scrim 不叠
        self.export_menu_open = true;
    }

    /// 格式菜单选定 → 保存对话框 (D8) → 启动作业。
    /// 明文非 JSONL 时美化/CSV **服务端同款守门** (菜单收口是 UI 层，点到了也不放行)。
    pub(crate) fn begin_export(&mut self, pick: ExportPick) {
        self.export_menu_open = false;
        if pick != ExportPick::Raw && self.schema.is_none() {
            self.set_notice("本文件非 JSONL, 仅可导出原始行".into(), NoticeKind::Warn);
            return;
        }
        let Some((stem, scope, ext)) = self.export_name_parts(pick) else {
            return;
        };
        let stamp = export::now_stamp();
        let default_name = export::default_export_name(&stem, scope, &stamp, &ext);
        let Some(path) = rfd::FileDialog::new()
            .set_title("导出")
            .set_file_name(&default_name)
            .save_file()
        else {
            return; // 对话框取消：零副作用
        };
        self.launch_export(path, pick);
    }

    /// 文件名三段：stem / scope 中缀 / 扩展名 —— 口径真身在
    /// [`export::scope_suffix`] / [`export::ext_for`], 这里只备料。
    pub(crate) fn export_name_parts(
        &self,
        pick: ExportPick,
    ) -> Option<(String, &'static str, String)> {
        let stem = self.path.file_stem()?.to_string_lossy().into_owned();
        let scope = export::scope_suffix(
            self.schema.is_some(),
            self.filtered.is_some(),
            self.search.is_some(),
        );
        let src_ext = self.path.extension().map(|e| e.to_string_lossy());
        let ext = export::ext_for(pick, src_ext.as_deref());
        Some((stem, scope, ext))
    }

    /// 行集快照 (spec D1): 口径真身在 [`export::line_set_of`] —— 这里只备料。
    pub(crate) fn export_line_set(&self) -> ExportSet {
        export::line_set_of(
            self.file.line_count(),
            self.schema.is_some(),
            self.filtered.as_ref().map(|h| h.as_slice()),
            self.search.as_ref().map(|n| n.hits().as_slice()),
        )
    }

    /// 启动作业 (D2): 冻结行集 + `Arc` 文件快照 (换文件由 `invalidate` 作废旧轮)。
    pub(crate) fn launch_export(&mut self, path: std::path::PathBuf, pick: ExportPick) {
        let set = self.export_line_set();
        let format = match pick {
            ExportPick::Raw => ExportFormat::Raw,
            ExportPick::Pretty => ExportFormat::Pretty,
            ExportPick::Csv => {
                // export D5: schema 首见序全列 —— 显示配置不影响交付物 (T5 回归锁)
                ExportFormat::Csv {
                    columns: self.export_csv_columns(),
                }
            }
        };
        if self
            .export_job
            .launch(Arc::clone(&self.file), set, format, path)
        {
            self.set_notice("导出中…".into(), NoticeKind::Info);
        } else {
            // 单作业语义下 UI 不应撞到; 撞到也不能「点了保存什么都没发生」(评审 Nit)
            self.set_notice("已有导出进行中".into(), NoticeKind::Warn);
        }
    }

    /// 导出收尾反馈 (完成/取消/失败一律如实报)。
    pub(crate) fn handle_export_result(&mut self, r: export::ExportResult) {
        match r.end {
            ExportEnd::Done { lines, bad_lines } => {
                let mut msg = format!("导出完成 {lines} 行 → {}", r.path.display());
                if bad_lines > 0 {
                    msg.push_str(&format!(" (含 {bad_lines} 行非 JSON, 已原样)"));
                }
                self.set_notice(msg, NoticeKind::Info);
            }
            ExportEnd::Cancelled { .. } => {
                self.set_notice("导出已取消，半成品已删除".into(), NoticeKind::Info);
            }
            ExportEnd::Failed { error } => {
                self.set_notice(format!("导出失败：{error}"), NoticeKind::Warn);
            }
        }
    }
}
