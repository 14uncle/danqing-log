# TODO: case-insensitive (搜索/过滤默认大小写不敏感)

> plan: `tasks/plan-case-insensitive.md` | spec: `docs/specs/SPEC-case-insensitive.md`
> intent: `docs/intent/case-insensitive.md`
> 逐条勾选推进; 每任务完成后跑三件套 (fmt + clippy + test), 在对应仓内跑。

## Phase 0: 准备

- [x] **T0: 树闸门 + patch + 基线** ✅ 2026-09-15
  - 树闸门: M2 已收口 —— interaction-polish M2/M3/M4/M5 三笔落地
    (`acee55b` / `f25cb0d` / `bfa8c21`), 工作树干净
  - patch: 已开 (`cp tools/local-patch.toml .cargo/config.toml`)
  - 基线实记: **logfile 51 绿** / **本仓 169 绿** = 51 lib + 110 main + 8 genlog
    (2026-09-15 实测。M2–M5 后从记档 133 涨上来 —— 旧 74 main 是 selection-copy 前)
  - Verify: `cargo test` 两仓全绿 ✅; `ls .cargo/config.toml` → PATCH-ON ✅

## Phase 1: danqing-logfile (过滤编译层)

- [x] **T1: `contains_ascii_ci` 原语** ✅ 2026-09-15
  - 实现: 首字节 **lower/upper 一对** 变体 `memchr2` 驱动 + 窗口
    `eq_ignore_ascii_case` 验窗 + 起点上界 `last` 续找
  - **穷举 oracle 当场抓到一个真 bug**: 初版写 `memchr2(first, first.to_ascii_uppercase())`
    —— 首字节本就是大写时两变体同值, 退化成单字节搜索、**漏掉小写候选**
    (`"error level"` 搜 `ERROR` 假阴)。改成 lower/upper 一对即修。
    这就是「正确性靠对拍不靠眼」的实证
  - 对拍规模 28,985 组 (字母表含 0xE9 非 ASCII 字节; 断言 `checked > 20_000` 防缩水)
  - Verify: `cargo test contains_ascii_ci` → 2 绿 ✅
  - 内容: jsonl.rs 加私有 `fn contains_ascii_ci(haystack: &[u8], needle: &[u8]) -> bool`
    —— 首字节双变体 `memchr2` 驱动 + 窗口 `eq_ignore_ascii_case`; 空 needle → true
    (与 memmem 同语义); needle > haystack → false; 窗口扫描右界 `<= len - needle.len()`
  - Acceptance: **regex oracle 对拍** (随机小字母表含非 ASCII 字节, 逐组与
    `(?i-u)`+`escape` 的 `is_match` 一致) + 空 needle / 非 ASCII 首字节 / 贴首尾边界用例
  - Verify: `cargo test` (logfile 仓)
  - Files: `../danqing-logfile/src/jsonl.rs`

- [x] **T2: Bare 裸词不敏感化** ✅ 2026-09-15
  - `Compiled::Bare` 类型不变 (仍 `Vec<u8>`), `line_matches` 换 `contains_ascii_ci`
  - 旧措辞收敛 (**grep 过**: 模块头 / `Compiled::Bare` / `Clause::Bare` 三处
    一并更新; 余下 memmem 命中全是 Flat/FieldExtractor 路径, 属实保留)
  - **旧断言扫描: 零翻红** —— 既有 51 条里没有一条断言裸词大小写敏感
  - 新测试: `bare_word_is_case_insensitive` (四种输入 × 同一批行) /
    `bare_word_does_not_fold_across_multibyte_bytes` (0xE9 不折成 'e'; 汉字查询词)
  - **测试自身一处笔误被逮**: 第二行原写「纯中文」—— 它**本身就含「中文」子串**,
    断言错在期望不在实现。改「纯汉字一行」
  - Verify: `cargo test` → 55 绿 ✅

