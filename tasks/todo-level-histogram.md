# TODO: level-histogram (级别计数侧栏)

> spec: `docs/specs/SPEC-level-histogram.md` | plan: `tasks/plan-level-histogram.md`
> 逐条勾选推进; 每任务完成后跑三件套 (fmt + clippy + test)。
> 构建序: 风险尖兵 → 明文垂直切片 → JSONL 垂直切片 → 增量 → 交互收尾。

## Phase 0: 风险尖兵

- [x] **T1: 分类器 `classify_level`** ✅ 2026-09-12
  - 说明: 单一谓词 `classify_level(&[u8]) -> Level`, 6 桶按优先级首个命中即定;
    明文传行首 200 字节, JSONL 传字段值, 两条路径共用 (口径统一靠它)
  - Acceptance: 6 桶 (FATAL/ERROR/WARN/INFO/DEBUG+TRACE/其他);
    `FATAL` 先于 `ERROR`; 一行含多个级别词**只计一次**;
    含干扰子串 (`informational`/`errors`) 的行不误判;
    **6 桶之和 == 总行数** (其他桶兜底不漏不重)
  - Verify: `cargo test`
  - Files: `src/levels.rs` (新增), `src/lib.rs`
  - Scope: S
  - 实测: 11 个 levels 单测绿; 全量 **51 绿** (27 lib + 16 main + 8 genlog), clippy 0。
    两处实现决策: ① **大小写敏感** (与 `level_color`/`level_cell_color` 一致) ——
    `"finished with no errors found"` 归「其他」, 否则正文会污染 ERROR 计数;
    代价是小写 `"error:"` 不识别, 已在 doc comment 记明。② **FATAL 与 ERROR 拆桶**
    (既有 `err_fg` 两者同色, 但 FATAL 才是「一眼」要抓的)。
    「6 桶之和 == 总行数」是计数函数的性质, 对拍留 T2。

- [x] **T2: 并行计数 `count_levels` + 索引零回归实测** ✅ 2026-09-12 ⚠️ **高风险闸门**
  - 说明: `[0, line_count)` 切 N 段, `std::thread::scope` + `lines_from(seg_start)`
    各段并行分类后归并; `LevelCounts` 结构 + 增量累加接口 (T6 用)
  - Acceptance: 并行结果 == 串行结果 (对拍单测);
    **1GB 明文 / JSONL 墙钟实测落档** (不估算);
    **logbench 索引耗时与基线一致** (计数在索引趟外, 该数字不得变动)
  - Verify: `cargo test` + `cargo run --release --bin logbench -- <1GB>` 对照 + 实测表落档
  - Files: `src/levels.rs`, `src/bin/logbench.rs` (增「级别计数」落档段)
  - Scope: M
  - 实测: 19 levels 单测绿 (含差分测试), 全量 **59 绿** (35 lib + 16 main + 8 genlog), clippy 0。

    **① 计数成本 (本机 20 核, 1GB) —— 判定线 150ms**

    | 版本 | 明文 | JSONL |
    |---|---|---|
    | 朴素分类器, 8 线程 | 219 ms | 175 ms |
    | 朴素分类器, 16 线程 | 157 ms ❌ | 120 ms |
    | **memchr3 分类器, 16 线程** | **94 ms** ✅ | **75 ms** ✅ |

    8 线程 (照抄 `MAX_FILTER_THREADS`) 是瓶颈 → 放到 16。157ms 超线 5%, 触发
    Checkpoint A; **用户裁决 C (优化分类器)**: 每行最多 6 次 memmem (每次为 needle
    建 prefilter, 在 200 字节短 haystack 上 setup 比扫描还贵) 改为 2 趟 `memchr3`
    扫首字母 (`"FEW"` / `"IDT"` = 零 setup 纯 SIMD) + 命中处 `starts_with` + 位集
    按优先级取桶。**语义完全等价**, 18 个原测未改一字仍全绿, 另加
    `optimised_matches_naive_reference` 差分测试 (对抗语料: 子串碰撞/优先级冲突/
    200 字节边界) 把朴素版当参照钉死。计数结果逐位不变
    (JSONL FATAL 4760 / ERROR 43464 / INFO 4,544,133)。

    **② 索引零回归 —— 做了真 A/B, 不是推理**: `git stash` 掉计数代码重建 release
    对拍。无计数代码: 明文 93/93/95 · JSONL 92/90/95; 有计数代码: 明文 95/92/95 ·
    JSONL 121/96/101 —— **同一噪声域**。结论: D6 成立。
    附带发现: **09-11 基线 77/88ms 今日复现不出** (同一二进制跑出 81→95 的漂移),
    属机器状态 (磁盘/热), 非本次改动。**后续索引对照应做同会话 A/B, 不要拿历史数字比**。

    **③ 交叉验证**: JSONL 的 ERROR 桶 43464 == 搜索 `ERROR` 命中行数 43464;
    FATAL 4760 / ERROR 43464 / INFO 4,544,133 与 ROADMAP 引用的 LogViewPlus 截图
    同组数 (genlog 同分布) —— 两条独立路径互证。

