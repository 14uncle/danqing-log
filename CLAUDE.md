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
- 2026-09-12 (**level-histogram T1–T8 全绿, 未 commit push**): **级别计数侧栏交付**
  —— 6 桶 (FATAL/ERROR/WARN/INFO/DEBUG/其他) + 对数横条 + JSONL 点选筛选 + `Ctrl+L` 显隐。
  spec → plan → build 走完, 机器部分闭环。三项实现中的关键改判见
  `tasks/todo-level-histogram.md`: **D6** (计数不进索引趟, 索引耗时 77/88ms 基线不受影响
  —— 做了 stash 对拍的真 A/B)、**字段口径改前缀匹配** (`col=X*`, 让计数与筛选结果
  逐桶相等成为构造保证 —— 字节全等的 `level=WARN` 在文件写 `WARNING` 时会筛出 0 行)、
  **窄窗自动折叠** (原 Open Question 二选一)。计数成本: 1GB 明文 94ms / JSONL 行口径 76ms /
  JSONL 字段口径 112ms。**待人工验收 + review**
- 2026-09-12 (**用户实机报回归 → 定位并修复**): 打开 1GB JSONL 状态栏写「索引 92ms」
  却要等 ~10s 内容才出。**慢的不是索引也不是级别计数, 是列发现** —— 它按 512 **行**
  采样且用 serde_json 完整解析每行, 成本随**行宽**无界 (实测 200 MiB / 563 KiB 行 /
  每行上万小对象: 列发现 **3.17s**, serde 解析这种形状只有 ~60 MB/s; 外推 1GB ≈ 16s)。
  修在兄弟 crate `danqing-logfile`: 采样加**字节预算** (4 MiB / 2 MiB, 行宽 ≤ 8 KiB 时
  不生效 → 普通日志零影响) + 值宽度探测有界化 (原来为算一个最终 `clamp(4,32)` 的宽度,
  对每个字符串 `chars().count()` 走完全文、对每个对象/数组先 `to_string()` 整棵序列化)。
  修后同一文件 3.17s → 71ms。**本次事故的教训**: 状态栏那个「索引 N ms」只是
  `LogFile::open` 的耗时, **不含**其后的列发现与级别计数 —— 「数字与视觉不符」的根因
  是那个数字从来不等于用户在等的时间。故同时给进度显示加**阶段感知**
  (`OpenPhase`: 索引 → 列发现中 → 级别计数中), 后两段不再伪装成卡住的 99%。
- 2026-09-12 (**用户第二轮实机反馈「加了日志类别统计就变慢了」→ 结构性修复**):
  上一条把根因判给了列发现 (那确实是真缺陷, 已修), 但**不是这条反馈的成因** ——
  用户明确指出: 加统计**之前**打开 .jsonl 也很快。这把范围锁死在本次新增的计数上,
  且只对 .jsonl 生效 → 那正是唯一 JSONL 独有的改动: **字段口径计数**
  (`count_levels_field` 的 `extract_field` 要在**整行**里找 `"level":`, 成本随行
  内容走; 而 .log 走的行口径只扫行首 200 字节 —— 这就是「同做一份统计、只有 .jsonl
  慢」的解释)。
  **修法 (结构性)**: 计数**移出打开管道**, 改为独立后台作业 (`levels_job`,
  作业体 `levels::counts_for`)。打开只交出文件与口径列名, 内容不再等计数;
  侧栏在计数未就绪期间显示「…」并只读 (拿 0 冒充真实计数是假信息)。
  打开管道日志随之只剩 `索引 · 列发现` 两段。
  **这一项我先前自己撤销过** (理由是「全链 200ms 不值得拆」), 用户的实机数据推翻了
  那个判断 —— 计数在真文件上可以远超我的测量。
