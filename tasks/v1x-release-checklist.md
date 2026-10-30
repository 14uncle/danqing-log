# v1.x 发布检查单（Paddle 权限落地后零思考执行链）

- @author 十四叔
- @date 2026/10/08
- 状态: **已触发（2026-10-23 权限落地），发布日定档 2026-10-28（用户 10-27 裁定）**
- 上游: `docs/intent/v1x-release-window.md`

## 阶段 0：权限落地前（空窗期现在就能做）

- [x] 版本号裁定 = **1.5.0**（2026-10-08 裁 2.0.0 → **10-09 用户改判 1.5.0**，与仓内「v1.x」代号同族）—— 阶段 1 时同步：`Cargo.toml` + `Cargo.lock` + MSIX manifest（四段式 `1.5.0.0`）；`package_portable.ps1` 从 Cargo.toml 自动读版本。对外物料（release notes / 商店文案 / 截图内版本行）一律 1.5.0
- [x] 商店 add-on 图标重制 300×300 —— **已产** `release-archives/log/store/addon-icon-300.png`
      （`tools/export-logo.py` 加了一档 300 输出，SVG 同几何 4× 超采样；重跑脚本零 churn 已验）
- [ ] add-on 在 Partner Center 预先创建（硬顺序「父应用发布之后」已满足 ✅；能否不随应用版本单独提交【待核】）—— **用户动作**
- [x] release notes 起草 —— **真身 = `docs/release-notes-v1x.md`**（10-07 既有草稿；10-09 修三连分层错置 + 回填版本号 1.5.0；10-08 误建的重复稿 `release-notes-v2.0.0-draft.md` 已删 —— 「写新文档前先全局搜」同日第二次犯）
- [x] 商店文案三项改动预起草 —— **已产**：`docs/ms-store-copy.md` 新增「§6. v1.5 提交物料预起草」
      （产品功能 13 条 / 免费节改写 / 专业版段 / IARC 改答 / add-on 字段；**add-on 价格档待裁**；
      隐私政策正文已是 1.x 版，「适用版本」天然覆盖 1.5.0，**版本行免改**）

## 阶段 1：权限落地 → 可购买（预计半个工作日）

- [x] Paddle 后台生成 checkout 链接（Personal $29 / Business $59 两产品）—— **2026-10-27
      建成**（hosted checkout 表单：Redirect 必填 = `14uncle.github.io/thank-you`，
      早鸟折扣表单直挂；链接存档 `docs/reseller-setup.md` §一·2，国内实测均 200）
- [x] 回填 `PURCHASE_URL`（`src/license.rs`；显隐判据 `src/settings.rs:341` D8）——
      **2026-10-27 已回填**，D8 反向锁翻正向，521 绿
- [x] 早鸟折扣 expiry 设为发布日 +1 个月（「首月或前 50 单先到为准」，2026-10-07 裁定；
      Uses left 50 已设）—— **10-28 已设 `2026-11-27T15:59:59Z`（= 北京时间 11-27
      23:59:59）**：用户点名「你代劳」后走 API PATCH（钱路写明示点头合规）；
      两折扣（−$10 personal / −$20 business）均 active、50 次限额、已用 0
- [x] 三件套：`cargo fmt` / clippy 0 / 521 测试绿（基线见 CLAUDE.md，别拿旧数）——
      **10-27 复跑绿（含版本号 1.5.0 同步后）**；发布打包前再跑一次
- [x] 手动签发运营链自检：邮件模板（`docs/specs/SPEC-v1x-licensing.md` §二定稿）+
      **WeChat Pay 延迟捕获 ~10 分钟 → 以 transaction 完成态为准再签 key**（`docs/reseller-setup.md` §一·6）

## 阶段 2：打包与自验

- [x] 便携 zip：`powershell -NoProfile -File tools/package_portable.ps1` + sha256 ——
      **10-28 已产** `danqing-log-v1.5.0-win-x64.zip`（4,988,608 B <5MB 红线内，
      sha256 `801c975c…`）
- [x] MSIX：`build_msix.ps1` → `sign_msix_local.ps1`（sidecar 签名后重算逻辑已有）——
      **10-28 已产已签** `danqing-log-store-v1.5.0-x64.msix`（v1.5.0.0，签名后
      sha256 `838f8dd4…` 已回写 sidecar）
- [x] 两包 exe 与 `target/release` 三方同哈希核对 —— **10-28 过**：三方同
      `fc48fedd0d8535e2`
- [ ] 本机侧载 + 便携版实机：购买入口显隐（D8）、key 激活、付费门控三道 ——
      **侧载已装（1.5.0.0 顶掉 1.0.2.0）**；实机三道 = **用户动作**

## 阶段 3：发布

- [x] dev → master 合并 + tag（发布点打，不提前 —— 死标签教训）—— **10-28 已落**：
      master `b17a2e4`（--no-ff）+ tag `v1.5.0`，均已推
