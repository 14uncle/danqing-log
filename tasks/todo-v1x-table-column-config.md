# todo-v1x-table-column-config: 列配置三件套 任务清单

- @author 十四叔
- @date 2026/09/23
- Spec: `docs/specs/SPEC-v1x-table-column-config.md` · Plan: `tasks/plan-v1x-table-column-config.md`
- 状态: **五段全闭**（2026-09-23 一日: build T1–T6 → 双路评审并账 Critical×1+
  Required×6 全修 +17 锁 → code-simplify Nit×5 清零）—— **人工验收记账延后**
  （2026-09-23 用户裁定「全部记账」, 总清单 `tasks/acceptance-pending.md` A 组）
- 测试基线: **314**（2026-09-23 export 收口实测）→ build 收口 348 → review 收口
  **365**（simplify 行为零变化维持 365）

## Phase 1: 纯逻辑核心（`src/columns.rs`, 零 UI）

- [x] **T1: ColumnConfig 模型 + 约束 + 对账 merge** —— `src/columns.rs`（新文件头
  `//! @author 十四叔` + `//! @date 2026/09/23`）。**先核实**: plan「T1 开工第一件事」
  六条（列名唯一性 / serde derive 现状 / sync 通路 / temp 路径注入形态 / apply_fresh
  插入点 / Msg 风格）, 核完回填 plan。内容:
  - `ColumnConfig { order, hidden, widths }` 三映射（列名 exact key）+ `Default`
    （=schema 首见序全显采样宽, D1）
  - 有效布局: 显示列序 = `order` 过滤 `hidden`; 有效宽 = `widths` 否则采样建议宽
  - 约束: **≥1 可见**（关最后一可见列拒绝零变更）/ 宽 clamp `MIN_COL_W=40.0`·`MAX_COL_W=512.0` /
    恢复默认一键（D6）
  - 对账 `merge_with_schema`: 增列补尾 / 减列剔除 / 失配静默 / 重开取回, 列名 exact（D4）
  Acceptance: ①默认=首见序全显 ②三件套变更/恢复默认 ③全隐藏拒 ④对账五态（增/减/
  重开/失配/空表）各一测 ⑤摘对账精确红（A/B 记录: 失配列残留/新列丢失）。
  Verify: `cargo test` + clippy 0。
- [x] **T2: columns.json 持久化** —— 独立文件 `config_path()` 同目录（`license.key` 先例）,
  serde_json 读写（**零新依赖**; T1 核实 derive 与否, Value 手拼保底）。结构
  `{ files: [ { path, order, hidden, widths, updated } ] }`, **per-路径 key + LRU 64 条**
  （`updated` 淘汰最旧）; 损坏/缺失 → 空配置**不炸不覆盖**（下次保存才写好）;
  `columns_path` 测试构建无注入路径 `panic!`（与 `cfg_path`/`license_path` 同规,
  「测试不得写真实配置」家法）。
  Acceptance: ①roundtrip 全等 ②第 65 条挤掉最旧（LRU）③损坏文件容错 + 不覆盖断言
  ④无注入 panic 封死 ⑤摘 roundtrip 精确红（A/B 记录）。
  Verify: `cargo test`（逐测唯一临时文件）。

## Checkpoint A（T1–T2）✅

- [x] 纯逻辑 **15 锁全绿** (11 模型 + 4 持久化); 两处 A/B 精确红在案: ①摘对账补尾/retain
      → `["a","b"] ≠ ["a","b","c","d"]` (新列丢失) + `hidden.is_empty()` 红 (失配残留)
      ②摘 `entry_to_value` widths 写出 → roundtrip `widths:{} ≠ {"ts":133.5}` 精确红。
      **基线 314→329** (09-23 实测); fmt / clippy 0 (含 `sort_by_key(Reverse)` 一条修正)。
      实现记: serde derive 不在依赖 → Value 手拼 (plan 核实②定案); `ColumnConfig::default`
      = 空摆法 (未载入态), 有 schema 走 `from_schema`; **panic 封死随 T5 `columns_path`
      落位** (license_path 同规, 属 LogApp 接线, 从 T2 验收项移记此处)。