- 2026-09-13 (**用户实机给出决定性数据 → 计数成本真因找到并修掉**): 用户给出
  「debug 构建、同机、20 线程可用」下的两条对照 —— 字段口径 **9312ms** (1GB JSONL,
  483 万行) vs 行口径 **417ms** (1GB 明文, 635 万行), **22 倍**; 而字段口径扫的
  字节**更少**(找到 `"level":` 即返回)。故慢的不是扫描, 是**每行的两次构造**:
  `field_needle` 每行分配一个 `Vec`, 且 `memchr::memmem::find` 是**懒构造** ——
  每次调用都为 needle 重做一遍 prefilter 分析。修在兄弟 crate: 新增
  `jsonl::FieldExtractor` (needle 与 `Finder` 各建一次逐行复用), 顺手把
  `Compiled::Flat` 里同款问题一并修掉 (过滤路径也受益)。
  本机: 字段口径 91ms → **24ms** (3.8x), 且快于行口径 (24 vs 70ms); 真实 app 路径
  (debug) `perf levels_job`: 86ms → **23ms**。
  **用户机器实测确认: `perf levels_job: 计数 34ms` —— 9312ms → 34ms (274x)。事故闭环。**
  **这是我在 review 阶段主动延期的 Optional 项** (嫌要动兄弟 crate), 代价是用户
  替我付了三轮排查。
  **方法论教训**: 我前四次归因全错, 每次都是拿自己机器上的测量去套用户的文件;
  真正定位靠的是用户给的**两条同机对照**(字段 vs 行、debug、同一台机器) ——
  **对照组比绝对值有用得多**。
- 2026-09-13: **level-histogram 人工验收通过 (Checkpoint D)** —— 用户实机确认功能闭环,
  并给出三条 UX 反馈 (可点行无 hover / 回退无处可寻 / 快捷键不可知), **均当日修复**
  (`4c8105d`)。模块至此功能 + 验收双闭环。
- 2026-09-13: **设置卡三页签收口** (常规 / 快捷键 / 关于) —— 主题下拉挪进常规页
  (关于页是只读身份页, 开关混进去分不清「能改」与「只是展示」); 去掉卡面上与
  关于页重复的关于区/版本行。后续 review 收口: 页签序号改由 `settings.rs` 的
  `.tab()` 处**单点定义** (加「常规」时两处 main.rs 注释各抄一份序号, 双双漂了);
  「超 `PANEL_CONTENT_H` 会被裁切」的注释是**错的** —— 框架 `Box`/`Column` 都不裁剪,
  超高是溢出画到卡片外, 已改为可执行断言 `panel_contents_fit_fixed_height`。
  **教训 (复发性)**: 一批工作里「加新决定、不回头清旧文字」犯了 8 次 (代码注释 4 + spec 4)。
  加页签/改语义时, 顺手 grep 一遍旧措辞 (`关于`/`两页签`/`Stack`/`Never`), 别只追加不收敛
- 2026-09-13: **用户报「快捷键内容和 tab 间隔大」→ 根因不是框架** —— `Tabs` 的面板间距
  (`panel_pad` = `spacing_md` = 12px) 正常; 是 `content_row` 里的 `Center`
  **在两个轴上都居中** (`center.rs` 的 paint 按 `(area高 - 子高)/2` 定 y), 而快捷键页的
  `content_row` 正好是固定高盒子的直接子级 → 整块内容被垂直居中, tab 栏下凭空 45px。
  去掉 `Center` 后顶对齐。连带 `PANEL_CONTENT_H` 由 216 **按实测收紧到 180**
  (常规 36 / 快捷键 125 / 关于 133.5, 有更新提示 165.5) —— 原 216 的「留余量给常规页长」
  理由是错的: 常规页是最矮那页, 贴上限的关于页内容固定。**教训**: 布局数值别估算,
  量了再写; 估算的余量会变成用户能看见的空白
