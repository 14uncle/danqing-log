//! @author 十四叔
//! @date 2026/09/29
//! LogApp · 过滤/搜索/书签簇: 行集过滤、命中导航、书签切换与跳转。
use super::*;

impl LogApp {
    /// 点侧栏柱条：套用该桶的过滤子句 (复用既有过滤通路，零新语法);
    /// 点的已是当前生效项 → 清除 (切换语义)。
    ///
    /// 子句来自 `level_queries` —— 它是**按当前文件的级别类列**生成的，
    /// 不是写死的 `level=X`: 列名可能是 severity/lvl, 值可能是 WARNING。
    /// 无子句的桶 (`其他` / 合并的 DEBUG+TRACE / 明文模式) 在侧栏侧已挡，
    /// 此处再兜一层 —— 消息源不止一处时不会漏。
    pub(crate) fn apply_level_filter(&mut self, level: Level) {
        let Some(q) = self.level_queries[level as usize].clone() else {
            return;
        };
        if self.filter_applied == q {
            self.update(Msg::ApplyFilter(String::new()));
        } else {
            self.update(Msg::ApplyFilter(q));
        }
    }

    /// 实时过滤：增量行追加命中表 (只跑 `[from, …)`，不全量重跑)。
    pub(crate) fn append_filter_hits(&mut self, from: u64) {
        if self.filter_applied.is_empty() {
            return;
        }
        // 过滤在途窗口禁行 (R5 族): 此刻 `filtered` 还是旧串的行集，
        // 把新串的增量命中合进去是错账 —— 在途的全程重跑本就会覆盖追加行。
        if self.filter_pending {
            return;
        }
        if self.filtered.is_none() {
            return;
        }
        let clauses = self.parse_filter(&self.filter_applied);
        let new_hits = jsonl::run_filter_from(&self.file, &clauses, from);
        self.merge_filter_hits(new_hits);
    }

    /// 摘掉过滤表中 `>= from` 的旧命中 —— 它们落在本次重算区间内，会被重新跑出来。
    ///
    /// 必须与 [`Self::append_filter_hits`] 的起点**同一个 `from`**: 重叠行若只摘不补
    /// 就漏，只补不摘就重，两种都让底栏行数与侧栏柱条一起偏 (且一起偏就意味着
    /// D2 的对照检查看不出来)。
    pub(crate) fn drop_filter_hits_from(&mut self, from: u64) {
        if self.filter_pending {
            return; // 同 append_filter_hits: 在途窗口内不动旧行集
        }
        let Some(existing) = &self.filtered else {
            return;
        };
        // 表按行号升序 (过滤产出即有序), 故二分找到第一个 >= from 的位置
        let keep = existing.partition_point(|&l| l < from);
        if keep == existing.len() {
            return; // 无命中落在重算区间，无需摘
        }
        let mut kept = existing.as_ref()[..keep].to_vec();
        kept.shrink_to_fit();
        self.filtered = Some(Arc::new(kept));
    }

    /// 合并增量命中进过滤表 (本地扫描与 worker 下沉共用合并半段，review R2)。
    pub(crate) fn merge_filter_hits(&mut self, new_hits: Vec<u64>) {
        if new_hits.is_empty() {
            return;
        }
        let Some(existing) = &self.filtered else {
            return;
        };
        let mut merged = existing.as_ref().clone();
        merged.extend(new_hits);
        self.filtered = Some(Arc::new(merged));
    }

    /// 解析过滤查询 —— **全应用唯一的过滤解析入口** (parse + 键名规范化)。
    ///
    /// 三处调用点 (应用过滤 / 巨量追平作业 / live-tail 重滤) 必须都走这里：
    /// 键名规范化 (`LEVEL=ERROR` → `level=ERROR`) 只做在一处，同一个查询串在不同
    /// 路径上就会得到不同命中集 —— 而增量与全量不一致只在「开着过滤又赶上追加」
    /// 时才现形，是最难查的一类差异 (D2 红线同款理由)。
    ///
    /// `.log` 文件 schema 为 None → 跳过规范化，行为与从前一字不差。
    pub(crate) fn parse_filter(&self, query: &str) -> Vec<jsonl::Clause> {
        let mut clauses = jsonl::parse_query(query);
        if let Some(schema) = self.schema.as_deref() {
            jsonl::normalize_clause_keys(&mut clauses, schema);
        }
        clauses
    }

