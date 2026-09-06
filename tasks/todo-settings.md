# TODO: 设置入口 + 轻量设置卡

> plan: `tasks/plan-settings.md` | spec: `docs/specs/SPEC-settings.md`
> 依赖框架 `update-check`(先 push danqing → `cargo update -p danqing`) + app-chrome(状态栏在位)。
> 轻量单页: 关于 + 版本行 + 反馈; 无快捷键表/无 tab。

- [x] **S1: app_update 接线 + Cargo feature** ✅ 编码
  - Acceptance: Cargo 加 `danqing = { features=["update"] }`; `src/app_update.rs` 供 UpdateSpec + 启动
    spawn_check() + current_hint() 包装; LogApp 初始化调一次
  - Verify: `cargo build`(经 danqing 透传 update feature)
  - Files: `Cargo.toml`, `src/main.rs`, `src/app_update.rs`
  - 实测: Cargo.toml 加 features=["update"]; app_update.rs 封装 UpdateSpec + init/hint/go_download;
    main.rs 启动调 app_update::init(); 62 测试绿, clippy 0

- [x] **S2: 状态栏设置入口** ✅ 编码
  - Acceptance: LogView 状态行最右加「⚙ 关于」入口; hover + 点击 → Msg::OpenSettings; 位置计数左移让位; 不重叠
  - Verify: `cargo test`(命中区单测) + 人工
  - Files: `src/view.rs`(LogView)
  - 实测: LogView 加 settings_hover/settings_btn_rect (Cell); paint 画「⚙ 关于」(hover 亮色);
    event CursorMoved→hover 检测, MouseInput→settings_btn_rect 命中→Msg::OpenSettings;
    位置计数左移到设置入口左侧; Msg 增 OpenSettings/CloseSettings/OpenUrl;
    Cargo.toml 加 open="5"; 62 测试绿, clippy 0

- [x] **S3: 设置卡 widget** ✅ 编码
  - Acceptance: `src/settings.rs` settings_card() = Stack+Box+Center; 关于(名称/v/设计一语) + 版本行
    (UpdateHint 状态+按钮) + 反馈 Link + CloseButton; 遮罩/Esc/✕ 关; 焦点限制卡内
  - Verify: `cargo test`(卡布局/事件: Open/Close/OpenUrl)
  - Files: `src/settings.rs`
  - 实测: settings_overlay() = SettingsOverlay 组件 (open 态, 关闭时零高不可见);
    settings_card() = Box(card_bg).radius(12).child(Padding[Column[close_row, about, version, feedback]]);
    Scrim 点击→CloseSettings; VersionRow 有新版时显示提示+按钮(go_download);
    Link 点击→OpenUrl; Esc 关闭; 62 测试绿, clippy 0

- [x] **S4: Msg 与布局接入** ✅ 编码 (含在 S2/S3 中)
  - Acceptance: Msg 增 OpenSettings/CloseSettings/OpenUrl/UpdateAction; LogApp 增 settings_open;
    view() 叠卡 overlay; event 处理关/链接
  - Verify: `cargo build` + `cargo test`
  - Files: `src/main.rs`, `src/settings.rs`
  - 实测: Msg 增 OpenSettings/CloseSettings/OpenUrl; LogApp 增 settings_open;
    view() 改为 Stack[Column[TitleBar+LogView], SettingsOverlay]; event 开头检查 settings_open→Esc 关;
    settings_open 时 SettingsOverlay.sync 读态, paint/event 仅 open 时工作; 62 测试绿, clippy 0

- [x] **S5: 三件套 + 人工验收** ✅ 三件套绿 (人工验收待用户)
  - Acceptance: 三件套绿; 人工: 入口开卡/项齐全/反馈开浏览器/版本行(有新版前往下载/无则静默)/
    Esc·✕·遮罩关/焦点不逃逸
  - Verify: `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`; 人工
  - Files: —
  - 实测: cargo fmt + clippy 0 + 62 测试绿 + release 构建成功;
    exe 在 `/.cargo-target/release/danqing-log.exe`

- [ ] **Checkpoint: 模块验收** ⏳ 三件套绿; 入口/开卡/版本行/反馈/关闭/焦点陷阱人工过; 进 review