- 2026-09-13: **修发布阻塞: `[patch]` 提交在 Cargo.toml → 外部克隆构建不了** ——
  起因是查「发布包会不会拿到旧引擎」。查证两点: ① 提交进仓库的 `[patch]` 指向仓库外的
  `../danqing`, 外人克隆直接失败; ② `Cargo.lock` 里 danqing / danqing-logfile **一个
  rev 都没钉**(只有 path 记录) —— 于是农场 CLAUDE.md「Cargo.lock 钉 rev 保可复现」那句
  **是假的** (danqing-pomodoro 同款签名, 不是本仓独有)。修法: patch 移出 `Cargo.toml`
  进 gitignore 的 `.cargo/config.toml` (模板 `tools/local-patch.toml`, **默认关**) +
  `cargo update` 钉上 rev, 并**实测**验证了无 patch 状态下 cargo 真从 GitHub 拉
  `danqing#3d5e5e10` / `danqing-logfile#baee0a8b` 编译、98 测试全绿。
  **方法论教训 (第三次了)**: 我对「patch 与 pinned lock 能否共存」连下两个相反结论,
  两次都是推理不是实测; 最终靠 `cargo metadata` 与 `cargo test` 的**对照**才定案
  (**metadata 不改写 lock, test 会** —— 拿 metadata 当验证会得出相反答案)。
  **对照组比单个证据可靠**, 与 level-histogram 那次同一个教训。
- **同日 push**: danqing `dev` (1 笔, 纯文档) / danqing-logfile `master` (3 笔, 含
  9312→34ms 那个修复) / 本仓 `dev` (32 笔) —— 三仓全部推上远端。
  **仍未做**: 农场根 CLAUDE.md 与其余三仓仍是旧模型 ([patch] 提交在 Cargo.toml), 待裁决是否全线铺开
- 当前: **UI 改造五模块已闭环; v1.0 收尾是唯一主线** ——
  ① **UI 视觉重构** (2026-09-13 立项 → **同日五模块全闭环**; 意图
     `docs/intent/ui-redesign.md`, spec `docs/specs/SPEC-ui-redesign.md`):
     `color-pipeline` / `theme-recalibrate` / `component-polish` / `token-completion` /
     `layout-rhythm` 五格全 ✅, 每格机器 + 用户真机双闭环。
     含: 双重 gamma 修复、全框架控件补 per-frame `bind_theme`、暗色 token 重校、
     展开块底色 / 斑马与 hover 拆通道、**暗色语义色板 (2026-09-13 用户实机报
     「ERROR 选中行看得眼花」→ 两成因各修: 语义色板两套 + 选区带 30%→20%)**。
     **遗留四条, 正文在 `tasks/todo-open-decisions.md` (D1–D4), 别处不抄数字**:
     **D1 / D2 已裁已修** (D1 浅色 `surface_variant` 与底色同色 → 按暗色台阶取,
     框架 `0d91ed7`; D2 浅色语义色 sub-AA → 按 AA 压暗色板, 见下面 2026-09-13 那条);
     **D3/D4 待裁** —— 浅色两条着色路径分叉 (D2 压暗后**分叉没自己消失**,
     ERROR 3.70 / WARN 3.42 仍在 JND 之上) /
     框架 `composite_over` 混在 sRGB 空间 (文档还在推荐用它, 是量错对象的入口)。
  ② **v1.0 收尾** (原为并行线, 现为唯一主线):
     **(a) MSIX 打包 —— 链路已落** (2026-09-13): 工艺照搬 `danqing-pomodoro`
     (其 2026-09 商店版实测成稿), 四个脚本 + 20 个商店素材, **真机侧载实测通过**
     (装进 WindowsApps、启动正常、托盘图标装上、无资产告警)。详见下面当日条目。
     **余**: Partner Center 注册 + 预留名称 (**只有用户能做, 唯一硬阻塞**) →
     用真实 `-AppName` / `-PublisherCN` 重打包 → 上架物料 (隐私政策 / 文案 / 截图)
     **(b) 版本号已升 `1.0.0`** (含 lock 与打包脚本示例); **tag 未打** ——
     等素材齐备、在发布点再打, 打早了就是死标签
     **(c)** 对外文案 / 截图素材 —— **未动**, 且必须从**最终版**拍
     **(d) 新发现一项物料**: `runFullTrust` 会**强制**把应用标成「收集个人信息 = 是」
     并要求**隐私政策** (pomodoro 踩过: 别在「是/否」上硬扛, 走「提供隐私策略文本」)。
     pomodoro 那份**至今只在 Partner Center 字段里、没落盘仓库**, 我们应落盘进 `docs/`
  **交叉点 (已解)**: UI 方案决定商店首图与截图素材 → ① 已不再是 ②(c) 的前置。
  **暗色配色已真机验收** (2026-09-13, 用户「可以」) —— 那批色号整个换了, 故单独过目,
  未停留在「模块闭环」。到此 ① 的**机器 + 观感双闭环**才算齐。
