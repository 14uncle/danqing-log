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

- [ ] **T7: `Ctrl+L` 显隐 + config 持久化 + 主题适配 + 窄窗口**
  - 说明: `Ctrl+L` 切换侧栏 (与既有 Ctrl 组合不冲突); 开关落 `config.toml`
    (复用 `src/config.rs` 现有通道); 6 行在深浅两套主题下均可辨识 (走 `Theme` trait);
    窄窗口下走定下的策略 (自动折叠 or 内容区保最小宽度)
  - Acceptance: `Ctrl+L` 显隐即时生效; 重启后开关状态保持;
    深浅主题切换后 6 行均可辨识; 窄窗口不破版
  - Verify: `cargo test` + 手动重启验证持久化
  - Depends: T3
  - Files: `src/main.rs`, `src/config.rs`, `src/histogram.rs`
  - Scope: M

- [ ] **T8: 文档收口**
  - 说明: `CLAUDE.md` 状态段 + 结构段 (`levels.rs` / `histogram.rs` 两个新模块);
    `docs/ROADMAP-v1x.md` §一 勾销「等级直方图」并注明已交付;
    `README.md` 功能段补一行; `SPEC.md` 地图已加 (本次 plan 前已完成)
  - Acceptance: 四处文档与实现一致, 无残留「待做」措辞
  - Verify: 人工通读对照
  - Depends: T1–T7
  - Files: `CLAUDE.md`, `docs/ROADMAP-v1x.md`, `README.md`
  - Scope: S

### Checkpoint D: 完成

- [ ] 三件套绿 (`cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`)
- [ ] 数字落档: 并行计数墙钟 (1GB 明文 / JSONL), 与基线索引耗时对照
- [ ] spec 成功判据逐条对照过单
- [ ] 人工验收清单全过 (用户实机)
- [ ] 进 review 阶段
