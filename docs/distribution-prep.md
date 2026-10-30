# 1.5 分发准备（第二波 · 增量计划）

- @author 十四叔
- @date 2026/10/08
- 状态: 备料中 —— **推送时机 = 1.5 可购买后**（`PURCHASE_URL` 回填 + 发布，见 `tasks/v1x-release-checklist.md`）
- 上游: `docs/intent/v1x-release-window.md`（空窗裁决）

## 0. 本文档与 09-20 底稿的关系（先读这段，别重复）

**底稿 = `danqing/docs/intent/curated-channel-submission-plan-2026-09-20.md`**，它是本波的基座：
渠道入口（中文 6 站官方原文已核实）/ 定稿文案 1–9（已过事实红队）/ 投放记录表（§三）/ 判据（§四）
**都在那里，本文档不复制**。本文档只写 **1.5 波次相对底稿的增量**。

（教训留痕：本文档 10-08 初稿曾把渠道清单与通用文案重抄了一遍 —— 审稿时才发现底稿存在。
「写新文档前先全局搜既有产物」，与「同一内容写两处然后漂了」同族。初稿另有四处事实错误：
0.6s 误标「热缓存」（实为**冷**启动，热启动 ~90ms）；「记事本卡死」（未实测，且与禁令
「记事本拒开」同族）；「单个 exe 不到 5MB」（zip 才是 4.8MB，exe 本体更大）；均已随重写消除。）

**投放记录的单一真身**仍在底稿 §三表；校准钟读数在 `tasks/calibration-30d.md`（T+20 快照 10-08 已落）；
两处分工：前者记「动作」，后者记「读数」，互相引用不同步复制。

**1.5 可粘贴成品 = `docs/distribution-wave1-copy.md`（2026-10-30 产）** —— 逐渠道成品文案
（小众软件编辑+回帖 / 少数派 / 技术向同稿四发 / dev.to / WinGet·Scoop），含统一口径与
两个投递前必须解决的错位。本文件仍只管**增量计划与判据**，不再承载成品文本。

## 1. 底稿当前状态（2026-10-08 盘点）

- **已发（09-21，1.0 内容）**：小众软件论坛（topic/92198，177 浏览 1 陌生人回复）/ CSDN（228）/
  公众号（116）/ 知乎 / 掘金 / 抖音 / 开源中国博客（后四者各几十）——阅读数均为 10-09 用户后台读，
  详单与拉取日在底稿 §三
- **少数派 Matrix：审核通过已发布**（60 阅读，10-09 读）——策展渠道首收，0 拒 1 收；
  **HelloGitHub：仍审核中**（10-09 确认，月刊节奏）—— 后续回读属校准钟动作
- ~~**就绪待投（用户动作，一直没投）**：软餐 / 异次元 / 反斗 / 阮一峰周刊~~
  **2026-10-09 用户裁决不投**：前三家活跃度预判不足；阮一峰周刊适配度预判不足（估计不会收录）。
  文案 5/6/8 留档（底稿），复活即用——但复活投稿前仍需先按 §2 改价格口径。

## 2. 1.5 波次变更点

### 2.1 文案更新（所有稿件统一改）

1. **价格口径**：免费层 = 单文件全功能，永久免费开源；付费专业版 1.5 = 买断 **$29 个人 / $59 企业**，
   **首发早鸟 $19/$39（首月或前 50 单，Paddle 轨）**。帖子是带日期的内容，可写早鸟；
   商店常驻文案不写早鸟（会过期的文字不进 listing）。
   渠道规则依据：小众软件「付费必须标价」、跨站共性「价格挑明，否则视为伪免费」。