- 2026-09-13 (**用户真机审查四张截图 → 两批**, 均已 push):
  **(甲) 内容区「面」阶梯重解** —— 用户报浅色展开块与暗色表头「挨着时看不出是一块」,
  实测确认**根因不是取值偏了而是判据错了**: 记档里每个面都只对**页面底**取值
  (表头 6.30 / 斑马 6.38, 各自合格), 而屏幕上表头底下挨着的是**斑马行** ——
  谁也没管邻居 (实测 0.19 / 0.08 Δ`L*`)。新增跨面守卫
  `table_surfaces_are_separated_from_their_neighbours` (每面对底 ≥3 且**两两** ≥3),
  整条阶梯一起解; 暗色区间窄到装不下两点各 3.0 (底只有 9.04, 原区间 6.30, 8 个
  8-bit 步长吃掉余量), 故让表头往外走一步。**浅色 hover↔选中 0.79 是唯一已知例外**,
  明写在守卫里 —— 浅色那段养不起六个两两 ≥3 的面。
  **(乙) D2 收口** —— 浅色语义色按 AA 压暗 (判据: **常驻面** 页面底/斑马过 4.5,
  **瞬时面** hover/选中 ≥3.0, 与暗色同一把尺; **斑马必须进常驻档**, 只量页面底
  = 漏掉一半的行)。保持 HSL 色相只降亮度 —— 同一批颜色变深, 不是换色板;
  ERROR 本来就过线**一个字节没动**。顺带把框架里那句「浅色中亮度前景本来就够」
  (拿 ERROR 一支推出全称结论) 补成实测版。
  **(丙) 展开块「行间隔」** —— 用户报浅色展开区每行之间有横线。**不是新缺陷, 是
  (乙) 的深色块把它照出来的**: 展开块原先**逐行铺**底面, 相邻矩形在逻辑坐标上严丝合缝,
  但每个都自己做边缘抗锯齿、各自跟底色混一次 —— 两次半透明叠不出一次全不透明,
  交界留下 1–2px 浅缝 (截图实测 `(207,216,212)`, 块色 `(200,209,205)`、底 `(240,248,246)`)。
  旧块色 `#E4EEEA` 与缝只差 ~2/255 故看不见。改成**一段连续子行只铺一个矩形**
  (+ 抽成纯函数 + 守卫, `expand_block_rects`)。全仓查过: 多行文本选区的矩形每行
  上下各内缩 2px, 是唯一另一处相邻面, 且刻意留缝 —— 不属同类。
  **方法论**: 两批都是同一个错 —— **指标/判据跟现象对不上**
  (前有拿 WCAG 对比度量大面积色差, 后有拿页面底当屏幕上的邻居)。
  **并记一条**: (乙) 改色后**连带照出 (丙)** —— 改一个取值时, 早先就错的东西会现形,
  别把它当成回归。
