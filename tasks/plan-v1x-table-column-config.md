# PLAN: table-column-config (列配置三件套)

> 日期: 2026-09-23 · spec: `docs/specs/SPEC-v1x-table-column-config.md` (已批准, 三项口径
> 全按推荐 + Open Q①② plan 裁定见「衍生设计」) · 零 commit 推进, 人工验收留用户闸门

## 组件与依赖

| 组件 | 位置 | 依赖 |
|------|------|------|
| ColumnConfig 模型 + 对账 + 持久化 | `src/columns.rs` (新) | serde_json (既有, 零新依赖), `Schema` 列名序列 |
| 表头几何缓存 + 命中 + 两手势 | `src/view.rs` | columns 有效布局; `col_spans`/hit rect/拖拽三先例 |
| 「列管理」弹层卡体 | `src/settings.rs` | export_menu 四件套先例 (`settings.rs:401-465`) |
| Msg / 状态真身 / 落盘 / apply_fresh | `src/main.rs` | columns; `cfg_path`/`save_config`/`license_path` 注入先例 |
| 文档收口 | README / ROADMAP / spec / map / CLAUDE.md | — |

切片原则: **模型 → 几何 → 手势 → 接线**纵向走; 判据最厚的对账/持久化先行 fail fast。
实施顺序 **T1 → T2 → T3 → T4 → T5 → T6**。Checkpoint **A**(T1–T2) / **B**(T3–T5) /
**C**(T6+人工验收+review)。

## 关键实现事实 (2026-09-23 调研已核实, 不靠猜)

1. **列模型在引擎且只有建议宽**: `Column { name, width_chars }` (`danqing-logfile/src/jsonl.rs:25-34`),
   语义=采样最大值 clamp [4,32] 字符 (`:23,:55-62`), `MAX_COLUMNS=16` 首见序 (`:41,:144-180`)。
   **不改写** `width_chars`——用户配置是产品层新状态, 与 `discover_schema` 重跑/换文件零互踩 (spec D1)。
2. **列几何铁律**: `event` 无 `TextBatch`, 列宽重算不可能（`view.rs:827-830` `col_spans` 注释「D2」）——
   手柄热区/表头体区间/「列…」按钮 rect 全部 **paint 缓存 event 只读**。列宽现算法 =
   `"8"×width_chars` 实测 + `COL_PAD=16` 左→右累加（`view.rs:1348-1371`）, 溢出右缘整列不画;
   横向偏移 `x_offset` 是视图局部态**不经应用层**（`view.rs:1988-1993`——拖宽预览走同款先例）。
3. **表头现状零交互**: `HEADER_H=28`（`view.rs:123`）有绘制无 hit rect/hover/事件分支,
   点表头现落「此处无行」notice（`view.rs:2196-2206`）——T3 把这段事件分支换掉。
   命中矩形双先例: 纯函数 (`settings_hit_rect`/`export_hit_rect`, `view.rs:92,101`) + paint 写
   `Cell<Rect>` event 读。
4. **拖拽零框架改动**: `CursorMoved` 全树广播 + `MouseInput{button,pressed,position}` + 框架
   指针捕获（拖出区域抬起仍路由回来, `handler.rs:117-119,958-977`）; 两条完整先例——滚动条
   拇指（`view.rs:758-761,1957-1994,2214-2219`）与文本框选「按下潜伏→超 `CLICK_DIST=4.0`
   升级→跟手→抬起落定/Esc 连潜伏一起清」（`view.rs:831-835,2008-2018,2266-2286`）;
   双击产品侧自判 `DOUBLE_CLICK_MS=300`/`CLICK_DIST=4.0`（`view.rs:279-281,1089-1098`, text/cell
   共用可直接给手柄）。`MouseInput` 无修饰键是**行多选**的前置, 本模块不需要。
