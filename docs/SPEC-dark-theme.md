# Spec: 深色主题完整接入 + 下拉选择器

> 状态: **待审批**
> 日期: 2026-09-11
> 作者: 十四叔

---

## 能力地图

| 模块 id | 仓库 | 职责 | 依赖 |
|---------|------|------|------|
| `danqing:dark-theme` | danqing | 框架层 DarkTheme 实现 | — |
| `danqing:dropdown` | danqing | 框架层下拉选择器 widget | — |
| `danqing-log:theme-integration` | danqing-log | 产品侧全面接入框架 Theme | `danqing:dark-theme`, `danqing:dropdown` |

**构建顺序**: `danqing:dark-theme` + `danqing:dropdown`（并行）→ `danqing-log:theme-integration`

---

## 模块 1: `danqing:dark-theme`

### Objective

在 danqing 框架中新增 `DarkTheme` struct，实现 `Theme` trait，为所有产品提供统一深色主题 token。

### 设计决策

- **accent 色保持玉色 `#0F766E`**，深色背景下对比度足够（~6:1）
- **分割线/边框跟随文字色**，暗场景下自动变亮（复用 SceneTheme 的派生逻辑）
- **玻璃表面 alpha 降低**，暗背景上不需要高透明度

### 颜色 token

| Token | LightTheme | DarkTheme | 说明 |
|-------|-----------|-----------|------|
| `background` | `#F0F8F6` | `#191920` | 深灰偏蓝 |
| `surface` | `rgba(1,1,1, 0.72)` | `rgba(1,1,1, 0.08)` | 暗玻璃 |
| `surface_input` | `rgba(1,1,1, 0.95)` | `rgba(1,1,1, 0.12)` | 输入区略实 |
| `surface_variant` | `#EEF6F2` | `rgba(1,1,1, 0.10)` | 悬停/次级 |
| `accent` | `#0F766E` | `#0F766E` | 玉色不变 |
| `text_primary` | `#0F172A` | `#E5E5EA` | 近白 |
| `text_secondary` | `#475569` | `#8E8E93` | 中灰 |
| `divider` | `rgba(0,0,0, 0.10)` | `rgba(229,229,234, 0.15)` | 跟随文字色 |
| `border` | `rgba(0,0,0, 0.18)` | `rgba(229,229,234, 0.28)` | 跟随文字色 |
| `selection` | `rgba(15,118,110, 0.30)` | `rgba(15,118,110, 0.30)` | 跟随 accent |
| `caret` | `#0F766E` | `#0F766E` | 跟随 accent |
| `danger` | `#EF4444` | `#EF4444` | 不变 |
| `scrim` | `rgba(0,0,0, 0.35)` | `rgba(0,0,0, 0.35)` | 不变 |

**非颜色 token**（字号/间距/圆角/阴影/动效）与 LightTheme 一致。

### 文件

- `danqing/src/theme.rs` — 在 `LightTheme` 后新增 `DarkTheme` struct + `impl Theme`

### 成功标准

- [ ] `DarkTheme` 实现 `Theme` trait，所有方法返回合理颜色
- [ ] 深色背景下 text_primary vs background 对比度 ≥ 7:1（WCAG AAA）
- [ ] accent vs background 对比度 ≥ 4.5:1（WCAG AA）
- [ ] `cargo test` 全绿，含对比度护栏测试

---

## 模块 2: `danqing:dropdown`

### Objective

在 danqing 框架中新增通用下拉选择器 widget，供所有产品使用。

### 功能需求

1. **点击展开** — 显示选项浮层（Overlay 机制）
2. **键盘导航** — ↑↓ 移动高亮、Enter 选择、Esc 关闭
3. **hover 高亮** — 鼠标悬停当前选项高亮
4. **选中回调** — `on_select(idx)` 消息回调
5. **绑定当前值** — `bind_selected(closure)` 从应用状态读取当前选中索引
6. **主题感知** — 颜色从 Theme trait 读取

### 接口草案

```rust
Dropdown::new(vec!["浅色".into(), "深色".into()])
    .on_select(|idx| Msg::SelectTheme(idx))
    .bind_selected(|app: &LogApp| app.theme_index())
```

### 边界