    /// Enter (过滤): 应用。空查询 = 回全量; 非空走 AsyncJob (1GB 亚秒，不冻界面)。
    pub(crate) fn apply_filter(&mut self, query: String) {
        self.filter_applied = query.clone();
        self.filter_clear_rev += 1; // 应用后清空输入框 (显示"已应用"占位)
        if query.is_empty() {
            // 回全量同步生效 —— 顺手作废旧一轮在途过滤：它晚到会覆盖掉
            // 这里的 None (评审 R5 同族：在途窗口内行集与过滤串脱钩)。
            self.filter_job.invalidate();
            self.filter_pending = false;
            self.filtered = None;
            self.filter_landed.clear();
            self.filter_elapsed = None;
            self.set_top(0.0);
            self.set_selected(0);
            self.refresh_status();
            return;
        }
        let clauses = self.parse_filter(&query);
        let file = Arc::clone(&self.file);
        self.set_status(format!("{} · 过滤 \"{query}\" 中…", self.base_status));
        self.filter_pending = true;
        self.filter_job.launch(move || {
            let t = Instant::now();
            let lines = jsonl::run_filter(&file, &clauses);
            FilterOutcome {
                lines,
                elapsed: t.elapsed(),
            }
        });
    }

    /// Esc (过滤): 清已应用过滤，回到全量。
    pub(crate) fn clear_filter(&mut self) {
        self.filter_clear_rev += 1;
        self.filter_job.invalidate(); // 在途结果不得晚到复活 (同 R5 族)
        self.filter_pending = false;
        self.filter_applied.clear();
        self.filter_landed.clear();
        self.filter_elapsed = None;
        self.filtered = None;
        self.set_top(0.0);
        self.set_selected(0);
        self.refresh_status();
    }

    /// 表格/原始互切 (JSONL 检出才可用): 进表格自动聚焦过滤栏，回原始清 focus_bar。
    pub(crate) fn toggle_mode(&mut self) {
        if self.schema.is_none() {
            return;
        }
        if self.mode == ViewMode::Table {
            self.mode = ViewMode::Raw;
        } else {
            self.mode = ViewMode::Table;
            self.focus_target = Some("log-bar");
        }
        self.refresh_status();
    }

    /// 聚焦搜索栏 (`/` 原始模式 / Ctrl+F 任意模式)。
    ///
    /// **T21 (P39): 不再「聚焦即干净开始」**。Ctrl+F 是「回到搜索框」的**反射键**,
    /// 而原实现每次都把已输入未应用的草稿清掉 —— 反射键不该销毁工作。现在：
    /// 没持焦 → 聚焦 (草稿原样留着); 已持焦 → 全选 (直接覆写)。
    /// 清空仍归 Esc, 那条路径没动。
    pub(crate) fn open_search(&mut self) {
        self.focus_target = Some("log-bar");
        self.search_refocus_rev += 1;
        self.refresh_status();
    }

    /// Esc (搜索): 清搜索态，栏保持可见 (搜索栏始终显示，不可隐藏)。
    pub(crate) fn clear_search(&mut self) {
        // 在途搜索一并作废 (`clear_filter` 同规，评审 M7): 不作废的话，清空/
        // 应用空搜索会话后旧搜索结果会经 tick 拾取复活 —— spec「代次拒旧护在途」
        // 的搜索侧半边。
        self.search_job.invalidate();
        self.search_clear_rev += 1;
        self.search = None;
        self.search_query.clear();
        self.search_pattern = None;
        self.search_elapsed = None;
        self.refresh_status();
    }