5. **弹层四件套**: `Overlay::themed + Center + bind_open 谓词 + on_scrim_click`
   (`settings.rs:401-465`, 两卡互斥靠谓词); Esc 次序与模态守卫在 `app_key_filter`
   (`main.rs:2059-2118`)——**必须保持在 ctrl+字符筛选之后**（第一版放错位置被 review 抓出,
   注释有案）; 换文件关菜单锁先例 `apply_fresh_closes_export_menu` (`main.rs:2837-2849`)。
6. **持久化双先例**: `config.toml` 两键朴素解析 + 整文件覆盖写 + 「两键必须同源写」红线
   (`config.rs:56-101`)——**不碰它**; `license.key` 同目录独立文件先例 (`license.rs:249`);
   `save_config` 测试构建无注入路径直接 `panic!` (`main.rs:606-613`)——`columns_path` 同规封死。
7. **测试范式**: 合成几何注入 (`cell_fixture`, `view.rs:5541-5575`) + temp 路径 panic 封死
   真实桌面副作用 + 面板守卫 (`panel_contents_fit_fixed_height`, `settings.rs:1030`) +
   **真 paint 接线锁** (`log_view_paint_wires_the_update_dot`, `view.rs:4639-4648`)——v1.0.2
   教训: **绘制锁的 paint 原点必须含非零平移态**。不许调会弹真对话框/写真配置的入口
   (export 评审事故有案)。
8. **免费层白送**: 地图模块表依赖列为空, **不接 `Feature` 门控**（spec D7 显式防后人）。
9. **测试基线 314**（2026-09-23 export 收口实测）——T1 开工三件套复核回填, 别拿旧值当回归基线。

## T1 开工核实结论 (2026-09-23 已核, 不靠猜)

1. **列名唯一是构造保证** —— `discover_schema` find-or-push 收敛同名 key
   (`danqing-logfile/src/jsonl.rs:160-168`), `ColumnConfig` 以列名 exact key 成立,
   不需要 (name, 序位) 复合 key。
2. **serde derive 不在依赖**（`Cargo.toml` 仅 serde_json + preserve_order）→ `columns.json`
   走 **`serde_json::Value` 手拼**（零新依赖保底即定案）。
3. **sync 通路**: `Widget::sync(&mut self, state: &dyn Any)` downcast `LogApp` 拷入 view
   (`view.rs:1243`); view→app = `msgs: &mut MsgQueue` push + `LogApp::update`
   (`main.rs:1753`); view 局有 reconciliation 保活（`press`/`x_offset`/`col_spans`
   等同层先例）——`ColumnConfig` 走 sync, 拖拽预览归 view 局部态。
4. **注入形态**: `temp_cfg_path(tag)` = temp_dir + pid + tag (`main.rs:2334`);
   `license_path` = `cfg_path.with_extension("license.key")` 测试邻居 + 生产 default +
   无注入 `panic!` (`main.rs:623-633`) —— `columns_path` 同规照抄。
5. **apply_fresh 插入点** (`main.rs:1121-1194`): 三 job invalidate → 关 export 菜单 →
   换 file/schema/mode → 清视图态 —— **列配置载入插在 `self.schema = schema`
   (`:1161`) 之后**（先有 schema 才能对账）, `col_menu_open = false` 同批关。
6. **Msg 风格**: 无载荷/单载荷元组为主（`ExportFormatChosen(ExportPick)` 带枚举先例）,
   dispatch = `fn update(&mut self, msg: Msg)`; `is_double_click(&self, Instant, Point) -> bool`
   (`view.rs:1092`, 300ms/4px) 直接可用, text/cell 共用可给手柄。

## 衍生设计 (状态归属与几何缓存分叉 + Open Q①② plan 裁定)

**状态归属**: `ColumnConfig` 真身放 **`LogApp`**（落盘与 `save_config` 同源写哲学）, 经既有
sync 通路注入 `LogView`（事实 3）; **拖拽预览 = `LogView` 局部态**（`x_offset`/`h_drag` 同款
「不经应用层」）, **抬起才发 Msg 提交** + 落盘; Esc/放弃 = 回预览前值, 零半提交。

