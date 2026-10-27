# 调研: v1.x 付费层竞争力 (四路合成)

- @author 十四叔
- @date 2026/09/27
- 状态: **已裁决** —— 2026-09-27 用户裁决五点 (§八); `ROADMAP-v1x.md` 与 `specs/SPEC-v1x-map.md` 已同步翻案
- 方法: 四路并行 Web 调研 (各路 75 分钟硬时间盒, 子代理执行); 证据纪律 = 带日期 / 付费行为单列 / 言词抱怨≠付费 / 找不到写「未找到」

## 一、问题与评估对象

interview (付费边界重裁) 中段用户发题: 「danqing-log 付费层有竞争力吗」。

评估对象: 付费层三腿 (**field-analytics** 字段统计 / **export** 导出 / **workspace-sessions** 命名会话,
$29 个人 / $59 企业买断) + 两个候选动作 (**腿一** 多文件时间戳合并——09-19 裁「第二波」;
**收三连**——列配置/书签持久化/免语法字段查询从免费层欠账改判付费)。

## 二、一句话结论

**价格与模式有竞争力 (带内零偏离, 买断在 2026 是差异化卖点); 功能内容撑不起——现三腿买家实锤≈零。
证据指向的承重梁是腿一 (合并), 外加 roadmap 上没有的新发现 (远程源 = 免费真空带)。**

## 三、逐腿裁决总表 (免费覆盖 × 买家付费实锤)

