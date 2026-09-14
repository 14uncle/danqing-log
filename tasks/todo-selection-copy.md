# TODO: selection-copy-consistency (选区/复制一致性)

> spec: `docs/specs/SPEC-selection-copy.md` | plan: `tasks/plan-selection-copy.md`
> 逐条勾选推进; 每任务完成后跑三件套 (fmt + clippy + test)。
> 2026-09-14 build auto: **T1/T3–T8 机器闭环全绿** (danqing 590 lib; 本仓 51 lib + 72 main + 8 genlog; clippy 0)。
> 2026-09-14 **review 收口**: 两路独立审查 (框架分词 APPROVE / 产品接线 REQUEST CHANGES
> → 两条必修 + 三条建议全落); 现计数 danqing **592** lib; 本仓 51 lib + **74** main + 8 genlog。
> T2 留用户闸门 (push + 重钉 lock); Checkpoint 人工验收待用户实机。
> 实现与 spec 的三处分叉 (引导段规则 / URL 查询串 / 多字节断言值) 已回本 spec §9。
> 构建序: M1 (框架) → M2 → M3 → M4; commit/push 点标 ⏸ 待用户点头。

## Phase M1: 框架分词 (danqing)

- [x] **T1: `token_at` 重写 + 测试全套**
  - 说明: 五类字符分类 (Space/Cjk/Word/Conn/Other) + 复合词规则
    (Conn 连续段两侧皆 Word → 内部化, D1); Cjk = `4E00–9FFF ∪ 3400–4DBF ∪
    F900–FAFF`; Conn = ``- : . / \ = @ _ + %``; Word = `is_alphanumeric` 且非 Cjk;
    非内部 Conn 落 Other 段。文档注释重写 (新语义 + spec §2 已接受代价清单)。
    **签名不变** (`token_at(&str, usize) -> (usize, usize)`), 不动
    `TextSelection`/`copy_text`/`row_slice`/`floor_char_boundary`。
  - Acceptance: spec §2 验收用例全过 —— 时间戳 (含 `+08:00`)/IPv4/IPv6 `::`/
    `level=ERROR`/两种路径/URL/email 整选 (点词中与点连接符上同效);
    `hello,world` 三分; `"ERROR"` 引号分离; `订单创建成功` 逐字;
    混合行 `订单(order_service) 创建成功` 各类各归; 空白段惯例; 空行/全空白;
    off 超界归最后字符; 任意 off 不劈 UTF-8 字符;
    旧断言 `token_multibyte_never_splits_char` 反向 ((0,6)→(3,6), 腹中 off 归下一字符),
    其余旧测试不动仍绿
  - Verify: `cd ../danqing && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
    (基线 583 lib + 集成不破)
  - Files: `../danqing/src/text/selection.rs`
  - Scope: M
  - **前置**: `cp tools/local-patch.toml .cargo/config.toml` (本仓, patch 默认关)

- [ ] **T2: 联动落地** ⏸
  - 说明: danqing push (**待用户点头**) → 本仓 (patch 关着)
    `cargo update -p danqing` → 三件套绿 → 提交 lock (待用户点头, message 注明关联)
  - Acceptance: `Cargo.lock` 的 danqing `source` 钉到新 rev; 本仓 115 基线 + 全测试绿;
    无 patch 状态下 `cargo build --locked` 可复现
  - Verify: `cargo test` + `grep -A2 'name = "danqing"' Cargo.lock`
  - Files: `Cargo.lock`
  - Scope: S
  - 注意: RustRover 并发 cargo 抢锁会把 lock 写回旧 rev —— 报
    `could not find ... in danqing` 时重跑, 验 lock 前关 IDE 自动 cargo

## Phase M2: 原始模式行复制 (本仓)

- [x] **T3: `selected_text` 三级链 + 原始模式行兜底**
  - 说明: 取文案链统一为 ① 非空文本选区 (含超限) > ② 单元格选中 (留位, M4 填) >
    ③ 行选中两模式统一 (普通行 = 原文; 子行 = 父行原文沿用, M3 再改子行串);
    删 `view.rs:1438` 的 `table_mode()` 条件; 改写 `view.rs:1421` 旧注释
    (「只认文本选区」→ 三级链说明)
  - Acceptance: 原始模式行选中 (无文本选区) → 该行原文 (含 GBK 解码行);
    旧断言「原始模式仅行选中 → None」(`view.rs:2510-2512`) 反向为 Some(整行);
    文本选区优先于行选中; 空选区 + 行选中 → 走行兜底;
    超限检查仍只作用于文本选区
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: S

## Phase M3: 展开块选区 (本仓, 依赖 M1)

- [x] **T4: 子行命中几何 + `hit_text` x_offset 分叉**
  - 说明: 子行 paint 分支 (`view.rs:950-968`) 构建 `RowGeom` 入缓存
    (文本 = paint 同串 `format!("{} = {}", label, value)`, base 0, start_x = indent,
    右界同 raw 行); `hit_text` (`view.rs:687`) 按 `line_at(row).1 > 0` 分叉 —
    子行的 content_x **不加** `self.x_offset` (子行不参与水平滚动, D3)
  - Acceptance: 子行命中返回 (row, 子行串字节偏移); 水平滚动 (x_offset > 0) 后
    子行命中不偏移; 普通行命中行为零变化 (回归)
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: M

- [x] **T5: 子行事件接线 + 复制供给**
  - 说明: 表格模式左键按压条件 (`view.rs:1339`) 放宽为 `!table_mode() || 子是行`
    → 子行走 `handle_text_press` 全套 (潜伏锚点/双击 M1 新语义/框选);
    子行 gutter 区 (x < text_x) = 清选区; 选区 `line` 回调 (`view.rs:1429-1435`)
    删 `String::new()` 防御改供子行串; 行兜底 (T3 第③级) 子行供给同步改子行串
  - Acceptance: 子行双击 = M1 新语义作用于子行串; 块内跨子行框选 Ctrl+C =
    子行串 `\n` 拼接 (反向拖动一致); 表格模式拖到普通行 → caret 冻结最后有效点
    (回归锁); raw 模式子行↔普通行跨界框选混排正确; **选中子行 Ctrl+C = 子行串
    (#5 回归锁)**; 选中普通表格行 Ctrl+C = 原文 (回归)
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: M

- [x] **T6: `expand_rev` 失效守卫**
  - 说明: `LogApp` 加 `expand_rev: u64`, `toggle_expand` (`main.rs:1119`) +1;
    `LogView` 记所见 rev, `sync` 不一致 → 清选区 + 潜伏按下 (复用
    `view.rs:731-735` 失效块); M4 的 `selected_cell` 预留同清钩子
  - Acceptance: 展开中有选区时折叠 → 选区清空; 折叠再展开不复活旧选区;
    无选区时 toggle 无副作用 (回归)
  - Verify: `cargo test`
  - Files: `src/main.rs`, `src/view.rs`
  - Scope: S

## Phase M4: 表格单元格复制 (本仓)

- [x] **T7: 列区间缓存 + `selected_cell` 状态 + 双击接线 + 高亮**
  - 说明: paint 把可见列区间缓存进 `RefCell<Vec<(f32, f32, usize)>>`
    (绝对窗口 x, D2); 新状态 `selected_cell: Option<(u64, usize)>` 存 LogView (D6);
    表格普通行左键接入 `last_click` 双击检测 (300ms/4px 复用常量),
    双击命中列 → 选中; paint 对该格画 `th.selection()` 底 (随缓存滚动);
    单击 = `Msg::Select` 现状不变
  - Acceptance: 双击单元格 → 高亮出现且行列正确; 双击无字段的格/行号槽/gutter
    → 无选中; 单击他格/文本选区动作 → 选中消失; 水平滚动后高亮跟格不跑偏
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: M

- [x] **T8: 复制第 2 级 + 失效同清 + 优先级链测试**
  - 说明: `selected_text` 第②级 = 该行 parse 后 `col.name` 字段经
    `jsonl::cell_display` 的完整串; parse 失败/无字段 → 不产 `selected_cell`
    (T7 接线处保证); `sync` 失效块 (文件/过滤/模式 + expand_rev) 同清
    `selected_cell`; Esc 清 (扩现有 Esc 分支)
  - Acceptance: 双击被截断长值 → Ctrl+C 得完整值 (长于显示串); level 列/数字
    右对齐列同样得值; 优先级 文本选区 > 单元格 > 行 (测试锁);
    Esc/切模式/过滤变化/折叠 → 选中消失
  - Verify: `cargo test`
  - Files: `src/view.rs`
  - Scope: S

## Phase M5: review 收口 (2026-09-14)

- [x] **T9: 两路独立审查 + 必修/建议全落**
  - 说明: 框架侧 (`danqing/src/text/selection.rs`) 与产品侧 (`src/view.rs` +
    `src/main.rs`) 各一路; 详见 spec §9 的 review 条目。
  - Acceptance: **必修两条** —— ① 越界行造隐形选中 (`cell_value` 加
    `row >= display_count()` 守卫; 回归锁经 **A/B 对照**验证: 摘掉守卫即红在
    `Some((9, 0))`); ② 单元格高亮与行底色同 token 不可见 (改两笔: 底色 +
    `accent()` 描边, 抽 `cell_highlight_colors` + 回归锁)。
    **建议三条** —— ③ 子行串抽 `sub_row_text` 唯一构造点 (原两处各 `format!`);
    ④ Esc 连同潜伏按下作废; ⑤ 框架侧三条 (枚举注释订正 / `?`&`#` 断言补齐 /
    边界用例组) + 边界落档两条 (滚出视口仍复制 / 行兜底无字节闸)。
  - Verify: `cd ../danqing && cargo clippy --all-targets -- -D warnings && cargo test`;
    本仓同款 —— **全绿 3 连跑**, clippy 0
  - Files: `../danqing/src/text/selection.rs`, `src/view.rs`
  - Scope: M
  - **未覆盖 (如实记)**: 全部新测试都塞合成几何, 没有一条走真实 paint →
    「三源一体」只锁了「给定正确几何, 命中/复制一致」。人工验收需重点看
    单元格描边是否真能一眼看出选中的是哪一格。
  - 余: code-simplify 阶段 (五阶段最后一段)。

