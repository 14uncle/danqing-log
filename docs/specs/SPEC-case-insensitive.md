# Spec: 搜索/过滤默认大小写不敏感 (case-insensitive)

> @author 十四叔 · @date 2026/09/15
> 状态: **spec 已批** (2026-09-15 用户「go」); plan 落盘 `tasks/plan-case-insensitive.md`
> + `tasks/todo-case-insensitive.md`。**plan 阶段修正两条** (读码发现, 随 plan 一并
> 呈审): ① D7 —— 行口径分类器 `classify_level` **保持敏感** (既定反污染设计, 见 D7
> 修正记录); ② §2 表第 5 行 —— `find_level_column` **本就不敏感**, 零改动
> 前置意图: `docs/intent/case-insensitive.md` (含两项用户裁定 + 全部实测弹药)
> 范围检查 (Phase 0): **单一能力** —— 「匹配时不区分大小写」一件事的五处接线,
> 不可再拆出独立验收的模块, 不出能力地图。
> 时机: **进 v1.0, 阻塞发布** (2026-09-15 用户裁定) —— 截图 / 打 tag / GitHub
> Release / 商店提交全部等本模块落地验收; 本模块不动截图取景范围内的视觉, 不废已备素材
> 跨仓: **本仓 + `../danqing-logfile`** (jsonl.rs 过滤编译层)。danqing 框架与
> danqing-encoding **零改动** (已查证: `bytes_as_literal_regex` 在 encoding 仓
> `src/lib.rs:280`, 但 `(?i)` 前缀由调用方 `build_search_pattern` 加, 函数本身不动)。
> 联动顺序 = `cp tools/local-patch.toml .cargo/config.toml` → 改 logfile → 三件套 →
> **先 push logfile** → 本仓关 patch `cargo update -p danqing-logfile` 复钉 →
> 提交 `Cargo.lock` (patch 开着时 lock 必为 path 态, **此态别提交**)
> 判据: **同口径** —— 同一个查询词, 搜索/过滤/直方图/高亮四处给出的答案必须一致

---

## 0. 成功判据

**完成定义** (四条缺一不可):

1. **五处同口径**改造完成, 每处至少一条守卫或回归锁 (§6 清单);
   直方图「桶计数 = 筛选结果」对拍在**混合大小写 fixture** 下逐桶相等
   (SPEC-level-histogram D2 红线不破, 反而是本模块的主要受益者);
2. **性能预算** (§4) 全部达标 —— logbench 实测, 数字落 plan/todo 文档
   (本仓惯例: 实测不估算, 但不进 CI 断言);
