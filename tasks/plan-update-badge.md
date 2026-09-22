# PLAN: update-badge (三腿)
> Verify 栏数字 = 2026-09-22 收口实测终值 (非当时点值)。基线: danqing-log 253→258, danqing 617(默认)/646+1ignored(update) —— 初稿误抄 CLAUDE.md 冻结的 184/592, 评审 Nit 同步订正。

> 日期: 2026-09-22 · spec: `docs/specs/SPEC-update-badge.md` (已批准) · 零 commit 推进, T6 留用户闸门

## 组件与依赖

| 腿 | 组件 | 仓库 | 依赖 |
|----|------|------|------|
| B | TLS 根证书 → `RootCerts::PlatformVerifier` | danqing | — |
| C | 双轨分派 + `mod store` 移植 + 隐私披露 | danqing-log | — (与 B 异仓, 并行安全) |
| A | 设置按钮角标 (paint_update_dot) | danqing-log | — (只读 `hint()`, 不知运输) |

实施顺序: **T1(B) → T2(C核心) → T3(C运输+披露) → T4(A) → T5(收口) → T6(联动, 用户闸门)**。
T1 与 T2–T4 异仓可并行, 但单人带宽下顺序走; T2→T3 同文件必须串行; T4 独立殿后 (最可见, 先让数据通)。

## 关键实现事实 (开工前已核实, 不靠猜)

1. **ureq 3.4 API**: `ConfigBuilder::tls_config(TlsConfig)` (`config.rs:462`) +
   `TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build()` (`tls/mod.rs:179/265`);
   默认 `RootCerts::WebPki`。feature 行: `["rustls", "json"]` → `["rustls", "json", "platform-verifier"]`。
2. **RectBatch 内省已有**: `#[doc(hidden)] instance_rects/instance_colors/instance_radii`
   (`render/rect.rs:402-418`) —— 腿 A 可「真画一遍」断言, **零框架改动**, §4 文件清单不越界。
3. **store_license 可复用面**: `find_main_window() -> Option<HWND>` (`pub(crate)`) +
   RoInitialize/IInitializeWithWindow 纪律在库内; `windows` + `windows-collections` 依赖已齐。
4. **pomodoro 成稿移植源**: `danqing-pomodoro/src/update.rs` `mod store` + 双轨分派形状
   (`current_version`/`spawn_check`/`current_hint`/`perform_action`)。

## D6 衍生设计 (单二进制双轨的两处细节, pomodoro 成稿没有的)

1. **判据取反锁**: `Track` 枚举显式分派 (`track_of` 纯模型单点) + GitHub 臂体内
   `github_http_allowed` 隐私双保险 (直读包标识, 独立于分派) —— 取反 = 商店版走
   GitHub 轨发 HTTP = 隐私主张变假 (D6 最大风险), 破主张需两次独立错误 (评审加固后形态)。
2. **缓存防串味**: pomodoro 编译期分轨天然不串; 我们运行时分轨、**同机双装共用
   `%APPDATA%\danqing\`** —— 商店轨缓存改落 `update-check-14uncle-danqing-log-msix.json`
   (与便携轨分文件, `cache_path` 换名), publish 全局态每进程单轨天然隔离。
   不分文件的后果: 便携缓存的 `KnownVersion` 会让商店版显示带版本号的假提示 (action 还错)。

## 任务 (详见 todo-update-badge.md)

- **T1** 腿 B: danqing ureq feature + Agent TLS 配置 + `update_agent_trusts_platform_root_certs`
  (先红后绿)。验证: danqing 646+1ignored 全绿 + clippy。
- **T2** 腿 C 核心: `app_update.rs` 双轨分派 (Track 模型 / current_version / spec() 每次现合成 /
  hint 覆写 pure fn / perform_action 更名) + 两锁 (`dispatch_polarity_is_never_inverted` /
  `store_track_hint_overrides_action_to_update`), 旧 `updates_are_enabled_in_a_plain_binary`
  语义并入极性锁重写。验证: danqing-log 258。
- **T3** 腿 C 运输: `mod store` 移植 (package_version/check_update/request_update/spawn_check_store
  + 防重入 + 独立缓存文件名) + `settings.rs` 调用点一行 + privacy-policy/ms-store-copy 披露两处。
  验证: 编译 + clippy (运输行为归人工 f/h)。
- **T4** 腿 A: `paint_update_dot` + 几何纯函数 + 两锁 (显隐两态对拍 / 几何),
  断言走 RectBatch 内省 (A/B 摘掉 push 须精确红)。验证: 258。
- **T5** 收口: 两仓三件套 + 基线对账 (258 / 646+1ignored) + `SPEC-update-check.md:97` 勾选。
- **T6** 联动 (**用户闸门**): danqing commit+push → danqing-log 关 patch `cargo update -p danqing`
  复钉 → 两仓分别提交注明关联。patch 开着 lock 是 path 态不许提交 (既有纪律)。

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| 判据取反 (D6) | `dispatch_polarity_is_never_inverted` 双臂锁; 取反后果写进测试注释 |
| patch/lock 陷阱 | 全程零 commit; T6 单列且按「先 push 后复钉」顺序 |
| windows crate API 漂移 (pomodoro 移植) | store_license 同库同版本已趟平; 复用其 import 面, 不引新 API |
| RectBatch 断言通道 | 已核实 `#[doc(hidden)]` 三件套存在, 无需框架改动 |
| 商店运输侧载不可全验 | spec §7 (f/g/h/i) 如实两截; 植缓存可验全套 UI |
| 观感像素 (6px/accent) | 验收 (e) 当场裁, spec/锁同步回写 |

## 验证检查点

- T1 后: danqing `cargo test` + clippy 0 (646+1ignored)。
- T2/T3 后: danqing-log `cargo test` + clippy 0 (258)。
- T4 后: 同上 (258), 含 A/B 先红后绿留痕。
- T5 后: 两仓 `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + 全绿对账。
- T6 后: `cargo check --locked` (无 patch) 过; 人工验收 (§7) 另行排。
