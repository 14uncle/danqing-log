# Project: 丹青日志 (danqing-log)

大文件日志/JSONL 查看分析器 —— 丹青第四件产品。岗位: **性能碾压 POC → 正式产品**。

## 状态

- 2026-09-05: 开枪 + 当日建仓 + POC 双前提判过 → 用户发起 spec = 转正; 深夜 /build auto 零 commit core-viewer T1–T7 全绿
- 2026-09-06: jsonl-table / live-tail 闭环 (均 spec→plan→build→review + 人工验收); app-chrome A1–A5 + settings S1–S5 落地; 过滤/搜索栏已重构成真 TextInput (IME 三补丁删除); 切浅色主题 (白底不回头) + 命名「丹青日志 LogLens」+ Ctrl+O
- 2026-09-07: 无参启动空态; genlog 参数白名单; 浅色 UI 精修
- 2026-09-08 (已 commit): 字号 14 / 启动默认最大化 / **分段并行索引 1GB 冷 1028→584ms, 热 439→113ms** / 浅色可读性对齐竞品
- 2026-09-08 (后已 commit `961c03b`): **async-open + text-selection-copy 机器部分全闭环** (spec→plan→build 走完, 三件套绿); `src/open.rs` 新模块 + SPEC/plan/todo 文档 + 5 文件改动; text-selection T1 含 danqing 引擎改动 `App::propagate_unhandled_keys()`
- 2026-09-12 (**已 commit push**): **定价重裁** —— 免费层 = 看懂 (单文件全功能) / 付费层 = 批量·留存·交付 (**v1.x 起 $29 个人 · $59 企业**买断); 渠道分层 GitHub 永久免费开源 / MS Store 走 trial 且**过期降级不变砖**; 付费层清单落档 `docs/ROADMAP-v1x.md`。同时收口全仓 8 处过时报价 + 删引擎拆分残留 **2266 行死代码** (47 测试静默不跑)
- 2026-09-12: **人工验收全部通过** (用户实机, 已 commit `c27dcb8`) —— async-open 五项 (10GB 冷开全程可响应 / 索引中 Ctrl+O 取消 / 索引中关窗干净退出 / 轮转重建旧内容可见 / Loading 文案**按现状定档** `{pct}% · {done}/{total} MiB`) + text-selection 五种姿势 (双击选词/框选/跨行/表格行复制/焦点切换)。**v1 功能闭环 + 验收闭环均已完成**
- 2026-09-12 (**文档收口批, 未 commit**): v1.0 收尾面定档 + 两项用户裁决 ——
  ① **等级直方图做进 v1.0** (原列 ROADMAP v1.x 免费层欠账; 改判理由: 它是免费层对
  LogViewPlus 对比话术的成立前提, 不该让首发热缺); ② **MS Store 纳入 v1.0, 形态为纯免费层上架**
  (trial / 买断基建仍留 v1.x)。**推论: v1.0 两渠道皆免费层 → 无购买路径 → 前提③ (首单外检)
  只能在 v1.x 付费层上线后判定**。同批: README 全篇重写 (原稿停在 core-viewer 首版 `5f22ec6`,
  仍自称 POC 阶段且 4 条边界 3 条已失效) + 清 SPEC/ROADMAP 陈旧项
- 当前: **v1.0 收尾** —— 代码功能已闭环, 余工作面 ① 等级直方图 (待 spec) ② MSIX 打包 +
  Store 上架物料 ③ 版本号 `0.1.0`→`1.0.0` + 重打包 + git tag ④ 对外文案/截图素材。
  **未获用户指示不 commit/push**
- 联动顺序 (仅当 danqing 有**代码**改动): danqing 先提交 push → 本仓 `cargo update -p danqing` → 两仓分别提交, message 注明关联。danqing 仅文档改动时**不触发**
- 测试基线: **40 绿** (16 lib + 16 main + 8 genlog), 2026-09-12 实测 (引擎拆分后口径;
  lib = expand/open/search, main = view/main; 引擎 47 条随迁 `danqing-logfile`, 另有 `danqing-encoding` 10 条)
- POC 及格线不过则终止, 仓库转档案 (clipboard 先例); 余前提③ = 发布后首单外检

## 必读

- 意图 (为什么做/竞品裂缝/MVP 边界/开枪前提/定价锚): `../danqing/docs/intent/log-viewer-poc.md`
- 框架规则: `../danqing/CLAUDE.md`
- 农场跨仓约定: `../CLAUDE.md`