## Phase 2: 几何、手势与全链路（`view.rs` / `settings.rs` / `main.rs`）

- [x] **T3: 表头几何缓存 + 命中 + hover** —— `header_geom: RefCell<Vec<HeaderSpan{x0,x1,handle_x}>>`
  + `cols_btn_rect: Cell<Rect>`（paint 写 event 读, 绝对坐标含 `x_offset` 平移, `col_spans`
  同口径——**event 无 TextBatch 铁律**）; 手柄热区 `±HANDLE_HALF=4.0` **优先于表头体**;
  「列…」按钮固定角落**不随** `x_offset`; hover 高亮（`settings_hover`/`export_hover` 先例）;
  换掉「点表头落『此处无行』」的事件分支（`view.rs:2196-2206`）。
  Acceptance: ①手柄命中边界值（热区内外各断言）②表头体/手柄分离 ③按钮 hit rect 与
  真 paint 产出一致 ④hover 态写读 ⑤含非零 `x_offset` 平移命中正确。
  Verify: `cargo test` + clippy 0。
- [x] **T4: 两手势（列宽拖拽 + 表头拖拽换位）** —— 列宽: 手柄按下 → `CursorMoved` 跟手
  预览（**view 局部态**, `x_offset` 先例）→ 抬起发 Msg 提交 + 落盘; Esc 回滚拖前值;
  **双击手柄**恢复该列采样宽（`is_double_click` 300ms/4px）; clamp。换位: 表头体按下潜伏 →
  超 `CLICK_DIST=4.0` 升级换位拖拽（文本框选同款）→ **插入位指示**（accent 细线）→ 落下改
  `order` → Msg; **Esc 放弃连潜伏按下一起清**; 表头体单击无动作（不排序）。状态机与
  文本选区/滚动条**互不串扰**（滚动条先例: 「压根没进 self.press」）。
  Acceptance: ①拖宽跟手/提交/落盘 ②Esc 回滚 ③双击恢复采样宽（300ms/4px 判据）④升级
  阈值两侧（4px 内=无动作, 外=换位）⑤换位落点正确（首/中/尾）⑥Esc 放弃含潜伏按下
  ⑦拖宽中再点别处/滚轮不串手势 ⑧摘升级阈值精确红（A/B: 单击变拖拽）。
  Verify: `cargo test`（合成几何注入, **不真弹任何对话框**）。
- [x] **T5: 「列管理」弹层 + 全链路接线** —— 卡体: 每列开关行 `[x] 列名`/`[ ] 列名`
  （**ASCII 勾选框**, GB2312 字体约束）+ 底行「恢复默认」（D3）; 入口 = 表头「列…」按钮 +
  表头右键**同 Msg**; 四件套照抄 export（`Overlay + bind_open + on_scrim_click + 行按钮`）;
  Esc 次序插入 `upgrade > settings > col_menu > export_menu > 栏` + 模态守卫补
  `col_menu_open`（**保持 ctrl+字符筛选之后**）+ 与 `export_menu_open` **互斥**;
  `apply_fresh`: 先载入该路径条目再重绘 + 关菜单（先例锁同款）; LogApp 真身 + sync 注入
  + 提交 Msg 落盘（同源写）; **真 paint 接线锁**: 手动宽生效后字形 x 位移正确、隐藏列
  零字形、列序变化字形序跟随——**paint 原点含非零平移态**（v1.0.2 教训）; **导出不跟随
  回归锁**: 摆列后 CSV 仍 schema 首见序全列（export D5 口径不动）。
  Acceptance: ①开关行切换 + 最后一可见列拒 ②恢复默认一键 ③Esc 次序锁 ④互斥锁
  ⑤apply_fresh 载入/关菜单锁 ⑥真 paint 位移锁（非零平移）⑦CSV 不跟随锁 ⑧测试零
  真实桌面副作用（`columns_path` panic 封死已由 T2 兜底）。
  Verify: `cargo test` + clippy 0。

## Checkpoint B（T3–T5）✅ (机器部分)