### Checkpoint A: 成本与正确性定档 ✅ 2026-09-12

- [x] 6 桶之和 == 总行数 (对拍绿)
- [x] 并行 == 串行 (对拍绿) —— 段数 1/2/3/5/7/8/64 + 不整除 7001 行边界全过
- [x] 1GB 明文 / JSONL 计数墙钟实测落档 —— **94 / 75 ms**, 判定线 150ms 内
- [x] logbench 索引**未变动** —— 做了 stash 对拍的真 A/B, 同噪声域 (见 T2 实测②)
- [x] ~~墙钟 > 150ms → 改判「先显示文件、计数随后补入」~~ —— **未触发**:
      超线后用户裁决 C (优化分类器) 而非改判; 该 Ask-first 分支未被激活,
      侧栏无需「计算中」态, T3 按原设计「随 `OpenOutcome` 交付」推进

## Phase 1: 明文垂直切片

- [x] **T3: 接进 OpenJob + 应用状态 + 侧栏组件 (明文端到端)** ✅ 2026-09-12
  - 说明: worker 内计明文计数 → `OpenOutcome` 增 `level_counts` → 应用层状态字段 →
    新增侧栏组件渲染 6 行 (级别名 + 计数 + 对数横条, 复用既有语义色);
    顶层布局改 `Column[TitleBar.embed(Bar), Row[Histogram, LogView.fill]]`
    —— **`src/view.rs` 不改**, LogView 只拿到更窄的 area
  - Acceptance: 1GB 明文打开 → 侧栏出现, 6 行数字与人工核对 (过滤结果交叉核对) 一致;
    0 计数桶仍显示; 索引期间不显示脏数 (无「行数已更新、计数还是旧的」窗口)
  - Verify: `cargo test` + 手动开 1GB 明文比对
  - Depends: T1, T2
  - Files: `src/open.rs`, `src/main.rs`, `src/histogram.rs` (新增), `src/view.rs` (仅四处 `fn` 放宽为 `pub(crate)`)
  - Scope: M
  - 实测: 37 lib + 21 main + 8 genlog = **66 绿**, clippy 0。
    启动冒烟 (release, 各 8–10s 存活至超时, 无 panic): 明文 1GB ✓ / JSONL 1GB ✓ / 空态 ✓。
    JSONL 检出 10 列含 `level` (T4 的列名落点)。

    **三处与 spec 文字的偏离 (均为实现中发现, 记录待复核)**

    ① **侧栏不留 `STATUS_HEIGHT` 内边距** (spec Acceptance 原文要求留)。读码发现
    底栏**不画自己的底色**, 只画 1px 分隔线 —— 故侧栏整条填 `background()` 就与
    内容区无缝, 不需要为「状态栏视觉通栏」做任何事。少一处跨模块常量耦合。
    实际效果: 侧栏是通高导轨, 底栏分隔线自 x=112 起。

    ② **增量计数提前到 T3, 未留到 T6**。`apply_appended` 是 tail 每 250ms 就可能走的
    **UI 线程同步热路径**, 在那儿全量重算等于每次追加卡一次全文件扫描 (1GB ≈ 94ms)。
    故 T3 就加了 `levels::count_levels_from(file, from)` 并在落点做增量合并;
    T6 收缩为「重建重算 + 增量等价单测」。

    ③ **`DEBUG/TRACE` 合并桶不可点** (spec D3 说 JSONL 模式可点)。结构性原因:
    该桶合并了 DEBUG 与 TRACE 两个**字段值**, 而 `level=` 是等值过滤, 单子句表达不了
    「DEBUG 或 TRACE」(空格分词是 AND, 裸词是整行子串)。故可点桶实为 4 个
    (FATAL/ERROR/WARN/INFO)。**要让它可点须把该桶拆成两行 —— 那是改 spec D1**,
    待用户裁。

    **测试**: `fresh_outcome_carries_full_level_counts` (6 桶之和 == 总行数);
    `append_outcome_levels_are_a_delta_not_a_total` (钉死「追平臂交付增量而非全量」
    —— 语义反了会让计数翻倍或丢旧数据);
    `apply_appended_merges_level_counts_incrementally` (增量合并终值 == 全量重算);
    histogram 侧 4 个纯函数测试 (对数刻度 / 命中测试 / 可点桶语法唯一 / 行矩形不重叠)。