- [x] **T3: Flat/Verify 值不敏感 + `normalize_clause_keys`** ✅ 2026-09-15
  - 新增私有 `starts_with_ascii_ci` (字节比对, 不要求 UTF-8 边界) ——
    `token_matches` 与 `compare_val` 两处前缀共用; Eq 直接用 `eq_ignore_ascii_case`
  - `normalize_clause_keys(clauses, schema)`: 仅单段 path 查表改写; 查无此列保持
    原样; 多段/裸词跳过 (用 `let [name] = path.as_mut_slice() else` 表达单段)
  - 旧断言: `token_matches_eq_and_prefix` / `compare_val_operators_and_boundaries`
    全是同大小写比较 → **零翻红**
  - 新测试: `field_value_match_is_case_insensitive` (值/前缀/数值算子不受影响) /
    `normalize_clause_keys_maps_to_real_column_name` (改写·查无此列·多段·裸词·
    端到端) / `nested_key_stays_case_sensitive_but_value_does_not` (D6 边界锁)
  - Verify: `cargo test` → 58 绿 ✅

- [x] **Checkpoint A: logfile 三件套全绿** ✅ 2026-09-15 —— `cargo fmt` (无改动) +
  `cargo clippy --all-targets -- -D warnings` (0 警告) + `cargo test` **58 绿**
  (基线 51 + 新增 7)。改动面: `../danqing-logfile/src/jsonl.rs` 单文件

## Phase 2: 本仓接线

- [x] **T4: 搜索 (?i) 两分支 + 逃逸舱锁** ✅ 2026-09-15
  - `build_search_pattern` 两分支套 `(?i)`; view.rs 高亮处加同源注释
    (「只认 `app.search_pattern` 这一串, 勿另拼 pattern」)
  - **口径翻转 1 处**: `build_search_pattern_utf8_keeps_regex_gbk_literalizes`
    的期望串加前缀 (`(?-u)\xD6…` → `(?i)(?-u)\xD6…`) —— 非回归, 已注明
  - 新锁: `search_pattern_is_case_insensitive_by_default_with_opt_out` (逃逸舱
    `(?-i)` 恢复敏感) / `case_prefix_does_not_change_regex_class_semantics`
    (`\w` 仍认汉字、`\d` 仍认全角数字 —— 防有人把 `(?i)` 改成 `(?i-u)`)
  - Verify: `cargo test --bin danqing-log` → 112 绿 ✅
  - 内容: `build_search_pattern` (main.rs:1092) —— UTF-8 分支 `format!("(?i){query}")`;
    非 UTF-8 分支 `format!("(?i){}", bytes_as_literal_regex(...))`; view.rs:915-921
    加同源注释 (「与搜索同一串, 勿在第二处拼 pattern」)
  - Acceptance: `build_search_pattern_utf8_keeps_regex_gbk_literalizes` (main.rs:1656)
    口径翻转改写 (两分支都带 `(?i)` 前缀); 新锁 —— `(?i)(?-i)ERROR` 只中全大写
    (逃逸舱) / `\w` 类语义不受前缀影响 / GBK 分支输出 = `(?i)` + 字节转义
  - Verify: `cargo test`
  - Files: `src/main.rs`, `src/view.rs` (注释)

- [x] **T5: `parse_filter` 单点规范化** ✅ 2026-09-15
  - 新增 `LogApp::parse_filter` (parse + schema 在手时 `normalize_clause_keys`);
    三处调用点 (apply_filter / 巨量追平作业 / live-tail 重滤) 全部改走它;
    open.rs worker 经调用点一的入参继承, 未加新面
  - `filter_applied` 仍存**用户原文** (状态栏回显与清空重放都拿它)
  - 新测试: `parse_filter_normalizes_keys_only_when_schema_present` (无 schema
    原样 / 有 schema 改写 / 裸词与多段不动) / `apply_filter_keeps_raw_query_for_display`
  - Verify: `cargo test --bin danqing-log` → 114 绿 ✅
  - 内容: main.rs 新增 `LogApp::parse_filter(&self, query) -> Vec<Clause>`
    (parse_query + schema 在手则 normalize_clause_keys); 三处调用点改走它:
    `apply_filter` (:945) / 追加作业构造 (:525) / live-tail 重滤 (:798)。
    `filter_applied` 存**用户原始串**不变 (状态栏无惊讶); open.rs worker 经 :525
    继承, 不加新调用点
  - Acceptance: 测试 —— schema 含 `level` 列时 `LEVEL=ERROR` 过滤命中与
    `level=ERROR` 相同; schema 为 None (.log) 时行为不变; grep 确认 main.rs 里
    `parse_query` 直达调用清零 (全走 parse_filter)
  - Verify: `cargo test`
  - Files: `src/main.rs`

