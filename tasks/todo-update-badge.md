# TODO: update-badge
> Verify 栏数字 = 2026-09-22 收口实测终值 (非当时点值)。基线: danqing-log 253→258, danqing 617(默认)/646+1ignored(update) —— 初稿误抄 CLAUDE.md 冻结的 184/592, 评审 Nit 同步订正。

> 日期: 2026-09-22 · spec: `docs/specs/SPEC-update-badge.md` · plan: `tasks/plan-update-badge.md`
> 零 commit 推进; T6 是用户闸门, 未点头不动 git

- [x] T1: 腿 B —— danqing update.rs TLS 走系统证书库
  - Acceptance: Agent 根证书 = `RootCerts::PlatformVerifier`; 测试 `update_agent_trusts_platform_root_certs` 先红后绿 (红 = 仍为默认 WebPki 时断言不过)
  - Verify: danqing `cargo test` 646+1ignored 全绿 + `cargo clippy -- -D warnings` 0
  - Files: `danqing/Cargo.toml`, `danqing/src/update.rs`
- [x] T2: 腿 C 核心 —— `app_update.rs` 双轨分派
  - Acceptance: `use_store_track` 单点分派; `spec()` 每次现合成 (商店轨版本取包身份); hint 动作覆写纯函数; `go_download` 更名 `perform_action`; 锁 `dispatch_polarity_is_never_inverted` + `store_track_hint_overrides_action_to_update` 先红后绿; 旧 `updates_are_enabled_in_a_plain_binary` 语义并入极性锁
  - Verify: danqing-log `cargo test` 258 全绿 + clippy 0
  - Files: `danqing-log/src/app_update.rs`, `danqing-log/src/settings.rs` (调用点一行)
- [x] T3: 腿 C 运输 —— `mod store` 移植 + 隐私披露
  - Acceptance: `package_version` (OnceLock)/`check_update` (StoreContext)/`request_update` (IInitializeWithWindow + AtomicBool 防重入)/`spawn_check_store` (缓存闸门→后台线程→publish, 独立缓存文件名 `-msix` 防串味) 照 pomodoro 成稿; 复用 `store_license::find_main_window`; privacy-policy 联网表加行; ms-store-copy 检查单挂同步项
  - Verify: 编译过 + clippy 0 (运输行为归人工验收 f/h, 不单测)
  - Files: `danqing-log/src/app_update.rs`, `danqing-log/docs/privacy-policy.md`, `danqing-log/docs/ms-store-copy.md`
- [x] T4: 腿 A —— 设置按钮角标
  - Acceptance: `paint_update_dot` (push_rect 圆点, radius=3) + 几何纯函数; 锁 `settings_button_shows_update_dot_only_when_hint_exists` (两态对拍: 有→恰 1 个 accent 圆点实例, 无→空) + `update_dot_sits_at_the_label_corner_and_outside_layout` (6px, 文字右上, 不压文字不进位置计数区) 先红后绿 (A/B: 摘掉 push 须精确红); 断言走 `RectBatch::instance_rects/instance_colors` (已核实存在)
  - Verify: danqing-log `cargo test` 258 全绿
  - Files: `danqing-log/src/view.rs`
- [x] T5: 收口 —— 三件套 + 文档勾选 + 基线对账
  - Acceptance: 两仓 fmt/clippy/测试全绿; 基线对账 (253→258 / 617·646; 184/592 冻结旧值已订正); `danqing/docs/specs/SPEC-update-check.md:97` checkbox 勾上; 测试名 grep 确认真存在 (settings.rs:953 教训)
  - Verify: 两仓三件套
  - Files: `danqing/docs/specs/SPEC-update-check.md`
- [ ] T6: 联动落地 (**用户闸门 —— 未点头不动 git**)
  - Acceptance: danqing commit+push → danqing-log 关 patch `cargo check` 驱动重解 → `cargo update -p danqing` 复钉 → 两仓分别提交注明关联; lock 无 path 态
  - Verify: 无 patch `cargo check --locked` 过
  - Files: 两仓提交 + `danqing-log/Cargo.lock`

## 人工验收 (build 后另行, spec §7)

- [ ] 便携版 a–e (SteamTools 劫持态: 检查成功/角标/零痕迹/跳转/观感)
- [ ] 商店轨 f–h (侧载: 查询跑通/零 HTTP/植缓存出全套 UI)
- [ ] 商店轨 i (真商店更新流) —— **挂起至下次商店提交后补勾** (spec §8 已记欠账)
