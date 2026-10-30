# MS Store 英文 listing（EN-US）提交稿

- @author 十四叔
- @date 2026/10/30
- 状态: **可直接粘贴**；提交动作 = 用户 Partner Center（真人账号，agent 不代做）
- 上游: `docs/ms-store-copy.md`（中文 listing，v1.5 口径已含）· `docs/overseas-submission-copy.md`（英文口吻底稿，10-28 已核）
- 记账去向: 提交后回填 `tasks/v1x-release-checklist.md` 阶段 3

> **英文腔调不另起一套** —— 主稿取 `overseas-submission-copy.md` §二（10-28 成稿，
> 已过禁声称清单），只补 v1.5 专业版段和界面语言披露。两份英文文档不许各写一版。

## 0. 提交前三个必看

### ① 截图语言不一致 —— 唯一的真风险

`ms-store-copy.md:185` 记的规则是「截图**与 listing 语言一致**」。我们的 UI **只有中文**
（i18n 已裁决 deferral），所以英文 listing 的截图**只能拍出中文界面**。

三种处置，建议 **B**：

| | 做法 | 代价 |
|---|---|---|
| A | 不提交英文 listing，等 i18n | 海外需求永远测不出 —— 而这正是 11-27 失效标志要读的东西 |
| **B** | **照常提交，中文界面截图 + 全篇明写「界面为简体中文」** | 可能被审核打回一次，打回就按审核的话改 |
| C | 只提交英文文案、截图沿用中文那份 | 与 B 实质相同，只是不承认 |

**B 的自洽性**：listing 描述的是一套中文界面的工具，截图就是它真实的样子。
把它藏起来反而制造「下载后货不对板」。**所以披露必须写在最短的那个字段里**，
不是塞在详细说明末尾。

### ② 名称：`Danqing LogLens`

商店显示名按语言分列。EN-US 用 **Danqing LogLens**（不加「丹青日志」，英文 listing 里
四个汉字只会被当成乱码式噪音）。**窗口标题、托盘、README 仍是「丹青日志 LogLens」**
—— 这是商店名与产品名的**合法分叉**（pomodoro 同款），不是两处漂移：
一处是商店招牌，一处是程序自称。

### ③ 这是 listing-only 提交，不重传包

Partner Center 新建提交时选**「不更改程序包」**（包还是 10-30 认证通过的
`danqing-log-store-v1.5.0-x64.msix`）。程序包换不换与 listing 增语言是两件事。

---

## 1. 字段速查（EN-US 一览）

| 字段 | 值 |
|---|---|
| 语言 | English (United States) |
| 产品名称 | `Danqing LogLens` |
| 基础价格 | `0`（本体免费，不变） |
| 类别 | Developer tools |
| 支持信息 | **只填** `https://github.com/14uncle/danqing-log/issues` |
| 隐私策略 | 贴 `docs/privacy-policy.md` 正文（中文那份，政策原文以中文为准） |
| 年龄分级（IARC） | **不改** —— IARC 按应用走，与语言无关，10-30 已重答「是」 |
| 程序包 | **不更改程序包** |
| 发布时机 | 通过认证后立即开始发布 |

> 隐私策略贴中文正文这件事：商店允许多语言 listing 共用一份政策文本。
> 英文 listing 下贴中文政策**不算缺失**（政策是法律文本，不是营销文案），
> 但若审核要求英文版，就把 `privacy-policy.md` 全文翻译贴一份 —— **那时再翻，不预做**。

## 2. 粘贴块

### 2.1 简短说明（Short description）

```
A fast native Windows log & JSONL viewer. Opens a 1 GB file in ~0.6 s, with regex
search, live tail, a level histogram and a JSONL column view. Free tier is full-featured;
Pro adds multi-file timeline merge. Interface language: Simplified Chinese.
```

### 2.2 详细说明（Description）

```
LogLens is a native Windows viewer for large log and JSONL files, written in Rust.
It does not parse or load your file: it memory-maps it and builds a line index, so
opening a 1 GB log takes about 0.6 seconds (line index: 92 ms) and the window stays
responsive while you search, filter and tail.

Interface language: Simplified Chinese. The core workflow is keyboard-driven, and
the log content itself is displayed in its original encoding.

WHAT IT DOES
- Full-text regex search with highlighting (case-insensitive by default)
- Live tail: follows the file and auto-scrolls to the newest line
- Log-level histogram with one-click filtering
- JSONL column view with nested expand — no SQL, no query syntax to learn
- Chinese GBK logs detected automatically, no mojibake
- UTF-8 / UTF-16 / GBK auto-detection
- Light and dark themes
- Everything runs locally. No account, no sign-in, no telemetry.

MEASURED (this machine, release build, full report in the repository)
- Line index: 92 ms (plain log) / 94 ms (JSONL) for 1 GiB
- Regex search over 1 GiB: 115 ms plain / 129 ms JSONL
- Export 1 GiB of raw lines: 608 ms

PRO FEATURES (perpetual, one-time purchase — not a subscription)
- Merged timeline: several log files combined into one chronological line
- Per-source clock offset correction (±10 years, ±23 h timezone) — the usual reason
  three services' timestamps never line up
- Per-source colouring and hiding, cross-source tracing of a single request id
- Incremental merge while sources are still being written
- Field analytics: click a JSONL field and get its distribution (p50 / p99, top 20 values)
- Export the current filter result as raw lines, pretty JSON, or CSV (Excel-friendly)
- Named workspace sessions: filters, search history, column layout and expanded rows
  restored across restarts

Measured with three sources of 1 GiB each (16.98 million lines combined): merge ready
in 947–967 ms; tracing one request id across all sources: 164–168 ms.

THE FREE TIER
Single-file viewing, search, filtering, JSONL column view, GBK support, level histogram
and live tail are free, permanently, and open source (MIT / Apache-2.0). The Pro tier only
adds the batch, retention and delivery features listed above.

WHAT IT DOES NOT DO
No ANSI escape-sequence rendering. No cloud. No file uploads. Nothing leaves your machine.
The Store build makes no HTTP requests of its own — it talks only to Microsoft Store
services, for update checks and licence validation.

REQUIREMENTS
Windows 10 1809 (build 17763) or later, 64-bit; a DirectX 12 capable GPU.

Source and issue tracker: https://github.com/14uncle/danqing-log
```