- [x] **T10: code-simplification (五阶段最后一段)**
  - 说明: 行为不变前提下砍重复 —— 框架: 连接符段扫描改用 `run_over` (手搓 15 行
    与它是同一原语) + `is_punct` 命名谓词; 本仓: `text_x` / `row_at` /
    `is_double_click` 各抽成方法 (前两个原先在**三处**各推一遍同一个载荷几何式子,
    后者两处各抄一份双击判定)。
  - Acceptance: 测试**零改动**全绿; `--locked` 无 patch 构建通过; lock 复钉新 rev。
  - Verify: 两仓三件套; `cargo build --locked`
  - Files: `../danqing/src/text/selection.rs`, `src/view.rs`
  - Scope: S
  - **明确不做**: 按下处理的三分支链 —— 条件写明了才好读, 改成隐含不变式是
    用可读性换行数。

## Phase 收口

- [x] **两仓 push 完成** (2026-09-14): danqing `ce60236`→`b4b43e1`; 本仓 `4f36f95` + 重构笔;
      lock 钉 `danqing#b4b43e1b`; 工作区干净。

## Checkpoint: 人工验收 (用户实机)

- [ ] **spec §7 五条全过** (① .log 双击整选/选半/单字; ② 行复制 + 旧五种姿势回归;
  ③ .jsonl 双击截断格得全值; ④ 展开块三手势; ⑤ GBK 中文逐字 + 行复制)
- [ ] 全过后 ⏸: 重拍截图 → 打 tag → GitHub Release → 商店提交 (v1.0 链路, 待用户逐项点头)