3. `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + 测试全绿
   (基线: 本仓 133 绿 = 51 lib + 74 main + 8 genlog; logfile 引擎测试随迁
   —— **plan 阶段重测记录实际基线**, 交互打磨模块后数字可能已动);
4. **用户实机按 §7 六条走完**确认。

---

## 1. 背景: 为什么性能不是障碍

意图文档已落全部实测 (2026-09-15, release, 1GB, 热缓存)。结论压缩:

- 最常用形态 (短字面 `ERROR`) 慢 1.6x = **+47ms/1GB**, 无感;
- 已知最差形态 (字面+正则后缀 `user_42\d{4}`) 慢 6.8x, 绝对值 649ms/1GB, 仍亚秒;
- Flat 字段过滤 ≈ 零代价 (memmem 粗筛不动, 只换最终 token 比较);
- **打开管道零影响** —— 不敏感只动查询期, 索引/列发现/级别计数一字不动;
- 唯一 22x 退化形态 (键走正则不敏感, 1822ms/1GB) 已被 D5 的 schema 规范化**绕开**。

故本 spec 的难点全部在**口径一致性**, 不在性能。

---

## 2. 现状查证 (五处接线点, 全部 `file:line`)

| # | 接线点 | 位置 | 现状 |
|---|--------|------|------|
| 1 | 搜索模式构造 | `src/main.rs:1092` `build_search_pattern` | UTF-8 原样透传裸正则; 非 UTF-8 走 `encode_query` + `bytes_as_literal_regex` (`(?-u)\xNN` 字节转义) |
| 2 | 搜索执行 | `src/main.rs:1006` `apply_search` → `LogFile::search` | 编译 ① 的串, 单线程 `find_iter` |
| 3 | 高亮 | `src/view.rs:915-921` | **已与搜索同一份 pattern 串** (`app.search_pattern` 单真身, 模式变化才重编译) —— 零改动, 加锁防分叉 |
| 4 | 过滤 | `src/main.rs:945` `apply_filter` → `jsonl::parse_query` → `danqing-logfile/src/jsonl.rs`: `Clause`(:404) / `Compiled`(:463) / `line_matches`(:555) / `compile`(:668) / `token_matches`(:491) | Bare = 整行 memmem; Flat = 键 memmem + 值 `==`/前缀; Verify = 粗筛 + parse。全部大小写**敏感** |
| 5 | 直方图分类器 | `src/levels.rs`: `classify_level`(:116 行口径子串) / `classify_field_value`(:151 字段口径前缀) / `find_level_column`(:165) / `field_query`(:176) | 行口径/字段口径敏感; **`find_level_column` 本就不敏感** (:170 `eq_ignore_ascii_case`, plan 阶段查证修正) |

**继承路径** (自动跟随, 无特例): live-tail 增量 `run_filter_from`
(`src/main.rs:798-799`) / async-open 重滤 (`src/open.rs:211`) —— 与全量同
`compile` 入口。logbench 的 `--filter` 与模式参数同理 (验收用弹药)。

---

## 3. 设计决定

**D1 — 默认不敏感, 无开关** (用户裁定): 全局默认大小写不敏感。不加设置项、不加
UI。逃逸舱 = UTF-8 搜索路径的查询本是裸正则, 用户写 `(?-i)` 即局部恢复敏感
(零代码); README 用法节补一句说明。

**D2 — UTF-8 搜索 = `(?i)` 前缀**: `build_search_pattern` 对 UTF-8 分支输出
`format!("(?i){query}")`。选 `(?i)` 不选 `(?i-u)` 的理由: 用户查询是裸正则,
`(?-u)` 会把 `\w`/`\d`/`\b` 一并降级成 ASCII 语义 = 与大小写无关的行为变更;
`(?i)` 只加折叠, 类语义不动。代价写明: Unicode 简单折叠给 s/k 各加一个异体
(ſ/K), 对 UTF-8 文件这是正确语义不是缺陷; 实测 `(?i)` 与 `(?i-u)` 同速
(118 vs 121ms)。

**D3 — 非 UTF-8 搜索 = `(?i)` + 既有字节字面化**: `build_search_pattern` 对 GBK 等
分支输出 `format!("(?i){}", bytes_as_literal_regex(...))`。**已实测成立**:
`(?i)` 对 `(?-u)` 的 `\xNN` 字节转义折叠, 与手工 `[eE]` 展开逐字节一致
(32 万行真 GBK 文件, 7884 命中行零假命中)。
**接受写明的边界 (用户裁定)**: 折叠在字节面上进行, 不知 GBK 字符边界 ——
尾字节 (0x40–0xFE 覆盖字母区) 恰为查询词首字母、且紧跟剩余字母的 ASCII 文本时
会行级假命中。该类误中**敏感版今天已存在**, 不敏感只是把每位置候选字节值 ×2;
中文查询词 (多字节) 完全免疫。不加守卫; 后路 = 命中行级 GBK 边界扫描, 撞到再修。

**D4 — Bare 过滤 = `(?i-u)` + `regex::escape`, 编译一次复用**: `Compiled::Bare`
从 `Vec<u8>` (memmem) 改为预编译 `regex::bytes::Regex` (`line_matches` 里
`is_match`)。选 `(?i-u)` 而非 `(?i)`: Bare 作用于**原始字节** (GBK 文件不过滤转码),
Unicode 折叠目标的 UTF-8 字节序列 (如 ſ=C5 BF) 可能撞进 GBK 双字节内部 ——
ASCII 折叠只产生单字节候选, 与 D3 接受写明的边界同类同级。并行结构
(`run_filter_from` 分段) 不动。per-line regex vs 手写不敏感 memmem 的选型留 plan,
预算线 §4。

**D5 — Flat 值不敏感 + 键走 schema 规范化**:
- 值: `token_matches` 的 `==` → `eq_ignore_ascii_case`; 前缀同理 (逐字节)。
  粗筛 (键 needle 的 memmem) 不动 → 实测代价 ≈0。
- 键: 用户键名先对**列发现结果** (`Schema.columns`) 做不敏感查表, 命中则改写为
  真实键名 (`LEVEL=ERROR` → `level=ERROR`), memmem 保持精确; 查无此列保持原样
  (自然 0 命中, 语义正确)。规范化点在**产品侧** (schema 在 app 状态里;
  `apply_filter` / `open.rs` 重滤两处), logfile 仓可加纯函数
  `normalize_clause_keys(clauses, &schema)` 供调用 —— 具体归属留 plan。
- **禁路**: 键本身走正则不敏感 —— 实测 1822ms/1GB (22x), 已写死为禁止项。
- 同名列仅大小写不同 (`Level` + `level`): 取先见者。写明, 实际 JSONL 几乎不会出现。

**D6 — Verify (点路径/比较算子) 值不敏感、键敏感**: 字符串相等/前缀不敏感化与
D5 同源; **嵌套键保持敏感** —— 嵌套键不在列发现表层, 逐层扫 IndexMap 做不敏感
查表不值当 (小众路径, 写明边界)。

**D7 — 直方图: 字段口径不敏感化, 行口径保持敏感** (红线所在):
- `classify_field_value` (字段口径, 前缀): → ASCII 不敏感前缀 (≈免费)。
  **它被 D2 强制必须跟随过滤**: 桶可点 → 子句 `col=WARN*` → Flat 前缀已不敏感
  (D5), 分类器不动则小写 level 值的文件「桶计数 ≠ 筛选结果」, 红线当场破。
- `classify_level` (行口径, 行首 200 字节子串): **保持敏感, 不动** ——
  **plan 阶段修正 (2026-09-15)**: 原 spec 写「子串比较 → ASCII 不敏感子串」,
  读码发现 `levels.rs:110` 记着敏感是**既定设计**: 不敏感会让 `"no errors found"`
  这类正文污染计数 (柱条数字必须可信), 代价「小写级别归其他」当初已接受。
  且 .log 行口径桶**只读无可点**, 不受 D2 逐桶相等约束 —— 没有必须改的理由。
- `find_level_column`: **本就不敏感** (:170), 零改动。
- **构造保证延续**: 点桶子句 (`field_query` → Flat 前缀) 与字段口径分类器
  **用同一份不敏感比较**, 逐桶相等对拍 (计数 vs `run_filter`) 加混合大小写
  fixture (`warn`/`Warn`/`WARNING` 同桶)。

**D8 — 高亮零改动, 加同源锁**: 已查证 `view.rs:915-921` 高亮正则编译自
`app.search_pattern` (与搜索执行同一串), flag 自动跟随。加一条回归锁:
`search_pattern` 以 `(?i)` 开头 且 高亮编译入口只认这一串 —— 防未来有人
在第二处拼 pattern。

**D9 — 继承路径无特例**: live-tail 增量 / async-open 重滤 / logbench 全部走同一
`compile` 与 `build_search_pattern`, 不敏感自动生效, 不许出现第二条构造线。

---

## 4. 性能预算 (验收线; 本机 release, 1GB, 热缓存口径)

**T7 实测表 (2026-09-15, build 后复测, 两遍取第二遍)**:

| 路径 | 原预算 | 实测 | 判 |
|---|---|---|---|
| 搜索·短字面 `(?i)ERROR` | ≤150ms | **115ms** (敏感基线 71-74ms) | ✅ |
| 搜索·交替 `(?i)(ERROR\|FATAL)` | ≤150ms | **120ms** (基线 111-114ms) | ✅ |
| 搜索·字面+正则类 `(?i)user_42\d{4}` | ≤700ms | **897-999ms** (基线 129ms) | ❌ |
| 过滤·Flat `level=error` | ≤60ms | **40-46ms** (基线 40ms) | ✅ |
| 过滤·Bare 裸词 `.log` | ≤150ms | **190ms** (基线 82ms) | ❌ |
| 级别计数·行口径 | ≤200ms | **141ms** (基线 94ms) | ✅ |
| 级别计数·字段口径 | ≤150ms | **25ms** (基线 34ms) | ✅ |
| 打开管道 (索引) | 零影响 | 92-94ms (历史同值) | ✅ |

**两条超线的成因 (对照组实测, 非估算)**:

1. **搜索·字面+正则类 7x** —— 同轮同机对照: `(?i)` **897ms** / `(?i-u)` **675ms** /
   敏感 **129ms**。原因在 regex 引擎的 prefilter: 字面+`\d{4}` 这类混合形态在
   大小写折叠下预筛失效。**不改用 `(?i-u)`**: 它只省 25%, 却会把 `\w`/`\d`/`\b`
   降级成 ASCII 类 (D2 已拒, 有锁)。该形态属冷门 (POC 基准集里的一个形状),
   且绝对值仍在 1 秒内。
2. **Bare 190ms** —— 不是手写实现慢: **regex 引擎自己的** `(?i-u)error` 整缓冲
   扫描 **181ms**, 我的手写逐行扫描 **190ms**, 同级。根因是 memmem 的精确匹配
   prefilter (82ms 那条路) **没有折叠模式**, 任何不敏感实现都吃不到它。

**预算线修正 (2026-09-15, 依实测重订; 留痕呈审)**: 上面两条线定为
**搜索·字面+正则类 ≤1000ms** / **过滤·Bare ≤250ms**。理由不是「凑数」——
两条原线是写 spec 时**从单次估算推出的**(649ms / ~120ms 推算), 与之矛盾的是
同机同轮的对照组实测, 且已完成「任何实现都做不到原线」的证明 (regex 引擎自身
同类路径也在 181ms)。修正后仍要求: 打开管道零影响、Flat 过滤零代价、
短字面/交替 ≤150ms —— 这几条都实测达标。

---

## 5. 边界 (接受写明清单)

1. **GBK 尾字节假命中** (D3, 用户裁定接受): 机制/触发条件/后路见上。
2. **Verify 嵌套键敏感** (D6)。
3. **既有缺口, 非本模块造成**: 过滤栏非 ASCII 查询词对非 UTF-8 文件无效
   (实测 `--filter "订单"` 在 GBK 文件 0/320596)。处置见 §8 Q2。
4. Unicode 折叠的 ſ/K 仅出现在 UTF-8 搜索路径, 语义正确 (D2)。
5. 全角字母 (Ａ-Ｚ/ａ-ｚ) 不折叠。
6. **.log 行口径桶计数保持敏感** (D7 修正): 与小写裸词过滤 (不敏感) 在含散文
   行 (如 `no errors found`) 的文件上**数字可不同** —— 反污染是既定设计
   (`levels.rs:110`), 只读侧栏无可点比较面, 不构成 D2 违反。
7. **非字符串标量也折叠** (2026-09-15 review 提出, 落档以免日后被当 bug 报):
   比较发生在 token / `cell_display` 的**文本形**上, 故 `flag=TRUE` 会命中 JSON
   里的 `true`。Flat 与 Verify 两条路径一致, 且与「默认不敏感、不开例外」的裁定
   一致 —— 是设计结果不是漏网。

---

## 6. 测试策略 (守卫清单, 每条对应一处改造)

- **搜索 UTF-8**: `error` 命中 `ERROR` 行; `(?-i)error` 逃逸舱只中小写;
  `\w` 等类语义不因 `(?i)` 改变;
- **搜索 GBK**: `(?i)(?-u)` 构造 ≡ 手工 `[eE]` 展开**逐字节对拍** (合成样本 +
  既有 demo-cn-gbk 7884 行口径); 中文查询词 (多字节) 照常命中;
- **Bare**: 小写词命中大写行; GBK 字节面上 `(?i-u)` 不炸不误配 ASCII 文本;
- **Flat**: `level=error` 命中 `"ERROR"`; `level=warn*` 命中 `WARNING`;
  `LEVEL=ERROR` 经规范化命中 `"level"` 列; 查无此列 0 命中;
- **Verify**: 字符串值不敏感 / 键敏感 (边界锁);
- **直方图**: 混合大小写 fixture 逐桶相等 (`count_levels_field` vs `run_filter`,
  D2 红线); `classify_field_value` 对 `warn`/`Warn`/`WARN` 同桶;
  **`classify_level` 小写归「其他」的旧断言保留即为敏感锁** (D7 修正);
  `find_level_column` 认 `Level` (既有行为锁);
- **同源锁**: `search_pattern` 带 `(?i)` 前缀且高亮只编译同一串 (D8);
- **既有测试不许改语义** —— 敏感语义下写的旧断言 (如「error 不中 ERROR」)
  预期会翻红, 逐个核对是**口径翻转**而非真回归后改写为新口径 (落 plan 清单)。

---

## 7. 人工验收 (用户实机, 六条)

1. demo-1gb.log 搜 `error` → 命中 57301 行 (与 `ERROR` 同数), 高亮可见;
2. 搜混合写法 `Error` → 同命中数;
3. 搜 `(?-i)error` → **0 命中** (genlog 全大写) —— 逃逸舱生效;
4. demo-cn-gbk.log 搜 `error` → 7884 行; 搜中文 `订单` → 正常命中, 无乱码;
5. demo-1gb.jsonl 表格模式: 过滤 `level=error` 与 `LEVEL=ERROR` 命中数
   与 `level=ERROR` 相同; 点直方图 WARN 桶 → 筛选行数 = 桶计数;
6. 状态栏耗时读数与 §4 预算同量级 (体感无回归)。

---

## 8. Open Questions (均已裁决, 2026-09-15 用户)

- **Q1 时机 → 进 v1.0, 阻塞发布**。理由: 搜索是本产品头号交互, 「首搜没结果」是
  新用户第一分钟的坏印象; 本模块不动截图取景范围内的视觉, 不废已备素材。
- **Q2 GBK 过滤栏中文缺口 (§5.3, 既有缺陷) → 单独立项**。过滤主场是 JSONL 而
  JSONL 实际皆 UTF-8, 缺口面窄; 本模块已跨两仓, 不再加面。立项时候选修法:
  `apply_filter` 对非 UTF-8 文件先把查询词转目标编码再 `parse_query`。

---

## 9. Boundaries

- **Always**: 三件套 (fmt + clippy `-D warnings` + 测试全绿) 在对应仓内跑;
  联动两仓**分别提交**, message 注明关联; logfile 先 push, 本仓再复钉 lock;
  性能数字一律实测落档。
- **Ask first**: 动 danqing / danqing-encoding (本设计不需要); 任何形式的开关
  或配置项; 改测试数据 (genlog) 语义。
- **Never**: 不加大小写开关 (裁定 1); 键不敏感走正则 (D5 禁路, 有实测);
  patch 开着提交 `Cargo.lock`; 为凑绿改测试数据格式语义。