### Checkpoint B: 明文端到端

- [x] 机器部分: 66 测试绿, clippy 0; 明文/JSONL/空态三路径启动冒烟存活
- [ ] **人工验收 (待用户)**: 1GB 明文侧栏出现, 6 行数字与过滤结果交叉核对一致;
      0 计数桶仍显示; 无脏数窗口
- [x] 无脏数窗口 (结构保证: 计数随 `file` 同批交卷, 不存在中间态)

## Phase 2: JSONL 垂直切片

- [x] **T4: level 类列识别 + 字段计数 + 对抗样本单测** ✅ 2026-09-12
  - 说明: 从 `Schema.columns` 找 level 类列 (名清单本任务定), 命中则按
    `extract_field` 取字段值分类; 列发现为 None 或无 level 类列 → **降级只读**
  - Acceptance: **`{"level":"INFO","msg":"handle error failed"}` 计入 INFO 桶**
    (钉死 D2 红线); 无 level 类列的 JSONL → 直方图降级只读且不误报
  - Verify: `cargo test`
  - Depends: T3
  - Files: `src/levels.rs`, `src/open.rs`, `src/main.rs`, `src/histogram.rs`, `src/bin/logbench.rs`
  - Scope: M (原估 S; 见下「口径必须同构」一条, 实际牵动了子句生成与 app 状态)
  - 实测: 45 lib + 21 main + 8 genlog = **74 绿**, clippy 0。

    **① 关键发现: 计数谓词必须与过滤谓词同构, 否则撞 D2 红线**

    读码发现过滤的扁平等值是**字节全等** (`jsonl.rs` 的 `token_matches`: `Op::Eq =>
    token == target.as_bytes()`), 而最初设想「字段值按 [`classify_level`] 子串分类 +
    点选子句写死 `level=WARN`」在真文件上会分岔: 文件里写 `WARNING` 时柱条数得到,
    `level=WARN` 却筛出 0 行。

    解法是把字段口径改成**前缀匹配**, 点选子句用**前缀通配** `level=WARN*`
    (`Op::Prefix => token.starts_with(target)`)。两者对同一 token 走同一判断,
    **一致性成了构造保证而非碰巧**, 且不需要 per-file 观察值。六个 token 首字母
    互不相同 → 前缀匹配天然互斥, 连优先级都不需要。

    六个 token 的字段分类独立成 `classify_field_value`, 与行口径的 `classify_level`
    **有意不同** (前者前缀、后者子串 + 200 字节窗口), 因为两者的对齐对象不同:
    行口径对齐「行首有没有级别词」, 字段口径对齐「`col=X*` 能筛出哪些行」。

    **② D2 红线的两级验证**

    - 单测 `field_counts_equal_filter_hits_bucket_by_bucket`: 对 7 行对抗 fixture
      (含 `level=INFO` 而正文写 error、`WARNING` 别名、`ERR` 非规范拼写、小写 `error`、
      无 level 字段但 severity=DEBUG) 逐桶断言 `run_filter(子句).len() == 柱条数字`。
    - **真 1GB JSONL 复验** (logbench): FATAL 4760 / ERROR 43464 / WARN 96262 /
      INFO 4544133 / DEBUG 145086 —— 五桶与对应 `level=X*` 的命中行数**逐个相同**。

    **③ 口径必须全程一致 (新加的不变量)**: `OpenOutcome` 增 `level_column`,
    落点据此重建子句表,**也据此决定后续增量追加走哪条口径** —— 混用会让同一个
    侧栏里出现两种数法。巨量追平臂的 worker 拿不到 schema (该臂恒 None, 沿用应用层
    现值), 故列名由 `launch_append` 显式传入。

    **④ 列名清单**: `level` / `severity` / `lvl` / `loglevel` / `log_level` / `priority`
    (大小写不敏感)。只认清单内的名字是**有意的保守** —— 猜错列会给出看似合理实则
    全错的计数, 比「找不到 → 降级只读」糟得多。

    **⑤ 性能落档**: 1GB JSONL 行口径 76ms / **字段口径 112ms** (9090 MiB/s,
    与 `run_filter` 的 74ms 同量级); 明文 1GB 行口径 94ms。logbench 增字段口径段,
    连子句一并打印 (可点/只读一眼可辨)。

    **⑥ 一处保守取舍待裁**: 合并的 `DEBUG` 桶**不可点** —— `level=DEBUG*` 表达不了
    TRACE。本 fixture 恰好只有 DEBUG 没有 TRACE, 故它的柱条数与 `level=DEBUG*` 命中数
    都是 145086 (巧合成立)。真出现 TRACE 的文件就不成立, 故仍保守判只读。
    若要它可点, 两条路: 拆成两行 (改 spec D1) 或按实际值判定可点性 (引入 per-file
    观察值)。**待用户裁。**

