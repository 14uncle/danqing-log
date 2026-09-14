# Implementation Plan: selection-copy-consistency (选区/复制一致性)

> spec: `docs/specs/SPEC-selection-copy.md` · todo: `tasks/todo-selection-copy.md`
> 2026-09-14 plan 阶段。能力地图与 spec 均已批; 构建序 M1 → M2 → M3 → M4。

## Overview

把选区/复制补成全产品一致的手势体系: 框架 `token_at` 重写为混合连接器规则 +
中文逐字 (M1, danqing); 原始模式选中行 Ctrl+C 复制整行 (M2, 翻案);
展开块子行获得双击/框选/子行复制 (M3); 表格模式双击单元格复制完整值 (M4)。
**进 v1.0, 阻塞发布**。零新增依赖; pomodoro 不用 `token_at`, 框架波及面仅本仓。

## Architecture Decisions

- **D1: 复合词判定在「段」不在「字符」** —— Conn **连续段**整体看两侧邻居:
  紧前紧后都是 Word → 整段内部化。`::` (IPv6)、`:\` (Windows 盘符)、`://` (URL)
  无需特例清单自然整选; `foo---bar` 整选与 `1,000,000` 被切开是同一条规则的两面
  (spec §2 已记为已接受代价)。
- **D2: 列区间走 paint 缓存, 不在 event 重算** —— 列宽靠 `TextBatch.measure`,
  event 拿不到; paint 写 `RefCell<Vec<(x0, x1, col_idx)>>`、event 读, 与
  `settings_btn_rect` / `row_geom` / `gutter_w` 同一既有模式。滚出视口的列不在缓存
  = 点不到, 天然一致。
- **D3: 子行选区全套复用 raw 行机制, 唯一行为分叉 = `x_offset` 项** ——
  子行 paint 不含 `x_off` (`view.rs:954`, 不参与水平滚动), 而 `hit_text`
  (`view.rs:687`) 对所有行加 `x_offset` → 子行命中时该项必须为零, 按
  `line_at(row).1 > 0` 分叉。潜伏锚点/双击/拖动/Esc/超限提示全部零新路径。
- **D4: 展开态变化用 revision 守卫, 不做逐帧 diff** —— `LogApp.expand_rev: u64`,
  `toggle_expand` (`main.rs:1119`) 内 +1; `LogView` 记所见 rev, `sync` 不一致 →
  清选区 + 潜伏按下 + `selected_cell` (复用现有失效块, `view.rs:731-735`)。
  比 `ExpandMap` 派生 PartialEq 逐帧比对便宜, 且语义明确。
- **D5: `selected_text` 三级优先链, M2 搭链 M4 填级** ——
  ① 非空文本选区 (含超限检查) > ② 单元格选中 (M2 留位, M4 填) >
  ③ 行选中 (**两模式统一**: 普通行 = 原文; 子行 M2 阶段 = 父行原文沿用,
  M3 改为子行串)。现 `table_mode()` 条件 (`view.rs:1438`) 随链删除。
- **D6: `selected_cell` 存 LogView, 不进 LogApp** —— 与文本选区同层;
  应用层无需感知; 失效与文本选区同守卫块。

## Task List

### Phase M1: 框架分词 (danqing)

- [ ] **T1: `token_at` 重写 + 测试全套** (五类分类谓词 + 复合词规则 + 文档注释;
  旧断言 `token_multibyte_never_splits_char` 反向; spec §2 用例全落)
- [ ] **T2: 联动落地** ⏸ —— danqing 三件套 → **push (待用户点头)** →
  本仓 `cargo update -p danqing` → 全测试绿 → 提交 lock (待用户点头)

### Phase M2: 原始模式行复制 (本仓)

- [ ] **T3: `selected_text` 三级链 + 原始模式行兜底** —— 删 `table_mode()` 条件,
  留单元格级空位; 旧断言「原始模式仅行选中 → None」(`view.rs:2510-2512`) 反向;
  补: 选区优先于行 / 空选区落行兜底 / GBK 解码行

### Phase M3: 展开块选区 (本仓)

- [ ] **T4: 子行命中几何 + `hit_text` x_offset 分叉** —— 子行 paint 分支
  (`view.rs:950-968`) 构建 `RowGeom` (文本 = paint 同串, base 0, start_x = indent);
  `hit_text` 按 `is_sub_row` 分叉 x_offset; 水平滚动后子行命中不偏移的测试
- [ ] **T5: 子行事件接线 + 复制供给** —— 表格模式按压条件放宽
  (`!table_mode() || 子是行`); 选区 `line` 回调删 `String::new()` 防御改供子行串;
  行兜底子行供给同步 (D5); 双击走 M1 新语义
- [ ] **T6: `expand_rev` 失效守卫** —— LogApp 加 rev + toggle 递增 + LogView sync
  比对清选区; 折叠后选区不指错行/不复活的测试

### Phase M4: 表格单元格复制 (本仓)

- [ ] **T7: 列区间缓存 + `selected_cell` 状态 + 双击接线 + 高亮** ——
  paint 缓存列区间 (D2); 表格普通行双击命中列 → 选中; 矩形高亮随缓存滚动
- [ ] **T8: 复制第 2 级 + 失效同清 + 优先级链测试** —— `cell_display` 完整值;
  parse 失败/无字段不产选中; Esc/新按下/文本选区动作清; 三级链优先级测试

### Checkpoint: 人工验收 (用户实机, 全过才算完)

- [ ] spec §7 五条: ① .log 双击三样整选 + `hello,world` 选半 + 中文单字;
  ② .log 行复制 + 旧五种姿势回归; ③ .jsonl 双击截断单元格得全值;
  ④ 展开块双击/框选/子行复制; ⑤ GBK 中文双击逐字 + 行复制不乱码
- [ ] 全过后: 重拍商店截图 → 打 tag → GitHub Release → 商店提交 (v1.0 链路)

## 风险与注意

- **M1 是本批唯一跨仓改动**: 先 `cp tools/local-patch.toml .cargo/config.toml`
  再改 danqing (patch 默认关, 忘了 cp 则本仓静默用旧框架 —— 农场教训);
  push 前 `cargo test` 在 danqing 内跑 (583 lib 基线)。
- **RustRover 并发 cargo 会抢 package cache 锁**并把 lock 写回旧 rev
  (2026-09-13 实录) —— T2 重钉后若 clippy 报 `could not find ... in danqing`,
  重跑即顺, 验 lock 前关 IDE 自动 cargo。
- **row_geom 增子行缓存的 paint 成本有界**: 与 raw 行同款的逐字符测量 +
  右界截断, 只测可见窗口; 子行通常短 (`label = value`), 不预期可感知差异。
- **M3 的 raw 模式跨界框选** (普通行原文 + 子行串混排) 由 `copy_text` 逐行供给
  天然支持, 不特殊处理 (spec Open Q3)。
- 每任务完成跑三件套; **所有 commit/push 点都标 ⏸ 待用户点头**。
