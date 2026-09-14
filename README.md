# 丹青日志 LogLens (danqing-log)

Windows 上的大文件日志 / JSONL 查看分析器 —— 丹青第四件产品。
GB 级文件秒开、虚拟化滚动、tail 跟随 + 实时过滤、正则搜索高亮、JSONL 列化 + 嵌套展开 + 字段过滤。

**v1.0 = 免费层单二进制全功能**，GitHub 永久免费开源。多文件时间戳合并、字段分析、导出、
工作台会话持久化属付费层（$29 个人 / $59 企业买断），见 [`docs/ROADMAP-v1x.md`](docs/ROADMAP-v1x.md)。

## 实测数字

1 GiB 实测（release，本机，`logbench` 无窗口基准；完整报告见
[`PERFORMANCE_REPORT.md`](PERFORMANCE_REPORT.md)）：

| 指标 | 明文日志 | JSONL |
|---|---|---|
| 索引 | 77 ms | 88 ms |
| 搜索 (`ERROR`) | 71 ms | 69 ms |
| 字段过滤 (`level=ERROR`) | — | 74 ms |
| 随机访问 | 0.51 µs/行 | 0.57 µs/行 |
| 打开（冷 / 热） | ~600 ms / ~90 ms | 同 |

行索引为**步进表**（`memchr` SIMD 扫 `\n`，每 `INDEX_STRIDE` 行记一个绝对偏移，段内前扫定位），
不是逐行偏移表 —— 1 GB 文件行索引内存 ≤ 16 MB（`SPEC` 验收上界）。

## 功能

- **打开**：启动传参 / `Ctrl+O` / 拖到 exe 上；异步索引，1 GB 以上文件全程可响应，
  底栏显示 `{pct}% · {done}/{total} MiB`，索引中可取消或开另一个文件
- **滚动**：行锚定虚拟视口（不建全量行偏移表），2 亿像素域不失真
- **搜索**：正则（`regex::bytes`，全文扫描），可见区高亮 + 命中逐跳导航，上限 100 万条命中
- **tail 跟随**：250 ms 节流轮询，新行 ≤1 s 出现；上滚自动脱离、`End` 恢复；
  外部截断 / 轮转自动重建（旧内容重建期间保持可见），无增长时空闲 CPU < 1%
- **JSONL 表格**：自动检出（采样前 64 行，非空行中 ≥90% 可解析为 JSON object，不足 3 行不判）
  → 列发现（采样前 512 行，上限 16 列）→ 列化表格；
  嵌套对象以子行展开（行首 ▶/▼ 或 `→`/`←`）；字段过滤迷你语法跑在工作线程，不冻界面
- **编码**：UTF-8（含 BOM）/ UTF-16 LE·BE（有无 BOM 均可）/ GBK（原字节索引 + 行级 CP936 解码，
  中文可搜）/ 其余单字节编码 Latin-1 兜底
- **级别直方图**：左侧侧栏 6 桶计数（FATAL / ERROR / WARN / INFO / DEBUG / 其他）+ 对数刻度横条
  —— 线性刻度下 Info 454 万会把 Fatal 4760 压成亚像素，而后者才是要抓的那根。
  JSONL 模式下点柱条即筛出该级别（可点的行有 hover 反馈），侧栏底部出现 `清除筛选`
  可退回全部数据；`Ctrl+L` 收起侧栏（完整快捷键见设置卡 ⚙）
- **其它**：级别着色（ERROR/WARN/INFO/DEBUG）、书签（会话内）、文本选区复制、
  深浅主题（设置卡切换，即时生效且跨会话持久化，落 `config.toml`）、
  系统托盘（右键：设置 / 退出）、启动无参空态、设置卡含版本检查

## 下载

- **GitHub Releases**（主）：本仓 Releases 页取 `danqing-log-v<版本>-win-x64.zip`
- **Microsoft Store**（辅）：免费层同版本上架，v1.0 完成时开放

## 用法

```bash
# GUI: 打开日志文件 (JSONL 自动检出 → 列化表格)
cargo run --release -- <日志文件路径>

# 无窗口基准 (打开/索引/搜索/过滤/随机访问实测表)
cargo run --release --bin logbench -- <日志文件> [--filter "level=ERROR status=50*"] [正则...]

# 生成测试数据 (确定性)
cargo run --release --bin genlog -- out.log 1024            # 1 GiB 明文日志
cargo run --release --bin genlog -- out.jsonl 1024 --jsonl  # 1 GiB JSONL
```

打包：`powershell -NoProfile -File tools/package_portable.ps1` → `../release-archives/log/`

## 快捷键

| 键 | 作用 |
|---|---|
| 滚轮 / `↑` `↓` | 滚动 |
| `PageUp` `PageDown` / `空格` | 翻页（`Shift` 反向） |
| `Home` / `End` | 跳头 / 跳尾（`End` 恢复跟随） |
| `Shift` + 滚轮 | 横向滚动 |
| `→` / `←` | 展开 / 折叠选中行的嵌套子行（表格模式） |
| `/` | 开搜索栏（原始模式） |
| `Ctrl+F` | 开搜索栏（任意模式） |
| `Enter`（栏空时） | 下一条命中（`Shift+Enter` 上一条） |
| `Ctrl+T` | 原始 / 表格模式互切（JSONL 检出后） |
| `b` / `'` | 切换书签 / 跳下一书签（原始模式） |
| `Ctrl+B` / `Ctrl+G` | 同上（任意模式） |
| `f` | 跟随 toggle（原始模式） |
| `Ctrl+O` | 打开文件 |
| `Ctrl+L` | 级别直方图侧栏显隐（跨会话保持） |
| `Esc` | 关搜索栏 / 清过滤 / 关设置卡 |

JSONL 检出后开局即表格模式：直接打字进过滤框，语法 `level=ERROR status=50*`
（空格分词 AND，尾缀 `*` 前缀通配，裸词整行子串），`Enter` 应用，`Esc` 清除。
设置入口在状态栏右下角齿轮。

## 已知边界

- UTF-16 转码副本内存翻倍（1 GB → 约 500 MB 副本）；>2 GB 文件未验证
- JSONL 列化只认扁平顶层字段；嵌套对象以子行展开看，不进列
- GBK / Latin-1 文件搜索退化为字面量（无正则语法）
- 表格模式列无拖拽 / 显隐 / 重排（v1 欠账，在免费层补，见 `docs/ROADMAP-v1x.md`）
- 直方图的 DEBUG/TRACE 合并为一行，故该行与「其他」行**不可点**（无单子句能表达
  「DEBUG 或 TRACE」）；纯文本模式没有字段过滤语法，柱条一律只读
- 搜索命中上限 100 万条（超出时底栏标注总命中数）
- 书签为会话内，不持久化

## 许可

MIT OR Apache-2.0