- [x] GitHub Release（正文 = release notes 去文件头元信息段）—— **10-28 已发**：
      `github.com/14uncle/danqing-log/releases/tag/v1.5.0`，双资产齐
      （zip + .sha256）
- [x] **README 购买节补双链接 + 落地页购买按钮解注挂链** —— **10-28 已公开**：
      README 随 master 合并上线；pages 仓 `698f3f4` 已推（线上实测 2 条
      pay.paddle.io 链接在页）。**开业钟 = 此刻起算**
- [x] MS Store 提交：照 `docs/ms-store-copy.md`「v1.x 上架时必须改什么」逐项核
      （IARC 改答「是」→ 分级标签全变 / 隐私政策贴**新版**文本 / 文案三处 / add-on 图标 300×300）
      —— **2026-10-30 已提交认证**（Submission 4：包 = `danqing-log-store-v1.5.0-x64.msix`
      已 Validated / 年龄分级按「可购买数字商品=是」重答 / Store 一览四处改写 =
      说明全文重写 · 新增功能 · 产品功能 9+4 条且删「不联网」· 简短描述补填 /
      发布时机 = 通过认证后立即发布）。**2026-10-30 认证通过 = 商店版 1.5 已上架**
      （提交 4，距提交当日；线上商店版自此有付费入口）。**余 EN-US listing 待认证通过后
      单独提交**（要同时披露「界面为简体中文」）—— 许可条件已满足，可起
- [x] 商店 add-on 提交 —— **2026-10-30 已过审上架**：Offer ID 逐字 = `danqing-log-full`
      （对齐 `src/store_license.rs:32`）、耐用品、**$29.99 USD**、图标 300×300、
      Store 一览名称「丹青日志 LogLens 专业版」+ 描述。**注意：空的「提交 2」草稿勿建勿填**
      （加载项 listing 只有名称/描述/图标三项，父应用的说明不套用）

## 阶段 4：发布后外检（前提③开门）

- [x] 真付费链路验收：代销 key 真购买 → 激活（便携版通路）—— **10-28 自购单
      全链走通**（微信付 $19 早鸟 → transaction completed → keygen → 免费态激活
      → 付费态；收据邮件正常送达。剔除出首单外检）
