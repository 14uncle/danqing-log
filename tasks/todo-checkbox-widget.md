# todo-checkbox-widget: 框架复选框 + `[x]` 换挂 任务清单

- @author 十四叔
- @date 2026/09/28
- Spec: `docs/specs/SPEC-checkbox-widget.md` · Plan: `tasks/plan-checkbox-widget.md`
- 状态: **五段全闭**（2026-09-28 一日: 「go」→ plan → `/build auto`（零 commit,
  例外 = T2 联动已授权）→ 双路评审并账全修 → code-simplify 三项行为零变化）;
  人工验收记账 **I 组**（H 组被并行会话 notice-visibility 先占，让位）
- 测试基线: 本仓记档 495 → 实测复核 **507**（记档漂移）→ 收口 **516**（+8 锁，
  另含并行会话 +1）；框架记档 603 → 实测复核 **630** → 收口 **643**（+13 锁）

## Phase 1: 框架半边（danqing）

- [x] **T1: `src/widget/form/checkbox.rs` 新件** —— `CheckboxColors`（from_theme /
      on_accent 两套）+ `Checkbox` 全件 + **`paint_box` 静态画法单真源** +
      BOX_SIZE 14 / 边框 1.5 / 圆角 3 / 勾两笔 `push_diagonal`。
      Acceptance ①–⑦ **全过**（layout 固定+收窄 / 点击产 Msg·拖出不产·右键不冒充 /
      Space·Enter 持焦才产 / 焦点环多一笔 / bind·bind_theme 每帧刷新（SceneTheme
      可变 accent 夹具）/ paint_box A/B 锁 / widget-paint 委托全等锁）。
      **A/B 精确红在案**: 摘勾两笔 → 判罪锁红（「应有两笔勾的圆点队列实例」）。
      修程: 初版「勾中实例总数 > 未选」断言被边框实现推翻（圆角边框是多实例，
      总数假设不成立）→ 改断 border 色实例存在；clippy `manual_contains` ×4 修。
      Verify: `cargo test checkbox` 13 绿 + clippy 0 + fmt。
- [x] **T2: 导出 + showcase + 联动闸门** —— 两级导出；showcase「复选框」区
      （可交互 + 静态勾中 `bind(|_| true)` + 静态未选）；框架全量 **643 绿**；
      **danqing `c5b1fcf` 已 push**；本仓关 patch → `cargo update -p danqing`
      踩中已录陷阱（lock path 态报 did not match → `cargo check` 重解即钉上，
      09-14 实录解法奏效）→ 复钉 `danqing#c5b1fcfe` → **`b091040` 提交 lock**。
      Acceptance ①–⑤ 全过（showcase 构建绿 / 框架全量绿 / push 完成 /
      lock 钉态 / 本仓基线 507 不破）。
      Verify: 两仓 `cargo test`。

## Checkpoint A（T1–T2，框架半边）✅

- [x] T1 判据 ①–⑦ 全绿；摘勾精确红在案；框架全量 643 绿（复核基线 630 不破）
- [x] showcase 出现 Checkbox；danqing `c5b1fcf` 已 push；本仓 lock 复钉 `b091040`

## Phase 2: 产品半边（danqing-log）

- [x] **T3: `RowList::with_checkbox` 加法（零换挂）** —— 第五闭包 + sync 缓存
      `Vec<Option<bool>>` + 两套 CheckboxColors 每帧刷新 + paint 画盒（is_hi 行
      on_accent 套；盒垂直居中；`CHECK_W`=盒 14+6 同源）。
      Acceptance ①–④ **全过**（不装零盒 / 装了每行恰一盒（全勾数填充·全不勾数边框,
      单盒边框实例数探针量不写死）/ None 行无盒 / **摘重取精确红**（2≠1）在案 /
      既有 6 锁零修改绿）。
      Verify: `cargo test --bin danqing-log pick_list` 10 绿。
- [x] **T4: 两处换挂 + 守卫** —— `col_menu_rows` / `merge_source_rows` 删
      `{mark} ` 前缀 + 装 with_checkbox；`merge_source_label` 抽纯函数
      （merge_union_hint 先例）；`ROW_PAD_X` 常量化（paint 三处同源）+
      `SWATCH_W`/`CHECK_W` 升 pub(crate)；旧注释三处同步（pick_list 模块头 /
      settings 两处）。
      Acceptance ①–⑤ **全过**（产出串退役锁 ×2（运行时断言载荷不变）/
      勾态正确 / 行宽守卫 ×2（量内容式，plan 事实 7 预判的分叉）/
      D3b 反白锁（pick_list 层 is_hi×checked 矩阵）/ 既有锁全绿）。
      修程: clippy needless_borrow ×1 + doc_lazy_continuation ×2 修。
      Verify: `cargo test` 516 绿 + clippy 0（真退出码）+ fmt。