- 2026-09-13 (**v1.0 收尾 (a): MSIX 打包链路落地**, 已 push): 路径是「先调研 → 再找到
  **现成成功案例** → 改为照搬」—— 调研子代理的 **WebFetch 被网络策略整体拦截**,
  结论全部来自搜索摘要 (按「待验证」看待); 随后用户指出 `danqing-pomodoro`
  **已经上架过商店**, 于是整套工艺照搬它 2026-09 的实测成稿, 省掉整条从零试错。
  **工具链零安装**: pomodoro 把 `makeappx` / `makepri` / `signtool` 的 SDK 裁剪副本
  vendor 在 `tools/sdk-tools/` (19MB, **gitignore 不进仓库**), 本仓照做 ——
  我先前「本机没有 Windows SDK, 得先装 1GB」的判断因此作废。
  命令链: `gen_store_assets.py` → `build_msix.ps1` → `sign_msix_local.ps1`
  → `trust_cert_machine.ps1` (最后一个要 UAC, 一次性)。
  **三条不能删的工序** (pomodoro 各自炸过): ① **`resources.pri` (MakePri)** 缺了
  任务栏图标垫默认蓝底 (它排查一整轮的真因: BackgroundColor、DefaultTile/SplashScreen、
  scale 家族、清图标缓存、脏透明像素**全无效**); ② manifest 必须**无 BOM** +
  `Assets` 目录名与引用**同大小写** (前者装不上, 后者图标落回蓝色占位块);
  ③ 运行时资产必须随包 (`assets/logo/log_{16,256}.png`, 引擎经 `asset::resolve` 读)。
  **真机侧载实测通过**: 装进 `WindowsApps`、启动正常、托盘图标装上、无资产告警。
  **两条实测结论**: `explorer.exe shell:AppsFolder\...` 在本机**静默不启动**
  (要用 `Start-Process`); 本机 `WindowsApps` 的 ACL 有**非默认的当前用户完全控制** ——
  所以「exe 旁写得进」是**这台机器的特例, 别当普遍事实**。
  **连带还掉一笔农场欠账**: 框架日志目录加 `%LOCALAPPDATA%` 兜底 —— MSIX 下 exe 目录
  只读 + CWD 是 System32, 原两级**全落空**, 商店版**一条日志都没有** (pomodoro 8-01 就记了
  这条待办, 挂了一个半月)。**验证手法值得记**: 本机第一级居然能成功、新分支走不到,
  于是**用一个同名文件占住 `logs` 位置**逼出降级路径 —— 日志如期落到包私有目录。
  框架 `db050de` / 本仓 `5cd220b`。
- push 状态 (2026-09-13 当日末): **三仓 working tree 干净、`ahead=0`**。
  danqing `dev` 当日推 7 笔 —— 控件主题绑定 / 半透明表面守卫 / 选区带 30%→20% /
  **浅色 `surface_variant` (D1)** / **表头面与斑马撞车 (甲)** / **选区带那句补实测** /
  **日志目录加用户数据兜底 (MSIX 商店版)**;
  本仓 `dev` 同步推完 (色板+重钉 / 待裁立档 / 验收记录 / D1 落档+重钉 /
  **面阶梯 + D2 两批** / **MSIX 打包链路 + 版本号 1.0.0**)。
  本仓 lock 钉 `danqing#db050de7`。
  **仍守「未获用户指示不 push」** —— 上面每批都是用户逐项点头后才推的, 不构成默许