- [x] **T5: 点选联动 + 一致性端到端验证** ✅ 2026-09-12
  - 说明: 侧栏命中区 → `Msg::ApplyLevelFilter(Level)` → 套用该桶子句;
    再次点击当前生效行 = 清除; 当前生效行整行高亮;
    无子句的桶不可点 (吞掉点击, 不穿透到底下列表)
  - Acceptance: 点 ERROR 柱条 → 表格筛出该级别, **底栏行数与柱条数字一致**;
    再点一次 → 清除; 原始文本模式柱条纯展示 (点击无响应);
    `其他` 桶两种模式都不可点
  - Verify: `cargo test` + 手动 1GB JSONL 端到端
  - Depends: T4
  - Files: `src/histogram.rs`, `src/main.rs` (均为 T3/T4 已建结构的补齐)
  - Scope: S
  - 实测: 45 lib + 23 main + 8 genlog = **76 绿**, clippy 0; release 冒烟存活。

    **子句形态与 spec 文字的出入**: spec 写「套用 `level=<NAME>`」, 实际是
    `col=<TOKEN>*` (前缀通配) 且 `col` 取自当前文件的级别类列 —— 这是 T4
    「计数谓词必须与过滤谓词同构」的直接结果, 见 T4 实测①。spec 该句待复核。

    **两条端到端测试** (不依赖鼠标, 直接驱动真实代码路径):
    - `level_filter_toggles_and_ignores_queryless_buckets`: 首次点击 → 套
      `level=ERROR*`; 再点 → 清空; `其他`/`DEBUG`/明文三种情况**点不动**
      (「点不动」必须真不动 —— 若把过滤改成空串, 用户点在「其他」上会意外
      清掉正在看的过滤)。
    - `clicking_a_bar_filters_to_exactly_the_bar_count`: 走**真实 AsyncJob 过滤
      管道**, 对 ERROR/INFO/WARNING 三桶各断言「筛出行数 == 柱条数字 == 100」。
      其中 WARNING 是关键样本 —— 它证明别名靠前缀通配被吃到 (字节全等的
      `level=WARN` 会筛出 0 行, 即 D2 红线破裂的原始形态)。
    - 只读侧栏点击吞消息由 histogram 的 `readonly_sidebar_swallows_clicks_without_message` 钉住。

    **仍待人工**: 真实鼠标点柱条 + 视觉上「底栏行数与柱条数字一致」的体感确认。

### Checkpoint C: JSONL 端到端 + 一致性红线

- [x] 对抗样本归桶正确 (含 INFO 桶而正文写 error / WARNING 别名 / ERR 非规范 /
      小写 error / 无 level 字段但 severity=DEBUG, 共 7 行 fixture)
- [x] 点柱条后筛出行数 == 柱条数字 —— 两级验证: 引擎层逐桶相等单测 +
      真 1GB JSONL 五桶复验 (4760/43464/96262/4544133/145086 逐个相同) +
      应用层真实管道三桶断言
- [x] 无 level 类列时降级只读 (子句表全 None → 点击吞消息不发 Msg)
- [ ] **人工验收 (待用户)**: 真实鼠标点柱条, 目视底栏行数与柱条数字一致;
      再点同一条目视过滤被清除

## Phase 3: 增量与生存

- [x] **T6: tail 追加增量计数 + 轮转/重建重算** ✅ 2026-09-12
  - 说明: 追加只算新行并累加 (用 `LevelCounts` 的增量接口);
    轮转/截断重建随重建重算; 计数与视图**同批次原子换入** (沿用 live-tail 快照一致性)
  - Acceptance: 单测对拍 —— 「全量计数」==「追加 N 行后的计数」(明文与 JSONL 各一组);
    重建后计数 == 重建后文件的全量计数
  - Verify: `cargo test`
  - Depends: T3, T4
  - Files: `src/main.rs`, `src/levels.rs` (生产路径已在 T3/T4 落地, 本任务补等价性证明)
  - Scope: S (原估 M)
  - 实测: 47 lib + 24 main + 8 genlog = **79 绿**, clippy 0。

    **T3/T4 已把增量落进生产路径, T6 交付的是等价性证明** —— T3 就在
    `apply_appended` 落点做了增量合并 (`count_levels_from` / `count_levels_field_from`),
    因为那是 250ms 同步热路径, 全量重算会冻帧; 见 T3 实测②。

    **多轮对拍** (`assert_incremental_matches_full`, 明文与 JSONL 字段口径各跑一遍):
    100 行起, 连跑 5 轮追加 (每轮 10 或 25 行, 桶分布在 ERROR/WARN 间交替),
    每轮断言「旧计数 + 新行增量 == 对当前文件的全量重算」且「6 桶之和 == 总行数」。
    单轮对拍容易碰巧过, 连跑五轮且桶分布每轮不同才逼得出「漏行 / 重计 /
    忘记合并」这类错法。

    **重建换格式** (`apply_rebuild_recomputes_counts_and_switches_column`):
    起点 JSONL 可点 → 轮转后内容变明文 → 断言计数等于新文件全量重算、列名被换掉
    (不沿用旧的)、子句表清空 (降级只读)。这条堵的是「拿旧列名的子句去点新文件
    会筛出 0 行」—— 即 T4 那条「口径必须全程一致」不变量的反面。