### 2.3 产品功能（Product features，13 条）

```
Opens 1 GB logs in ~0.6 s
Full-text regex search with highlighting
Log-level histogram with one-click filter
JSONL column view — no query syntax
Live tail that follows the file
Chinese GBK logs auto-detected, no mojibake
UTF-8 / UTF-16 / GBK auto-detection
Light and dark themes
Runs fully locally, no account needed
Pro: merged timeline across multiple files
Pro: per-source clock offset correction
Pro: field analytics with p50 / p99
Pro: export raw lines, JSON or CSV
```

> 界面语言那一句**不放进这个列表** —— 这一栏是卖点列表，塞一条披露会被读成
> 「功能」；披露的正确位置是最短字段（2.1 末句）+ 详细说明第二段。

### 2.4 搜索关键词（恰好 7 个）

```
log viewer, log analysis, JSONL, large files, regex search, developer tools, live tail
```

> 别放 `LogLens` —— 产品名本身就参与搜索，占一个词是浪费。

### 2.5 系统要求

```
Minimum: Windows 10 version 1809 (build 17763), 64-bit processor, DirectX 12 capable GPU
Recommended: Windows 11
```

### 2.6 完整信任说明（Complete trust statement）

```
This is a desktop tool with a self-drawn interface: the window is rendered directly by
the GPU rather than from standard system controls. It needs to read and write local log
files that the user picks through the system file dialog (opened read-only via memory
mapping, never modified in place), and it keeps an icon in the system tray so the window
can be called back quickly. The app does not declare any global hotkeys. It makes no HTTP
requests of its own; the Store build communicates only with Microsoft Store services for
update checks and licence validation, and no personal information is collected.
```

> 与中文那份逐条对应（`ms-store-copy.md` §完整信任说明）。**「不声明全局热键」这句
> 必须留** —— pomodoro 那份写了「全局快捷键」，我们 `hotkeys: vec![]` 显式置空，照抄即事实不符。

### 2.7 截图

用 `release-archives/log/shots/2026-10-08/` 现有六张（raw / jsonl / gbk × dark / light）。
首选：`jsonl-dark` + `gbk-cn-dark`（后者正是「中文界面 + 中文日志」的证据，与披露自洽）。

若 11-27 之后拍出了合并时间线，可追加一张 —— 但**本次先提交已有六张**，
不要为了等一张图拖住整个 listing。

## 3. 与中文 listing 的纪律对照

| 项 | 中文 listing | 英文 listing | 为什么 |
|---|---|---|---|
| 界面语言披露 | 不需要（本来就是中文） | **必带，两处** | 预期管理，这里挡退款和差评 |
| 专业版价格 | $29.99（商店 add-on） | 同 | 同一条 add-on |
| 早鸟 | 不进常驻文案 | **同** | 会过期的话不进 listing |
| 数字 | 只用实测值 | 同 | 禁声称清单 |
| 「独家 / 唯一」 | 禁 | **禁**（unique / only） | 同上；站得住的说法是 "native desktop, opens instantly" |
| 「不联网」 | 已删（09-22） | **已改成 "no HTTP requests of its own"** | 商店版经 Store 服务查更新与许可 |
| ANSI | 禁 | **禁**（"no ANSI escape-sequence rendering" 是明写不做，不是声称支持） | — |

## 4. 提交后记账

1. 回填 `tasks/v1x-release-checklist.md` 阶段 3 的「EN-US listing」行（提交日 / 结果 / 是否被打回）
2. 回填 `docs/overseas-submission-copy.md` §三 记录表（这是海外 8 站之外的第 9 条通路，
   而且是唯一有官方流量入口的那条）
3. **11-27 判读时它是读数源**：英文 listing 上线后，商店的地理分布与安装量才第一次
   「有海外需求可被看见」的通道。届时判的是「**海外用户愿不愿意用中文界面的付费软件**」
   —— 不是「英文 listing 有没有用」。