2. **版本号**：对外一律 **1.5.0**（10-08 曾裁 2.0.0，10-09 改判 1.5.0 —— 与仓内「v1.x」代号同族对齐，代号即版本族）。
3. **新弹药（1.5 付费层，全部实测值，出处 `PERFORMANCE_REPORT.md`）**：
   - **合并时间线**（腿一）：3 源 × 1 GiB（1698 万行）合并就绪 **947–967 ms**；
     每源时钟偏移校准（±10 年 / 时区 ±23h）、合流态 live-tail（增量合流 27–50ms/4MiB）、
     跨源 `req_id` 追踪（1699 万行 164–168ms）、按源着色/隐藏。
     **口径陷阱**：这是「我们合并 3 源」对「LogViewPlus 单开 1 源 60s」——**只有「大文件+多源」
     组合一起讲才成立**，单拎出去比是错口径。竞品求而未得的原话锚：LogViewPlus 论坛
     2021 求 time offset calculator、2023-12 求合并视图隐藏某文件，**至今未满足**（FEATURE-MATRIX §3.3）。
   - **字段分析**（腿二）：1 GiB 全文件单列 **1001ms**（402 万行，分位数超 100 万值标「采样估计」）；
     跟随过滤 36,123 行 124ms（精确）。
   - **导出**（腿三）：raw 全集 **608ms**（1684 MiB/s ≈ SSD 写盘上限）/ pretty 18.1s / CSV 20.7s
     （BOM+CRLF，Excel 开不乱码）。
   - **命名工作台会话**（腿四）：过滤器组合 + 搜索历史 + 列配置 + 展开态跨重启。
   - **三连（2026-09-27 改判付费）**：列配置（列宽拖拽/显隐/重排）、书签跨重启持久化、
     免语法字段点选查询 —— **会话内书签与 .log 直用保持免费**（ROADMAP-v1x §一）。
   - 免费层无功能新增噱头可讲 —— 1.5 的叙事主体就是付费专业版 +「免费层照旧免费」。