## Checkpoint B（T3–T4，产品半边）✅

- [x] T3 判据 ①–④ 全绿；T4 判据 ①–⑤ 全绿；`[x]`/`[ ]` 产出零残留
      （grep 命中仅退役锁自身的断言字面量）；507 → **516**（+8 锁）；三件套干净

## Phase 3: 收口

- [x] **T5: 文档收口** —— spec §9 实现记（5 条分叉如实记：焦点环外扩/守卫改量内容/
      showcase bind 恒真/基线复核更正/组别让位 H→I）/ acceptance-pending **I 组三条**
      记账（三条均需付费态）/ CLAUDE.md 状态行 / todo 落账。
      （无性能面；FEATURE-MATRIX/ROADMAP 不动——无能力增减，纯观感。）

## Checkpoint C（T5）(机器部分) ✅

- [x] spec 成功判据机器部分逐条过（基线复核不破 + A/B 红两处 + 516 绿 + 钉态 lock）
- [x] I 组三条记账进总清单（需付费态 key，与 G/H 同窗口实机）
- [x] **进 review 阶段（`/agent-skills:code-review-and-quality`）** —— 2026-09-28
      **双路独立评审（五轴全量 + 红队六区深潜，互不知情）均 APPROVE**，零
      Critical/Required；Optional×6 + Nit×4 **全清**（受约束 paint 取短边双轴居中 +
      锁 / showcase 静态对改互镜像可点 / paint_box 正方形前提 doc+debug_assert /
      明暗双主题映射锁+产品暗色用例 / 勾形几何签名锁（谷底偏左+右端上翘;
      首版「带内全左」断言被真勾形打脸改单点极值）/ 守卫改名+「−6px」更正实测
      −8px / 文档错命令 `--example showcase`→`danqing-showcase` 并补验真 exit 0
      （T2 验收①原是管道假绿）/ B-O4 长文案无裁剪挂账 ROADMAP §四）。
      **留档不修**: `hovered` 只写字段（Switch 同病, 族内一致优先）。
      修复锁 +4 → 框架 **705 绿**（lib 646+集成 59）/ 产品 **517 绿**。
      框架修复 danqing `8151d46` 已 push; 本仓 lock 二次复钉 `3f00814`
      （**非常规**: cargo update 连带重解撞 wgpu 的 windows 双版本错配,
      回 HEAD+单点换钉, diff 恰一行, 517 绿验证 —— 根因未查, commit 有案）。
      明细见 spec §10 评审记。
- [x] **进 code-simplify 阶段** —— 2026-09-28 收口: 三项行为零变化
      （框架 BOX_SIZE 并入主 impl / `per_row` 泛型助手收 swatches+checked 同形
      双块 / `merge_source_idx` 收两闭包查源起手式），**测试零修改**;
      不动清单（hovered 只写字段 parity / 行首件双块不合 / 测试探针重复 /
      Switch 同病另案）见 spec §11。框架 704（--lib --tests）/ 产品 517 全绿,
      两仓 clippy 0。环境备注: showcase.exe 被用户窗口文件锁 → 备用
      target-dir 绕锁验证链接 exit 0。联动: danqing `7813d1d` push →
      本仓单点换钉 `5275a0f`。**五段全闭**。

## 人工验收（记账 → `tasks/acceptance-pending.md` I 组，需付费态 key 实机）

- [ ] I-a 合并源卡：盒渲染（勾中/未选/**高亮行反白**）、点行切显隐 —— 明暗两主题
- [ ] I-b 显示列弹层：盒渲染、点行切显隐 —— 明暗两主题
- [ ] I-c 字段查询弹层：**无盒**回归（第三消费者零变化）

## 并发备注

- notice-visibility 会话同窗口 build（`src/toast.rs`/`main.rs`/`view.rs`/
  `tasks/acceptance-pending.md` H 组是其改动，本批零触碰；本批独有 =
  `pick_list.rs`/`settings.rs`/框架三文件+showcase）。**后续若有任何一方 commit
  src，按路径 add 前先 diff 确认不混入对方改动**（09-15 事故同型）。
- 教训兑现一条：clippy 管道假退出码踩中一次（09-13 记过的同型耙），
  已改 `> file 2>&1; echo $?` 取真码。