## Phase 4: 交互收尾

- [x] **T7: `Ctrl+L` 显隐 + config 持久化 + 主题适配 + 窄窗口** ✅ 2026-09-12
  - 说明: `Ctrl+L` 切换侧栏 (与既有 Ctrl 组合不冲突); 开关落 `config.toml`
    (复用 `src/config.rs` 现有通道); 6 行在深浅两套主题下均可辨识 (走 `Theme` trait);
    窄窗口下自动折叠
  - Acceptance: `Ctrl+L` 显隐即时生效; 重启后开关状态保持;
    深浅主题切换后 6 行均可辨识; 窄窗口不破版
  - Verify: `cargo test` + 手动重启验证持久化
  - Depends: T3
  - Files: `src/main.rs`, `src/config.rs`, `src/histogram.rs`
  - Scope: M
  - 实测: 47 lib + 29 main + 8 genlog = **84 绿**, clippy 0; 明文/JSONL release 冒烟存活。

    **① `config.rs` 重构为单真身 (顺手堵一个既有隐患)**: 原 `AppTheme::save` 是
    **整文件覆盖写**, 只写 `[theme]` 一行。直接加第二个键的话, 用户改主题会顺手
    抹掉侧栏开关 (或反过来)。改为 `Config { theme, histogram }` 单结构 + 单一
    写入点, 两个键同源写; 并加 `load_from/save_to` 接路径参数 —— 否则测试会写到
    `dirs::config_dir()` 即**用户的真配置文件**上。
    键格式: `[theme] mode` + `[view] histogram`。缺键取默认 (old 文件仅 `[theme]`
    仍可读, 向后兼容), 由 `legacy_file_without_view_section_still_loads` 钉住。

    **② 窄窗 = 自动折叠 (原 Open Question 二选一, 定为折叠)**: 内容宽 < 640px
    时侧栏宽度归零, 优先保内容区 —— 新用户未必知道有 `Ctrl+L`, 卡在没法看的
    布局里比看不到直方图糟。

    **③ 判定只留一处**: `effective_width(visible, available)` 是唯一的显隐判定,
    paint/event 不重判 visible, 只看 layout 给的 `area.size.width`
    (`< 1.0` 即不画/放行事件) —— 不存在「宽度 0 却还在画 / 还在吃点击」的漏判。
    `collapsed_sidebar_ignores_events` 钉住折叠态**放行**而非吞掉事件
    (零宽侧栏吃掉落在内容区的点击是隐形 bug)。

    **④ 主题**: 文字/生效行底色走 `Theme` trait (`text_primary`/`text_secondary`/
    `surface_variant`); 横条复用 `view.rs` 的四个语义色, 与**行着色同源** ——
    深色下两者一起明暗, 不会出现「侧栏看得清但行看不清」的错配。四个语义色
    都是中高明度值, 在深底上可辨 (未做深色实机目视, 待人工)。

    **⑤ 键位**: `Ctrl+L` 走 `app_key_filter` 前置 (与 Ctrl+T 同级), 故过滤/搜索栏
    聚焦时也生效。与既有 Ctrl 组合 (T/F/O/B/G) 无冲突。

- [x] **T8: 文档收口** ✅ 2026-09-12
  - 说明: `CLAUDE.md` 状态段 + 结构段 (`levels.rs` / `histogram.rs` 两个新模块);
    `docs/ROADMAP-v1x.md` §一 勾销「等级直方图」并注明已交付;
    `README.md` 功能段补一行; `SPEC.md` 地图已加 (本次 plan 前已完成)
  - Acceptance: 四处文档与实现一致, 无残留「待做」措辞
  - Verify: 人工通读对照
  - Depends: T1–T7
  - Files: `CLAUDE.md`, `docs/ROADMAP-v1x.md`, `README.md`, `docs/specs/SPEC-level-histogram.md`
  - Scope: S
  - 实测: 四处均改。`CLAUDE.md` 测试基线 40 → **84** 绿 (口径同步); 结构段补
    `levels.rs` / `histogram.rs` 两条并改写 `config.rs` 条目 (单真身 + 整文件同源写);
    `README.md` 补功能条 + `Ctrl+L` 键位 + 「DEBUG/其他不可点」边界;
    `ROADMAP-v1x.md` §一 已交付清单加项、改判块注明当日交付、§五 勾销。
    spec 的 Open Questions 同步关闭 (留待裁项仅「DEBUG 桶可点性」)。

