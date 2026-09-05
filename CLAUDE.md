# Project: 丹青日志 (danqing-log)

大文件日志/JSONL 查看分析器 —— 丹青第四件产品。岗位: **性能碾压 POC → 正式产品**。

## 状态

- 2026-09-05: 开枪 (新选型方针首次实战, 五域扫描→双深潜→用户裁决), 当日建仓 + POC v0 落地
- 2026-09-05 晚: POC 双前提判过 → 用户发起 spec 技能 = **转正**; 能力地图+3 模块 spec (core-viewer/live-tail/jsonl-table) 写完, 停在 spec 评审门 (SPEC.md + docs/specs/)
- 2026-09-05 深夜: /build auto 零 commit 连跑 core-viewer T1–T7 全绿 (用户裁决: 全程不 commit)
- 当前: **用户人工验收门** (上手试 PageUp/搜索/书签/横滚/编码) → review 阶段 (/agent-skills:code-review-and-quality); 余前提③ = 发布后首单外检
- POC 及格线不过则终止, 仓库转档案 (clipboard 先例)

## 必读

- 意图 (为什么做/竞品裂缝/MVP 边界/开枪前提/定价锚): `../danqing/docs/intent/log-viewer-poc.md`
- 框架规则: `../danqing/CLAUDE.md`
- 农场跨仓约定: `../CLAUDE.md`

## 仓库与分支

- 远程: 未建 (发布前建 GitHub 仓库; 命名待定, danqing-log 为工作名)
- 分支: `dev` 默认, `master` 发布基线 (全家同一模型)
- 本地 git 身份: 十四叔 <gwhun@qq.com> (建仓时已核对)

## Tech Stack

- Rust 1.85+, edition 2024 (工具链 stable-x86_64-pc-windows-gnu, rustup override 已设)
- UI 框架: danqing — git 依赖 (Cargo.lock 钉 rev), 本机经 `[patch]` 段用本地 `../danqing`
- 引擎: memmap2 (mmap) + memchr (SIMD 行索引) + regex::bytes (全文搜索)
- 共享编译产物: `.cargo/config.toml` → `../.cargo-target` (全家共用)

## 结构

- `src/logfile.rs` — 引擎层 (mmap/行索引/搜索), 全部碾压主张在此
- `src/jsonl.rs` — 前提②引擎: JSONL 检测/列发现/memmem 字段提取/字段过滤 (零 parse; serde_json 需 preserve_order 保首见列序)
- `src/main.rs` + `src/view.rs` — GUI (行锚定虚拟视口, 不用 Scrollable: f32 像素偏移在 2 亿像素域失真, 见 view.rs 模块头; 表格模式四区 = 过滤栏/表头/虚拟化行/状态栏)
- `src/encoding.rs` — 编码检测/转码 (BOM→交替NUL→UTF-8合法性→GBK统计→Latin-1兜底; CP936 零依赖 FFI)
- `src/search.rs` — AsyncJob (worker+tick拾取泛化) + SearchNav 命中导航
- `src/bin/logbench.rs` — 无窗口基准 (截图弹药的数字源)
- `src/bin/mmap_lab.rs` — mmap 存活期外部修改实验台 (T3 数据源)
- `src/bin/genlog.rs` — 确定性测试数据生成

## Commands

- 构建: `cargo build`
- 测试: `cargo test`
- 静态检查: `cargo clippy --all-targets -- -D warnings`
- 提交前: `cargo fmt` + clippy 零警告 + 测试全绿
- 打包: `powershell -NoProfile -File tools/package_portable.ps1` (产物 `../release-archives/log/`)

## Boundaries (沿袭全家约定)

- 注释/文档一律中文; 新 `.rs` 文件头 `//! @author 十四叔` + `//! @date yyyy/MM/dd`
- danqing 依赖提交状态固定 git; 本地联动临时切 path 不提交
- 引擎缺口当场修进 `../danqing` (打磨寄生), 已知缺口: NamedKey 无 PageUp/PageDown
- spec 流水线: 用户发起 spec 技能后按 spec→plan→build→review→code-simplify 推进, spec 写完不立即编码
- 未获用户指示不 commit/push、不改测试数据格式语义

## Patterns

`src/logfile.rs` 是全仓风格基准: 中文 doc comment 说明做什么+不做什么+为什么; 统计一律实测不估算 (OpenStats); 失败语义明确 (UTF-16 报错不猜码)。
