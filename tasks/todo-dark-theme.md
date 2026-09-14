# Tasks: 深色主题完整接入 + 下拉选择器

> Spec: `docs/SPEC-dark-theme.md`
> Plan: `tasks/plan-dark-theme.md`
> 状态: **全部完成** ✅

---

## Phase 1: 框架层（danqing 仓库）

- [x] Task 1: 新增 DarkTheme 实现 Theme trait — `375e40d`
- [x] Task 2: 新增 Dropdown widget — `31e7b04`

### Checkpoint: 框架层 ✅
- [x] danqing 仓库 push
- [x] 产品侧 `cargo update -p danqing`

---

## Phase 2: 产品层接入（danqing-log 仓库）

- [x] Task 3: AppTheme 枚举 + config 持久化 — `38b86d9`
- [x] Task 4: view.rs 全面接入 Theme token — `bd30a71`
- [x] Task 5: settings.rs 接入 Theme + Dropdown — `41b3bfb`
- [x] Task 6: 标题栏主题化 + LOGO 适配 — `467700a`
- [x] Task 7: 删除 src/theme.rs + 清理 — `1c3468d`

### Checkpoint: 全部完成 ✅
- [x] 浅色/深色主题切换即时生效
- [x] 主题选择重启后保持 (config.toml)
- [x] 标题栏跟随主题变色
- [x] 设置卡使用 Dropdown 切换主题
- [x] `cargo build` 编译通过
- [x] `cargo clippy -- -D warnings` 零警告