### Checkpoint D: 完成

- [x] 三件套绿 (`cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`)
- [x] 数字落档 —— 全部实测不估算:

  | 指标 | 1GB 明文 | 1GB JSONL |
  |---|---|---|
  | 索引 (**不得变动**, D6 判据) | 95 / 92 / 95 ms | 121 / 96 / 101 ms |
  | 索引 (stash 掉计数代码对拍) | 93 / 93 / 95 ms | 92 / 90 / 95 ms |
  | 计数 行口径 | 94 ms | 76 ms |
  | 计数 字段口径 | — | 112 ms (9090 MiB/s) |

  索引两侧同噪声域 → **D6 成立 (有证据, 非推理)**。附带发现: 09-11 基线 77/88ms
  今日复现不出 (同一二进制跑出 81→95 的漂移), 属机器状态 —— **后续索引对照须做
  同会话 A/B, 不拿历史数字比**。
- [ ] spec 成功判据逐条对照过单 (机器项已对照; 详见下方人工项)
- [ ] **人工验收清单全过 (用户实机)** —— 待办, 清单如下:
  1. 明文 1GB 打开 → 侧栏出现, 6 行数字与过滤结果交叉核对一致; 0 计数桶仍显示
  2. JSONL 1GB 打开 → 点 ERROR 柱条 → **底栏行数与柱条数字一致**; 再点清除
  3. 纯文本模式柱条只读 (点击无响应); 「其他」行两种模式都不可点
  4. tail 追加后计数随之变化; 外部截断/轮转后计数与重建结果一致
  5. `Ctrl+L` 显隐即时生效; **重启后开关状态保持**; 窄窗自动折叠
  6. 深浅两套主题下 6 行均可辨识
- [x] 进 review 阶段 (`/agent-skills:code-review-and-quality`) ✅ 2026-09-12
      —— 结论 **Request changes** (一条 Required 已按用户裁决修毕), 详见下节

---

## review 阶段 (2026-09-12, `/agent-skills:code-review-and-quality`)

**方式**: 按 skill 的多模型模式, 把五个焦点分给**三个独立上下文的审查者**并行证伪
(作者自查有盲区), 同时作者自过架构/规范/规模轴。

**审查者确认为「成立」的 (有证据, 非「看起来没问题」)**

| 声明 | 证据 |
|---|---|
| D2 一致性 = 构造保证 | 两侧是**同一提取函数 + 同一谓词**, 可证恒等 (首字节守卫冗余, `starts_with` 已蕴含); 12 方向排除 (嵌套首命中/重复 key/转义与未转义假 `"level":`/unicode 转义/非字符串值/值带空白/CRLF/列错配/并行/增量) + 31 行对抗语料实测 |
| 分类器等价 | **~35.6M 例差分 fuzz 零分岔** (含 A–Z 五元组穷举 14.9M、200 字节边界扫描), 附结构证明: `FEW`/`IDT` 恰覆盖 6 个首字节, `from = at + 1` **逐字节**推进故重叠/被包含的命中也必被访问, 位集优先级与早退序无关 |
| 并行分段 | len=0/1/`<threads`/整除处算术均正确, 段数不超 threads, `seg>=1` 保证终止; `lines_from` 确为 O(log) 二分, **无**从 0 前扫的隐藏线性 |
| 增量派发 | 四条路径 (Fresh/Rebuild/真增量/退化重建) × `rebuilt` 逐一对核**无缝隙**; `apply_appended` 的 merge 在 `self.file` 换入后、`match worker_hits` 之前, 无分支可跳过 |
| UI 线程 32MB 同步扫描不冻帧 | **实测 3ms** (33.5MB / 151k 行, 含线程 spawn 开销) |
| 配置无丢键路径 | 全仓唯一写入口 `save_config` 构造完整 `Config` 再整文件写; `AppTheme::save` 已彻底移除; 测试全走 `load_from/save_to` 不碰用户真配置 |

**已修 (本批)**

1. **[Required, 文档] spec 判据不可评** —— 「索引与 2026-09-11 基线一致」今日不可复现
   (同二进制 81–95ms 漂移), 照字面判会得假结论。已改述为「同会话 A/B 无差异」并落实测数据。
