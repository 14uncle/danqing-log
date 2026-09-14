# Spec: 选区/复制一致性 (selection-copy-consistency)

> @author 十四叔 · @date 2026/09/14
> 状态: **能力地图已批** (2026-09-14); **spec 已批** (2026-09-14 用户「go」);
> plan 落盘 `tasks/plan-selection-copy.md` + `tasks/todo-selection-copy.md` (待批)
> 前置意图: `docs/intent/selection-copy-consistency.md` (interview-me 三轮裁定 + 显式 yes)
> 时机: **进 v1.0, 阻塞发布** —— 截图/打 tag/商店提交全部等本 spec 落地验收
> 跨仓: M1 落在 `../danqing` (框架), M2–M4 落在本仓。**分仓分别提交**;
> 联动顺序 = danqing 先 push → 本仓 `cargo update -p danqing` 重钉 lock → 提交 lock
> 旧 spec 修订: 本文档**翻案** `SPEC-text-selection-copy.md` 的两条已拍板决策
> (① 双击 = 空白分隔 token; ② 原始模式 Ctrl+C 只认文本选区) —— 翻案依据 = 用户
> 2026-09-14 真机反馈 + interview 裁定, 见意图文档裁决记录。

---

## 0. 症状定性与目标行为矩阵 (已批, 摘自意图)