- [x] **T6: 字段口径分类器不敏感化 + 逐桶对拍 (D2 红线)** ✅ 2026-09-15
  - `classify_field_value`: 首字节 `to_ascii_uppercase` 分派 + 共用
    `jsonl::starts_with_ascii_ci` (已提为 pub, 与过滤侧**同一原语** = 口径同源)
  - `classify_level` **不动**, 并在注释里写明「2026-09-15 复访后仍保持敏感」
    (防后人当漏改)
  - **口径翻转 2 处** (均非回归): ① `field_value_matches_by_prefix` 的
    `error → Other` 改为 `→ Error` (附翻转理由); ② D2 逐桶对拍测试的期望
    (小写 error 归 Error, Other 由 3 → 2)
  - **fixture 加强**: 加入 `Warn` / `fatal` 混合写, 9 行 6 桶逐桶相等仍成立
  - Verify: `cargo test` → 173 绿 (51 + 114 + 8; 基线 169) ✅
  - 内容: `classify_field_value` (levels.rs:151) 首字节分派折叠 +
    前缀比较不敏感化; **`classify_level` 不动** (spec 修正 D7: 行口径敏感是
    levels.rs:110 既定反污染设计, 旧断言保留即为敏感锁); `find_level_column`
    已不敏感, 零改动 (查证记录)
  - Acceptance: **混合大小写 fixture 逐桶对拍** —— 合成 JSONL (level 值含
    `warn`/`Warn`/`WARNING`/`error`/`FATAL` 等), `count_levels_field` 每桶计数
    == `run_filter(field_query(列, 桶))` 命中数, 逐桶相等; `classify_level`
    小写归「其他」的旧断言**仍在且绿** (敏感锁)
  - Verify: `cargo test`
  - Files: `src/levels.rs`

- [ ] **Checkpoint B: 本仓三件套全绿** (patch 态) —— fmt + clippy + test

## Phase 3: 实测与文档

- [x] **T7: logbench 复测 + 文档刷新** ✅ 2026-09-15
  - **两条超线** (详见 spec §4 修正段): 搜索·字面+正则类 **897-999ms** (原线 700) /
    Bare 裸词 **190ms** (原线 150)
  - **成因用对照组定死, 不靠推理**:
    - 搜索: 同轮 `(?i)` 897 / `(?i-u)` 675 / 敏感 129 —— regex prefilter 在折叠下
      失效; `(?i-u)` 只省 25% 却要赔上 `\w`/`\d` 的 Unicode 语义 (D2 已拒)
    - Bare: **regex 引擎自己的** `(?i-u)error` 整缓冲扫描 **181ms** ≈ 我手写逐行
      **190ms** —— 证明「150ms 这条线任何实现都达不到」(memmem 的精确 prefilter
      无折叠模式); 不是手写实现的问题
  - 其余全达标: 短字面 115 / 交替 120 / Flat 过滤 **40ms** (零代价成立) /
    计数·字段口径 25 / 索引 92-94 (未受影响)
  - 预算线已按实测重订 (搜索·最差 ≤1000ms / Bare ≤250ms), 留痕 spec §4
  - 文档刷新: README (性能表 + 搜索/过滤功能行) / PERFORMANCE_REPORT (两处搜索行
    + 字段过滤行) / **ms-store-copy (性能表 + 两条功能行)** —— 对外文案里的旧数字
    71/69ms 已同步, 老数字明确标注为「敏感时代值」
  - `grep -rni 大小写` 全 docs 扫描: 无残留过时声称 ✅
  - Verify: logbench 输出 (见 spec §4 表) ✅
  - 内容: release 构建 logbench, 对 demo-1gb.log / demo-1gb.jsonl 复测 spec §4
    全表 (搜索短字面/交替/最差形态 + Flat/Bare 过滤); 过滤不敏感经 `--filter
    "level=error"` 等真实走新代码路径; 数字落档本节。文档: README 性能数字刷新 +
    用法节补「默认大小写不敏感, 正则 `(?-i)` 恢复敏感」一句; PERFORMANCE_REPORT
    搜索行同步; **grep 扫描** `大小写|case.sensitive|case.insensitive` 全 docs
    (含 ms-store-copy 禁止声称清单)
  - Acceptance: §4 预算逐行达标; 数字实记于此; 文档无过时声称
  - Verify: logbench 输出贴本节; `grep -rni "case\|大小写" docs/ README.md`
  - Files: `README.md`, `PERFORMANCE_REPORT.md`, 视扫描结果而定
  - 实测落档: ______ (build 后填)