- [x] 三件套绿: **348** (139 lib + 198 main + 8 genlog + 3 keygen; 314→329(A)→
      334(T3)→342(T4)→348(T5)); clippy 0 (4 条修正: sort_by_key(Reverse) /
      empty_line_after_doc(T4 插入劈开 cell_fixture 夹具 doc 已归位) /
      iter_next_slice→is_empty)。**A/B 精确红三处齐**: ①对账 (摘补尾/retain → 新列丢
      `["a","b"]≠["a","b","c","d"]` + 失配残留 `hidden.is_empty()` 红) ②持久化
      (摘 widths 写出 → `{}≠{"ts":133.5}` 红) ③接线 (摘 paint 预览分支 → 自定义断言
      「预览宽须进 paint」精确红)。
- [x] 实现口径记: **落盘随 T4 提交链提前落地** (原计划 T5 —— 提交 Msg 必须带落盘才
      验得动「提交/落盘」), T5 留弹层/Esc/模态/apply_fresh/两把回归锁; **panic 封死
      当场抓到 3 条既有测试**走 `new_empty()` 触 apply_fresh 读 columns.json
      (lvlcnt/freshfocus/freshinv 补注入 —— 守卫按 save_config 先例起效); format_btn
      泛化收 `impl Fn() -> Msg` (动态列行要捕获闭包); 弹层数据**取值传入** (引用会被
      'static widget 捕获 —— export_menu_card 同款教训)。
- [ ] **实机各走一遍归人工验收（Checkpoint C 用户闸门）** —— 测试侧已锁机器判据 1–9
      (含真 paint 平移不变字形锁 + CSV 不跟随锁)。

## Phase 3: 测量与收口

- [x] **T6: 文档收口** —— ROADMAP §一欠账表勾销「列宽拖拽/列显隐/列重排」/ README
  免费层能力行补列配置三件套 / `docs/ms-store-copy.md`「v1.x 上架时必须改什么」清单
  核对（免费层能力话术素材）/ spec 实现记 + 成功判据机器部分逐条回填 / map·todo·
  CLAUDE.md 状态落账。
  （本模块无性能测量面——O(≤16 列) 每帧布局, 不设 logbench 项。）

## Checkpoint C（T6）(机器部分)

- [x] spec §成功判据机器部分 1–9 逐条过（348 绿; A/B 三红见 Checkpoint A/B）
- [ ] **人工验收（记账延后 → 总清单 A 组六条）**: ①手柄拖宽/双击恢复/hover ②表头
      拖拽换位/插入指示/Esc 放弃 ③「列…」按钮与右键弹层显隐/末列关不掉/恢复默认
      ④重启记住/换文件各记/坏 columns.json 如常 ⑤深浅主题 + Ctrl+T 往返 ⑥截断列
      可见边缘拖宽（评审 B2 验点）
- [x] 进 review 阶段（`/agent-skills:code-review-and-quality`）—— 2026-09-23 **双路
      独立评审**（五轴全量 + 三区深潜互不知情）均 Request changes; 并账去重
      **Critical×1+Required×6 全修** + Optional 修 8/预防锁 1/文档化 1, **+17 锁**
      （348→365）; 修复清单与 A/B 红记录见 spec 评审记。Nit×5 留 simplify。
- [x] 进 code-simplify 阶段 —— 2026-09-23 收口: **Nit×5 清零**（move_column 边界 /
      `HeaderHit::None`→`Miss` / `last_is_move`→`msg_count` / widths 键排序落盘 /
      「排尾」doc 补全）+ 消重复形状（`header_gesture_active` / `abandon_header_gesture`
      各收 3 处）, 365 绿行为零变化, 不动清单见 spec 简化记。

## 遗留 / 待用户动作

1. 人工验收（记账中 —— 见 `tasks/acceptance-pending.md` A 组, 免费层无门控）
2. Open Q① 后补联动: `CursorIcon::ColResize`（实机反馈驱动, 不进首版）
3. Open Q② 后补: 弹层内上移/下移微调（实机反馈驱动, 不进首版）