2. **[Nit] config 朴素子串判定两个方向的错** —— 注释里的 `histogram = false` 会静默隐藏
   侧栏; 无空格的 `mode="dark"` 被无视。已改按行解析 (跳注释/`split_once('=')`/trim),
   补 2 测试 (`whitespace_free_assignment_is_honoured` /
   `commented_out_assignment_is_ignored`)。
3. **[Optional] `panic = "abort"` 让「catch_unwind 兜底」在 release 下不存在** ——
   `Cargo.toml:36` 属实 (已核)。`levels.rs` 的注释原文在宣称一个 release 下不成立的
   保证, 已改为「panic 不静默」而非「panic 可恢复」, 并点明 UI 线程那条路**没有**
   catch_unwind。当前**不可达** (审查者穷举未找到能 panic 的输入), 属注释纠偏。
4. **[Optional] `count_levels_with_threads` 是 pub 且可 spawn 到 line_count 线程** ——
   已加 `#[cfg(test)]` (它本就只是测试守卫口)。
5. **[Nit] `effective_width` 可见性虚高** —— `pub(crate)` → 私有。
6. **[Nit] 落点四行重复** —— `apply_fresh` / `apply_rebuild` 逐字重复的列名采纳逻辑
   抽成 `LogApp::adopt_level_column`。

**Required (已修) —— 增量漏计「被补全的半行」**

review 发现并第一手复现: 旧快照末行若**无换行**结尾, 外部补全且补全内容在行首
200 字节内引入级别词 → 增量起点 `[old_line_count, new)` 不含该行, 该行永不重算。
复现数据: 明文 全量 ERROR=1/其他=1 vs 增量 ERROR=0/其他=2; 过滤 全量=1 vs 增量 0。

**用户裁决: 现在修两侧** (排除「只修计数」—— 两侧同源漂移故 D2 此刻仍成立,
只改一侧会当场打破红线)。修法:

- `levels::update_for_append(old, new, old_counts, column)` —— 重算起点**退一行**
  (即 `old_counts − 旧重叠区间 + 新重叠区间`)。旧尾本就完整时减旧加新相抵,
  故**无条件启用**安全, 不必先判断旧尾是不是 `
` (少一个判据就少一类 bug)。
  另加 `LevelCounts::sub` (饱和减)。
- 过滤侧同起点: `drop_filter_hits_from(from)` 先摘掉 `>= from` 的旧命中, 再
  `append_filter_hits(from)` 重跑同区间。摘/补必须同 `from`, 否则重叠行漏算或重算。
- **连带收敛**: `OpenOutcome.level_counts` 从「按臂而定 (增量/全量)」改为
  **一律绝对量** —— 那种契约要在每个落点判断「这份是增量还是全量」, 判错一次就是
  计数翻倍或旧数据全丢; 改成绝对量后误用空间消失 (agent C 原本把旧契约的优点
  记为「最扎实的一笔」, 但改为绝对量更强)。

**并发前提 (已核并写入注释)**: worker 用发起时的旧行数、落点用落地时的, 二者能
相等靠 `poll_growth` 开头的 `open_job.is_some()` 门禁 —— 在途期间不叠加任何 tail
动作。**若将来允许并发追加, 摘/补对称会静默失效**, 届时须把 `from` 随产物交回。

**回归测试 (4 条 + 多轮对拍插一轮)**:
`update_for_append_covers_completed_half_line` (明文 + 字段两支) /
`update_for_append_is_idempotent_on_complete_tail` (旧尾完整时相抵) /
`update_for_append_from_empty_old_file` /
`append_outcome_covers_completed_half_line` (worker 路径) /
`apply_appended_covers_completed_half_line_on_both_sides` (应用层两侧 + D2 断言);
多轮对拍改为走**生产入口**且第 3 轮落半行、第 4 轮补全。

**仍未裁 (非阻塞)**: 合并的 DEBUG 桶可点性 —— 倾向暂不动。

**既有问题 (非本模块引入, 仅 FYI)**

- `FileStat::is_stale` 在生产路径是死代码: 引擎的过期语义与应用层手写的三分支判据
  是两套, 改引擎时容易走岔。
- UTF-16 文件小幅增长走同步路径时 `AppendOutcome::Rebuilt` 被当纯增量, **跳过 Rebuild
  重置链** (filter/bookmark/search 不清); 同一文件增长 ≥32MB 时走 worker 会清。
  同一文件同一次增长因大小不同而换入语义不同。计数本身正确 (prefix 对齐)。
- `extract_field` **每行分配一个 needle `Vec`** (`danqing-logfile/src/jsonl.rs` 内
  `field_needle` 在每次调用里重建) —— 在 1GB 字段口径这条最需要快的路径上每行一次堆分配。
  实测 112ms 即含这份开销; 修法需兄弟 crate 暴露预建 needle 入口 (跨仓)。
