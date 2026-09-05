# SPEC: 丹青日志 MVP v1 —— 能力地图与项目级约定

> 2026-09-05 用户裁决转正 (POC 双前提已过, 见 `../danqing/docs/intent/log-viewer-poc.md`),
> 发起 spec 流水线。本文件 = Phase 0 能力地图 + 三模块共享约定;
> 模块 spec 在 `docs/specs/SPEC-<模块id>.md`, 以本文件地图为索引。

## 产品一句话

Windows 上「klogg 的速度 × LogViewPlus 的结构化」大文件日志/JSONL 查看分析器:
GB 级秒开、虚拟化滚动、tail + 实时过滤、正则搜索高亮、JSONL 列化 + 嵌套展开 + 字段过滤。
v1 单二进制全功能 (付费层刀法留 v1.x, 2026-09-05 用户裁决)。

## 能力地图

| 模块 id | 职责 | 依赖 |
|---|---|---|
| `core-viewer` | mmap 引擎硬化 (截断/轮转/编码)、虚拟视口、正则搜索+高亮、级别着色、书签 | — |
| `live-tail` | tail 跟随、增量索引、实时过滤刷新、截断/轮转生存 | core-viewer |
| `jsonl-table` | 真 parser 换 memmem、行内子行嵌套展开、点路径/数值比较过滤 | core-viewer |

构建序: `core-viewer` → `live-tail` ∥ `jsonl-table` (两模块可并行)。
意图文档付费层 (多文件时间戳合并/过滤器会话/导出) 不在 MVP, 归 v1.x。

## Tech Stack

- Rust 1.85+, edition 2024 (工具链 stable-x86_64-pc-windows-gnu, rustup override 已设)
- UI 框架: danqing — git 依赖 (Cargo.lock 钉 rev), 本机经 `[patch]` 段用本地 `../danqing`;
  联动改动先 push danqing 再 `cargo update -p danqing` 提交 lock, 顺序不能反
- 引擎: memmap2 (mmap) + memchr (SIMD) + regex::bytes + serde_json (`preserve_order` 必须,
  否则 Object 是 BTreeMap 字典序, 列序失真)
- 共享编译产物: `.cargo/config.toml` → `../.cargo-target` (全家共用)

## Commands

```bash
cargo build                                     # 构建
cargo test                                      # 全部测试 (纯逻辑, 无需 GPU)
cargo clippy --all-targets -- -D warnings       # 静态检查 (必须零警告)
cargo fmt                                       # 格式化
cargo run --release --bin logbench -- <文件> [--filter "level=ERROR status=50*"] [正则...]
cargo run --release --bin genlog -- out.log 1024 [--jsonl]   # 确定性测试数据
powershell -NoProfile -File tools/package_portable.ps1       # 打包 → ../release-archives/log/
```

## Project Structure

```
src/             → 库 + 应用 (当前: lib.rs / logfile.rs / jsonl.rs / main.rs / view.rs)
src/bin/         → logbench (基准) / genlog (测试数据)
docs/specs/      → 模块 spec (本文件地图是索引)
tools/           → package_portable.ps1
assets/          → logo (占位, 命名定稿后换真 LOGO)
```

core-viewer 预期演化 (plan 阶段细化, 非承诺): `src/engine/` (mmap/索引/编码/搜索)
与 `src/app/` (GUI/交互) 分层; POC 的 logfile.rs 是种子不是包袱, 可重写。

## Code Style

基准 = `src/logfile.rs` 与 `src/jsonl.rs`: 中文 doc comment 说「做什么+不做什么+为什么」;
统计一律实测不估算; 失败语义明确 (UTF-16 报错不猜码); 行锚定 f64 不回 f32 像素域。

```rust
/// JSONL 检测: 采样前 DETECT_SAMPLE 行, 非空行中 ≥90% 能 parse 成 JSON object。
/// 采样不足 3 行不判 (证据不足), 空文件/全坏行 → false。
pub fn detect(file: &LogFile) -> bool {
```

约定: 新 `.rs` 文件头 `//! @author 十四叔` + `//! @date yyyy/MM/dd`; 注释/文档一律中文;
danqing 公开 API 一律经 `danqing::` 根路径 re-export 消费, 不抄深层模块路径。

## Testing Strategy

- 纯逻辑单测写在对应模块 `#[cfg(test)]`; 引擎测试用临时文件 fixture
  (见 `jsonl.rs::tests::open_with` 模式), 不依赖 GPU
- GUI 交互走人工验收清单 (写进各模块 spec 成功判据)
- 性能回归 = logbench 数字对照 POC 基线 (见下), 每次引擎改动后复跑
- 提交前三件套: `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test` 全绿

## 性能基线 (POC 实测, 正式版不退化 = 全局成功判据)

| 指标 (1GB / 635 万行明文, 483 万行 JSONL, release 热缓存) | POC 基线 | v1 门槛 |
|---|---|---|
| mmap 建立 | 134 µs | ≤ 1 ms |
| 行索引 | 425 ms (2406 MiB/s) | ≤ 600 ms |
| 字面量全文搜索 (`ERROR`) | 69 ms | ≤ 150 ms |
| 结构正则 (时间戳) | 1051 ms | ≤ 1500 ms |
| JSONL 字段过滤 (`level=ERROR`) | 235 ms | ≤ 400 ms |
| GUI 双击到窗口可见 | 1.26 s (含 wgpu init ~1.1s) | ≤ 1.5 s |
| 随机访问 | 0.37 µs/行 | ≤ 1 µs/行 |

## Boundaries

- **Always**: 提交三件套; 性能主张必须带 logbench 实测数字; 行锚定 f64; 失败语义明确
- **Ask first**: 新增依赖 (尤其 JSON 流式库 / 编码检测库); danqing 引擎改动 (联动提交顺序);
  数据格式/持久化协议; 付费层工程化; 水平滚动/列交互等超出模块 spec 的 UX 加法
- **Never**: 未获用户指示 commit/push; 显示/过滤热路径全文件 JSON parse (1GB 必慢, POC 已证
  memmem 路线 235ms); 编辑/SSH/协作/图表/SQL/订阅制 (意图文档 Out); 改 genlog 数据格式语义

## Open Questions (项目级)

- GBK 启发式检测的准确率边界: 零依赖统计启发式 vs 引入检测库 (后者 = Ask first)
- 产品命名与真 LOGO (danqing-log = 工作名; 发布前裁决, 连带仓库名与 assets/logo)
