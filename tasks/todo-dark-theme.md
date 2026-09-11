# Tasks: 深色主题完整接入 + 下拉选择器

> Spec: `docs/SPEC-dark-theme.md`
> Plan: `tasks/plan-dark-theme.md`

---

## Task 1: 新增 DarkTheme 实现 Theme trait

**仓库**: danqing
**描述**: 在 `danqing/src/theme.rs` 中新增 `DarkTheme` struct，实现 `Theme` trait。深灰偏蓝背景 + 暗玻璃表面 + 玉色 accent 不变。

**验收标准:**
- [ ] `DarkTheme` 实现 `Theme` trait 所有方法
- [ ] text_primary vs background 对比度 ≥ 7:1
- [ ] accent vs background 对比度 ≥ 4.5:1
- [ ] 与 LightTheme 非颜色 token（字号/间距/圆角/阴影）一致

**验证:**
- [ ] `cargo test` 全绿（含对比度护栏测试）
- [ ] `cargo clippy -- -D warnings` 零警告

**依赖**: 无

**文件:**
- `danqing/src/theme.rs`

**规模**: S（1 文件）

---

## Task 2: 新增 Dropdown widget

**仓库**: danqing
**描述**: 新增通用下拉选择器 widget，支持点击展开、键盘导航（↑↓/Enter/Esc）、hover 高亮、选中回调。

**验收标准:**
- [ ] 点击展开选项浮层，再点收起
- [ ] ↑↓ 键盘导航 + Enter 选择 + Esc 关闭
- [ ] hover 高亮当前选项
- [ ] `on_select(idx)` 回调触发
- [ ] `bind_selected(closure)` 绑定当前值
- [ ] 颜色从 Theme trait 读取

**验证:**
- [ ] `cargo test` 全绿
- [ ] `cargo clippy -- -D warnings` 零警告

**依赖**: 无

**文件:**
- `danqing/src/widget/form/dropdown.rs`（新增）
- `danqing/src/widget/form/mod.rs`（加 pub mod）
- `danqing/src/widget/mod.rs`（re-export）
- `danqing/src/lib.rs`（re-export）

**规模**: M（3-4 文件）

---

## Checkpoint: 框架层完成

- [ ] Task 1 + Task 2 全部验收通过
- [ ] danqing 仓库 push
- [ ] 产品侧 `cargo update -p danqing`

---

## Task 3: AppTheme 枚举 + config 持久化

**仓库**: danqing-log
**描述**: 新增 `AppTheme` 枚举（Light/Dark），提供 `theme()` 方法返回框架 Theme 引用。新增 `config.rs` 读写 `config.toml`，启动时加载、切换时保存。

**验收标准:**
- [ ] `AppTheme::theme()` 返回 `&'static dyn Theme`
- [ ] 启动时从 config.toml 读取主题，默认 light
- [ ] 切换主题时写入 config.toml
- [ ] config.toml 不存在时自动创建

**验证:**
- [ ] `cargo test` 全绿
- [ ] 手动验证：切换主题 → 重启 → 主题保持

**依赖**: Task 1（需要 DarkTheme）

**文件:**
- `danqing-log/src/config.rs`（新增）
- `danqing-log/src/main.rs`（AppTheme 枚举 + 启动加载）

**规模**: M（2 文件）

---

## Task 4: view.rs 全面接入 Theme token

**仓库**: danqing-log
**描述**: view.rs 中所有硬编码颜色改为从 Theme trait 读取。包括：行号色、状态栏色、选区色、滚动条色、斑马纹、hover 色、搜索命中色、级别着色。

**验收标准:**
- [ ] 所有颜色函数从 `app.theme().xxx()` 读取
- [ ] level_color/cell_color/status_color 正文色从 Theme.text_primary 读取
- [ ] 书签行号色从 Theme.accent 派生
- [ ] 展开 `[+]/[-]` 色从 Theme.text_secondary 派生
- [ ] Bar 输入色/光标色/占位色从 Theme 读取

**验证:**
- [ ] `cargo test` 全绿
- [ ] 手动验证：深色主题下所有文字清晰可读

**依赖**: Task 3

**文件:**
- `danqing-log/src/view.rs`

**规模**: L（1 文件但改动多）

---

## Task 5: settings.rs 接入 Theme + Dropdown

**仓库**: danqing-log
**描述**: 设置卡片颜色改用 Theme token。主题切换从文字按钮改为 Dropdown widget。

**验收标准:**
- [ ] 卡片背景/边框/文字从 Theme 读取
- [ ] 主题切换使用 Dropdown（选项：浅色/深色）
- [ ] Dropdown 选中后即时切换主题

**验证:**
- [ ] `cargo test` 全绿
- [ ] 手动验证：设置卡在两种主题下视觉一致

**依赖**: Task 2, Task 3

**文件:**
- `danqing-log/src/settings.rs`
- `danqing-log/src/main.rs`（Msg::SelectTheme 处理）

**规模**: M（2 文件）

---

## Task 6: 标题栏主题化 + LOGO 适配

**仓库**: danqing-log
**描述**: `title_theme()` 根据 AppTheme 切换浅色/深色 ScenePalette。LOGO 在深色主题下使用浅色版本。

**验收标准:**
- [ ] 标题栏背景色跟随主题切换
- [ ] 标题栏文字色跟随主题切换
- [ ] LOGO 在深色主题下清晰可见

**验证:**
- [ ] 手动验证：切换主题后标题栏即时变色

**依赖**: Task 3

**文件:**
- `danqing-log/src/main.rs`

**规模**: S（1 文件）

---

## Task 7: 删除 src/theme.rs + 清理

**仓库**: danqing-log
**描述**: 删除产品侧自建的 `src/theme.rs`，清理所有 `crate::theme::` 引用。确认无残留。

**验收标准:**
- [ ] `src/theme.rs` 已删除
- [ ] 无 `crate::theme::` 引用残留
- [ ] 无未使用的 import 警告

**验证:**
- [ ] `cargo build` 零警告
- [ ] `cargo test` 全绿
- [ ] `cargo clippy -- -D warnings` 零警告

**依赖**: Task 4, Task 5, Task 6（所有主题接入完成后）

**文件:**
- `danqing-log/src/theme.rs`（删除）
- `danqing-log/src/main.rs`（删除 mod theme）
- `danqing-log/src/view.rs`（清理 import）

**规模**: S

---

## Checkpoint: 全部完成

- [ ] Task 1-7 全部验收通过
- [ ] 浅色/深色主题切换即时生效
- [ ] 所有文字在两种主题下清晰可读
- [ ] 主题选择重启后保持
- [ ] 标题栏跟随主题变色
- [ ] 设置卡使用 Dropdown 切换主题
- [ ] `cargo test` 全绿 + `cargo clippy -- -D warnings` 零警告
- [ ] danqing 先 push → danqing-log `cargo update -p danqing` → 两仓分别提交
