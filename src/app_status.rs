//! @author 十四叔
//! @date 2026/09/29
//! LogApp · 状态栏与提示簇: notice 生命周期、底栏合成、Loading 文案。
use super::*;

impl LogApp {
    /// 合成底栏状态：base + 模式 + 过滤 + 搜索。job 在途时整行被 loading 覆盖。
    /// loading 显示三元 (底栏动词，文件名，进度细节): job 在途才有。
    /// refresh_status 的整行覆盖源 (计算与 mutation 分离)。
    fn loading_parts(&self) -> Option<(&'static str, String, String)> {
        let job = self.open_job.as_ref()?;
        let (done, total) = job.progress();
        let name = job
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // 索引之后的阶段 (列发现 / 级别计数) 没有细粒度字节进度 —— 继续显示百分比
        // 就会卡在 99% 不动，用户看到的等待于是和状态栏那个「索引 N ms」对不上
        // (2026-09-12 用户反馈)。如实报阶段名，把这段等待显性化。
        if let Some(phase) = job.phase_name() {
            return Some((phase, name, "…".to_string()));
        }
        let verb = match job.kind() {
            OpenKind::Fresh => "正在索引",
            OpenKind::Rebuild => "重建中",
            OpenKind::Append => "追平中",
        };
        // done==0 = 刚发起或 UTF-16 读取/转码段 (无细粒度钩子，见 plan D2)
        let detail = if done == 0 {
            "读取中…".to_string()
        } else if let Some(pct) = (done * 100).checked_div(total) {
            // 封顶 99: 在途分子可超分母 (索引期间文件增长 / UTF-16 转码口径),
            // 完成时 loading 分支随 job 消失，永远看不到 100 (review O1)
            format!("{}% · {}/{} MiB", pct.min(99), done >> 20, total >> 20)
        } else {
            "…".to_string()
        };
        Some((verb, name, detail))
    }

    /// 置一条底栏提示 —— **notice 的唯一入口** (T18)。
    ///
    /// 自带消退期限 (Q3 裁的「自动消退」): 提示是**对刚才那个动作**的回答，
    /// 一直赖在底栏会变成噪声，还会让「底栏读数」这件事失去可信度。
    ///
    /// **`NOTICE_TTL` 是待实机核对的估值**: spec 说「具体时长 build 时**实测定**,
    /// 不估算」, 而本机跑不了真机走查 —— 故先取一个，并挂进矩阵 §6 的核对单
    /// (实机那轮把「太短没看见 / 太长碍事」两个方向都试一次)。
    pub(crate) fn set_notice(&mut self, text: String, kind: NoticeKind) {
        self.notice = Some((text, kind));
        self.notice_until = Some(Instant::now() + NOTICE_TTL);
        self.refresh_status();
    }

    /// 清 notice (toast 点掉 / 到点消退同途 —— 单点收口，SPEC-notice-visibility T1)。
    /// 清完必须刷底栏 (家族病史⑤: set_notice/refresh_status 次序陷阱)。
    /// apply_fresh 里那组清置**不调它** —— 那处嵌在更大的清置序列里，序列尾
    /// 已有一次 refresh_status, 不必多刷。
    pub(crate) fn dismiss_notice(&mut self) {
        self.notice = None;
        self.notice_until = None;
        self.refresh_status();
    }

    /// notice 到点即消退 (T18/Q3)。抽成独立方法是为了**可测**: `tick` 要
    /// `AnimationCtx`, 而本方法不必。
    pub(crate) fn expire_notice(&mut self) {
        if self.notice_until.is_some_and(|t| Instant::now() >= t) {
            self.dismiss_notice();
        }
    }