    /// Enter (搜索): 栏内有输入 = 应用新搜索; 栏空 = 下一命中。
    pub(crate) fn apply_search(&mut self, q: String) {
        if q.is_empty() {
            return;
        }
        let pattern = build_search_pattern(self.file.encoding(), &q);
        let Ok(re) = regex::bytes::Regex::new(&pattern) else {
            self.set_status_error(format!("{} · 搜索 \"{q}\" 正则无效", self.base_status));
            return;
        };
        self.search_clear_rev += 1; // 应用后清空输入框 (显示"已应用"占位)
        self.search_query = q.clone();
        let file = Arc::clone(&self.file);
        self.set_status(format!("{} · 搜索 \"{q}\" 中…", self.base_status));
        self.search_job.launch(move || {
            let t = Instant::now();
            let (hits, total, _) = file.search(&re, SEARCH_HIT_CAP);
            SearchOutcome {
                hits,
                total,
                elapsed: t.elapsed(),
                pattern,
                query: q,
            }
        });
    }

    /// 跳到命中行 (置视口中部，选中跟随)。
    pub(crate) fn jump_to_file_line(&mut self, file_line: u64) {
        let row = self.display_row_of(file_line);
        self.set_top(clamp_top(
            row as f64 - PAGE_ROWS / 2.0,
            self.display_count(),
        ));
        self.set_selected(row);
    }

    pub(crate) fn next_hit(&mut self) {
        if let Some(line) = self.search.as_mut().and_then(SearchNav::jump_next) {
            self.jump_to_file_line(line);
            self.refresh_status();
        }
    }

    pub(crate) fn prev_hit(&mut self) {
        if let Some(line) = self.search.as_mut().and_then(SearchNav::jump_prev) {
            self.jump_to_file_line(line);
            self.refresh_status();
        }
    }

    /// `b` / Ctrl+B: 切换选中行书签 (按文件行号，过滤模式下语义不漂移)。
    /// 状态栏补动作反馈 —— 此前只有行号变金一个信号，用户按完不知道成没成;
    /// 计数由 `refresh_status` 的常驻「书签 N」段承担，这里只缀动作。
    /// **幽灵行号守卫** (评审 R1): `line_at` 的 `unwrap_or((0,0))` 会把空文件/
    /// 过滤 0 命中/越界选中塌成「行 0」—— 落盘即跨会话污染，无有效显示行 =
    /// 拒绝 + 说清 + 零变更。**上限守卫** (D3): 满
    /// [`danqing_log::columns::MAX_BOOKMARKS`] 拒绝新增 + 说清 + 零变更
    /// (删除照常 —— 守卫不得堵死腾位路径)。增删成功即落盘 (D2), 落盘失败
    /// **不许说谎** (评审 R①)。
    pub(crate) fn toggle_bookmark(&mut self) {
        // 合并工作区：书签打在 (源，文件行) pack 键上，进 merge bundle ——
        // 单文件书签集与 state.json 通路**零触碰** (D4); 合并书签的持久化
        // 随会话载荷走 (T8), T3 会话内有效。
        if let Some(m) = self.merge_active_mut() {
            let Some(row) = m.row_at(m.selected) else {
                self.set_notice("当前无有效行可夹书签".into(), NoticeKind::Warn);
                return;
            };
            match m.toggle_bookmark(row.src, row.line, danqing_log::columns::MAX_BOOKMARKS) {
                Some(true) => self.set_notice("已添加书签".into(), NoticeKind::Info),
                Some(false) => self.set_notice("已去掉书签".into(), NoticeKind::Info),
                None => self.set_notice(
                    format!(
                        "书签已达上限 {}, 先去掉一些",
                        danqing_log::columns::MAX_BOOKMARKS
                    ),
                    NoticeKind::Warn,
                ),
            }
            return;
        }
        let Some((line, _)) =
            expand::file_line_at(self.cur_selected(), self.lines(), &self.expanded)
        else {
            self.set_notice("当前无有效行可夹书签".into(), NoticeKind::Warn);
            return;
        };
        let added = if self.bookmarks.remove(&line) {
            false
        } else if self.bookmarks.len() >= danqing_log::columns::MAX_BOOKMARKS {
            self.set_notice(
                format!(
                    "书签已达上限 {}, 先去掉一些",
                    danqing_log::columns::MAX_BOOKMARKS
                ),
                NoticeKind::Warn,
            );
            return;
        } else {
            self.bookmarks.insert(line);
            true
        };
        let saved = self.save_state();
        // **次序陷阱**: `set_notice` 内含 `refresh_status` 会重建 status ——
        // 必须先 notice 再 push 后缀，否则「(未落盘)」被重建冲掉 (评审 R①锁伺候)。
        if !saved {
            self.set_notice("书签已改但落盘失败，重启可能丢失".into(), NoticeKind::Warn);
        } else {
            self.refresh_status();
        }
        self.status.push_str(match (added, saved) {
            (true, true) => " · 已添加书签",
            (true, false) => " · 已添加书签 (未落盘)",
            (false, true) => " · 已去掉书签",
            (false, false) => " · 已去掉书签 (未落盘)",
        });
    }