- **只做单选下拉**，不做多选/级联/搜索
- **不做异步加载**，选项列表构造时给定
- **浮层动画**沿用框架 Overlay 默认

### 文件

- `danqing/src/widget/form/dropdown.rs` — 新增
- `danqing/src/widget/form/mod.rs` — 添加 `pub mod dropdown;`
- `danqing/src/widget/mod.rs` — re-export `Dropdown`
- `danqing/src/lib.rs` — re-export `Dropdown`

### 成功标准

- [ ] 点击展开选项列表，再点收起
- [ ] ↑↓ 键盘导航 + Enter 选择 + Esc 关闭
- [ ] hover 高亮当前选项
- [ ] 选中后触发 `on_select` 回调
- [ ] 颜色从 Theme trait 读取，浅色/深色主题下均可用
- [ ] `cargo test` 全绿

---

## 模块 3: `danqing-log:theme-integration`

### Objective

产品侧全面接入框架 Theme trait，删除自建 `theme.rs` 颜色系统，深色主题全链路生效。

### 已知遗漏清单

> **⚠ 本清单已于 2026-09-13 逐条核对并结清。此后暗色相关工作以
> `docs/specs/SPEC-ui-redesign.md` 为准, 不要再照本表施工。**
> 核对时发现它**谎报进度**: 12 条里 8 条其实早已完成 —— 清单停在上一轮结束时,
> 之后又清掉一批。而它**从未列过**真正最大的那一条 (`clear_color`, 见下 #13)。

| # | 位置 | 状态 |
|---|------|------|
| 1 | `level_color()` | ✅ 已完成 (`th.text_primary()`) |
| 2 | `level_cell_color()` | ✅ 已完成 |
| 3 | `status_color()` | ✅ 已完成 |
| 4 | `cell_color()` | ✅ 已完成 |
| 5 | `Bar::base_input()` | ✅ 2026-09-13: 挂 `bind_theme`, 删掉三个写死色 |
| 6 | `Bar` 占位色 | ⚠️ **有意保留**: 中性灰在两个主题上都读得动 (暗色 3.20 / 浅色 3.9)。`TextInput::themed()` 本来就把它定成与主题无关的 `(160,160,160)`, 把它并进主题绑定会**顺带改掉所有其它产品的占位色** |
| 7 | 书签行号色 | ✅ 2026-09-13: 两主题各一支金。**未采纳**原方案「从 `Theme.accent` 派生」—— 那会把书签与选中/强调混成一个通道 |
| 8 | 展开 `[+]/[-]` | ✅ 已完成 (已是 `th.text_primary()`) |
| 9 | `title_theme()` | ✅ 2026-09-13: 六项改取 `LogTheme`; `backdrop_light/dark` 框架无 token, 保留手写 |
| 10 | `settings.rs` | ✅ 已完成 (走 `sync()`) |
| 11 | 主题切换 UI | ✅ 已完成 (Dropdown) |
| 12 | 删除 `src/theme.rs` | ✅ 已完成 |
| **13** | **`clear_color`** | ✅ 2026-09-13。**原清单里根本没有这一条**: `WindowConfig.clear_color` 写死浅色且全仓**零处** `set_clear_color` 调用 —— 存暗色配置启动会开在白底上, 运行中切主题也不通知窗口 |

**另有一整类问题原清单没覆盖**: 「切主题后不跟随」。视图树只在启动时构建一次
(`window/mod.rs` 的 `let tree = app.view();`), 所以任何在 `view()` 里
`themed(&theme)` 构造的控件都把主题色烘死在启动那一刻。框架原先**只有
`TitleBar` 有 per-frame 主题绑定** —— 已为 `Tabs` / `Dropdown` / `TextInput`
补上 (`bind_theme`, 形状统一)。详见
`tasks/todo-theme-recalibrate.md` 的「附二」。

### 主题切换架构

```rust
// LogApp 中存储枚举
pub(crate) enum AppTheme {
    Light,
    Dark,
}

impl AppTheme {
    fn theme(&self) -> &'static dyn Theme {
        match self {
            Self::Light => &LightTheme,
            Self::Dark => &DarkTheme,
        }
    }
    fn index(&self) -> usize {
        match self {
            Self::Light => 0,
            Self::Dark => 1,
        }
    }
    fn from_index(i: usize) -> Self {
        match i { 0 => Self::Light, _ => Self::Dark }
    }
}
```

- `LogApp.theme: AppTheme` 替代当前 `theme: ThemeMode`
- 调用: `app.theme().surface()` 替代 `crate::theme::surface(theme)`
- Dropdown: `Dropdown::new(vec!["浅色", "深色"]).bind_selected(|app| app.theme.index())`

### 标题栏主题化

```rust
fn title_theme(app: &AppTheme) -> SceneTheme {
    match app {
        AppTheme::Light => SceneTheme::new(ScenePalette {
            base: Color::from_srgb8(240, 248, 246),
            accent: Color::from_srgb8(15, 118, 110),
            text_primary: Color::from_srgb8(15, 23, 42),
            text_secondary: Color::from_srgb8(71, 85, 105),
            surface: Color::rgba(1.0, 1.0, 1.0, 0.72),
            surface_input: Color::rgba(1.0, 1.0, 1.0, 0.95),
            backdrop_light: Color::from_srgb8(240, 248, 246),
            backdrop_dark: Color::from_srgb8(200, 220, 215),
        }),
        AppTheme::Dark => SceneTheme::new(ScenePalette {
            base: Color::from_srgb8(25, 25, 32),
            accent: Color::from_srgb8(15, 118, 110),
            text_primary: Color::from_srgb8(229, 229, 234),
            text_secondary: Color::from_srgb8(142, 142, 147),
            surface: Color::rgba(1.0, 1.0, 1.0, 0.08),
            surface_input: Color::rgba(1.0, 1.0, 1.0, 0.12),
            backdrop_light: Color::from_srgb8(40, 40, 50),
            backdrop_dark: Color::from_srgb8(15, 15, 20),
        }),
    }
}
```

### LOGO 适配

标题栏 LOGO 需要根据主题切换颜色。检查 `LogoKind::Log` 的渲染逻辑，深色主题下使用浅色 LOGO。

### 文件

- `danqing-log/src/main.rs` — AppTheme 枚举、Msg::SelectTheme、title_theme 改动
- `danqing-log/src/view.rs` — 全面改用 Theme token、删除旧颜色函数
- `danqing-log/src/settings.rs` — 卡片改用 Theme token、Dropdown 替换文字按钮
- `danqing-log/src/config.rs` — **新增** 配置文件读写（主题持久化）
- `danqing-log/src/theme.rs` — **删除**

### Commands

```
构建: cargo build
测试: cargo test
静态检查: cargo clippy --all-targets -- -D warnings
提交前三件套: cargo fmt + clippy + test
```

### 成功标准

- [ ] 切换深色主题后，所有文字在深色背景上清晰可读（WCAG AA 对比度）
- [ ] 标题栏跟随主题变色，LOGO 适配深色
- [ ] 设置卡使用下拉选择器切换主题
- [ ] 切换主题后无需重启，即时生效
- [ ] 过滤栏/搜索栏输入色、光标色、占位色跟随主题
- [ ] 级别着色（ERROR/WARN/INFO/DEBUG）在深色主题下可辨识
- [ ] 书签行号在深色主题下可见
- [ ] `src/theme.rs` 已删除，无残留
- [ ] `cargo test` 全绿（含深色主题对比度测试）
- [ ] `cargo clippy -- -D warnings` 零警告

### 边界

- **Always**: 提交前三件套；新 `.rs` 文件头 `//! @author` + `//! @date`；注释中文
- **Ask first**: 修改框架 Theme trait 接口；添加新依赖
- **Never**: 不提交测试数据文件；不删除现有测试

---

## Open Questions

1. ~~accent 色~~ → 保持玉色 `#0F766E` 不变（已确认）
2. ~~存储方式~~ → 枚举 + 薄封装（已确认）
3. ~~持久化~~ → 需要持久化到配置文件（已确认）

### 持久化方案

- 配置文件路径: `dirs::config_dir()` / `danqing-log/config.toml`
- 字段: `[theme] mode = "light"` 或 `"dark"`
- 启动时读取，默认 `light`
- 切换主题时写入
- 格式: TOML（与产品线其他配置统一）
