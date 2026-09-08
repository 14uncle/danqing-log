# Project: 丹青日志 (danqing-log)

大文件日志/JSONL 查看分析器 —— 丹青第四件产品。岗位: **性能碾压 POC → 正式产品**。

## 状态

- 2026-09-05: 开枪 + 当日建仓 + POC 双前提判过 → 用户发起 spec = 转正; 深夜 /build auto 零 commit core-viewer T1–T7 全绿
- 2026-09-06: jsonl-table / live-tail 闭环 (均 spec→plan→build→review + 人工验收); app-chrome A1–A5 + settings S1–S5 落地; 过滤/搜索栏已重构成真 TextInput (IME 三补丁删除); 切浅色主题 (白底不回头) + 命名「丹青日志 LogLens」+ Ctrl+O
- 2026-09-07: 无参启动空态; genlog 参数白名单; 浅色 UI 精修
- 2026-09-08 (已 commit): 字号 14 / 启动默认最大化 / **分段并行索引 1GB 冷 1028→584ms, 热 439→113ms** / 浅色可读性对齐竞品
- 2026-09-08 (**未 commit**): **async-open + text-selection-copy 机器部分全闭环** (spec→plan→build 走完, 三件套绿); 工作区含 `src/open.rs` / `src/selection.rs` 新模块 + SPEC/plan/todo 文档 + 5 文件改动; text-selection T1 含 danqing 引擎改动 `App::propagate_unhandled_keys()` (danqing 工作区同未 commit)
- 当前: **等用户人工验收 + 提交授权**。遗留人工项 —— async-open: 10GB 冷开复核 / 索引中 Ctrl+O 取消体感 / 索引中关窗干净退出 / 轮转重建旧内容可见 (copytruncate+create 两流派) / Loading 文案定档; text-selection: 实机五种姿势 (双击选词/框选/跨行/表格行复制/焦点切换)
- 获授权后的联动顺序: danqing 先提交 push → 本仓 `cargo update -p danqing` → 两仓分别提交, message 注明关联
- 测试基线: **99 绿** (79 lib + 12 main + 8 genlog), 2026-09-08 实测
- POC 及格线不过则终止, 仓库转档案 (clipboard 先例); 余前提③ = 发布后首单外检

## 必读

- 意图 (为什么做/竞品裂缝/MVP 边界/开枪前提/定价锚): `../danqing/docs/intent/log-viewer-poc.md`
- 框架规则: `../danqing/CLAUDE.md`
- 农场跨仓约定: `../CLAUDE.md`

## 仓库与分支

- 远程: `git@github.com:14uncle/danqing-log.git` (已建, dev 已 push; 本地领先 3, 待授权 push)
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
- `src/settings.rs` — 轻量设置卡 (scrim 遮罩 + 玻璃卡: 关于/版本/反馈)
- `src/app_update.rs` — 更新检查接线 (薄封装 `danqing::update`)
- `src/expand.rs` — 展开行模型 (行内子行嵌套展开的显示行↔文件行双向映射, 前缀和)
- `src/open.rs` — 异步打开管道 OpenJob (启动/Ctrl+O·拖拽/轮转重建/巨量追平四路径统一进 worker)
- `src/selection.rs` — 文本选区纯逻辑 (token 边界/规范化/复制拼装, 偏移=解码行字节)
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
- 引擎缺口当场修进 `../danqing` (打磨寄生; NamedKey PageUp/PageDown 已落地, 在途: `propagate_unhandled_keys` 键回退未提交)
- spec 流水线: 用户发起 spec 技能后按 spec→plan→build→review→code-simplify 推进, spec 写完不立即编码
- 未获用户指示不 commit/push、不改测试数据格式语义

## Patterns

`src/logfile.rs` 是全仓风格基准: 中文 doc comment 说明做什么+不做什么+为什么; 统计一律实测不估算 (OpenStats); 失败语义明确 (UTF-16 报错不猜码)。
