# Todo: notice-visibility

> @author 十四叔 · @date 2026/09/28
> 上游: `docs/specs/SPEC-notice-visibility.md` (已批) · plan `tasks/plan-notice-visibility.md`
> 基线: 495 绿 (186 lib + 295 main + 11 genlog + 3 keygen)

- [x] **T1: toast 组件本体** —— 新 `src/toast.rs` (文件头规则触发)
  - 内容: 非模态 Widget —— sync 读 `app.notice` + 主题 (取值/bind 随既有惯例);
    **仅 Warn 现身** (Info/None 零绘制); layout 底部水平居中、底缘贴状态栏顶
    (`view::STATUS_HEIGHT`, T2 才 pub(crate) —— 本 T 先以占位常量替身, T2 换同源,
    或一并先改 pub(crate) 再引用, build 时定); 自然宽随文案 + D8 上限
    `min(窗口宽×0.6, 520px)` 省略截断; paint = 不透明 `background()` 底 + 圆角 +
    `danger()` 左色条 + 正文色文案; paint 缓存命中矩形 (同源); 命中矩形内点击 →
    `Msg::DismissNotice`, 矩形外 `Ignored` 放行 (非模态)
  - Acceptance: 合成几何下 —— Warn 画底/条/文三样、Info/None 零绘制、点击矩形内
    出 DismissNotice、矩形外 Ignored、超长文案宽 ≤ 上限、双主题底色 alpha==1
  - Verify: `cargo test --bin danqing-log toast`
  - Files: `src/toast.rs` (新)

- [x] **T2: main 接线** —— Msg 链 + 挂载 + 收口
  - 内容: `Msg::DismissNotice` 臂 (调 `dismiss_notice()`); 抽 `dismiss_notice()`
    收口两处清置 (tick_notice 到点分支同调, 行为零变化); `STATUS_HEIGHT` 改
    `pub(crate)` (view.rs); Stack 末位挂 `toast::Toast::new()` (main.rs:3507 后);
    **不进** `close_popovers`/`popover_open`/Esc 次序表
  - Acceptance: 挂载后 Warn → toast 现身 (bind 显隐锁); DismissNotice → notice 双清;
    tick 到点锁 `notice_expires_when_its_deadline_passes` 不破; Esc/互斥测试不涉
    toast (负向断言: toast 不在 popover_open 清单)
  - Verify: `cargo test --bin danqing-log` (main + toast 全绿)
  - Files: `src/main.rs`, `src/view.rs` (一字), `src/toast.rs`

- [x] **T3: view 分派 + Info 色块** (须 T2 后, 见 plan 依赖序)
  - 内容: `view.rs` paint —— Warn 时底栏 notice 位**不画** (挪 toast); Info 时画
    圆角色块衬底 (token 打底 + text_primary, 几何 = 文本 measure + padding, 位置
    不动); **Q2 回填**: 实测两主题色块 vs 底栏底对比度, 记 spec §8
  - Acceptance: 双向分派锁 —— Warn: 底栏无该文本 + toast 画; Info: 底栏画(带衬底)
    + toast 不画; 色块几何包住文本; A/B: 摘 kind 分派 → Warn 文本回底栏 (精确红)
  - Verify: `cargo test --bin danqing-log view`
  - Files: `src/view.rs`

- [x] **T4: 端锁 + 收口**
  - 内容: `merge_menu_open=true` 态注入 Warn → toast 仍画 (弹层之上锁, Stack 反序
    分发构造保证的可执行版); 消退锁补 toast 侧断言; spec §8 Q2 实测回填;
    三件套全绿
  - Acceptance: 全部新锁 + 495 基线不破; clippy 0; fmt 过
  - Verify: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
  - Files: `src/main.rs` / `src/view.rs` / `src/toast.rs` (测试), spec 回填

- [ ] **人工验收 H 组** (spec §7 五条, 记账 `tasks/acceptance-pending.md`) —— 待实机,
  用户裁定节奏