- 生效行高亮是**精确串匹配**: 手打 `level=ERROR` (不带 `*`) 不点亮任何行 (点击路径不受影响)。

---

## 事故复盘 (2026-09-12 用户实机报回归)

**现象**: 打开 1GB JSONL, 状态栏写「索引 92ms」, 但界面 ~10s 内容才出来。明文 `.log` 正常。

**排查路径 (每一步都以实测为准, 两次假设被自己推翻)**

| 假设 | 实测 | 结论 |
|---|---|---|
| 我的级别计数慢 | 1GB / 483 万行 107ms; 400MB 短行 250ms; 7196B 长行 16ms; 2MB 大行 16ms | ✗ 计数是线性的, 最坏(每行全扫)1GB 也只 ~80ms |
| 重建循环把索引跑了很多遍 | `FileStat` 只有 len/mtime/head, 读页不扰动它; app 日志无重复重建 | ✗ |
| 渲染慢 (状态栏已出、内容区空白) | 用户答: 状态栏**卡在「正在索引 99%」** → 产物根本没落地 | ✗ 峰值在 worker 内 |
| 列发现按 **512 行** 采样 + serde 完整解析 → 成本随**行宽**无界 | 200 MiB / 563 KiB 行 / 每行上万小对象: **列发现 3.17s** (serde 这种形状 ~60 MB/s), 外推 1GB ≈ 16s | ✓ |

**关键判别证据**: 明文 `.log` 不走 `detect`/`discover_schema` —— 只有 `.jsonl` 慢,
这条把范围直接缩到「JSONL 独有的两步」。

**两个我的失误 (与老代码缺陷叠加成 10s)**

1. **T2 只量了计数本身 (94–157ms) 就判「用户无感」, 没量它对总等待的叠加。**
   本机实测计数 ≈ 索引 (107 vs 111ms), 那个「索引 N ms」只讲了实际工作量的一半。
2. **进度显示对索引之后的阶段完全盲**: 索引一到 100% 就被 `pct.min(99)` 封顶,
   之后列发现再慢也只是「99%」不动 —— 用户无法从界面判断卡在哪。

**修复 (两处, 均已落地)**

- **兄弟 crate `danqing-logfile`** (`perf(jsonl): 列发现/检测采样加字节预算`):
  ① 采样字节预算 `SCHEMA_SAMPLE_BYTES = 4 MiB` / `DETECT_SAMPLE_BYTES = 2 MiB`
  + `MIN_SAMPLE_LINES = 6`; 行宽 ≤ 8 KiB 时不生效 (512 行 × 8 KiB = 4 MiB) → 普通日志**零影响**
  (test-1g.jsonl 列发现 1.4 → 1.57ms, 10 列不变)。
  代价**有意**: 窗外的列不发现, 由 `schema_sampling_stops_at_byte_budget` 显式钉住。
  ② 值宽度探测有界化: 原来为算一个最终 `clamp(4, 32)` 的宽度, 对每个字符串
  `chars().count()` 走完全文、对每个对象/数组先 `to_string()` 整棵序列化再逐字符数;
  改为字符串取前 33 字符、非字符串走有界序列化 (`compact_prefix`), `WIDTH_CLAMP`
  提为常量与探测上界同源。由 `bounded_value_width_equals_unbounded_after_clamp` 逐值对拍。
  **修后**: 大字符串行 113 → 6.6ms; 大嵌套数组行 **3170 → 71ms** (44x)。
- **本仓**: 进度显示加**阶段感知** (`OpenPhase`: 索引 → 列发现中 → 级别计数中),
  后两段不再伪装成卡住的 99% —— 这是「数字与视觉不符」的根因治理 (数字与实际等待
  对齐), 而不只是让数字更好看。另加 `perf open_phases` / `perf open_landed` 两条日志。

**未做 (与用户确认过要做的两项中的一项, 主动收敛)**

原计划还有「**计数改成非阻塞** (先显示文件、计数随后补入)」。**没有做**, 因为根因修掉后
1GB JSONL 全链 ≈ 200ms (索引 111 + 列发现 1.5 + 计数 107), 拆分两个 job 换来的
复杂度不再有对应收益 —— 而阶段显示已经解决了可见性。若日后某形态下计数重新成为
大头, 再按 spec 的原始 fallback 拆分。

**跨仓状态**: 兄弟 crate 的修复已**本地提交 (`0155690`) 未 push**; 本机经 `[patch]`
指向本地路径, 故本地构建已生效。**发布前必须 push 兄弟 crate 并在本仓
`cargo update -p danqing-logfile` 提交 lock** —— 否则发布二进制仍是旧引擎。
