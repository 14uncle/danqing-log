# PLAN: export (腿三, 三格式)

> 日期: 2026-09-23 · spec: `docs/specs/SPEC-v1x-export.md` (已批准, 四项口径 + Open Q① 已裁) · 零 commit 推进, 人工验收留用户闸门

## 组件与依赖

| 组件 | 位置 | 依赖 |
|------|------|------|
| 导出核心 (行集 + 三格式 writer + 流式写出循环) | `src/export.rs` (新) | danqing-logfile (`lines`/`FieldExtractor`/`Schema`/`run_filter` 产出行集), danqing-encoding |
| ExportJob (在途/进度/取消) | `src/export.rs` | `AsyncJob` 范式 (`search.rs`), `Arc<LogFile>` |
| 入口 + 门控 + 格式菜单 + 保存对话框 | `main.rs` + `view.rs` | 导出核心, `Feature::Export`, `rfd` |
| 进度/完成/取消反馈 | `view.rs` 底栏 | ExportJob 状态 |
| 测量通路 | `src/bin/logbench.rs` | 导出核心 |
| 文档收口 | `settings.rs` SHORTCUTS / README / ROADMAP / PERFORMANCE_REPORT | — |

切片原则: 每个格式任务 = **行集 → 格式化 → 写出 → 验收**的完整路径（不按「转义函数/表头」横切）;
T1 的行集 + 流式循环是三格式共同前置, 必须先行。实施顺序 **T1 → T2 → T3 → T4 → T5 → T6 → T7**
（T2/T3 逻辑独立, 先 CSV 因它的判据最厚 fail fast）。Checkpoint **A**(T1–T3) / **B**(T4–T5) / **C**(T6–T7+人工验收+review)。

## 关键实现事实 (开工前已核实, 不靠猜)

1. **`file: Arc<LogFile>`** (`main.rs:164`) — worker `Arc::clone` 即可共享, **零所有权改造**。
2. **`line(i)` 剥行尾** (`\n`/`\r` 都不含, `logfile.rs:366`) — D3 字节保真需行尾策略, 见「衍生设计」。
3. **全文遍历走 `lines()` 单遍** (`logfile.rs:410` 注释的教训: 逐行 `line(i)` 全量遍历 = 过滤 235ms→1072ms 退化) —
   导出写出循环 = `lines()` + 行集游标（升序指针单向走）; 稀疏命中下按行走 OK, **不许**全量 `line(i)`。
4. **行集三来源**: `jsonl::run_filter` → `Vec<u64>` (`jsonl.rs:676`) / `SearchNav.hits: Arc<Vec<u64>>`
   文件行号升序 (`search.rs:80` — 「一行多命中只出一次」是**构造保证**, hits 本身就是行号表) /
   全集 = `0..line_count`。live-tail 冻结 = 启动时 clone 行集 + clamp `line_count`。
5. **`AsyncJob` 无取消原语** (`search.rs:16-75`, 只有 `invalidate` 丢结果) — 取消 = 任务内
   `AtomicBool` 协作退出 + **删半成品**; 进度 = `Arc<AtomicU64>` (已写行/总行) 每帧读;
   在途拒绝 = 入口变「取消导出」按钮, 不再弹格式菜单（语义优于弹「忙」提示, spec 判据
   「重复入口被拒」的意图 = 不产生第二作业）。
6. **门控范式现成**: `Msg::ShowUpgradePrompt(Feature)` (`main.rs:398`) + `entitlement.allows`
   判定 (`main.rs:675` FieldAnalytics 同款); `Feature::Export` 枚举与文案「导出」**已备**
   (`license.rs:197`) — 本模块只接线不发明。
7. **快捷键 (Open Q② 初核通过)**: SHORTCUTS 表 `settings.rs:479-484` 无 E; Ctrl 分发链
   `main.rs:1809` 现有 `b`/`g`/`t`/`a` — **`Ctrl+E` 无占用, 可用**。T5 落表 + `action_of`
   一致性锁（1041 行先例: 两处抄同一清单会漂）。
8. **rfd 0.15 已在依赖** (`pick_file` 先例 `main.rs:1968`), `save_file()` 同款, 零新增。
9. **零新依赖成立**: serde_json / danqing-encoding / memchr 均已有; 默认文件名时间戳
   **手写** civil-from-days 纯函数 + 测试（Cargo.toml 无 time/chrono, 不为此引依赖）。