- 2026-09-13: **UI 视觉重构立项** (interview-me 收敛, 用户显式 yes) —— 意图
  `docs/intent/ui-redesign.md`。要点: 好看到「一眼像个正经工具」/ **含布局** / 浅色暗色都做 /
  **密度不许降**(用户原话「不允许」) / **框架哪儿挡路动哪儿**(已授权) / 不加功能不引依赖 /
  **先出视觉方案给用户过, 过了再写码**。
  **定案 (2026-09-13)**: 暗色「一片灰泥」根因 = **双重 gamma 编码** —— 渲染目标强制
  sRGB (`render/mod.rs:160-165`) + `rect`/`text` 管线原样输出作者态 sRGB 值
  (`rect.wgsl:102`/`text.wgsl:68`), 硬件再编码一次。`Color` 在框架里有**两句互相矛盾的
  契约** (`layout.rs:10` 说线性 / `theme.rs:61` 说 sRGB 编码), GPU 通路与 WCAG 护栏
  **各信一句** → 护栏按 token 算出 14:1 全绿, 屏幕真实是 6.4:1。推算: 暗色背景
  `#191920` 显示成 (88,88,99), 正文 `0.12` 显示成 ≈97 —— **文字与背景差 9/255**。
  **用户裁定方案 B** (保留 sRGB 目标 + 在 GPU 边界补转换; A 会弄坏本来正确的
  image/background 管线)。产品侧另有半成品: 暗色 token 没接完 (12 条清单余 4 条),
  clear_color 仍写死浅色。
  **上轮失败教训** (`docs/SPEC-dark-theme.md` 产出当前暗色): 对比度数值全达标仍难看 ——
  **对比度合格 ≠ 好看**, 本轮判据是整屏观感
- 联动顺序: 见「依赖与联动」节 (2026-09-13 重写 —— 原措辞「danqing 先 push → 本仓 cargo update」缺了前提: **patch 默认关**, 改兄弟仓前得先 `cp tools/local-patch.toml .cargo/config.toml`)
- 测试基线: **115 绿** (51 lib + 56 main + 8 genlog), 2026-09-13 实测
  (含设置卡溢出守卫与 `VERSION_ROW_H` 同源两条、暗色语义色板的两条 AA 守卫、
  跨面阶梯守卫、浅色语义色 AA 守卫、**展开块整段铺一次守卫**;
  lib = expand/levels/open/search, main = view/main/settings;
  引擎 51 条随迁 `danqing-logfile`, 另有 `danqing-encoding` 10 条)。
  框架侧 **583 lib + 全部集成测试绿**
- POC 及格线不过则终止, 仓库转档案 (clipboard 先例); 余前提③ = 发布后首单外检

## 必读

- 意图 (为什么做/竞品裂缝/MVP 边界/开枪前提/定价锚): `../danqing/docs/intent/log-viewer-poc.md`
- 意图 (UI 视觉重构的约束与裁决): `docs/intent/ui-redesign.md`
- 框架规则: `../danqing/CLAUDE.md`
- 农场跨仓约定: `../CLAUDE.md`

## 仓库与分支

- 远程: `git@github.com:14uncle/danqing-log.git` (dev 与 origin/dev 同步, 无待 push 提交)
- 分支: `dev` 默认, `master` 发布基线 (全家同一模型)
- 本地 git 身份: 十四叔 <gwhun@qq.com> (建仓时已核对)

## Tech Stack

- Rust 1.85+, edition 2024 (工具链 stable-x86_64-pc-windows-gnu, rustup override 已设)
- UI 框架: danqing — git 依赖; 引擎: danqing-logfile — git 依赖。**两者都由 `Cargo.lock`
  钉住 rev (`source = "git+…#<sha>"`)**, 提交进仓库的 `Cargo.toml` **不含 `[patch]`**
- 引擎: memmap2 (mmap) + memchr (SIMD 行索引) + regex::bytes (全文搜索)
- 编译产物: 各仓独立 `target/` —— 2026-09-10 去掉 `../.cargo-target` 共享 (RustRover 多仓并发编译触发 race condition), `.cargo/config.toml` 中 `target-dir` 行已注释
- **本地联动 patch 默认关** (2026-09-13 定, 见「依赖关系」节): 要改兄弟仓时
  `cp tools/local-patch.toml .cargo/config.toml`

## 依赖与联动 (2026-09-13 定)

**提交进仓库的形态**: `Cargo.toml` **纯 git 依赖 (不含 `[patch]`)** + `Cargo.lock` 钉住
danqing / danqing-logfile 的 rev (`source = "git+…#<sha>"`)。外部克隆能构建, `--locked`
能复现。这是 2026-09-13 修掉的: 此前 `[patch]` 提交在 `Cargo.toml` 里、指向仓库外的
`../danqing`, **外人克隆直接构建不了** —— 与「GitHub 免费开源」的承诺直接冲突。