    /// `'` / Ctrl+G: 跳下一书签 (严格大于当前行，环绕)。状态栏报位次 `书签 i/N`。
    pub(crate) fn goto_next_bookmark(&mut self) {
        // 合并工作区：按时间线位置序找下一个 (环绕), 位次报底栏。
        if let Some(m) = self.merge_active_mut() {
            let Some((pos, rank, total)) = m.next_bookmark_pos() else {
                self.set_notice("没有书签 ('b' 夹在当前行)".into(), NoticeKind::Warn);
                return;
            };
            m.selected = pos;
            m.top_row = crate::clamp_top(pos as f64, m.row_count());
            self.set_status(format!("书签 {rank}/{total}"));
            return;
        }
        if let Some(line) = next_bookmark(&self.bookmarks, self.file_line_of(self.cur_selected())) {
            self.jump_to_file_line(line);
            self.refresh_status();
            // line 必在集合内：位次 = 比它小的书签数 + 1
            let i = self.bookmarks.range(..line).count() + 1;
            let n = self.bookmarks.len();
            self.status.push_str(&format!(" · 书签 {i}/{n}"));
        } else {
            // M3 (P25): 无书签时 Ctrl+G 按了没反应 —— 说清为什么。
            self.set_notice("尚无书签 (b 添加)".into(), NoticeKind::Warn);
        }
    }
}

/// 拼查询子句 (SPEC-v1x-field-picker-ui D1): 字段 + 算符 + 值 → 语法串 ——
/// `parse_query` 的逆向壳，**不造新语法** (前缀 = `=` + 值尾 `*`, `jsonl::
/// parse_clause` 现语义)。拒收面见 [`clause_reject_notice`] (判据同源)。
pub(crate) fn build_clause(field: &str, op: jsonl::Op, value: &str) -> Option<String> {
    if clause_reject_notice(field, op, value).is_some() {
        return None;
    }
    Some(match op {
        jsonl::Op::Eq => format!("{field}={value}"),
        jsonl::Op::Prefix => format!("{field}={value}*"),
        jsonl::Op::Gt => format!("{field}>{value}"),
        jsonl::Op::GtEq => format!("{field}>={value}"),
        jsonl::Op::Lt => format!("{field}<{value}"),
        jsonl::Op::LtEq => format!("{field}<={value}"),
    })
}