**几何缓存**: 表头几何**新建缓存**不硬塞 `col_spans`（后者是单元格列区间, 职责不同）——
`header_geom: RefCell<Vec<HeaderSpan { x0, x1, handle_x }>>` + `cols_btn_rect: Cell<Rect>`
(paint 写 event 读, 含 `x_offset` 平移后的绝对坐标, `col_spans` 同口径); 手柄热区
`±HANDLE_HALF=4.0` **优先于表头体**判定; 「列…」按钮固定角落**不随** `x_offset`。
拖宽预览时缓存跟手重写, 保证拖拽中命中不失真。

**Open Q① 裁定（光标）: 首版零联动** —— `CursorIcon` 仅 `Default/Pointer/Text`
(`danqing/src/event.rs:40-48`), 加 `ColResize` 变体=联动数行, 但滚动条/拖选两条拖拽先例
均无特殊光标（一致性）, 手柄 hover 高亮已给发现性, 本模块零框架改动的价值更高。
`ColResize` 列**后补联动**, 实机反馈驱动。

**Open Q② 裁定（弹层微调）: 上移/下移不进首版** —— 裁定②已定拖拽为主手势; 远距离拖拽
若实机喊累, 弹层补两枚小按钮是低成本后补, 不预做。

## 任务 (详见 `todo-v1x-table-column-config.md`)

- **T1** 纯逻辑 I: `ColumnConfig` 模型（order/hidden/widths 三映射）+ 有效布局计算 +
  约束（≥1 可见 / 宽 clamp / 恢复默认）+ 对账 `merge_with_schema`（增列补尾/减列剔除/
  失配静默/列名 exact）+ 对拍测试。
- **T2** 纯逻辑 II: `columns.json` 持久化（roundtrip / per-路径 LRU64 / 损坏容错不炸不
  覆盖 / `columns_path` 测试 panic 封死）+ 测试。
- **T3** 表头几何 + 命中 + hover: `header_geom`/`cols_btn_rect` 缓存、手柄热区边界、
  表头体分离、「列…」按钮、hover 高亮（`settings_hover` 先例）。
- **T4** 两手势: 列宽拖拽（跟手预览/提交/Esc 回滚/双击恢复采样宽/clamp）+ 表头拖拽换位
  （潜伏→超 `CLICK_DIST` 升级→插入位指示→落下/Esc 连潜伏清）。
- **T5** 弹层 + 全链路接线: 「列管理」卡体（`[x]`/`[ ]` 开关行 + 恢复默认行）+ 表头按钮/
  右键同 Msg + Esc 次序插入 + 模态守卫补项 + 与 `export_menu` 互斥 + `apply_fresh`
  载入/关菜单 + LogApp 提交落盘 + **真 paint 接线锁**（非零平移原点）+ **导出不跟随回归锁**。
- **T6** 文档收口: ROADMAP §一欠账表勾销三件套 / README 免费层能力行 / `ms-store-copy.md`
  清单核对 / spec 实现记+机器判据回填 / map·todo·CLAUDE.md 落账。

## Checkpoint

- **A（T1–T2 后）**: 纯逻辑测试全绿; 对账/持久化各有「摘掉实现精确红」A/B 记录; 基线
  314+N 回填; clippy 0。
- **B（T3–T5 后）**: 两手势实机可走; 弹层 Esc/互斥/apply_fresh 锁绿; 真 paint 接线锁绿
  （含非零平移）; 导出不跟随锁绿; 测试零真实桌面副作用。
- **C（T6 后）**: spec 成功判据机器部分 1–9 逐条过; **人工验收（用户实机, spec 五条）**;
  review 阶段走 `/agent-skills:code-review-and-quality`。
