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

- [ ] **T3: 接进 OpenJob + 应用状态 + 侧栏组件 (明文端到端)**
  - 说明: worker 内计明文计数 → `OpenOutcome` 增 `level_counts` → 应用层状态字段 →
    新增侧栏组件渲染 6 行 (级别名 + 计数 + 对数横条, 复用既有语义色);
    顶层布局改 `Column[TitleBar.embed(Bar), Row[Histogram, LogView.fill]]`
    —— **`src/view.rs` 不改**, LogView 只拿到更窄的 area
  - Acceptance: 1GB 明文打开 → 侧栏出现, 6 行数字与人工核对 (过滤结果交叉核对) 一致;
    0 计数桶仍显示; 侧栏底部留 ⠀STATUS_HEIGHT 使状态栏视觉通栏;
    索引期间不显示脏数 (无「行数已更新、计数还是旧的」窗口)
  - Verify: `cargo test` + 手动开 1GB 明文比对
  - Depends: T1, T2
  - Files: `src/open.rs`, `src/main.rs`, `src/histogram.rs` (新增), `src/lib.rs`
  - Scope: M

### Checkpoint B: 明文端到端

- [ ] 1GB 明文侧栏数字与人工核对一致
- [ ] 无脏数窗口

## Phase 2: JSONL 垂直切片

- [ ] **T4: level 类列识别 + 字段计数 + 对抗样本单测**
  - 说明: 从 `Schema.columns` 找 level 类列 (名清单本任务定), 命中则按
    `extract_field` 取字段值喂 `classify_level`; 列发现为 None 或无 level 类列 →
    **降级只读** (不退回子串计数, 否则撞 D2 一致性红线)
  - Acceptance: **`{"level":"INFO","msg":"handle error failed"}` 计入 INFO 桶**
    (钉死 D2 红线); 无 level 类列的 JSONL → 直方图降级只读且不误报
  - Verify: `cargo test`
  - Depends: T3
  - Files: `src/levels.rs`, `src/open.rs`
  - Scope: S

- [ ] **T5: 点选联动 + 一致性端到端验证**
  - 说明: 侧栏命中区 → `Msg::ApplyLevelFilter(Level)` → 套用既有 `level=<NAME>`
    过滤语法; 再次点击当前生效行 = 清除; 当前生效的 `level=` 过滤行高亮;
    `其他` 桶不可点 (无对应语法)
  - Acceptance: 点 ERROR 柱条 → 表格筛出该级别, **底栏行数与柱条数字一致**;
    再点一次 → 清除; 原始文本模式柱条纯展示 (点击无响应);
    `其他` 桶两种模式都不可点
  - Verify: `cargo test` + 手动 1GB JSONL 端到端
  - Depends: T4
  - Files: `src/histogram.rs`, `src/main.rs`
  - Scope: M

### Checkpoint C: JSONL 端到端 + 一致性红线

- [ ] 对抗样本归桶正确
- [ ] 点柱条后底栏行数 == 柱条数字
- [ ] 无 level 类列时降级只读

## Phase 3: 增量与生存

- [ ] **T6: tail 追加增量计数 + 轮转/重建重算**
  - 说明: 追加只算新行并累加 (用 `LevelCounts` 的增量接口);
    轮转/截断重建随重建重算; 计数与视图**同批次原子换入** (沿用 live-tail 快照一致性)
  - Acceptance: 单测对拍 —— 「全量计数」==「追加 N 行后的计数」(明文与 JSONL 各一组);
    重建后计数 == 重建后文件的全量计数
  - Verify: `cargo test`
  - Depends: T3, T4
  - Files: `src/main.rs`, `src/open.rs`, `src/levels.rs`
  - Scope: M

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