| 腿 | 免费世界 | 买家付费实锤 | 判决 |
|---|---|---|---|
| export 导出 | GUI 日志圈空白 (klogg 无; LogExpert #187 求 5 年官方劝写脚本); lnav 三形态全有 | ≈0 (43 条 LogViewPlus 买家评价无人提; 最像先例 Modern CSV **免费版即给导出**) | **软**: 基线预期型功能; GUI 人群有凑合证据 (分批复制粘贴) 但 workaround 太便宜 |
| workspace-sessions 会话 | **LogExpert 已有免费命名会话** (.lxj/.lxp); lnav 自动会话 | ≈0 (LogViewPlus 有 workspaces 但购买动机缺席; klogg 求会话票 0 反应) | **最软**: 三独立来源一致 (免费等价物+买家沉默+内部诊断「重做一遍太容易」) |
| field-analytics 统计 | CLI 满 (lnav SQL/visidata/angle-grinder), **GUI 日志圈空白**; Modern CSV 统计放 Premium $39/$59 | 卖方吆喝买家沉默 (LogViewPlus SQL 仪表盘零购买评论提及) | **中**: 有市场先例可收, 但单押=收「有但没人为此掏钱」的功能; 零语法形态打 LogViewPlus 手写 SQL 的缝, 是体验差异不是购买理由 |
| 腿一 多文件合并 (未建) | **免费等价物最多** (lnav 核心特性 / OtrosLogViewer GUI 按钮 / 微软 logmerge / Chipmunk / VS Code 插件) | **最强聚类** (3/10 买家评价点名; 论坛 52 建议中 7 条=最大功能簇; Dadroit Union 钉 $198/年顶档; 四厂商卖点第一行) | **最硬但有条件**: 「没有 merge 卖不动, 只有 merge 也卖不动」= 付费**入场券**; 差异化必须带「大文件性能+JSONL 列化+GUI 零语法」组合 |

## 四、LogViewPlus 深扒 (品类付费标杆)

- **定价**: Personal $45 / Corporate $95 / 50 席 $2,000, **2019-01 起未涨**。名义买断实质
  「注册权订阅化」——机器名绑进许可证, 改名/换机即失效, 支持期过不能在新机注册
  (2026-02~08 论坛 support/2545 被用户当面骂「marketed as perpetual, 实质订阅」,
  官方承认为未来版本改, 旧系统不打补丁)。续费 3 折 (~$11.25-28.5/年)。
  **30 天全功能 trial, 无免费档**, trial 不可重置 (2026-07-21 官方明确拒绝)。
- **功能对照 (六条全有)**: 合并=核心卖点 (拖拽即合并按日期排序) / 统计=要手写 Transact-SQL
  (Statistics Grid 仅计数分布, 聚合走 SQL Scratchpad+Dashboards) / 导出 (CSV+原始格式) /
  命名 workspace (15 秒自动保存) / JSON 列化=**列须预定义** (support/2050:
  "columns must be defined in advance"——与我们自动分列有体验差) / 点选过滤 (全套对话框)。
- **付费动机** (Trustpilot 43 条, 4.5-4.6/5 + 论坛): ①「什么都能解析+过滤深挖」的取证流
  (底色, 几乎条条); ②合并 (3/10 点名, 第二名卖点); ③单人支持响应快+价格不离谱
  ("won't break the bank", $45 六年未涨)。
- **反面 (同样值钱)**: SQL 报表/仪表盘**零购买评论提及** (企业勾选框); 导出=基线预期无人付钱;
  workspace 对比文里有、购买动机缺席。
- **漏斗底**: 无免费档 + 30 天硬墙 + Reddit 自然推荐基本缺席 + 盗版站成群 →
  「试用后流失到免费工具」的人群真实存在, 我们免费层全功能正接此漏。
- 来源: logviewplus.com/purchase.html · /docs/merge_log_files.html · /docs/reports___dashboards.html
  · /support/2545 · /support/1859 · /support/2050 · /support/2586 · trustpilot.com/review/www.logviewplus.com

## 五、免费在位者覆盖图

| 工具 | 多文件合并 | 字段统计 | 导出 | 会话 | JSONL 列化 |
|---|---|---|---|---|---|
| klogg | ✗ (仅 tab) | ✗ | ✗ (仅剪贴板) | 半 (自动恢复单会话) | ✗ (#413 官方拒绝) |
| **lnav** | **✓** (活视图) | **✓✓** (SQL/PRQL 任意聚合) | **✓✓** (csv/json/jsonl/raw/view) | ✓ (自动+脚本导出) | ✓ (字段进 SQLite) |
| LogExpert | 半 (rollover 串联) | ✗ | ✗ (#187 open 5 年) | **✓** (.lxj/.lxp 命名会话) | ✓ (JsonColumnizer) |
| OtrosLogViewer | **✓** (GUI 合并按钮) | ✗ | ✗ | 未找到 | ✓ (log4j 生态) |
| visidata / angle-grinder / Tad | 半/✗/✗ | ✓✓ | ✓/✗/✓ | 半/✗/✗ | ✓ |
| microsoft/logmerge · logweaver · VS Code Log Interleaver | **✓** | ✗ | — | ✗ | ✗ |

- **lnav 一个工具免费覆盖全四腿** (有 Windows 二进制), 但全挂 **SQL/TUI 门槛**——
  对手不是功能有无, 是语法门槛; GUI+零语法点选才是差异点。
- Windows GUI 阵营 (klogg/LogExpert/glogg) 对**统计+导出全空白**; LogExpert #187 (2020-09-26)
  评论区土法 = 分批复制粘贴 (导出腿在目标人群里的直接凑合证据)。
- **klogg 官方 2021-10-15 明确拒绝 JSONL** (#413: "Major architecture changes are required", 关票)
  ——免费层楔子的外部确认。负证据: klogg 八年无人开票求合并/导出 (要么这人群不要, 要么要的人
  直接走了; 按纪律记「未观察到凑合」)。
- 项目节奏: klogg 最后稳定版 v22.06 (2022-06), continuous 预发布停 2024-11-26。

## 六、邻近品定价与切法

- **$29/$59 带内零偏离**: $25-45 = 个人桌面工具买断密集区 (WinRAR $29 / Beyond Compare $35 /
  Modern CSV $39 / JSONBuddy $39 / LogViewPlus $45); $59 企业档与 Fork $59.99 /
  Modern CSV Business $59 一比一同带。
- **最像先例 = Modern CSV** ($39/$59 买断, 免费版真能用 ["functional, not a de facto trial"]
  + 高级命令收费 + 功能完全相同的双档身份切); 其次 LogViewPlus ($45/$95 同品类双档,
  差价买「公司合规与可转让」)。
- 切法先例: 统计放付费 = Modern CSV Premium; **多文件合并放付费 = Dadroit Union 钉 $198/年顶档**;
  大文件收费 = EmEditor (大文件控制器) / JSONBuddy ($69 档 Large File View);
  **身份门 = EmEditor Free 企业禁用 / Dadroit 非商业 / JSONBuddy 个人教育** (我们此前未设=让渡)。
- **我们比最像先例狠的一刀**: Modern CSV 免费版即可导出 JSON/XML, 我们把导出整个收付费——
  需「导出=交付物」叙事对冲。
- 品类订阅化中 (EmEditor 2024 终售终身版 / Dadroit 全订阅 / LPL 公示未来订阅),
  **单人买断此刻是差异化卖点**; 「$29 比 EmEditor 一年订阅还便宜」可写营销。
- 更新窗模板: Modern CSV「大版本寿命」/ LogViewPlus「1 年更新+过期照用+70% off 续费」/
  Sublime「3 年窗」; 纯永久仅 WinRAR。
- 来源: moderncsv.com/buy · /license-terms (2026-09-22 更新) · dadroit.com/buy-licence
  (Cloudflare 拦截, 搜索两轮一致) · emeditor.com/buy · json-buddy.com/purchase.htm ·
  sublimetext.com/buy · forum.sublimetext.com/t/business-license-is-required/60632 ·
  scootersoftware.com/shop · git-fork.com/buy

## 七、付费行为一手证据 (按功能归因)

- **合并 ≈ 远程源: 并列最强 (各约 10 次)**。
  合并: 3/10 Trustpilot 点名 (2021-03/06/09) + 论坛 52 建议中 7 条 (最大单一功能簇,
  2021-2025 持续) + Dadroit 顶档 + lnav/Chipmunk/LogoRRR/Logier 四厂商卖点第一行。
  **远程源 (本次调研新发现)**: Trustpilot 4 条 + 论坛 6 条 (2025-2026 仍在求 CloudWatch/docker),
  而 **klogg/LogExpert 均不做 = 免费真空带**——LogViewPlus 的 $45-95 有相当部分在为
  「不离开 GUI tail 远端」付。
- **大文件 = table stakes**: Dadroit 按文件大小分档 (免费 50MB / $98 每年 2GB / $198 每年 1TB)
  是「文件越大越肯付」最直接证据; EmEditor/Gigasheet 邻类实证 "huge file, need answers now"
  人群肯掏钱; 但 "DuckDB is free" 压价格天花板。
- **JSONL 专场: 流量真, 付费零**。SO q/33472292 130k 浏览但答案池陈旧 (供给没跟上流量);
  **全网找不到一条「我买了 JSONL 工具」的一手证言** (JSONL Viewer Pro 仅作者 2025-11-18 HN 自荐
  零买家声音; Dadroit 无人晒单; JSONBuddy 无买家讨论)。→ **JSONL 是定位钩不是付费钩。**
- **统计 / 导出 / 会话: 卖方吆喝, 买家沉默** (43 条评价 + 52 条建议几乎无人提)。
- **企业合规付费真实且新鲜**: LogViewPlus 论坛 **2026-09-24** 企业询单
  "corporate invoice for 2 users (accounts for three years)" (support/2586);
  Log4View 站点授权 €2,390 / 全公司 €4,750; Sublime 君子协定 HN 多条一手
  (2017-09 公司政策禁无照 shareware / 2019-12 三类客户论 / 2025-01-29 续费实证)。
- **凑合证据日期分布**: SO「手写脚本合并日志」族 2008-2018 主峰 (2015 年 11 帖), 2020 后骤降——
  DIY 凑合已被免费工具吸收, **不可当 2026 年需求证据**。免费侧 2026 仍强势生长
  (lnav HN 2026-03-24 335 分 56 评论全帖零付费讨论; Chipmunk 2026-09 还在推代码)。
- **订阅疲劳延伸到 $20 级小工具**: HN 2026-05-19 "appreciate the transparent one-time purchase
  model in an era where subscription fatigue is so common" / 2026-08-11 "not something I would pay
  a subscription for... I don't mind paying for quality app"——**买断制本身是卖点**。
- **套件逻辑**: LogViewPlus 卖的是「大文件打底+合并+远程+解析」的事故复盘**套件**;
  单押 JSONL 或单押导出/统计/会话, 全网无付费先例。

## 八、用户裁决 (2026-09-27, interview 收口)

1. **腿一 (多文件时间戳合并) 从「第二波」提前为 v1.x 发布前置**——发布延期等它。
   理由: 收银台开张即有承重梁; 先发软腿测出的 0 转化是歧义信号 (腿软 vs 分发薄分不开)。
   校准钟 (10-18/10-21) 量 v1.0 免费层流量, 与 v1.x 解耦, 不挡。
2. **三连 (列配置 / 书签持久化 / 免语法字段查询) 收进付费层**——边界对齐不是付费钩;
   v1.x 未发布 = 零回收成本最后窗口; 会话内书签与 .log 直用场景保持免费 (v1.0 既有行为不动);
   接门 (Feature 门控接线) 与腿一同窗口完成。
3. **远程日志源 = 腿五候选挂 ROADMAP**, 本轮一行代码不写, 腿一发布后凭首单外检信号再裁。
4. **EULA 身份门: 收**——免费层二进制限个人/非商业, 商用请购 $59; 君子协定
   (开源管不住源码, 管二进制+购买页口径), EmEditor/Dadroit/JSONBuddy 先例。
5. **更新窗 = 永久买断**——不设 1 年窗; 单人信任建设期, 简单买断是对 LogViewPlus
   「换机即死」的最锋利对照; v2 大版本另收费留给 v2 再裁 (Modern CSV「大版本寿命」实质同构)。

下一件 = **腿一 (merge-timeline) 起 spec** (五阶段; 含 danqing-logfile 时间戳解析引擎前置)。
发布链动作 (merge/tag/打包/商店提交) 挂起, 另行点头。

## 九、证据缺口与复核项

- Dadroit 价格 ($98/$198 每年) 经搜索两轮读取官网一致, 但官网被 Cloudflare 拦截未能直接抓取,
  **引用前人工浏览器复核 dadroit.com/buy-licence**。
- r/sysadmin 两帖正文 / AlternativeTo 评论受检索限制未取得; G2/Capterra 无 LogViewPlus 档案。
- LogViewPlus v3.2.9 发布日两来源出入 (论坛公告 2026-04-03 vs 第三方转述 2026-05-12)。
- UltraEdit 试用天数未实证; klogg 无任何商业化数据可查 (纯 GPL + 微量赞助)。
- 「远程源」为子代理计划外新发现, 证据强度已单列 (≈10 次提及), 但**我们对其工程面零调研**
  (SFTP/SSH/Event Log 各自可行性未评估)——这正是把它挂后波而非本轮的原因之一。

## 十、创新候选池 (2026-09-27 用户追问「对比竞品还有没有创新功能」——候选非裁决, 按证据分级)

**有证据的空档 (值得进规划)**:
1. **脱敏导出 (anonymize)**——交付腿延伸: 导出时自动掩码 IP/邮箱/密钥等, 日志发给厂商/客户前
   过一道。CLI 先例 = lnav `:write-*-to --anonymize`; **GUI 圈空白**; 与企业合规叙事同向
   (PII 不出内网)。成本低 (export 流式管道已成型, 加一层变换)。候选进腿一后的小件波。
2. **合并时钟偏移校准**——**腿一 spec 的必须输入**: 多机日志时钟不同步是合并落地头号实操痛;
   LogViewPlus 用户 2021 求 time offset calculator、2024 求无时间戳日志的 offset 推算
   (BETA 已加 elapsedTime); 微软 logmerge 限制「假设 UTC」正是这道坎。每源时间偏移/时区校准 +
   无时间戳行处理策略, 写进腿一 spec 调研清单。
3. **合并视图按源着色/按源隐藏**: LogViewPlus 论坛 2023-12 求「merged view 里隐藏某文件的消息」;
   微软 logmerge 按源着色。同为腿一 spec 输入。

**观察名单 (培养型, 开枪前必须写失效标志)**:
4. **错误指纹 / 已知问题打标**——相同 stack trace 聚类计数 + 「已知问题」标记跨会话留存;
   LogViewPlus 论坛 2025-06-06 有用户求 (官方答「用过滤器模板变通」= 未满足)。单条请求证据薄,
   腿五裁决时一并评估。
5. **两次运行对比 (log diff)**——部署前后两份日志去噪对比「这次多了什么错」。品类内无人做,
   零直接证据 (行为现象 = 人们肉眼对比两个文件)。不立项, 只记录。

**趋势停放**:
6. **AI 问日志**——LogViewPlus 已在 SQL 编辑器加 AI chat prompt (趋势信号); 与我们离线/零遥测
   叙事冲突 (联网+API key), 且「小+AI」优势只在有融资团队成立 (ai-era-dev-survival 结论)。
   不跟进, 保持观察。