/// 拒收判据 + 说清文案 (**单一事实源**, 评审 Critical): `Some(文案)` = 拒收并
/// 说清为什么; `None` = 可拼。拼装方 [`build_clause`] 与提交方的提示**只许**经
/// 本函数判 —— 各算一份会漂 (判据变了文案没跟上 = 说错原因)。
///
/// **拒收面必须盖住 parse 破坏面** (评审 Critical, 双路并账): `parse_query` =
/// 空白分 token + `split_operator` 长算符首次出现切分 + Eq 值尾 `*` 改写 Prefix ——
/// 凡会被它**改写语义**的输入一律拒收，不静默拼出另一条查询：
/// - 字段：空 / 含空白 / 含 `.` (扁平键被拆嵌套 = 0 命中面) / 含 `=< >` (算符逃逸)
/// - 值：空 / 含空白 / 含 `<` `>` (算符切分逃逸，泛型/比较片段常见) /
///   **前导 `=`** (与 `>`/`<` 算符拼出双字符算符) / Eq 尾 `*` (被偷换前缀) /
///   Prefix 含 `*` (双重编码)
pub(crate) fn clause_reject_notice(
    field: &str,
    op: jsonl::Op,
    value: &str,
) -> Option<&'static str> {
    let field_reserved = field
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '=' | '<' | '>' | '.'));
    let value_reserved = value
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '<' | '>'))
        || value.starts_with('=');
    let star_abuse = match op {
        jsonl::Op::Eq => value.ends_with('*'),
        jsonl::Op::Prefix => value.contains('*'),
        _ => false,
    };
    if !field_reserved && !value_reserved && !star_abuse && !field.is_empty() && !value.is_empty() {
        return None;
    }

    // 文案分类 (优先级 = 最可能的用户本意在前; 判据不变，只管说哪句)
    Some(if field_reserved {
        "该列名含保留字符 (空格 . = < >), 暂不支持点选查询"
    } else if op == jsonl::Op::Eq && value.ends_with('*') {
        "精确匹配不接受结尾 *, 前缀查询请点 * 钮"
    } else if op == jsonl::Op::Prefix && value.contains('*') {
        "前缀值不能再含 *"
    } else {
        "值不能为空，且不能含空格 / < / > 或以 = 开头"
    })
}

/// 下一书签：严格大于 current 的最小书签，无则环绕到最小书签。空集 None。
pub(crate) fn next_bookmark(set: &std::collections::BTreeSet<u64>, current: u64) -> Option<u64> {
    set.range(current.saturating_add(1)..)
        .next()
        .or_else(|| set.first())
        .copied()
}

/// 搜索模式构造：UTF-8 **存储**编码直接用原查询 (完整正则语法); 非 UTF-8 存储
/// (GBK) 查询转字节 → `\xNN` 字面量 (正则语法退化为字面量，有意边界)。
///
/// 必须用存储编码而非检出编码：UTF-16 文件打开即转 UTF-8 副本，存储编码是 Utf8,
/// 走完整正则路径; 误用 stats().encoding (检出 Utf16*) 会把它踢进字面量分支，
/// `ERROR|FATAL` 之类交替正则静默失效 (review 当场抓住的回归)。
pub(crate) fn build_search_pattern(enc: Encoding, query: &str) -> String {
    // 两分支一律 `(?i)` 前缀 (2026-09-15, spec D2/D3): 默认大小写不敏感。
    // UTF-8 用 `(?i)` 而非 `(?i-u)` —— 查询是用户的裸正则，`(?-u)` 会顺带把
    // `\w`/`\d`/`\b` 降级成 ASCII 语义，与大小写无关的行为不该被本模块改掉。
    // 逃逸舱零代码：用户写 `(?-i)` 即局部恢复敏感 (组内 flag 覆盖，有测试锁)。
    // 非 UTF-8 分支同理套在字节字面量外 —— `(?i)` 对 `(?-u)\xNN` 折叠成立，已用
    // 真 GBK 文件实测 (7884 命中行，与手工 [eE] 展开逐字节一致)。
    if enc == Encoding::Utf8 {
        format!("(?i){query}")
    } else {
        format!(
            "(?i){}",
            bytes_as_literal_regex(&encoding::encode_query(enc, query))
        )
    }
}
