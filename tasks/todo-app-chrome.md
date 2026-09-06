# TODO: 窗口标题栏 (app-chrome)

> plan: `tasks/plan-app-chrome.md` | spec: `docs/specs/SPEC-app-chrome.md`
> 依赖框架 `titlebar-embed`(先 push danqing → `cargo update -p danqing` → 本项目再搬)。
> 逐条勾选; 每任务后跑三件套。

- [x] **A1: view() 换 TitleBar + 接三窗键** ✅ 编码 (人工开窗验收待用户)
  - Acceptance: `view()` = `Column[TitleBar.embed(Bar), LogView.fill]`; TitleBar 设 title/logo_kind=Log/
    on_close(Close)/on_minimize(Minimize)/on_maximize(MaximizeOrRestore)/bind_maximized; Bar 不再作 sibling
  - Verify: `cargo build` + 人工开窗看标题+三键
  - Files: `src/main.rs`
  - 实测: TitleBar::themed(&title_theme(), title) + LogoKind::Log + 三键 → WindowAction + bind_maximized
    (LogApp 加 maximized 字段 + override maximized_changed); Bar 挪进 embed 槽;
    title_theme() 用 SceneTheme 深色调色板(浅色文字, log 深色系); 56 lib + 6 主测试绿, clippy 0
  - **人工验收 2026-09-06 ✅ 用户「通过」**: 标题/三窗键/过滤栏聚焦 OK; 拖拽补 on_drag(5aaf250) 后生效

- [ ] **A2: Bar 适配 embed 槽几何**
  - Acceptance: Bar layout/paint/event 用槽 area; label_width/input_area 以槽为基准; Bar Hidden 时标题栏仅标题+三键
  - Verify: `cargo test` + 人工(原始无搜索无输入/表格搜索有输入)
  - Files: `src/view.rs`(Bar)

- [ ] **A3: 标题串 + 窗口配置一致性**
  - Acceptance: TitleBar.title = `丹青日志 POC [JSONL] — <file>`(或 文件名·模式); WindowConfig.title 同串;
    随 mode 带/不带 JSONL
  - Verify: 人工(任务栏与 TitleBar 一致; Ctrl+T 切模式)
  - Files: `src/main.rs`

- [ ] **A4: 键盘/焦点/IME 回归**
  - Acceptance: 栏聚焦 Enter 应用/Esc 清关/PageUp·PageDown 滚动/中文 IME 贴框/粘贴/点击落焦;
    Ctrl+T 在栏聚焦仍生效; Tab/方向键不冲突
  - Verify: 人工回归(参考 todo-v1-textinput.md) + `cargo test`
  - Files: `src/main.rs`, `src/view.rs`

- [ ] **A5: 三件套 + 人工验收**
  - Acceptance: 三件套绿; 手动三键(最小/最大/还原/关闭)/拖拽移窗/双击最大化全过
  - Verify: `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`; 人工
  - Files: —

- [ ] **Checkpoint: 模块验收**
  - [ ] 三件套绿; 三键/拖拽/输入/IME/快捷键人工过; 进 review