    pub(crate) fn refresh_status(&mut self) {
        // 合并工作区 (腿一 T3): 底栏报合并口径 (源数/行数/跟随/书签),
        // 不拼单文件行数 —— 单文件 base_status 在合并期间保持冻结，退出即还原。
        if let Some(m) = self.merge_active() {
            // 追踪态 (T6): 行数口径 = 过滤后命中数，值与清除路径常驻明示
            // (无过滤栏的合并视图里，底栏是「当前有过滤在生效」的唯一去处)。
            let mut s = if let Some(v) = &m.trace {
                format!(
                    "合并：{} 源 · 追踪 \"{}\" → {} 行 (Esc 清除)",
                    m.sources.len(),
                    v,
                    m.row_count()
                )
            } else {
                format!("合并：{} 源 · {} 行", m.sources.len(), m.row_count())
            };
            if m.follow {
                s.push_str(" · 跟随");
            }
            // T7: 断流源计数常驻 (弹层行内也有标记) —— 「这条时间线有一部分
            // 不再更新」必须随时可见，否则用户拿旧行当实时。
            let stale = m.sources.iter().filter(|s| s.stale).count();
            if stale > 0 {
                s.push_str(&format!(" · 断流 {stale} 源"));
            }
            if !m.bookmarks.is_empty() {
                s.push_str(&format!(" · 书签 {}", m.bookmarks.len()));
            }
            self.set_status(s);
            return;
        }
        if let Some((verb, name, detail)) = self.loading_parts() {
            self.set_status(format!("{verb} {name} · {detail}"));
            // 无旧文件才上占位文案 (有旧文件：列表照画，进度只上底栏)
            self.loading_label = if self.has_file {
                None
            } else {
                Some((name.clone(), format!("{verb} {detail}")))
            };
            return;
        }
        self.loading_label = None;
        let mut s = self.base_status.clone();
        if self.mode == ViewMode::Table {
            s.push_str(" · JSONL 表格");
        }
        if !self.filter_applied.is_empty() {
            let elapsed = self
                .filter_elapsed
                .map(|d| format!(" ({} ms)", d.as_millis()))
                .unwrap_or_default();
            s.push_str(&format!(
                " · 过滤 \"{}\" → {}/{}{}",
                self.filter_applied,
                self.display_count(),
                self.file.line_count(),
                elapsed
            ));
        }
        if !self.search_query.is_empty() {
            let nav = self
                .search
                .as_ref()
                .and_then(|n| n.position())
                .map(|(k, n)| format!("第 {k}/{n} 命中"))
                .unwrap_or_else(|| "无命中".into());
            let total = self.search.as_ref().map(|n| n.total()).unwrap_or(0);
            let total_tag = if total > SEARCH_HIT_CAP as u64 {
                format!(" (总命中 {total})")
            } else {
                String::new()
            };
            let elapsed = self
                .search_elapsed
                .map(|d| format!(" ({} ms)", d.as_millis()))
                .unwrap_or_default();
            s.push_str(&format!(
                " · 搜索 \"{}\" → {nav}{total_tag}{elapsed}",
                self.search_query
            ));
        }
        if !self.bookmarks.is_empty() {
            s.push_str(&format!(" · 书签 {}", self.bookmarks.len()));
        }
        if self.follow {
            s.push_str(" · FOLLOW");
        }
        // notice **不进这个串** —— 它是第二条通道，由 `LogView::paint` 单独取色单独
        // 落笔 (T11)。此前把它拼进来，结果是同一句话被画两遍 (串尾一遍、notice 段
        // 又一遍), 而且「警示色」和「常态色」压在同一个字符串上根本没处分。
        self.set_status(s);
    }

    /// 写底栏**常态**信息 —— 顺手清掉错误态 (错误是**这一句**的属性，换句就没了)。
    pub(crate) fn set_status(&mut self, s: String) {
        self.status = s;
        self.status_error = false;
    }

    /// 写底栏**错误** —— **常驻红，不消退**。
    ///
    /// P27 的收口 (2026-09-15 用户裁定「后者」): 「正则无效」这类错误**不走
    /// notice 通道** —— notice 有 4 秒消退期，而它是「你刚按的那下没生效」,
    /// 不该自己消失。判据是 P27 原文那句「**错误在视觉上不存在**」: 修之前它与
    /// 打开耗时/过滤统计同色同字号, 只有读文字才知道出错了。
    pub(crate) fn set_status_error(&mut self, s: String) {
        self.status = s;
        self.status_error = true;
    }
}

/// 底栏打开统计一行流 (截图弹药)。
pub(crate) fn status_text(path: &Path, file: &LogFile) -> String {
    let s = file.stats();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // 报**打开总墙钟**在先，行索引在后。
    //
    // 原来只报「索引 N ms」, 而它不含 UTF-16 的读整文件 + 转码 —— 实测 100 MiB
    // UTF-16LE: 报「索引 7 ms」而实际 open 200 ms (**13 倍**), GB 级按比例是秒级。
    // 「数字和视觉不符」的这类反馈，根子就是报了个不等于等待时间的数字。
    let mut load = format!("打开 {} ms", s.open.as_millis());
    if s.preprocess > Duration::ZERO {
        load.push_str(&format!(" (转码 {} ms)", s.preprocess.as_millis()));
    }
    format!(
        "{name} · {} · {:.1} MiB · {} 行 · {load} · mmap {} us · 索引 {} ms ({:.0} MiB/s)",
        s.encoding.label(),
        s.file_bytes as f64 / (1024.0 * 1024.0),
        s.line_count,
        s.map_us,
        s.index.as_millis(),
        s.index_mib_per_s(),
    )
}