- [x] **Checkpoint C: 预算判定** ✅ 2026-09-15 —— 6/8 达标; 2 条超线**已找成因**
  (对照组实测证实原线不可达) 并按实测重订, 非凑数 (spec §4 修正段 + T7 记录)

## Phase 4: review → simplify → 联动 → 验收

- [x] **T8: review + code-simplify** ✅ 2026-09-15 (审查 REQUEST CHANGES → 必修 + 全部建议已落地)
  - **code-simplify 收口 (2 处)**:
    ① `contains_ascii_ci` 手写 `while` + 反复重切片的循环 → `memchr2_iter(..).any(..)`,
    12 行 → 5 行 (候选窗口先切尾, 越界判定随之消失);
    ② `histogram.rs` 的 `active_queries` 快照字段是多余的 —— 重算就发生在
    `self.queries` 赋值之后, 直接用即可 (少一个字段 + 每次一次克隆)。
    两处都靠既有测试兜住 (穷举 oracle 121k→24k 组仍全绿)
  - **穷举 oracle 规模回调**: 加了大写 `B` 后对拍涨到 12 万组、套件从 1.0s 拖到
    5.0s。收到 needle ≤2 (约 2.4 万组) —— 函数分支只按「空/单字节/有 rest」三态分,
    更长 needle 不增覆盖; 位置多变体由 haystack 侧提供。**套件回到 1.0s**
  - **最终三件套 (两仓各 3 连)**: logfile **58 绿 ×3** / 本仓 **178 绿 ×3**
    (51 + 119 + 8); 两仓 clippy **0 警告**; fmt 无改动
  - **结论: REQUEST CHANGES** (独立审查代理, 五轴)
  - **判定为做对了的**: `contains_ascii_ci` 全边界核对无误 (空 needle/超长/贴边/
    重叠候选/非 ASCII 不折叠; lower-upper 一对确实必要); 三处应用内过滤解析路径
    确实全走 `parse_filter`; 「桶计数 == 筛选结果」是**构造同源** (共用
    `starts_with_ascii_ci` + 同一 FieldExtractor) 而非碰巧
  - **Critical: 无**

  **必修 (2 条)**:
  - [x] **R1 `Cargo.lock` 是 path 态, 不能提交** —— 属 T9 工序 (关 patch → 重解 →
        复钉), 已在 T9 落档; 审查同时提醒了 `cargo update` 报
        `did not match any packages` 的那个变体 (先 `cargo check` 再 update)
  - [x] **R2 直方图「生效桶 / 清除筛选」指示器没跟着规范化** ✅ **已修**
        (`src/histogram.rs`): 原判定拿 `queries[i] == app.filter_applied` **比原串**,
        故手打 `LEVEL=ERROR*` 时 —— 结果确实被筛了, 却**没有桶行高亮**、且
        「清除筛选」**点不动** (它只在 `active.is_some()` 时可点)。这是本模块
        「同口径」主张唯一没兜住的地方。
        修法: 两侧都过 `parse_filter` 比**子句集** (`Clause: PartialEq`);
        重算按「过滤原串 或 子句表变化」缓存 (`active_src` / `active_queries`
        两个字段), **不进每帧 sync 热路径** (规范化是分配型操作)。
        回归锁 `active_bucket_follows_normalized_filter_not_raw_string`
        (大小写不同的键 / 同写法 / 不等价查询 / 空过滤 四种)
  - [ ] **R1 待 T9 执行** (需 push 授权)

  **Optional / Nit 全部落地 (2026-09-15 恢复后)**:
  - [x] `src/bin/logbench.rs` —— **第二条过滤构造线 (违反 D9)** ✅ 已修:
        该处直接 `parse_query` 不规范化 → `LEVEL=ERROR` 报 **0 命中**。
        修后实测 **36123 命中**, 与 `level=error` 逐数一致 (它是 §4 数字与 §7
        验收弹药的来源, 不修则验收工具本身说谎)
  - [x] `contains_ascii_ci` 最坏 O(n·m) ✅ 已加「已知并接受 + 后路」注释
  - [x] `normalize_clause_keys` 无谓重分配 ✅ 加 `if col.name != *name` 守卫
  - [x] 非字符串标量折叠 (`flag=TRUE` 命中 `true`) ✅ 落 spec §5 边界第 7 条
  - [x] `Op::Eq` 文档「全等」 ✅ 改为「等值 (ASCII 不敏感, 非字节全等)」,
        `Prefix` 同步标注; 模块头补上 `starts_with_ascii_ci`
  - [x] 穷举 oracle 字母表 ✅ 加了大写 `B` (`[a, A, b, B, 0xE9]`, 对拍规模
        约 12 万组)
  - [x] 三件套复测 ✅ logfile 58 绿 / 本仓 178 绿 (51+119+8); 两仓 clippy 0

  **审查另记一条覆盖缺口 (如实)**: `bare_word_does_not_fold_across_multibyte_bytes`
  用的是 `0xE9` (Latin-1 é), **没跑真 GBK 双字节序列** —— 而那才是 spec D3/D4
  接受边界所谈的形状。真 GBK 只在外层 logbench 手工验过 (7884 行), 不在单测里。

  **停手原因 (2026-09-15)**: 另一个会话正在并发写 `src/main.rs` / `src/view.rs`
  (用户裁定「我停手, 等那边收工」)。停手时状态: 我的改动**全部在位未被覆盖**
  (已用 grep 核), R2 已落地并随 118 绿编译通过 —— 但**那个绿是混着对方半成品的**,
  不能当验收基线。**恢复时第一件事: 重测三件套基线**, 再继续上面的 Optional 项。

  - Verify: 审查报告要点已全量归档于本条目 ✅ (原报告另存于会话记录)