4. **叙事主线建议**：1.0 的钩子是「快」（已被 9 渠道验证为触达力有限——9 渠道合计仍是小量）；
   1.5 的钩子换成**「合并时间线」这个付费入场券场景**（调研：LogViewPlus 43 条买家评价里
   3/10 点名合并，Dadroit 把 Union 钉在 $198/**年** 顶档 —— 我们对照是买断 $29）。

### 2.2 渠道增量

- ~~**新增（开发者正对，底稿未列）**：V2EX 分享创造（`v2ex.com/go/create`）、LINUX DO 发现频道~~
  **已砍（2026-10-09）**：用户报两站不可达，本机 curl 实测双双连接超时（12s，`http_code 000`）——
  同 HN 死法（GFW），无代理通道。文案未写，无留档损失；环境变化可复活。
  空位（连同不投四家）由 §2.4 三件机制新渠道顶替：知乎回答 / dev.to / WinGet·Scoop。
- **二波已投渠道**：CSDN / 掘金 / 知乎 / 开源中国博客 —— 新发一篇 1.5 文无平台障碍；
  少数派 Matrix 自助发布，新发一篇体验文（1.5 场景向）。
- **小众软件论坛**：规则明令「不要重复发帖」—— 1.5 更新走**编辑原帖 + 回帖**（规则明文
  「欢迎开发者回帖互动」），不开新帖【此解读待用户发帖前再核一次版规】。
- **英文提交站改判试投（2026-10-28，见 §2.5）**：原「延后」裁决被 i18n 裁决 B 改判；
  Reddit / Product Hunt / HN 维持出局（10-28 实测直连超时，用户无代理）。
- **维持不动**：抖音/小红书/百家号/公众号（无外链或私域，不指望 referrer，最多同步发个存在感）。

### 2.3 新截图需求（1.5 功能，现有两张 dark 图之外）

| 图 | 内容 | 状态 |
|---|---|---|
| 已有 | JSONL 列化视图（dark）/ GBK 中文视图（dark），1920×1032 | ✅ 09-20 已确认可用 |
| 新拍 | 合并时间线：三源按源着色 + 源卡（付费态，本机 license.key 已在） | ⬜ 需交互构图，见 §4 |
| 新拍 | 字段分析面板（数值分布 + p50/p99） | ⬜ 需交互（点字段行） |
| 新拍 | 明文主视图 + 直方图侧栏（dark/light 各一） | ⬜ 零交互，agent 可拍 |
| 新拍 | GBK 中文（light 一） | ⬜ 零交互，agent 可拍 |

### 2.4 渠道折叠三件（2026-10-09，用户「go」批准）：知乎回答 / dev.to / WinGet·Scoop

背景：同日两刀（不投四家 + V2EX/LINUX DO 不可达）清出 6 个空位，折入三件**机制新渠道**——农场此前从未用过这三种机制：
① 知乎回答 = **拦截机制**（答已有问题，提问者此刻正在找解法，与广播式专栏不同）；② dev.to = **英文搜索资产**
（HN 砍掉后的英文拦截位）；③ WinGet/Scoop = **安装路径管道**（`winget install` 是对的受众 + 零摩擦 + 信任状三合一）。
**纪律不变：全部只备料，随 1.5 波次发（可购买后）**——提前发会污染 10-18 校准钟归因窗；想提前铺管道需用户特批。

#### ① 知乎回答（待人工选题）

**机器选题失败（如实记）**：WebSearch 本会话空转（连英文查询都零结果）、WebFetch 服务被网络策略整体拦截、
本机 curl 打 cn.bing / 知乎站内搜索 / 搜狗知乎 / 百度四路全空（JS 壳或反爬）。
**零命中 = 工具故障面，不代表问题不存在**（09-20 红队 R1 教训原句），故不编问题 URL，转人工选题。

**人工选题包**（用户登录知乎站内搜索）：

- 检索词：「大日志文件怎么打开」「日志分析工具 推荐」「Windows 日志查看器」「几个G 的日志 怎么查」「log viewer 大文件」
- 选题判据（四条全过才入选）：
  1. 问题描述里有**真实凑合现象**（卡死 / 记事本打不动 / grep 太慢）——心法第一条「谁还在凑合」
  2. 关注者 ≥50 或浏览 ≥1 万
  3. 现有回答无强势答案（或答案是 klogg/LogViewPlus 且评论区在抱怨）
  4. 近一年仍有新回答/新关注（老票多新票无 = 已死，日期分布检）
- 答案骨架：场景痛点一句 → 同机实测对照（VS Code ~10s / Daucloud ~7s / LogViewPlus 60s / 我们冷开 0.6s）
  → 原理两句（mmap + 行索引，不解析不加载）→ 不做什么（无 ANSI / 无云端）
  → 价格挑明（免费层永久免费 + 买断 $29/$59，早鸟 $19/$39）→ GitHub 链接 + 「自研工具」披露
- **回填槽位**（选定后填，关注/浏览数以用户后台读数为准）：

| 问题 | URL | 关注/浏览 | 入选理由 | 答案角度 |
|---|---|---|---|---|
|  |  |  |  |  |

#### ② dev.to 英文体验文

文案 10 已备（底稿 §二 文案 10，1.5 口径，front matter `published: false` 防误发）；
两张现有截图（JSONL 列化 / GBK 中文）可直贴。发后读数进底稿 §三记录表（行已立）。

#### ③ WinGet / Scoop 收录 PR（清单已备，名字零冲突）

**名字零冲突（2026-10-09 实证）**：winget code search `LogLens` = 0 命中；scoop extras 的
`loglens.json` / `danqing-log.json` 均 404。

**winget**（成稿实证 = microsoft/winget-pkgs 里 `junegunn.fzf`（**zip + portable 同款形态**）与
`yt-dlp.yt-dlp`（单文件 portable），schema **1.12.0**，两稿均由 komac 生成）：

- 标识符建议 **`14uncle.LogLens`**（待用户点头）
- installer.yaml 草案（形态照抄 fzf 成稿）：

```yaml
PackageIdentifier: 14uncle.LogLens
PackageVersion: 1.5.0        # 提交时 = 当时发布版本
InstallerType: zip
NestedInstallerType: portable
NestedInstallerFiles:
- RelativeFilePath: danqing-log.exe
Installers:
- Architecture: x64
  InstallerUrl: https://github.com/14uncle/danqing-log/releases/download/v1.5.0/danqing-log-v1.5.0-win-x64.zip
  InstallerSha256: <发布时从 .sha256 sidecar 填>
ManifestType: installer
ManifestVersion: 1.12.0
```

  另需 locale.en-US / locale.zh-CN / version 三个 yaml（名称「丹青日志 LogLens」、Publisher `14uncle`、
  License `MIT OR Apache-2.0`）。提交工具 **komac**（fzf 稿同款，从 GitHub Release 自动生成全套并开 PR）
  或 wingetcreate；PR → microsoft/winget-pkgs，CI 校验。

**scoop**（成稿实证 = extras 里 `everything.json`（GUI zip 应用全套）与 `obsidian.json`
（checkver github + autoupdate 标准式））：

- GUI 应用走 **extras bucket**（main 是命令行工具）；快捷方式用 `shortcuts` 字段（不是 bin）：

```json
{
    "version": "1.5.0",
    "description": "Native Windows log/JSONL viewer — opens 1 GiB logs in ~0.6 s (mmap + line index).",
    "homepage": "https://github.com/14uncle/danqing-log",
    "license": "MIT/Apache-2.0",
    "url": "https://github.com/14uncle/danqing-log/releases/download/v1.5.0/danqing-log-v1.5.0-win-x64.zip",
    "hash": "<发布时填>",
    "shortcuts": [["danqing-log.exe", "Danqing LogLens"]],
    "checkver": { "github": "https://github.com/14uncle/danqing-log" },
    "autoupdate": {
        "url": "https://github.com/14uncle/danqing-log/releases/download/v$version/danqing-log-v$version-win-x64.zip"
    }
}
```

  （双协议串 `MIT/Apache-2.0` 的写法以 PR CI 验证为准；备选 = 自建 bucket——零评审摩擦但零发现价值，首选 extras。）

**清单已产（2026-10-28）**：`release-archives/log/packagers/`（仓库外）——winget 四件
（installer / locale.en-US / locale.zh-CN / version，schema 1.12.0，sha256 = v1.5.0
sidecar 实测值 `801c975c…`）+ scoop `loglens.json`（JSON 校验过、全 ASCII）。
**提交 PR 仍是对外动作，用户点头后走**（komac 或手动）。注：发布夜本机 curl 拉
GitHub 资产 CDN 连续 000（exit 35），`gh api` 实证资产 uploaded——CDN 路由抖动
非资产问题；国内用户直连下载不稳正是 MS Store 兜底轨存在的理由之一。

### 2.5 海外提交站试投（2026-10-28 立案 —— i18n 裁决 B 的执行臂）

上游裁决：`intent/i18n-deferral-2026-10-28.md`（UI 不做中英文切换；触点英文化 +
小剂量海外试投先行；失效标志/复活扳机写死在该文档，判读日 2026-11-27 与首单外检钟同案）。
划线前提：用户浏览器**无代理** —— 名单按「本机直连可达」裁（10-28 curl 实测）。

| 平台 | 实测 | 形态/备注 |
|---|---|---|
| MajorGeeks | ✅ 200 | Windows 工具主场，编辑审核制投稿 |
| SnapFiles | ✅ 200 | 老牌 freeware 目录 |
| Softpedia | ⚠️ 403（WAF 拦 curl，网络通） | 大型下载站，投稿入口浏览器复核 |
| FileHippo | ⚠️ 412（同） | 下载目录 |
| SourceForge | ⚠️ 403（同） | 镜像 + 开发者受众 |
| AlternativeTo | ⚠️ 403（同） | 「LogViewPlus alternative」卡位 |
| SaaSHub | ⚠️ 403（同） | 替代品站 |
| Portable Freeware | ⚠️ 403（同） | 便携目录 —— 便携 zip <5MB 正中靶心 |
| dev.to | ✅ 200 | 已有备料 = §2.4② 文案 10，同波即发 |
| HN / Reddit / Product Hunt / FileHorse / Uptodown / CNET / Slant | ❌ 超时/故障 | 出局（直连不可达 + 无代理） |

**触点现状（10-28 核）**：落地页 `14uncle.github.io` **已是英文**（`lang="en"`）✅；
README 为「英文开场 + 中文主体」半英态 → 待补全英文版；MS Store 列表 EN-US locale
待核（随 v1.5 商店提交窗口同办，`ms-store-copy.md` §6 已有预起草可译）。

**分工不变（§5）**：注册/投稿 = 用户真人；英文简介稿起草 + 逐站投稿要求核对 +
读数回填 = agent。**读数归因**：投稿日落底稿 §三表；新 referrer 域名
（majorgeeks / snapfiles / softpedia / …）出现时进 `../tasks/calibration-30d.md`
周拉 —— 那就是「外国人来没来」的直接读数。

## 3. 1.5 发布主稿（anchor，各渠道按底稿调性改编）

### 标题候选

1. 「丹青日志 LogLens 1.5：把多个服务的日志合成一条时间线」
2. 「1 GB 日志秒开的 LogLens 出 1.5 了：多文件合并时间线 + 字段分析」

### 正文骨架

- **承（一句）**：上个月发布的 LogLens 是那个「1 GB 日志冷开 0.6 秒、正则搜索 115ms」的
  Windows 原生日志查看器（免费开源，单文件便携 zip <5MB）。
- **痛点场景（合并）**：微服务时代一次请求横跨几个服务，日志散落几个文件、各自的时钟还对不齐。
  拿肉眼对三个文件的 14:00:03 是哪个先发生的，是值班夜里最反人类的活。LogViewPlus 的用户
  2021 年就在官方论坛求 time offset 功能，到现在没有。
- **1.5 带来了什么**（付费专业版）：
  - **合并时间线**：多个日志文件按时间戳合成一条时间线；**每个源可单独校时钟偏移**
    （±10 年 / 时区 ±23h）；按源着色、按源隐藏；`req_id` 一键跨源追踪；源还在写就增量合流。
    实测：3 源 × 1 GiB（1698 万行）合并就绪约 0.95 秒。
  - **字段分析**：点一个 JSONL 字段即出分布（数值 p50/p99、枚举 Top20），1 GiB 全文件单列约 1 秒。
  - **导出**：当前过滤结果导出原始行 / pretty JSON / CSV（Excel 直接打开不乱码），
    1 GiB 原始行导出 608ms。
  - **命名会话**：过滤器 + 搜索历史 + 列配置 + 展开态，存成命名工作区跨重启恢复。
- **价格挑明**：免费层 = 单文件全功能，**永久免费开源**，与 1.0 相比不收回任何东西；
  专业版买断 **$29 个人 / $59 企业**（一次买断，不换机失效那种），首发早鸟 **$19/$39**（首月或前 50 单）。
- **不做什么**：不做 ANSI 转义渲染；不做云端、不传任何数据；商店版不自己发 HTTP。
- **下载**：GitHub Releases（便携 zip）/ MS Store 搜 LogLens。一个人做的，问题提 issue。

### 纪律自检（本稿已过）

- 数字全部出自 `PERFORMANCE_REPORT.md` 实测口径（冷/热、文件大小已标）✅
- 无 ANSI / 无「独家唯一」（JSONL 限定词在免费层段落如提及须带「原生桌面、秒开的」）✅
- 竞品断言带日期：LogViewPlus 60s（本机实测）/ 论坛 2021·2023-12 求功能未满足（FEATURE-MATRIX §3.3）/
  klogg 停更 4 年 273 issues（2026-09-16 查证）✅
- 不对 klogg 声称速度 ✅ / 不写记事本卡死 ✅ / 早鸟不进商店常驻文案 ✅
- 合并对比只用组合口径（大文件+多源一起讲）✅

## 4. 时机、埋点与判读

- **推送触发**：`tasks/v1x-release-checklist.md` 阶段 3 完成后。顺序建议：
  二波渠道同日批量投（referrer 归因互不污染，底稿 §三方法）。
  （原「就绪待投四家改稿后同批投」10-09 随用户不投裁决取消，见 §1；
  V2EX / LINUX DO 同日因不可达砍掉，见 §2.2）
- **记录**：动作记底稿 §三表；读数记 `tasks/calibration-30d.md`；发帖日期全部落账，
  校准回填按日期分段解读（自然量 vs 推送增量可分离）。
- **判据沿用底稿 §四**：正向 = 任一策展站单篇 >50 GitHub uniques 或 >50 商店页面视图；
  反向 = ≥3 家主流策展站被拒或收录后 <10 uniques ⟹ 策展渠道走不通，转处方 (c)。
- **1.5 波次新增判据**：本波首次可测 L3 付费（1.5 可购买）——Paddle sales + 商店 add-on
  acquisitions 进记录表；自购剔除纪律不变（用户记时间戳）。

## 5. 执行分工（不变）

- **用户**：注册 / 投稿 / 发帖 / 回帖（真人账号，agent 不代发）；自购时间戳记录。
- **agent**：改稿、截图备料、记录表维护、按周期拉读数回填。