## 仓库与分支

- 远程: `git@github.com:14uncle/danqing-log.git` (dev 与 origin/dev 同步, 无待 push 提交)
- 分支: `dev` 默认, `master` 发布基线 (全家同一模型)
- 本地 git 身份: 十四叔 <gwhun@qq.com> (建仓时已核对)

## Tech Stack

- Rust 1.85+, edition 2024 (工具链 stable-x86_64-pc-windows-gnu, rustup override 已设)
- UI 框架: danqing — git 依赖 (Cargo.lock 钉 rev), 本机经 `[patch]` 段用本地 `../danqing`
- 引擎: memmap2 (mmap) + memchr (SIMD 行索引) + regex::bytes (全文搜索)
- 编译产物: 各仓独立 `target/` —— 2026-09-10 去掉 `../.cargo-target` 共享 (RustRover 多仓并发编译触发 race condition), `.cargo/config.toml` 中 `target-dir` 行已注释

## 结构

- ~~`src/logfile.rs` / `src/jsonl.rs`~~ — 引擎层 (mmap/行索引/搜索, 全部碾压主张) 与
  前提②引擎 (JSONL 检测/列发现/memmem 字段提取/字段过滤, 零 parse; serde_json 需
  preserve_order 保首见列序)。2026-09-10 拆为兄弟 crate `danqing-logfile`, 经 `lib.rs` 的
  `pub use danqing_logfile::{jsonl, logfile}` re-export; **2026-09-12 删除仓内残留副本**
  —— 拆分时漏删, 两份 2266 行已与兄弟 crate 分叉, 且 47 个测试静默不跑 (无 `mod` 声明 = 无人编译)
- `src/main.rs` + `src/view.rs` — GUI (行锚定虚拟视口, 不用 Scrollable: f32 像素偏移在 2 亿像素域失真, 见 view.rs 模块头; 表格模式四区 = 过滤栏/表头/虚拟化行/状态栏)
- ~~`src/encoding.rs`~~ — 编码检测/转码 (2026-09-10 独立为兄弟 crate `danqing-encoding`, danqing 通过 `pub use danqing_encoding as encoding` re-export)
- `src/search.rs` — AsyncJob (worker+tick拾取泛化) + SearchNav 命中导航
- `src/settings.rs` — 轻量设置卡 (scrim 遮罩 + 玻璃卡: 关于/版本/反馈/主题下拉)
- `src/config.rs` — 配置持久化 (主题; `dirs::config_dir()/danqing-log/config.toml`, `AppTheme::load/save`)
- `src/tray.rs` — 系统托盘右键菜单 (设置 / 退出; 自定义 ID 10/11 避与框架 1/2/3 冲突)
- `src/app_update.rs` — 更新检查接线 (薄封装 `danqing::update`)
- `src/expand.rs` — 展开行模型 (行内子行嵌套展开的显示行↔文件行双向映射, 前缀和)
- `src/open.rs` — 异步打开管道 OpenJob (启动/Ctrl+O/轮转重建/巨量追平四路径统一进 worker)。
  **待核**: 模块头与 `OpenKind::Fresh` 注释写「Ctrl+O·拖拽」, 但全仓无文件拖放事件处理
  (danqing 引擎侧亦无 drop 事件) —— 「拖文件到窗口打开」很可能从未实现, 该「拖拽」疑为
  拆分/起草期遗留的路径标签。待用户裁: 补实现, 还是改注释
- ~~`src/selection.rs`~~ — 文本选区纯逻辑 (token 边界/规范化/复制拼装, 偏移=解码行字节)。2026-09-11 迁移 danqing (commit `34c6a77`, 簇F 联动), 现为 `danqing::text::selection`
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
- 引擎缺口当场修进 `../danqing` (打磨寄生; NamedKey PageUp/PageDown 与 `App::propagate_unhandled_keys()` 键回退**均已落地**, 两仓 working tree clean)
- spec 流水线: 用户发起 spec 技能后按 spec→plan→build→review→code-simplify 推进, spec 写完不立即编码
- 未获用户指示不 commit/push、不改测试数据格式语义

## Patterns

`danqing-logfile/src/logfile.rs` 是全仓风格基准: 中文 doc comment 说明做什么+不做什么+为什么; 统计一律实测不估算 (OpenStats); 失败语义明确 (UTF-16 报错不猜码)。兄弟 crate 零 UI 依赖。