- [x] **T9: 联动收口** ✅ 2026-09-15 (本仓**未 push** —— 用户选「不 push 等看过」)
  - **logfile**: 三件套 3 连绿 (58×3) → commit `b35e2bb` → **push `baee0a8..b35e2bb`** ✅
  - **身份**: logfile 仓此前**没有本地 git 覆盖**, 全部历史是全局的
    `tomato <ganweihun@yunksj.com>` (与农场约定不符, 且那是工作邮箱 + 公开仓库)。
    用户裁定 → 本仓设 local override `十四叔 <gwhun@qq.com>`, 本笔起生效
  - **复钉**: 关 patch (`rm .cargo/config.toml`) → `cargo check` **一步**重解到
    `danqing-logfile#b35e2bbe` (远端拉的) → lock 三行 git 源核对:
    danqing **`24bd9a4f` 未动** (本模块零框架改动; 且框架仓 HEAD 恰为
    `24bd9a4f` 在 origin/dev, 故重解无漂移) / encoding `d86741ca` 未动 /
    logfile **`b35e2bbe`**; `cargo check --locked` 通过 = 可复现
  - **提交**: 本仓 `ceede56` —— 12 文件 (含 CLAUDE.md 状态条目 + 测试基线
    115 → **178** 订正)。**精确暂存**: 对方会话在本仓留有**新一轮未提交在途改动**
    (`focus_bar` → `focus_target` 泛化 + matrix 文档), **一行未扫入**
  - **混提处置 (用户裁定: 留着不动)**: `ee770f5` 里已含本模块 main.rs/view.rs
    的改动, 本笔 message 首段已注明「该部分随 ee770f5 落地」
  - **余**: 本仓 push 待用户点头 (dev 现 ahead 2)
  - Verify: `git show --stat HEAD` ✅ / lock 三行 ✅ / `--locked` ✅
  - 内容: logfile 三件套 → commit (message 注明关联本模块) → **push logfile** →
    本仓 `rm .cargo/config.toml` 关 patch → `cargo check` (重解) →
    `cargo update -p danqing-logfile` (复钉) → `--locked` 无 patch 构建验证 →
    本仓 commit (含 spec/intent/plan/todo 文档 + 源码 + lock) → push 待用户点头
  - Acceptance: lock 钉 git rev (非 path 态); `--locked` 构建过; 两仓分别提交
  - Verify: `grep -A2 'name = "danqing-logfile"' Cargo.lock` 显示 git+…#rev
  - Files: `Cargo.lock` + 本批全部文件

- [ ] **Checkpoint D (用户闸门): 人工验收 spec §7 六条** —— 全过后进 v1.0 发布链
  (重拍截图 → 打 tag → GitHub Release → 商店提交)
