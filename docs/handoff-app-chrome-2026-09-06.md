# 对接文档: danqing-log 窗件产品侧 (2026-09-06 收工)

> 本文件供新会话续接。读了它 + 下列「必读」即可开干, 无需翻历史对话。

## 一句话状态

danqing-log 产品侧「窗件」build 进行中: **app-chrome A1 已完成并人工验证通过**
(标题栏 = logo+标题+内嵌过滤/搜索 Bar+三窗键, 拖拽/标题/三键/过滤栏聚焦都 OK)。
剩 **app-chrome A2→A5** + **settings S1→S5**。

## 必读 (按此顺序)

1. 本文件
2. `docs/specs/SPEC-app-chrome.md` + `tasks/todo-app-chrome.md` (模块职责/逐个任务)
3. `docs/specs/SPEC-settings.md` + `tasks/todo-settings.md`
4. 框架侧 (顶栏请参考): `../danqing/docs/specs/SPEC-titlebar-embed.md` +
   `../danqing/docs/specs/SPEC-update-check.md` — 均已落地, 只在需要理解 embed/update 机制时读

## 为什么在做 (背景)

danqing-log v1 三模块闭环后, 用户反馈「UI 很丑 / 无最小最大化 / 无关于设置」。
裁决 = A 类丑(缺窗件) → 做窗件: **框架 TitleBar 加 embed 槽**(已落地)+ 产品接上 +
状态栏设置入口 + 轻量设置卡(关于/版本/反馈), 版本检查下沉框架 `danqing::update`(已落地)。
产品侧任务即 app-chrome + settings 两模块。

## 已提交 (别重复做)

**danqing 框架 — 已推 `origin/dev` (HEAD `cced4cf`)**:
- `titlebar-embed` T1–T4 + review 修复 C1(坐标)/I1(零宽槽) — TitleBar 可选 `.embed(widget)` 槽
- `update-check` U1–U5 — `danqing::update` 版本检查核心(feature `update`, 默认关), GitHub 轨
- `fix(image)` — `Image::paint_image` 用 aspect_fit 居中, 不再拉伸填满传入区
- 全链测试绿(31 title_bar + 12 update + 全量), clippy 0, 默认树无网络栈

**danqing-log — 本地, 3 提交未推 (`origin/dev` 落后 3)**:
- `2a5be97` 规划 docs(app-chrome/settings 的 spec/plan/todo)
- `08682da` **A1** view() 换 TitleBar + 内嵌 Bar + 三窗键接线
- `5aaf250` fix: TitleBar 补 `on_drag`(漏接则拖拽失效)

## 下一步任务 (按依赖序)

### app-chrome (后续)
- **A2**: Bar 适配 embed 槽几何 — Bar `layout/paint/event` 用槽 area(非整窗 Column 行宽);
  槽内 `label_width`/`input_area` 以槽为基准; Bar 为 Hidden 时标题栏仅标题+三键(无输入).
  Verify: 原始模式无搜索时标题栏无输入, 表格/搜索有输入.
- **A3**: 标题串 + 窗口配置一致性 — TitleBar.title 与 `WindowConfig.title` 同串; 随 mode 带/不带 JSONL.
- **A4**: 键盘/焦点/IME 回归 — 栏聚焦 Enter 应用/Esc 清关/PageUp·PageDown 滚动/中文 IME 贴框/
  粘贴/点击落焦; `app_key_filter` Ctrl+T 栏聚焦仍生效; Tab/方向键不冲突. 人工回归(参考 v1 todo).
- **A5**: 三件套 + 人工验收(三键/拖拽/输入/IME/快捷键).

### settings (app-chrome 后, 依赖框架 update-check)
- **S1**: `Cargo.toml` 加 `danqing = { ..., features = ["update"] }`; `src/app_update.rs` 供
  `UpdateSpec` + 启动 spawn_check + current_hint 包装.
- **S2**: 状态栏右下「⚙ 关于」入口 → `Msg::OpenSettings`; 位置计数左移.
- **S3**: 设置卡 `Stack[Box(scrim), Center[Box(card)]]`: 关于(名称/v/设计) + 版本行(update_hint) + 反馈 Link.
- **S4**: Msg 增 OpenSettings/CloseSettings/OpenUrl/UpdateAction; LogApp 增 settings_open; 卡 overlay; 焦点陷阱.
- **S5**: 三件套 + 人工验收.

## 关键决策/对接事实 (免得重新踩坑)

- **产品经 `[patch]` 用本地 danqing**: danqing-log `Cargo.toml` 常驻
  `[patch."https://github.com/14uncle/danqing"] danqing = { path = "../danqing" }`。
  本地开发时 Cargo.lock 记 danqing 为 **path 依赖**(无 git source 行) — 新框架能力**立即可用**,
  无需 `cargo update`。rev 钉进 lock 是**发布时去掉 patch** 的事。
- **TitleBar 主题**: `src/main.rs` 的 `title_theme()` 用 `danqing::theme::SceneTheme` 深色调色板
  (浅色文字, 配 log 深色系 VS Code 基底)。渲染透明背景(logo/标题走 SceneTheme token)。
- **`LogApp` 加了 `maximized: bool`** + `impl App::maximized_changed` — 供 `bind_maximized` 读,
  最大化时优化按钮图标(□→□□)。初始化 false。
- **`TitleBar` 必接 `.on_drag(|| WindowAction::Drag)`**, 否则标题拖拽不产 `WindowAction::Drag` 窗口不动
  (A1 真实踩坑)。
- **Bar 挪进 TitleBar.embed 槽**, 不再是 Column sibling。Bar 的 `focus_id="log-bar"` 保留;
  首帧自动聚焦落在 embed Bar (日志里 `焦点变化：None -> Some([0, 0])` = Column[0]=TitleBar 的 child[0]=Bar)。
- **共享编译产物**: 各仓 `.cargo/config.toml` → `../.cargo-target`。exe 在
  `/.cargo-target/release/danqing-log.exe`。**构建前若 exe 被运行实例锁住会「拒绝访问」**, 先关窗口。
- **演示文件**: `target/demo/demo.jsonl`(嵌套 JSONL, 触发表格+过滤)+ `target/demo/demo.log`(明文)。
  启动: `cargo run --release -- target/demo/demo.jsonl`(或直接 exec 上述 release 路径)。

## Git 状态

- danqing: `dev` 与 `origin/dev` 同步(已推)。分支模型: dev 默认, master 发布基线。
- danqing-log: `dev`, **3 提交未推**。远程 `14uncle/danqing-log`(私有)。
- 本地 git 身份统一 `十四叔 <gwhun@qq.com>`(已核对)。

## 收尾提示

- 发布前: 推 danqing-log + 补 screenshot 弹药 + 命名/LOGO 定稿(clipboard 先例)。**发布命名未定**,
  仓库工作名 danqing-log。付费层($45/$95 买断 vs SPEC「v1 全功能免费」)张力待定, 见 intent。
- 框架 review 遗留(可选, 非阻塞): update-check 的 I2(运输层测试)/O1(每帧锁+String 分配)/O2·O3;
  `title_bar.rs` 体量 1944 行(或可抽副模块)。