- [x] 商店 add-on entitlement 链路验收 —— **2026-10-30 全链通过**（四步全过，
      三方互证见本节末）。**两项前置均已满足**（add-on `danqing-log-full`
      10-30 上架 / 商店包 v1.5 **10-30 认证通过已上架**）。**四步，第 4 步才是判据**
      （代码路径已核，`src/app_license.rs` + `src/settings.rs:345` + `src/store_license.rs`）：
      0. **先把 `%APPDATA%\danqing-log\license.key` 挪开** —— 2026-10-30 实测抓到：
         `initial_entitlement()`（`main.rs:713` → `license::load()`）**没有 `is_packaged()`
         门**，商店版**照样读便携版那份 Paddle key**。不挪开则一启动就是「付费层 · 个人版
         (g***@qq.com)」= `PaidSource::License`，商店通路**一步都没走到**；且
         `purchase_paid_layer()` 首行就短路成「你已是付费层，无需重复购买」，
         连购买入口都点不进去。挪开法是改后缀保留原文件，验完改回
      1. **先卸侧载版、再从商店装** —— 本机现装 10-28 侧载的 `14uncle.LogLens_1.5.0.0`，
         与商店包**同身份同版本**：侧载在装则商店不给更新；且侧载包的 broker 授权态
         与商店分发包不是一回事。要验的就是客户拿到的那一份（**10-30 已执行**：
         WindowsApps 里已是商店装的 `1.5.0.0`，见下条日志）
      2. 许可页显示「**当前：免费层 —— 单文件全功能，永久免费**」。
         ⚠️ **「获取付费层」按钮不是判据** —— 判据 `show_purchase_button()` =
         `PURCHASE_URL.is_some() || is_packaged()`，商店版**恒真**，add-on 在不在目录都显示
         （此前检查单写的「解锁专业版」是误记，实名就是「获取付费层」）
      3. 点「获取付费层」→ 商店购买框弹出 = `find_full_version_product` **真在目录里
         找到了** `danqing-log-full`（找不到只在日志留 `付费层 add-on 不在目录中`）。
         买 **$29.99 USD** → 卡内反馈「已解锁付费层，感谢支持」
         —— **10-30 17:49 弹框已实证**：标题「丹青日志 LogLens 专业版」、
         **¥214.00**（= $29.99 的中国区当地价，汇率≈7.14）、销售商「百视通网络电视
         技术发展有限责任公司」（微软中国 MoR）、支付宝可选 → **目录匹配 + 购买 UI
         属主挂钩 + 定价三件全过，链路只差真付款**。界面写「恕不退款」，
         实际微软客服可退，不是不可逆
      4. **重启应用** → 启动后台 `query_snapshot` 遍历 `AddOnLicenses` 命中该 token →
         许可页仍显示「**当前：付费层 · 商店内购**」。
         **只有这步算数**：第 3 步成功后是本会话内乐观置位（`adopt_purchase_outcome`
         直接写 Paid），**完全不经过读取通路**，通过它什么也证明不了
      **取证点（2026-10-30 现场标定，别按猜的路径找）**：商店版的日志**不在**
      `%LOCALAPPDATA%\danqing-log\logs`，MSIX 把写路径虚拟化进了包私有区 ——
      真实位置 `%LOCALAPPDATA%\Packages\14uncle.LogLens_3y3rwcp1ep416\LocalCache\Local\
      danqing-log\logs\`（日志首行自报的那个路径是**虚拟化前的样子，找不到别以为没日志**）。
      查什么：`store add-on license: active=… expires=…`（info，命中 add-on）/
      `store 授权查询失败`（warn，broker 报错 → fail-open 到免费态）/
      `付费层 add-on 不在目录中`（warn，只在点购买时出现，= 目录里没这个 token）。
      **注意「没命中」是静默的** —— 未持 add-on 时 `query_snapshot` 直接返回免费态快照，
      一行日志都不留，故「查不到那个 info 行」既可能是没买、也可能没跑到，
      得靠同份日志里的旁证分辨（见下条）。
      **broker 本身已实证可用（10-30）**：同一份日志有
      `[danqing_log::app_update::store] 商店更新查询: 待装更新 0 个` —— 那是
      `StoreContext::GetDefault()` + 一次商店 API 调用成功返回，与 `query_snapshot`
      起手式同一套（`RoInitialize` + `GetDefault`）。所以商店轨读通路的**前置没坏**，
      缺的只是 add-on 所有权本身
      **已知接受项（不修）**：`ExpirationDate()` 取不到时 `None → Paid` —— 该 fail-open
      只在 `IsActive` 已为真（= 已购）时生效，不会给未购者解锁；我们不卖试用 add-on，
      不影响。**这是自购单，须记时间戳剔除出 11-27 首单外检**
      **验收结论（2026-10-30 17:53，三方互证）**：① 重启后**新进程**
      `175312.940-p37528` 日志 `17:53:12.969 INFO [danqing_log::store_license]
      store add-on license: active=true expires=Some(253402214400)`
      —— `AddOnLicenses` 遍历命中 `danqing-log-full`、`IsActive` 真、
      过期 = 公元 9999（durable，≥ `DURABLE_THRESHOLD_EPOCH`）→
      `map_store_snapshot` 判 `Paid { StoreAddOn }`；② 许可页「当前：付费层 ·
      商店内购」；③ 页内反馈行**已清空** = 付费态来自读取通路，不是会话内乐观置位。
      **前提③的商店侧通路至此打通**（只差陌生人的钱）。记账落
      `tasks/calibration-30d.md` 自购验证单。**收尾动作**：`%APPDATA%\danqing-log\`
      里给测试挪走的 `license.key.paddle-backup-20261030` 要改回 `license.key`
      —— 不改回则**便携版变免费层**（Paddle key 是真身，商店授权不进便携版）
- [ ] **跨渠道授权不对称（2026-10-30 查出，待裁）**：两个方向**只通一边** ——
      Paddle key → 商店版**通**（`license::load()` 无条件读，即上面第 0 步踩到的）；
      商店 add-on → 便携版**不通**（`query_snapshot()` 在 `!is_packaged()` 直接返回免费
      快照）。后果：在商店买了 $29.99 的人，之后下载 GitHub 的便携 zip 只拿到免费层。
      **文案风险**：`docs/eula.md:52`「换机自由」讲的是 Paddle key（成立），
      分发稿只说「双通道下载 + 买断 $29」，**没有一句承诺跨渠道通用** —— 现有文案
      没错，但读者可能自己这么理解。三选一（待裁，不阻塞推送）：
      ① 不处理，认了（跨渠道各结各的）；② 文案加一句「两条渠道各自结算」；
      ③ 日后做「商店购买 → 便携版解锁」的导出/兑换流程（真做要服务端，与无后端架构冲突）
- [ ] **触发分发推送**（`docs/distribution-wave1-copy.md`，六渠道成品已备）——
      **商店版 1.5 已上架，稿件里的「1.5 正在认证中」一句须按现状改写**（见该文件 §0①）

- [x] 校准钟回填：10-18 GitHub / 10-21 商店（`tasks/calibration-30d.md`）——
      **双侧已回填**（GitHub 10-28 / 商店 **10-30**，均逾期登记流程事故）。
      商店 39 天实测 = **43 浏览 / 8 安装 / 18.6% 转化 / 成功率 100%**，
      稳态 **≈1.1 浏览/天 · ≈0.21 安装/天**（判读：商店是通道不是流量源，
      与 pomodoro 首月结论一致）。**余市场分布待拉**（i18n 判读输入）
- [ ] round8 复活条件检查：首单外检落地 or 10-21 校准钟 → 选型机器合法开机评估
