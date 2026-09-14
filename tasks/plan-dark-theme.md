# Implementation Plan: 深色主题完整接入 + 下拉选择器

## Overview

三个模块：框架层 DarkTheme + 框架层 Dropdown + 产品层全面接入。产品侧删除自建 theme.rs，改用框架 Theme trait；主题持久化到 config.toml。

## Architecture Decisions

1. **枚举 + 薄封装** — `AppTheme` 枚举存储，`theme()` 返回 `&'static dyn Theme`，零堆开销
2. **框架 Theme trait 统一** — 产品侧不再自建颜色系统，全部走 Theme token
3. **Dropdown 替代文字按钮** — 设置卡中主题切换用下拉选择器
4. **config.toml 持久化** — 主题选择随会话保留

## Task List

### Phase 1: 框架层（danqing 仓库）

- [ ] Task 1: 新增 DarkTheme 实现 Theme trait
- [ ] Task 2: 新增 Dropdown widget

### Checkpoint: 框架层

- [ ] DarkTheme 对比度测试全绿
- [ ] Dropdown 基本交互测试全绿
- [ ] danqing 仓库 push

### Phase 2: 产品层接入（danqing-log 仓库）

- [ ] Task 3: AppTheme 枚举 + config 持久化
- [ ] Task 4: view.rs 全面接入 Theme token
- [ ] Task 5: settings.rs 接入 Theme + Dropdown
- [ ] Task 6: 标题栏主题化 + LOGO 适配
- [ ] Task 7: 删除 src/theme.rs + 清理

### Checkpoint: 完成

- [ ] 浅色/深色主题切换即时生效
- [ ] 所有文字在两种主题下清晰可读
- [ ] 主题选择重启后保持
- [ ] cargo test 全绿 + clippy 零警告
- [ ] Review with human

## Risks and Mitigations

| 风险 | 影响 | 缓解策略 |
|------|------|---------|
| 框架 Theme trait 约束（Clone+Copy）限制扩展 | 中 | 枚举绕开，不改 trait |
| 级别色在深色主题下辨识度不够 | 中 | 用更亮的变体，WCAG 对比度测试验证 |
| LOGO 深色适配需要素材 | 低 | 用颜色反相或文字 LOGO 替代 |

## 依赖链

```
danqing:DarkTheme ──┐
                    ├──→ danqing-log:theme-integration
danqing:Dropdown ───┘
```

两个框架模块可并行实现，产品层等框架 push 后再接入。