用户终版真机报 6 症状, 代码级定性: 2 条刻意设计/spec 旧决策 (本文推翻), 3 条从未实现,
1 条无病 (.jsonl 行复制), 1 条按期望错位处理 (#5 展开块, 由 M3 整体换路径消除)。

改完后行为矩阵 (**本文档的成功判据**):

| 手势 | .log 原始模式 | .jsonl 表格模式 | 展开块子行 (两模式统一) |
|------|--------------|----------------|----------------------|
| 双击 | **新分词选词** (M1) | **选中单元格, Ctrl+C 拿完整值** (M4) | **新分词选词** (M3) |
| 拖动 | 框选 (现状保留) | 不做 (有意识留空) | **框选** (M3) |
| 单击选中行 + Ctrl+C | **复制整行** (M2, 翻案) | 复制整行原文 (现状保留) | **复制该子行 `label = value`** (M3) |
| 文本选区 + Ctrl+C | 复制选区 (现状保留) | — | **复制选区** (M3) |

**双击分词新语义 (M1)**: 时间戳 / IP / `level=ERROR` / 路径 / URL **整选**;
`hello,world`、引号、括号**断开**; **中文逐字**。

---

## 1. 能力地图 (已批 2026-09-14)

| # | 模块 id | 仓库 | 职责 | 依赖 | 状态 |
|---|---------|------|------|------|------|
| M1 | `danqing:token-connector` | danqing | `token_at` 重写: 混合连接器规则 + 中文逐字 | — | 待做 |
| M2 | `raw-row-copy` | 本仓 | 翻案: 原始模式选中行 (无文本选区) Ctrl+C = 整行 | — | 待做 |
| M3 | `expand-block-selection` | 本仓 | 子行命中几何 + 双击/框选 + 子行复制 | M1 | 待做 |
| M4 | `table-cell-copy` | 本仓 | 表格双击单元格 = 选中 + Ctrl+C 完整值 | — | 待做 |

构建顺序: **M1 → M2 → M3 → M4**。
M1 先行 (框架先 push 重钉, 后续在新 lock 上干活); M3 依赖 M1 的分词语义;
M4 与 M1 无关但放最后 (独立垫底, 前三个任何延误不阻塞它)。

**波及面已查实**: `token_at` 全农场唯一消费者 = 本仓 `view.rs:655`;
danqing-pomodoro **不用** `token_at`/`text::selection` (全仓 grep 零命中) →
框架改动对 pomodoro 波及面为零, 但它 `cargo update` 那天会看到新语义注释, 无需动作。

---

## 2. 模块 M1: `danqing:token-connector` (框架)

### Objective

双击分词从「空白/非空白两类」改为**混合连接器规则**, 让英文按单词断开的同时
保住日志场景的高价值整选 (时间戳/IP/key=value/路径), 中文逐字。
改一个函数: `danqing/src/text/selection.rs` 的 `token_at(line, off)`, **签名不变**。

### 字符分类 (五类)

| 类 | 判定 |
|----|------|
| Space | `ch.is_whitespace()` |
| Cjk | 码点 ∈ `4E00–9FFF` ∪ `3400–4DBF` ∪ `F900–FAFF` (基本区 + 扩展A + 兼容区) |
| Word | `ch.is_alphanumeric()` **且非 Cjk** (含全角字母数字、西里尔、希腊、假名、谚文) |
| Conn | 之一: ``- : . / \ = @ _ + %`` |
| Other | 其余一切 (引号/括号/逗号/分号/全角标点/组合符号…) |

### 选词规则 (off 归属字符 = 首个起点 ≥ off 的字符; off ≥ len 归最后字符 —— 现状保留)

- **Space** → 最大连续 Space 段 (编辑器惯例, 现状保留)。
- **Cjk** → 恰好这一个字符 (**中文逐字**)。
- **Word** → 最大「复合词」: 由 Word 字符与**内部 Conn 段**组成、首尾都是 Word 的最长段。
  内部 Conn 段 = 该 Conn 连续段的**紧前与紧后都是 Word**。
  例: `2026-09-08T12:34:56.789Z` / `10.0.0.1` / `2001:db8::1` (`::` 整段内部化) /
  `level=ERROR` / `C:\Users\gwhun` (`:\` 是一段 Conn) / `https://example.com/x` 全整选。
- **Conn** → 若所处 Conn 段是某复合词的内部段, 选**整个复合词** (点在 `-` 上与点在数字上同效);
  否则按 Other 处理。
- **Other** (含非内部 Conn) → 最大连续 {Conn, Other} 段 (标点段)。
  例: `---` 装饰线整段; `")"` 两字符一段; `hello,world` 的 `,` 自成一段。

### 已接受的连带代价 (写进文档防「改回」)

- `1,000,000` 千分位被逗号**切开** —— 用户要 `hello,world` 断开的直接推论。
- `foo---bar` 整选 (Conn 段两侧都是 Word → 内部化, 与 IPv6 `::` 同一条规则的两面)。
- 组合字符 (如 `e` +  combining ´) 拆开 —— 组合符归 Other; 日志场景可忽略。
- 日文假名/谚文按 Word 连成段 (本轮需求是中文, 不为它们开第三类)。

### 验收 (框架单测, `selection.rs` tests 模块重写)

必过用例 (每条同时验「点词中」与「点连接符上」两处 off):
`2026-09-08T12:34:56.789Z` / `2026-09-08T12:34:56+08:00` 整选; `10.0.0.1` 整选;
`2001:db8::1` 整选; `level=ERROR` 整选; `/var/log/app.log` 与 `C:\Users\gwhun` 整选;
`https://example.com/x?y=1` 整选; `user@host.com` 整选;
`hello,world` → `hello` / `,` / `world`; `"ERROR"` → 引号与词分离;
`订单创建成功` → 逐字; 混合行 `订单(order_service) 创建成功` → 中文字逐字、
`order_service` 整选、括号逗号各自断开; 空白段惯例; 空行/全空白;
off 超界归最后字符; **任意 off 不劈 UTF-8 字符** (多字节回归)。

**既有测试处置**: `token_covers_timestamp` / `token_middle_word` /
`token_on_whitespace_selects_space_run` / `token_empty_and_all_space` 语义不变继续绿;
`token_multibyte_never_splits_char` **断言失效重写** (`中文 日志` off=1: `(0,6)` → `(0,3)`,
整段变逐字) —— 这是本模块唯一的行为性测试变更。

### 明确不动

`TextSelection` / `ordered` / `copy_text` / `row_slice` / `floor_char_boundary`
及其全部测试 —— 本模块只动 `token_at` 与其文档注释。

### 联动

danqing 内: 三件套 (`cargo fmt` / `clippy --all-targets -- -D warnings` / `cargo test`) →
push。回本仓: `cargo update -p danqing` → 全测试绿 → 提交 lock (message 注明关联)。
**前置**: 改 danqing 前 `cp tools/local-patch.toml .cargo/config.toml` (patch 默认关, 农场约定)。

---

## 3. 模块 M2: `raw-row-copy` (本仓)

### Objective

翻案 `SPEC-text-selection-copy.md` 的「原始模式 Ctrl+C 只认文本选区」:
原始模式有选中行且无文本选区时, Ctrl+C 复制该行整行原文。

### 需求

`LogView::selected_text` (`view.rs:1422`) 的取文案链统一为**三级优先**:

1. **非空文本选区** (且不超 `COPY_MAX_LINES`) → 选区文本 (现状不变);
2. **单元格选中** (M4 引入, 本模块先留位) → 单元格完整值;
3. **行选中** (`has_file && selected < display_count()`) → 该行内容 ——
   **两模式统一**: 普通行 = 解码原文; 子行 = 父行原文 (**M3 落地后改为子行串**, 见 §4)。

即: 把现 `if self.table_mode() && ...` 分支的 `table_mode()` 条件去掉, 供给逻辑不变。
超上限提示逻辑 (`selection_over_limit`) 只作用于第 1 级, 不动。

### 验收

- 新增: 原始模式行选中 (无文本选区) → `selected_text()` = 该行原文 (含 GBK 文件解码行)。
- **改写**: `selected_text_raw_selection_and_table_row` (`view.rs:2481`) 里
  「原始模式仅行选中 → None」的断言**反向** (2510–2512 行) —— 翻案的测试落点。
- 回归: 文本选区优先于行选中; 空选区 (`(1,1)-(1,1)`) + 行选中 → 走第 3 级得整行。

### 明确不动

行选中的视觉 (选中底色 + 左侧强调条); `Msg::Select` 链路; 注释 `view.rs:1421`
那句「Ctrl+C 只认文本选区 (spec 决策)」**删除并改写**为新链说明。

---

## 4. 模块 M3: `expand-block-selection` (本仓, 依赖 M1)

### Objective

展开块子行 (`label = value` 纯文本行, 原始/表格两模式都渲染, `view.rs:950`)
获得与原始模式文本行**同款**的双击选词 (M1 新语义) / 框选 / Ctrl+C;
单击选中子行 + Ctrl+C = 该子行文本 (#5 期望错位在此消除)。

### 需求与实现要点

1. **子行命中几何**: 子行 paint 分支 (`view.rs:950-968`) 里按 `measure_row_geom`
   同款构建 `RowGeom` 入 `row_geom` 缓存。参数: 文本 = `format!("{} = {}", label, value)`
   (**与 paint 同一个串**, 保证命中/渲染/复制三源一体); `base_byte = 0`;
   `start_x = indent` ((depth-1)*16, 子行无水平滚动、无左截断); 右界同 raw 行。
2. **hit_text 的 x_offset 分叉** (`view.rs:687`): 子行**不参与水平滚动**
   (paint `draw_x = text_x + indent`, 无 `x_off`), 而 `hit_text` 现对所有行加
   `self.x_offset` —— 子行命中时该项必须为零。按 `line_at(row).1 > 0` 分叉。
3. **事件接线**: 表格模式的左键按下条件 (`view.rs:1339` 现 `!self.table_mode()`)
   放宽为 `!self.table_mode() || 子是行` —— 子行走 `handle_text_press`
   (潜伏锚点/双击判定/框选全套复用)。子行上 x < text_x (gutter 区) = 清选区, 同 raw 惯例。
4. **复制供给**: `selected_text` 选区分支的 `line` 回调 (`view.rs:1429-1435`)
   删掉「子行 → `String::new()`」防御, 改为子行 → 该子行串 (与几何同源);
   行选中兜底 (M2 第 3 级) 的子行供给**同步**从「父行原文」改为「子行串」。
5. **展开态变化的选区失效** (新引入的必需守卫): 现 `sync` 只在文件/过滤/模式变化时
   清选区 (`view.rs:731-735`); ToggleExpand 改变显示行映射, 旧选区会指向**错误的行**
   (今天不可能有表格选区所以无病, M3 后成真 bug)。`LogApp` 加 `expand_rev: u64`,
   `toggle_expand` (`main.rs:1119`) 内 +1; `LogView` 记录上次所见 rev, `sync` 不一致
   → 清选区 + 潜伏按下 (复用现有失效块), M4 的 `selected_cell` 同清。

### 验收

- 子行双击: `token_at` 新语义作用于子行串 (如 `msg = 订单创建成功` 双击 `订` 选单字,
  双击 `msg` 整选该词)。
- 子行框选: 块内跨子行拖选 → Ctrl+C = 各子行串 `\n` 拼接; 反向拖动规范化一致。
- 拖出块外: 表格模式拖到普通行 → caret 冻结在最后有效点 (既有语义, 测试锁住);
  原始模式从子行拖上普通行 → 选区跨界正常, 复制 = 普通行原文 + 子行串混排。
- 选中子行 Ctrl+C = 该子行串 (不再归父行原文) —— **#5 的回归锁**。
- 展开中折叠 → 选区清空 (rev 守卫); 折叠再展开不复活旧选区。
- 水平滚动后子行命中不偏移 (x_offset 分叉的测试)。

### 明确不动

展开块的底色/铺法 (`expand_block_rects` 系); 展开 glyph 的点击区 (`< EXPAND_W`
仍折叠/展开); 子行的渲染样式与缩进值。

---

## 5. 模块 M4: `table-cell-copy` (本仓)

### Objective

表格模式双击单元格 = 选中该单元格 (矩形高亮), Ctrl+C 复制**完整值**
(不受列宽截断省略影响) —— 「只想要这一个字段」的直达手势。

### 需求与实现要点

1. **列区间缓存**: 列布局只在 paint 可算 (`view.rs:802-820`, 需 TextBatch 测宽) →
   paint 时把可见列区间缓存进 `RefCell<Vec<(f32 x0, f32 x1, usize col_idx)>>`
   (绝对窗口 x, 与 `settings_btn_rect` 同法); event 侧命中只查缓存。
   滚出视口的列不在缓存 = 点不到, 天然一致。
2. **新状态 `selected_cell: Option<(u64 display_row, usize col_idx)>`**, 存 LogView
   (与文本选区同层, 不进 LogApp)。paint: 该格画选中底色 (行列定位用缓存,
   样式随行选中那套 `th.selection()`); 滚动后由缓存跟随, 无额外状态。
3. **双击判定**: 表格模式普通行 (非子行) 的左键按下接入 `last_click` 双击检测
   (300ms/4px, 复用常量); 双击且命中列 → `selected_cell = Some((row, col))`;
   单击 = 现状 `Msg::Select` 不变。任何新按下/文本选区动作/Esc → 清 `selected_cell`。
4. **复制**: `selected_text` 第 2 级 (M2 留位) —— 取该行 parse 后 `col.name`
   字段经 `jsonl::cell_display` 的**完整串** (与显示同源, 无截断);
   行 parse 失败或无该字段 → 不产 `selected_cell` (双击无效果)。
5. **失效**: `sync` 现有失效块 (文件/过滤/模式) 与 M3 的 `expand_rev` 守卫
   同清 `selected_cell`。

### 验收

- 双击被截断的长值单元格 → 高亮 + Ctrl+C 得**完整值** (长于显示串)。
- 双击 level 列 → 复制级别词; 双击数字右对齐列 → 复制数字串 (对齐不影响)。
- 双击无该字段的行的空格区 (行内列区间但无值) → 无选中。
- 单击他格/Esc/切模式/过滤变化/展开折叠 → 选中消失。
- 优先级: 文本选区 > 单元格选中 > 行选中 (M2 链, 测试锁住)。

### 明确不动

单元格的渲染/截断/右对齐规则; 列宽采样逻辑; 表头 (chrome 区, 不参与);
表格模式**不做**词级选区与框选 (意图已定, 防 scope creep)。

---

## 6. Commands / Testing / Boundaries (全模块共用)

```
构建:   cargo build
测试:   cargo test                                   # 本仓基线 115 绿
静态:   cargo clippy --all-targets -- -D warnings
提交前: cargo fmt + clippy 零警告 + 测试全绿 (对应仓库内)
框架侧: cd ../danqing && cargo test                  # 基线 583 lib + 集成
联动:   cp tools/local-patch.toml .cargo/config.toml # 改 danqing 前必做 (patch 默认关)
```

- 测试落点: M1 = `danqing/src/text/selection.rs` tests 模块; M2–M4 = `view.rs`
  tests 模块 (沿用现有 LogView 测试构造法, 含临时文件夹具)。
- 行为断言一律测**公开语义** (`token_at` 返回值 / `selected_text()` / 状态机字段),
  不测实现细节。
- 边界 (仓规复述): 注释/文档中文; 新 `.rs` 文件头 `//! @author 十四叔` + `//! @date`;
  danqing 依赖保持 git + lock 钉 rev, **不提交 `[patch]`**; 未获指示不 commit/push;
  两仓改动分别提交并注明关联。
- **Ask first**: 任何超出四模块清单的框架 API 变更; 新增依赖 (本 spec 零新增)。

---

## 7. Success Criteria (整体)

**机器侧**:
- danqing: 三件套绿, `token_at` 新用例全过, 框架基线 (583 lib + 集成) 不破。
- 本仓: 三件套绿, 基线 115 + 新增用例全过; `Cargo.lock` 重钉到新 danqing rev。
- 行为矩阵 (§0) 每格有测试或人工验收项对应。

**人工验收 (用户实机, 逐项过)**:
1. .log: 双击时间戳/IP/`level=ERROR` 整选; 双击 `hello,world` 选半个; 双击中文选单字。
2. .log: 单击选中行 Ctrl+C = 整行; 旧五种姿势回归 (双击选词/框选/跨行/表格行复制/焦点切换)。
3. .jsonl: 双击单元格 (含被截断的长值) → Ctrl+C 得完整值。
4. 展开块: 双击选词 / 框选跨子行 / 选中子行 Ctrl+C = 子行文本。
5. GBK 中文日志 (`demo-cn-gbk.log`): 双击中文逐字 + 行复制不乱码。

**收口**: 全过后才重拍商店截图 → 打 tag → GitHub Release → 商店提交 (v1.0 链路)。

---

## 8. Open Questions

1. ~~连接器字符集~~ **已定** (``- : . / \ = @ _ + %``, 能力地图批准含此)。
2. 单元格选中要不要底栏提示 (如「已选单元格, Ctrl+C 复制」)?
   **倾向不要** (高亮即反馈, 行选中也没有提示) —— 如无异议按此落地。
3. 子行框选跨**两个**展开块 (中间隔普通行): 选区连续跨越, 复制混排 ——
   语义自洽, 不特殊处理; 实机验收若觉得怪再裁。

---

## 9. 变更记录

- 2026-09-14: 初版 (能力地图同日获批; M1–M4 需求与验收落定)。