10. **测试基线 262**（2026-09-22 update-hint-ui 收口实测 258→262）— T1 开工三件套复核回填;
    CLAUDE.md「184」是 09-16 旧值, 别拿旧值当回归基线。

## 衍生设计 (spec D3 行尾策略分叉 — 「零引擎改动」预期修正)

`line(i)` 剥行尾 ⇒「导出后与源文件对应行逐字节相同」不能从 `line()` 直接得到。策略分叉:

- **全集 raw = mmap 字节整拷** — 保真**满分**（含混合行尾/无终行尾）, 也是最快路径。
- **稀疏 raw = 行内容 + 文件级行尾探测** — 采样文件前部 8 个行尾定 LF/CRLF, 统一写出; **零引擎改动**。
- **混合行尾文件** = 已知局限（实机撞到再裁: 升级为引擎 `line_with_ending(i)`, 一次联动）。
- **UTF-16**: T1 第一件事核实 `LogFile::open` 对 UTF-16 的行为（「UTF-16 报错不猜码」家法）——
  若 open 即拒, 导出天然不适用; 若能开, 行尾探测在宽字符下的语义要单独过一遍再定。

另两处口径澄清（与 spec 兼容, build 中按此执行）:

- **CSV 值 = 提取值源字节全文**（不截断不重序列化, 与表格显示的差异只在显示截断）——
  取值走 `FieldExtractor`（levels/analysis 同源）, 解码走 `danqing_encoding`; **不逐行
  serde parse**（D9 性能红线: 1 GiB 全 parse 光解析就 ~17s）。嵌套值在 JSONL 单行内
  本来就是单行, 源字节即紧凑形态。
- **pretty 必须 parse**（D4）, 成本计入 ≤30s 目标; 失败行原样 + 计数提示。

## 任务 (详见 `todo-v1x-export.md`)

- **T1** 导出核心 I: 行集快照 + 原始行 writer（整拷路径 + 稀疏路径 + 行尾策略）+ 流式
  写出循环骨架（`lines()` + 行集游标 + 取消协作点 + 进度计数）+ 字节保真对拍测试。
- **T2** 导出核心 II: CSV writer（RFC4180 手写转义 + BOM + CRLF + schema 列序 + 空缺/
  schema 外字段口径）+ 对拍测试（BOM 头字节/引号/逗号/换行/CJK/嵌套源字节）。
- **T3** 导出核心 III: pretty writer（逐行 parse → to_string_pretty indent 2 + 对象间
  空行 + 失败行原样计数）+ 对拍测试。
- **T4** ExportJob: 在途标志 + 进度 + 取消删半成品 + 完成回调（行数/路径/失败计数）+
  语义测试（取消后半成品不存在; 在途不二发）。
- **T5** 全链路 UI: 状态栏「导出…」+ `Ctrl+E` + 格式菜单按模式收口 + 门控（保存对话框前）
  + `rfd::save_file` 默认文件名 + 进度/完成/取消反馈 + SHORTCUTS 落表锁 + 门控接线锁。
- **T6** `logbench --export <raw|pretty|csv>` + 实测回填 `PERFORMANCE_REPORT.md`
  （目标: 1 GiB raw 热缓存 ≤ 8s / CSV·pretty ≤ 30s 量级, 达标线未到按 D9 处理: 数据说话,
  优化或重订, 不估算冒充实测）。
- **T7** 文档收口: README 付费层 bullet / ROADMAP §二腿三勾销 / spec 实现记回填 /
  `docs/ms-store-copy.md`「v1.x 上线时必须改什么」清单核对（导出属付费层话术素材）。

## Checkpoint

- **A（T1–T3 后）**: 三格式纯逻辑测试全绿; 字节保真/CSV 对拍/美化对拍各有「摘掉实现精确红」
  的 A/B 记录; 基线 262+N 回填; clippy 0。
- **B（T4–T5 后）**: 免费态零落盘 + 升级提示; 付费态三格式实机可导; 取消删半成品;
  在途不二发; Ctrl+E 与 SHORTCUTS 表一致锁绿。
- **C（T6–T7 后）**: logbench 数字落 PERFORMANCE_REPORT（标注冷热/硬件口径）;
  spec 成功判据机器部分逐条过; **人工验收（用户实机, spec §成功判据四条）**;
  review 阶段走 `/agent-skills:code-review-and-quality`。
