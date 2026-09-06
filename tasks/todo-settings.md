# TODO: 设置入口 + 轻量设置卡

> plan: `tasks/plan-settings.md` | spec: `docs/specs/SPEC-settings.md`
> 依赖框架 `update-check`(先 push danqing → `cargo update -p danqing`) + app-chrome(状态栏在位)。
> 轻量单页: 关于 + 版本行 + 反馈; 无快捷键表/无 tab。

- [ ] **S1: app_update 接线 + Cargo feature**
  - Acceptance: Cargo 加 `danqing = { features=["update"] }`; `src/app_update.rs` 供 UpdateSpec + 启动
    spawn_check() + current_hint() 包装; LogApp 初始化调一次
  - Verify: `cargo build`(经 danqing 透传 update feature)
  - Files: `Cargo.toml`, `src/main.rs`, `src/app_update.rs`

- [ ] **S2: 状态栏设置入口**
  - Acceptance: LogView 状态行最右加「⚙ 关于」入口; hover + 点击 → Msg::OpenSettings; 位置计数左移让位; 不重叠
  - Verify: `cargo test`(命中区单测) + 人工
  - Files: `src/view.rs`(LogView)

- [ ] **S3: 设置卡 widget**
  - Acceptance: `src/settings.rs` settings_card() = Stack+Box+Center; 关于(名称/v/设计一语) + 版本行
    (UpdateHint 状态+按钮) + 反馈 Link + CloseButton; 遮罩/Esc/✕ 关; 焦点限制卡内
  - Verify: `cargo test`(卡布局/事件: Open/Close/OpenUrl)
  - Files: `src/settings.rs`

- [ ] **S4: Msg 与布局接入**
  - Acceptance: Msg 增 OpenSettings/CloseSettings/OpenUrl/UpdateAction; LogApp 增 settings_open;
    view() 叠卡 overlay; event 处理关/链接
  - Verify: `cargo build` + `cargo test`
  - Files: `src/main.rs`, `src/settings.rs`

- [ ] **S5: 三件套 + 人工验收**
  - Acceptance: 三件套绿; 人工: 入口开卡/项齐全/反馈开浏览器/版本行(有新版前往下载/无则静默)/
    Esc·✕·遮罩关/焦点不逃逸
  - Verify: `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`; 人工
  - Files: —

- [ ] **Checkpoint: 模块验收**
  - [ ] 三件套绿; 入口/开卡/版本行/反馈/关闭/焦点陷阱人工过; 进 review