**本地联动 patch 默认关**: `.cargo/config.toml` 已 gitignore, 模板在
`tools/local-patch.toml`。要改兄弟仓、须本地改动即时生效时才开:

```
cp tools/local-patch.toml .cargo/config.toml     # 开
rm .cargo/config.toml                            # 关 (回默认态)
```

**为什么必须默认关 (2026-09-13 实测)**: patch 生效时 cargo 会把 `Cargo.lock` 里钉住的
rev **改写回 path 记录** —— 实测 `cargo test` 跑完 pinned → path, 且改回的正是 path 态。
(注意 `cargo metadata` **不会**改写, 拿它当验证会得出相反结论 —— 这次就先被骗了一次。)
path 态的 lock 给不了外人复现, 而 pinned 与「本地用未 push 的兄弟仓代码」**不可兼得**,
故取「默认关」。**代价记住**: 忘了开 patch 时, 兄弟仓的本地改动会**静默不生效** ——
改兄弟仓前第一件事就是 `cp`。

**联动落地链路** (动兄弟仓代码时):
1. 兄弟仓改完 → 三件套 → **先 push 兄弟仓** (rev 得先在远端存在, 否则下面 pin 不上)
2. 本仓 (patch 关着) `cargo update -p danqing` / `-p danqing-logfile` → 提交 `Cargo.lock`
3. 两仓分别提交, message 注明关联

## 结构

- ~~`src/logfile.rs` / `src/jsonl.rs`~~ — 引擎层 (mmap/行索引/搜索, 全部碾压主张) 与
  前提②引擎 (JSONL 检测/列发现/memmem 字段提取/字段过滤, 零 parse; serde_json 需
  preserve_order 保首见列序)。2026-09-10 拆为兄弟 crate `danqing-logfile`, 经 `lib.rs` 的
  `pub use danqing_logfile::{jsonl, logfile}` re-export; **2026-09-12 删除仓内残留副本**
  —— 拆分时漏删, 两份 2266 行已与兄弟 crate 分叉, 且 47 个测试静默不跑 (无 `mod` 声明 = 无人编译)
- `src/main.rs` + `src/view.rs` — GUI (行锚定虚拟视口, 不用 Scrollable: f32 像素偏移在 2 亿像素域失真, 见 view.rs 模块头; 表格模式四区 = 过滤栏/表头/虚拟化行/状态栏)
- ~~`src/encoding.rs`~~ — 编码检测/转码 (2026-09-10 独立为兄弟 crate `danqing-encoding`, danqing 通过 `pub use danqing_encoding as encoding` re-export)
- `src/search.rs` — AsyncJob (worker+tick拾取泛化) + SearchNav 命中导航
- `src/levels.rs` — **级别分类与计数** (level-histogram 纯逻辑层): 6 桶分类器
  (行口径 `classify_level` 子串 / 字段口径 `classify_field_value` 前缀 —— **两者有意不同**,
  各自与自己的可点行为对齐)、`LevelCounts`、`count_ranges` 并行骨架、level 类列识别与点选子句
- `src/histogram.rs` — **级别计数侧栏组件** (6 行 + 对数横条 + 点选); 是 LogView 的
  **sibling** (`Row[Histogram(Fit), LogView.fill]`), 不侵入 LogView 坐标数学
- `src/settings.rs` — 轻量设置卡 (scrim 遮罩 + 玻璃卡: 关于/版本/反馈/主题下拉)
- `src/config.rs` — 配置持久化 (**整个 config.toml 的单真身** `Config { theme, histogram }`;
  `dirs::config_dir()/danqing-log/config.toml`)。**写入必须整文件同源** —— 分头写会让
  「改主题」抹掉侧栏开关; `load_from/save_to` 接路径参数供测试用 (不碰用户真配置)
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
