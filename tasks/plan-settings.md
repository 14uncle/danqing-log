# Plan: 设置入口 + 轻量设置卡 (settings)

> spec: `docs/specs/SPEC-settings.md`; 依赖框架 `update-check`(版本行) + app-chrome(状态栏在位)。
> 轻量单页卡(无快捷键表/无 tab): 关于 + 版本行 + 反馈链接; 入口在**底部状态栏**。

## Overview

状态栏(LogView 底部 `STATUS_HEIGHT` 行)右下加 ⚙/「关于」入口 → 点开浮层卡(Stack+Box+Center):
关于(名称/vX.Y.Z/设计一句话) + 版本行(`danqing::update::current_hint`, 有新版→前往下载)
+ 问题反馈链接(GitHub Issues)。Esc/✕/遮罩关闭, 焦点限制在卡内。

## Architecture Decisions

1. **入口在状态栏**: 状态行现由 `LogView.paint` 画 `status` + 右下 `行 x/count`。加一个**可点击入口**
   在状态行最右(位置计数左侧), LogView `event` 增命中区 → `Msg::OpenSettings`; hover 底色。
2. **设置卡复用框架组件**: `Stack[Box(scrim), Center[Box(card_bg).radius.lg.child(Padding[Column])]]`;
   版本行/反馈链接/关闭行照 clip `settings.rs`。不需要 MultiPanel(单页)。
3. **版本检查走框架**: `app_update.rs` 供 `UpdateSpec`(repo=14uncle/danqing-log, user_agent,
   releases_page, current_version=env!("CARGO_PKG_VERSION")); 启动调 `update::spawn_check()`;
   版本行绑定 `update::current_hint()`。不抄后端。
4. **焦点陷阱**: 卡为聚焦容器(CloseButton/Link/版本按钮 focusable); 打开时焦点入卡, Esc 关回到列表。

## Task List

### S1: app_update 接线 + Cargo feature
- **Acceptance**: `Cargo.toml` 加 `danqing = { ..., features = ["update"] }`; `src/app_update.rs` 供
  `UpdateSpec` + 启动 `spawn_check()` + `current_hint()` 包装。`LogApp.update` 初始化时调一次。
- **Verify**: `cargo build --features update`(经 danqing 透传); 编译通过。
- **Files**: `Cargo.toml`, `src/main.rs`, `src/app_update.rs` | M

### S2: 状态栏设置入口
- **Acceptance**: LogView 状态行最右加「⚙ 关于」入口: hover + 点击 → `Msg::OpenSettings`; 位置计数
  左移让位; 入口与状态/位置不重叠。
- **Verify**: 人工 — 状态栏见入口, 点击开卡; `cargo test`(入口命中区单测)。
- **Files**: `src/view.rs`(LogView) | M

### S3: 设置卡 widget
- **Acceptance**: `src/settings.rs` `settings_card()` = Stack+Box+Center; 内容 = 关于(名称/v/设计一句话)
  + 版本行(`UpdateHint` 状态+按钮) + 反馈 `Link` + `CloseButton`; 遮罩/Esc/✕ 关闭; 焦点限制卡内。
- **Verify**: `cargo test`(卡布局/事件: OpenSettings/CloseSettings/OpenUrl)。
- **Files**: `src/settings.rs` | M

### S4: Msg 与布局接入
- **Acceptance**: `Msg` 增 `OpenSettings/CloseSettings/OpenUrl(String)/UpdateAction`; `LogApp` 增
  `settings_open` 态; `view()` render 卡 overlay(settings_open 时叠在 `Column[TitleBar, LogView]` 上);
  `event` 处理关闭/链接。
- **Verify**: `cargo build`; `cargo test`。
- **Files**: `src/main.rs`, `src/settings.rs` | M

### S5: 三件套 + 人工验收
- **Acceptance**: 三件套绿; 人工 — 状态栏入口开卡、卡项齐全、反馈链接开浏览器、版本行有新版显示前往下载
  (无网络/最新 → 静默)、Esc/✕/遮罩关、焦点不逃逸到列表。
- **Verify**: `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`; 人工。
- **Files**: — | S

### Checkpoint: 模块验收 (S5 后)
- [ ] 三件套绿; 入口/开卡/版本行/反馈/关闭/焦点陷阱全人工过; 进 review。

## Risks and Mitigations
| 风险 | 影响 | 缓解 |
|---|---|---|
| 版本检查拉网络/打断 | 低 | 后台线程 + 24h TTL; 失败静默 None |
| 状态栏入口与位置计数重叠 | 中 | S2 让位布局 + 单测 |
| 焦点陷阱不生效(底层仍可打字) | 中 | S3 卡为聚焦容器; Esc/点击遮罩关闭路由显式 |

## Open Questions
- 反馈链接地址(GitHub Issues URL): `https://github.com/14uncle/danqing-log/issues` → 发布定稿, 本期先用。
- 「设计一句话」文案 → 命名定稿时定, 本期占位。
