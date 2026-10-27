# 代销收银台：选型 + 注册提现流程（GitHub 轨 · 中国大陆个人卖家）

- @author 十四叔
- @date 2026/10/07
- 状态: **终选 = Paddle**（2026-10-07 用户实测 paddle.com 可达；接受个人卖家、
  CNY payout 不经 PayPal）。**执行中**（进展见 §一·1 末「执行进展」：注册 / 产品 /
  折扣 / 落地页 / 身份验证已完成；**域名审核 10-08 已过**（Checkout +
  Apple Pay 双徽标）；**hosted checkout 权限已获批（2026-10-23 Ivan 回信：
  「Hosted Checkouts have been enabled for your account」）**，账号资料两项
  同日确认已改（升级触发点 10-21~22 未走到即被回信解钟，不升级；等钟空窗
  清单 §七 随之作废，仅③ key 轮换仍有效）；payout 绑卡已完成
  （10-08 CNY 电汇）；**Checkout settings 已配保存**（10-08，§一·6））。
  平台政策时效性强，执行时以官网为准。
- 上游: `specs/SPEC-v1x-licensing.md` D2 / §8 待裁 · `ROADMAP-v1x.md` §三 ·
  `../tasks/todo-v1x-licensing.md` 末节用户侧清单

---

## 〇、选型终局（2026-10-07 两轮核实）

| 平台 | 大陆卖家 | 提现通路 | 费率 | 国内可达 | 判 |
|---|---|---|---|---|---|
| **Paddle** | ✅ 官方 unsupported 列表不含中国；**接受个人卖家**（Individual，无需公司主体） | **CNY 电汇 / Payoneer**（$100 起，不经 PayPal） | 5% + $0.50 全包（MoR 含税/拒付） | ✅ **用户实测可达** | **终选** |
| Creem | ✅ 官方文档明列 | 支付宝（个人单笔 ≤5 万 CNY） | 3.9% + $0.40 | 未实测（不需要了） | 备选存档 |
| Gumroad | ✅（仅 PayPal） | PayPal 2% → 电汇 $35/笔 | 10% + $0.50 | ❌ 主站被墙（用户实测） | 有稳定代理才可行（§四 存档） |
| Lemon Squeezy | ❌ 银行 payout 无大陆；Stripe 迁移不收大陆卖家 | — | — | — | 出局 |
| Payhip | ❌ 非真 MoR，payout 走 Stripe/PayPal | — | — | — | 出局 |

**Paddle 实收重估**（$29 单价）：29 − 5% − $0.50 = **$27.05（93.3%）**；
payout 货币 FX margin 1.5–3%（USD→CNY）→ **实收约 90–92%**
（优于 Gumroad 的 82–83%，略低于 Creem 的 94.7% —— Paddle 的成熟度、
审核信誉、CNY 直汇值这个差价）。隐形成本两条记住：某些货币/银行组合
$15 SWIFT 费；**退款不退 Paddle 抽成**（买断制工具退款率应低，接受）。

**共同前提（不因选型变）**：key 分发走**手动签发模式**（§二）——Paddle/
Gumroad 原生 key 均不兼容我们的 Ed25519 离线格式；D2 设计与隐私政策
「便携版零网络」零改动。

## 一、Paddle 流程（主流程）

### 0. 前置：EULA 落盘（注册 Paddle 之前先做）

Paddle 三段审核的第一段 = **Domain Review**，要求 website 有 **ToS 与
Privacy Policy 页面**。材料合并方案（一件事办两个裁决）：

- **EULA 写进 `docs/eula.md`**（公开仓库 → 天然有 URL）—— 内容即 09-27 裁决的
  身份门：免费层限个人/非商业、商用购 $59 企业档、君子协定措辞；同时它就是
  Paddle 要的 ToS
- 隐私政策已有 `docs/privacy-policy.md`（公开 URL 现成）
- README 加这两个链接（购买节同窗口补）
- website 就填 **GitHub 仓库 URL**（独立开发者过审先例存在）
  - **2026-10-07 执行勘正**：注册资料填仓库 URL 可行，但后台 **Add domain 只收
    裸域名**（不带协议/路径；`github.com` 非自有域名必拒）→ 退路兑现：
    **GitHub Pages 落地页已建成** `https://14uncle.github.io`（用户级 Pages 仓
    `14uncle/14uncle.github.io`，main 根目录即服务；含产品/定价/下载 +
    ToS/隐私/退款三链，政策正文只链接不复制防漂移）；页面资源一律**自托管**
    （`raw.githubusercontent.com` 国内不可达，外链 logo 裂图已修）

### 1. 注册与审核（网页 ~20 分钟 + 审核 48 小时–数天）

1. `paddle.com` 注册，Business Type 选 **Individual**（个人卖家，无需公司主体）
2. 三段审核：
   - **Domain Review**（1–2 工作日）：查 §0 备好的网站材料
   - **Business Identification**：真实姓名拼音（与身份证件、银行账户三者一致）、
     地址、产品描述（写清：桌面日志查看工具 LogLens，一次性买断 $29/$59）
   - **Identity Verification**（2–3 工作日）：走 Sumsub —— 护照或身份证 +
     地址证明 + 可能活体自拍
3. 被驳回按邮件原因补料重提（常见死因：网站缺 ToS/隐私政策 → §0 已防）
4. 税务信息按 onboarding 引导填（Paddle 为 MoR，终端销售税由它承担；
   你与 Paddle 之间按其指引完成税务声明）

**执行进展（2026-10-07）**：

- ✅ 注册完成（Individual；Business name Danqing）
- ✅ 产品 ×2 建成（Personal $29 / Business $59，one-time，英文描述）
- ✅ 折扣 ×2 建成（§4 表；各 Active、Uses left 50，**expiry 未设——发布日再设**）
- ✅ 落地页 `14uncle.github.io` 上线 + **域名审核已通过**（10-07 提交 →
  **10-08 过审**，Website approval 双徽标：Checkout approved + Apple Pay
  approved；注意每域名/子域名单独审——`14uncle.github.io` 覆盖本账号全部
  产品，与 §一·5 账号级约定吻合）
- ✅ 身份验证（Sumsub）**已通过**（10-07 当日过审）：身份证件 + **租房合同**
  作地址证明；入口 = 后台 **Get started → 02 Verify your account**
  （onboarding 页内直接开始，**不必等邀请邮件**）——地址证明踩坑见文末记
- ✅ **Hosted checkouts 权限（2026-10-23 已批）**：live 账号的 hosted checkout 只开放给
  「app-to-web 销售漏斗 / 桌面应用嵌入」（防被当免审核通用网店）——
  Settings → Hosted checkouts → **Request access**（mailto: sellers@paddle.com；
  机器无邮件客户端时右键复制链接、网页邮箱手动发，直接点会打开空白页）。
  话术核心 = **桌面应用 + 应用内许可页按钮打开浏览器购买**（我们恰好是
  允许的场景）；10-07 已申请，自动回执称 1–2 工作日回复。**别重复发件催**
  （官方明示重复工单反而拖慢）。**10-08 真人 Ivan 回复**「refer this to my
  team」= 已转交团队，**非最终答复**，继续等钟；同线程捎带账号资料两项
  修改请求（§五 末项，草稿已交用户）。**两道闸独立实证（10-08）**：域名
  过审（双徽标）后 Hosted checkouts 黄条仍在、产品菜单无 link 生成入口——
  Website approval 是前置不是解锁，别把域名翻绿误读成能生成 link 了
  - **2026-10-12 状态复核**：自 10-08 Ivan「转交团队」起**仅过 2 个工作日**
    （10-09 五 + 10-12 一，周末不计）；页面黄条不变**属预期**（权限未批前
    恒显，非进度指示，不构成信号）。**先别发**——10-08 那封（感谢 + 账号资料
    两项 + 「期待答复」）已算一次跟进，隔 2 个工作日再追偏急。**跟进触发点 =
    2026-10-14（周三）**：届时仍无回音 → 在**原线程**回短跟进（同线程 ≠
    新工单，不违「勿催」——那条防的是**重复开新票**），草稿见下「10-12 跟进
    草稿」。再等 3–4 天无果 → 换 Paddle 后台 support 入口升级，不再堆同线程。
    **先翻垃圾箱**：转交后团队回复走 `sellers@` 别名，QQ 邮箱规则易吞。
    **解耦提示**：本闸只锁 Paddle 轨，不锁商店 add-on 轨（§四·5 / §六）——
    首单外检不必死等此权限
  - **2026-10-16 状态复核**：10-14 触发点**错过未发**（用户确认）；后台
    Hosted checkouts 黄条依旧（同预期，非信号）。全沟通线程存档 =
    农场根 `docs/paddle_desktopapp_reply_1009.pdf`（10-09 导出，图片型）：
    时间线 = 10-07 申请 + 自动回执 → 10-08 04:23 Ivan「refer this to my
    team」→ **10-08 16:21 我方最后一封**（致谢 + 账号资料两项 + 期待答复），
    其后零回信——**PDF 即全部沟通，无 10-09 新消息**。处置 = **10-16 当日
    补发原线程短跟进**（草稿见下「10-16 跟进草稿」，较 10-12 稿加一笔
    **域名已过审**新事实）；**升级触发点 = 发出后 3–4 个工作日 ≈ 10-21~22**：
    仍无回音 → 换 Paddle 后台 support 入口升级，不再堆同线程。发前实核
    Account settings 两项资料（product website / display name）**10-16 23:32
    均未改**（display name=Not set / website 仍旧 repo 地址，页面底注「Paddle
    sets these details」证实正属工单管辖）→ **跟进须带催改行**（草稿已并入）。
    **跟进已于 10-16 当晚发出**（QQ 邮箱原线程回复 sellers@paddle.com，
    正文 = 下方定稿，含域名过审 + 催改行两笔）；**升级触发点 = 2026-10-21~22**：
    仍无回音 → 换 Paddle 后台 support 入口升级（届时不再堆同线程）。
    静默期间收信先翻垃圾箱（`sellers@` 别名 QQ 规则易吞）
  - **2026-10-23 回信解钟（终局）**：Ivan 原线程回复三笔——①**「Hosted
    Checkouts have been enabled for your account」= 权限批复本体**；②「I've
    updated your Account Settings as requested」= 10-08 捎带的账号资料两项
    （product website / display name）落实确认；③静默原因 = signups 激增
    响应变慢（道歉非搁置）。**处置：不升级、不再发件，工单线程就此收官**。
    **下一棒 = 生成 checkout link 回填 `PURCHASE_URL`（§五 机械链）**：
    Catalog → Products → LogLens Full — Personal → 生成 checkout/payment
    link（**挂 `early-bird-personal` 折扣自动应用**，§一·4）→ 给我回填
    `license.rs`；Business 档链接顺手一并生成（README 购买节用）。
    链接域名实样应为 `pay.paddle.io` 族（§一·2 勘误口径）；挂落地页/README
    前两族都测国内可达。onboarding 02 翻 Complete 的判据不变：链接生成 →
    挂落地页/README → 回填（§7.2）。05「Test and go live」= 真购买验证，
    自己下一单 $29 走通签发→激活后自然翻牌（§五 倒数第二项）
- ✅ payout 绑卡（2026-10-08 表单已保存，Wire transfer / CNY）：**Business account →
  Payouts → Payout Settings**（左栏 Business account 展开；绑卡**不在**
  Account settings——那个页面没有 Payout 区块）。方式 = Wire transfer，
  币种 **CNY 首选**（§一·3 既定：首笔试走看实到；小额起步避开 USD 电汇
  中转行 $10–25 级按笔费，CNY 只付 Paddle 换汇 margin 1.5–3% 且人民币
  直接入账不碰结汇额度；若要 CNAPS 联行号已从 APP「开户行查询」备查）。
  **USD 兜底已备齐**（10-07 建行官方「外汇境外汇入汇款途径」拿到）：
  收款行 SWIFT = 北京分行级 `PCBCCNBJBJX`（支行无独立 SWIFT，靠卡号
  落账到户），中转行 BoA/花旗/摩根大通纽约三路由打款方自选（表单无此栏，
  Notes 空着）；到账 = 美元现汇，建行 APP「结汇」换人民币（占个人年度
  5 万美元便利化额度）。收款人名 = 拼音与 Paddle 注册实名逐字一致
  （惯例姓在前全大写，校验不匹配再调顺序）。银行侧前置：跨境 KYC 问卷
  **已提交（10-08**；关键题「境外上游资金 = 是」）。卡号/支行等敏感信息
  不落库，原件存仓库外
- onboarding 页另有三步：01 Set up your live account（In progress，
  可能含 payout/税务余项）/ 02 Verify / 03 Test and go live（真购买验证在这步）

**10-16 跟进草稿（触发点已过, 当日补发; 原线程回复, 勿开新票）**：

```
Hi Ivan,

Following up on this thread — any update from the team on the hosted
checkout access, or anything further you need from me?

One update since my last message: the domain review for
14uncle.github.io has been approved (Checkout + Apple Pay), so the
website side is now fully set up on your end.

Also, the two account profile corrections from my Oct 8 email don't
seem to be applied yet (product website / display name) — could you
check those as well?

Quick recap: LogLens is a Windows desktop app whose in-app upgrade
screen opens the Paddle hosted checkout in the user's browser (an
app-to-web funnel for a desktop app; $29 / $59 one-time purchases; no
web storefront).

Thanks,
Weihun Gan
```

（较 10-12 稿两笔: ①「域名已过审」= 催问带新事实; ②催改行——10-16 实核
两项资料未改, 已并入正文。）
（~~10-12 稿~~ 已被本稿取代, 留档不删。）

**地址证明踩坑记（2026-10-07，Sumsub 三投三拒后当日过）**：

- Sumsub 要的是「**近 3 个月内发出的信件类文件**」（账单 / 合同 / 证明信），
  不是「证件」——北京电子居住证三投三拒：①照片质量 ②截图上的查询日期
  超 3 个月（被当文件日期）③不收屏幕截图、要实时照片 —— 结构性不匹配，
  不是拍摄问题，别再跟居住证耗
- **租房合同（tenancy agreement）是大陆租房个人卖家最顺的路**：姓名 +
  地址 + 日期俱全，拍关键页（双方姓名 / 房屋地址 / 签名页）一次过
- 支付宝生活缴费凭证**不收**：无姓名 + 住址部分打码
- 储蓄卡（建行/招行）流水与存款证明**不带住址**；银行路线 = 信用卡账单
  或柜台开带地址的「客户信息证明」盖业务章
- 姓名拼音三方一致（Paddle 注册 / 证件 / 银行开户名）是审核常卡三对口子

### 2. 产品配置

| 产品 | 定价 | Product ID | 说明 |
|---|---|---|---|
| LogLens 完整版 · 个人 | **$29** one-time | `pro_01m49tcq5qn3z6xnj2qbhg11r4` | EULA 限个人/非商业（君子协定） |
| LogLens 完整版 · 企业 | **$59**/seat one-time | `pro_01m49v7r4f8n4sdgye3q53ese7` | 商用；座位数不技术强制 |

（Product ID 2026-10-17 从 Taxable categories → Manage 页抄录；
taxable category 两产品均挂 **Standard digital goods**，见 §7.3。）

- 类型选 one-time（Paddle 支持一次性/lifetime 售卖；$29 单价在 $0.50 固定费甜区）
- **不开**任何平台自家 license key 功能（手动签发，§二）
- 生成 checkout / payment link —— **个人档那条 = 回填 `license.rs` 的
  `PURCHASE_URL`**；发布前实测 checkout 域名国内可达性（2026-10-17
  勘误：文档实证域名族 = `pay.paddle.io` / `sandbox-pay.paddle.io`，
  旧记 buy.paddle.com 以生成链接实样为准、两族都测），国内买家兜底 =
  MS Store 轨
- **New hosted checkout 表单实证（2026-10-27 创建时截图）**：字段 = Name /
  Description(选填) / **Redirect url（必填**——不是文档暗示的可选，空着报
  「Redirect url required」**）** / Default prices（选产品+价格）/
  **Default discount（表单直挂，选了就不用拼 `?discount_id=` 尾参）**。
  Redirect 填 `https://14uncle.github.io/thank-you`（专用感谢页，人工签发
  24h 预期 + 激活指引 + 退款入口，pages 仓 `thank-you/index.html`）
- **两条链接已建成（2026-10-27，国内实测均 HTTP 200 <1.2s）**：
  Personal（$29，挂 early-bird-personal −$10）=
  `https://pay.paddle.io/hsc_01m5z030nhbajgmkj987wmbva9_c85re6taregcexaf8nzvc980w8np56r0`；
  Business（$59，挂 early-bird-business −$20）=
  `https://pay.paddle.io/hsc_01m5z04yns6220j7jp385fdpwb_7fy2bhgj7jdyfq2z76gg3rb1rjs0nnw1`。
  Personal 已回填 `license.rs` `PURCHASE_URL`；**README 购买节与落地页按钮
  按既定裁决挂到 v1.5.0 发布日再公开**（落地页占位注释「v1.x 发布日启用」），
  开业钟与早鸟 expiry（开业日 +30 天）届时一并设
- 描述写清两句：① 买断制永久使用、含本大版本全部更新；② **key 经邮件人工
  交付，通常 24 小时内**

### 3. Payout

- 方式：**电汇（支持 CNY payout 货币）** 或 Payoneer；最低 $100
- 成本：FX margin ~1.5–3%（USD→CNY）+ 某些组合 $15 SWIFT 费 —— **首笔走 CNY
  电汇，看实际到帐金额再定长期路线**；银行账户开户名与 Paddle 注册名一致
- 放款节奏与持有期以 Paddle Billing 后台当前口径为准

### 4. 早鸟折扣（2026-10-07 用户裁定：标价不动 + 首发早鸟价）

标价 $29/$59 不动，开业首月**或前 50 单**（先到为准）早鸟 **$19/$39**。
**2026-10-27 重申（用户再裁 go）**：hosted checkout 权限批下后用户一度提议
压至地板价（理由：非自用领域不熟 / 对人工测试信心不足 / 目标只是走通付费
流程）——**驳回，维持原价 + 早鸟**。论据存档：① 走通流程靠自购单验证
（30 天退款兜底，净成本 ≈ Paddle 抽成不退的 ~$2），与公开价无关；② 价格
是首单外检实验的自变量，地板价卖出只证明地板价能卖，0 转化也分不清价不对
还是品不对（09-27「软腿 0 转化歧义」同族教训）；③ 「测试不充分」前提不实
——人工验收 26 条 09-27 实机全过，v1.0.2 下载 84 零 issue；④ 定价依据 =
09-27 竞品带调研（Modern CSV $39/$59 同构），不依赖所有者信心，判卷人是
市场；⑤ 价格棘轮不对称——公开价降易涨难，早鸟到期自动回正价正是为此设计
的回程票；⑥ 地板价会结构性污染及格线校准的算账基准（流量×转化×单价 vs
工时）。
Paddle 实现（Catalog → Discounts → Create discount，两个）：

| Discount | Type | Amount | 限制 |
|---|---|---|---|
| `early-bird-personal` | Fixed amount | **−$10** USD | expires = 开业日 +30 天；max redemptions = 50 |
| `early-bird-business` | Fixed amount | **−$20** USD | 同上 |

生成 checkout link 时把对应 discount 挂上（自动应用，买家不用输码）——
checkout 页会显示划线原价 + 折后价，锚点免费展示。到期/到量自动失效回正价。
**产品描述里不写早鸟**（会过期的文字不进常驻文案，防「清旧文字」事故）；
早鸟叙事放发布渠道（README / Release notes / 商店文案），届时随发布稿写。

### 5. 产品线共用约定（2026-10-07 定）

- **Paddle 域名审核是账号级**：`14uncle.github.io` 过一个，未来 farm01 全部
  产品经本账号销售共用此域名，新品只走产品级审核（轻），不用再审域名
- **Pages 仓布局**：一个用户级 Pages 仓装全产品线——根 = 当前唯一在售产品
  （现 LogLens 占根），未来产品各占一个子目录（`14uncle.github.io/pomodoro/`
  等）；没有第二件产品需要页面之前不重构根目录（外部旧链接成本）
- Store add-on 轨产品（番茄钟）用不上 Paddle（商店代收），它需要落地页的
  场景只是营销引流，与域名审核无关

### 6. Checkout settings 配置实录（2026-10-08 已保存）

Checkout → Checkout configuration → Settings 页签。核心概念先记一条（10-08
用户问答澄清）：**这页勾的是「买家能用什么付」，不是「卖家有什么账户」** ——
Paddle 以 MoR 身份代收全款，卖家回款只有 Payouts 里那条 CNY 电汇一条路，
与买家侧勾选完全无关（买家 PayPal ≠ 卖家 PayPal；选型规避的是后者）。

| 节 | 最终状态 | 依据 / 备注 |
|---|---|---|
| Payment methods | **开**: PayPal / Apple Pay / Google Pay / Bancontact / **WeChat Pay**；关: iDEAL / Pix / UPI / BLIK / MB Way / 韩国本地卡 / Naver Pay | 区域方法 Paddle 按买家地区自动显隐，不开的只是少个顺手选项。**WeChat Pay = 中国买家唯一通路**（官方：中国地址买家 + 桌面端 + USD/CNY + 一次性买断 ✅，无需卖家有微信商户号/中国主体）；注意它是**延迟捕获**（授权后 ~10 分钟才落账）——手动签发 SOP 以后台 transaction 完成态为准再签 key，天然容错。Alipay 不在 Paddle Billing 方法列表 |
| Saved payment methods | 开（Allow saved cards） | 订阅续费场景为主；买断制下仅省回头客输卡号，零成本 |
| Marketing consent | 开 | 买家 opt-in 进营销列表（默认文案「Danqing can send me product updates and offers via email…」）；买断 + v2 另议 + 产品线共用账号 → 买家邮箱列表是将来新品/大版本唯一直达渠道，合规收集 |
| Checkout discount | **关**（Allow buyers to add a discount code） | 早鸟挂 checkout link 自动应用、买家不输码；露出输码框诱发「搜码分心」流失。以后发补偿码/活动码再开 |
| Default payment link | `https://14uncle.github.io/checkout`（已过审域名，绿勾） | 官方口径：不设则创建 transaction 报 `transaction_default_checkout_url_not_set`。机制 = 该 URL 拼 `?_ptxn=txn_...` 且页面需嵌 Paddle.js 才弹 checkout。**我们的购买流不走它**（买家点 Paddle 托管 checkout link，buy.paddle.com 一族）——填上 = 拆潜在闸 + 满足表单，暂不为它在 Pages 站建页面 |
| Statement description | `LOGLENS`（原 `DANQING5` 改） | 账单描述符 2–10 字符全大写；买家认产品名，防「这是什么扣款」型拒付 |
| Recovery | Checkout recovery **开** / Apply discount **关** | 弃购挽回邮件 = 免费自动召回，与手动签发零冲突。挽回折扣不开：①早鸟挂着时它本就不发（每产品同时仅一折扣，页面写明）②防「弃购=拿折扣」训练效应，守住「标价不动」纪律 ③上线后看弃购数据再裁 |

Styling 页签（2026-10-08 勘误后口径）：官方明确 overlay checkout 只有**品牌色级**
微调、**无 logo 上传位**（深度定制须 inline + Paddle.js 集成，非我们路线；checkout
页头部只显示 business name = Danqing）——唯一要填的是 **Brand color (overlay) =
`#0F766E`**（框架浅色 accent，白底对比 ~5:1 实测值；若分模式填暗色则 `#1A9E8A`），
Fonts（默认 Lato）/ Focus state / General 维持默认。
**这页配完不解锁 hosted checkout 权限**（§一·1 工单等钟）——意义是权限一到
即可直接生成链接回填 `PURCHASE_URL`，中间不再卡表单。

## 二、每单 key 签发 SOP（手动模式）

```
1. Paddle 订单通知邮件到（或每天看一次后台 Sales）
2. 本地跑 keygen（私钥在仓库外）:
     cargo run --bin keygen -- sign <私钥路径> <买家邮箱> <personal|enterprise>
   （工具 09-19 licensing 已交付: generate/pubkey/sign 三件套齐, 无需补造）
3. 把 key 粘进邮件发给买家（平台后台可直接给该 customer 发消息，购买记录留痕）
4. 每月导出一次 Sales CSV 存档（对账 + 退款复查）
```

**agent 增强分工（2026-10-28 定）**：第 1 步盯单 = agent（API 读 transactions，
**以完成态为准**——微信单防延迟捕获；节奏 = 用户每天一句「巡检」或订单通知
邮件到了喊一声）；第 2 步签发 + 第 3 步草稿 = agent（keygen 本地跑 + 按下方
模板填好 key/邮箱/档位）；第 3 步**发送 = 用户本人**（买家信任链在卖家 QQ 邮箱，
agent 无发信能力；备胎 = Paddle 后台给 customer 发消息）。24h 交付承诺的瓶颈
= 用户发送那一下。首单实证（10-28 自购单）：收据邮件正常送达未进垃圾箱。

邮件模板（草稿）:

> 感谢购买丹青日志 LogLens 完整版（个人档）。
> 你的 license key：`<key>`
> 激活：打开 LogLens → 设置 → 许可 → 粘贴 key → 激活。离线校验，无需联网。
> 本 key 绑定你的邮箱，买断制永久有效，含 v1.x 全部更新；换机自由。
> 收据/发票自助（Paddle 客户门户）：https://customer-portal.paddle.com/cpl_01m49sdngv9g6vhtc1qday33mg
> 个人档限个人/非商业使用；商用请购企业档。
> 任何问题直接回复本邮件，或在 GitHub Issues 反馈。

## 三、一轮调研存档（2026-10-07 上午，仍有效结论）

- **Lemon Squeezy 出局**：银行 payout 国家列表有香港无大陆（shipkit.sh 2026-09）；
  Stripe（2024-07 收购）2026-01 宣布迁用户去 Stripe Managed Payments，不收大陆
  卖家；官方口径「国家不在列表即不可用」
- **Gumroad 主站被墙**：用户实测 + 知乎 2021 年即有「能上 google 却打不开
  gumroad」+ greatfire 屏蔽检查页 —— 长期状态。其流程存档：注册 30 分钟 +
  1–3 周审核；payout 仅 PayPal（2%）；PayPal 提现国内 = 电汇 $35/笔（占 5 万
  美元年额度，首笔前电话问开户行）或万里汇等第三方 0.3–1%。**有稳定代理时
  仍是成熟可用路，不删**
- **D2「自带 key 列表分发」核销**：Gumroad 原生 key 无自定义格式、验证须联网
  （LicenseSeat 2026）；LS 自定义 key 走 webhook 需服务端 —— 两家均不支持 →
  手动签发。先例：Obsidian 插件 Bases Power Pack 与 D2 逐字同构
  （`base64url(payload).base64url(signature)` + 内嵌公钥离线校验）
- **Payhip 出局**：非真 MoR，payout 依赖 Stripe/PayPal（Stripe 不收大陆）
- **Anyway 备查不推**：声称支付宝/银行提现（3.9% + $0.50），仅自家博客一证

## 四、裁决点（收敛中）

1. ~~手动签发模式~~ **已确认（2026-10-07 用户 go）**：key 邮件人工交付、24h 内 ——
   spec `specs/SPEC-v1x-licensing.md` 已回写销案（D2 核销 / §8 终选核销 /
   逃逸舱标「未启用 · 保留」，隐私政策零改动）
2. ~~终选平台~~ **已定 Paddle**（2026-10-07 实测可达）
3. ~~邮件模板~~ **已定稿（2026-10-07 用户 go）**：照用 §二 草稿
4. ~~EULA 措辞~~ **已核销（2026-10-07 用户过目通过）**：[`eula.md`](eula.md) 落盘 ——
   双语以中文为准；分界 = 谁出钱（**个人自购 $29 可商用**，用户裁定推翻 ROADMAP
   「商用请购 $59」字面）；**30 天无理由退款**（用户裁定）；评估例外 30 天（我推，获通过）；
   README 下载节身份门口径 + 许可节双轨链接已同步
5. **是否 MS Store 轨先行**：若 Paddle 审核意外拖长，先开 Store 轨把前提③
   首单外检跑起来、GitHub 轨后补，是合法选项；代价 = 两轨付费信号先后到，
   首单外检判读口径要注明渠道
   - **2026-10-12 复核**：Paddle 侧仍静默但**仅 2 个工作日**（§一·1 10-12 条），
     本项**尚未触发**——先按 10-14 触发点跟进。同时启动 **add-on 就绪度核查**
     （§六）以便此选项一旦要兑现即能落地，不必临时补课。**store add-on 轨与
     hosted checkout 权限零耦合**（走 Partner Center 内购 + `AddOnLicenses()`
     查询），是本闸唯一的独立解
6. 店铺/产品链接：产品 ×2 已建成（2026-10-07）；checkout link 生成原受
   hosted checkout 权限闸——**2026-10-23 权限已批，进入生成环节**：个人档
   link 给我回填 `PURCHASE_URL`（生成步骤见 §一·1 末 10-23 条）

## 五、终选落地后的仓库联动（我来做，逐项点头）

- [x] **`docs/eula.md` 起草**（身份门 + Paddle ToS 双职）→ 你过目 → README 加链接
      —— **已落（2026-10-07）**，README 下载节/许可节同步
- [x] `license.rs` `PURCHASE_URL = Some("<Paddle 个人档 checkout link>")` 回填
      —— 便携版设置卡「获取付费层」按钮随之现身（settings.rs:341 D8 显隐判据）。
      **已落（2026-10-27）**：链接建成即回填（存档 §一·2），D8 反向锁当日翻转
      为正向锁（`purchase_button_visible_since_shop_opened`）
- [x] spec `SPEC-v1x-licensing.md` 回写：D2 待核项核销（均不支持 → 手动签发）、
      §8「代销商终选」核销（Paddle；LS 大陆不可行 / Gumroad 被墙）、
      逃逸舱段标注「未启用 · 保留」—— **已落（2026-10-07）**
- [x] `tasks/todo-v1x-licensing.md` 末节用户侧清单打勾 —— **已落（2026-10-27）**：
      LS/Gumroad 项作废（终选 Paddle）、keygen 项核销（09-19）、`PURCHASE_URL` 核销
- [ ] README 购买节补 Paddle 链接（EULA 身份门口径同窗口）—— **文案已备，
      按既定裁决挂 v1.5.0 发布日公开**（dev 是默认分支即公开面，与落地页按钮
      同窗口；早鸟 expiry = 开业日 +30 天同窗口设）
- [x] checkout 域名国内可达性实测（买家侧）：**10-08 已测**——`buy.paddle.com`
      HTTP 200 / `pay.paddle.com` 405 / `checkout.paddle.com` 404（后两个根
      路径无 GET 是预期，TLS 握手全部成功未被墙）；真 checkout 页带表单的
      终验随真购买验证一并
- [x] 真购买链路验证（前提③前置）：自己下一单 $29 → 走通 签发→激活→升级提示消失
      —— **已走通（2026-10-28）**：自购单付款落账（`txn_01m5z37pc…` completed $19
      早鸟价 = 折扣自动应用实证，微信通路 + 买家 locale zh-Hans = 中文买家链实证）
      → keygen 签发 → 免费态激活 → 付费态翻转（实机截图在案）；收据邮件正常送达。
      **收银台全链（付款→落账→签发→激活→门控开）闭环**。此单剔除出首单外检
      （calibration-30d.md 记档）
- [x] Paddle 账号资料两项顺手改（Account settings 页标注「Paddle sets these
      details，改须发邮件」）：**Product website** `github.com/14uncle/danqing-log`
      → `https://14uncle.github.io`；**Company display name** Not set → Danqing
      —— **已随 Ivan 工单线程发出（10-08）**，同线程捎带、未单发邮件；
      **2026-10-23 Ivan 回信确认已改**（「I've updated your Account Settings
      as requested」）——顺手在后台 Account settings 核一眼即可销案

---

## 六、商店 add-on 轨就绪度核查（2026-10-12，§四·5 的可兑现性评估）

触发 = Paddle hosted checkout 静默（§一·1 10-12 条）→ 评估「先开 Store 轨拿首单外检」
这条应急选项**一旦要兑现，还差什么**。分代码 / 物料两面，逐项给差距。

### 6.1 代码侧：通路已闭合，只差一个占位常量

- **检测**走 `AddOnLicenses()` 遍历 + 逐项该 add-on 自身 `IsActive`
  （`src/store_license.rs:82-102`），**显式规避** pomodoro 那个「应用级
  `StoreAppLicense::IsActive` 免费上架时恒 true」的坑（源码注释
  `store_license.rs:78-81` 写明）
- **两条收款路汇合同一状态机**：Paddle key → `Entitlement::Paid{ License }`；
  商店 add-on → `Entitlement::Paid{ StoreAddOn }`（`src/license.rs:217-253`）
- **所有付费门控查同一 `self.entitlement.allows(Feature::…)`**
  （`license.rs:242-253` 单点判定）—— 列配置 / 书签持久化 / 字段点选 / 导出 /
  合并 / 会话 / 字段分析全收敛于此，无第二套布尔；商店里买成即全开
- **唯一缺口 = 数据占位**：`src/store_license.rs:32` 的
  `FULL_VERSION_OFFER_ID = "danqing-log-full"` 是**占位**（同
  `SPEC-v1x-licensing.md:137` 待回填）—— Partner Center 建的 add-on Offer ID
  须与它**逐字一致**（或改常量对齐）。未建 add-on 前：购买走
  `PurchaseOutcome::Failed`（`store_license.rs:210-213`），查询恒免费态
- **与商店轨无关的占位**：`license.rs:18` `PURCHASE_URL = None`（只关 Paddle/
  便携版）；`PRODUCT_PUBKEY` **已是真公钥**（`license.rs:30-33`），无需动
- 顺带清账：`license.rs:9-10` 模块头注释**陈旧**（现在时写「公钥占位为全零…
  等回填」，而值 09-19 就是真公钥了）—— 「加新决定不清旧文字」同族，待清

### 6.2 物料侧：多数已备，缺口集中在「用户动作 + 两个待裁」

**已备**：add-on 图标 300×300（`release-archives/log/store/addon-icon-300.png`，
`tools/export-logo.py` 可复现）/ §6 提交物料预起草（`ms-store-copy.md:369-415`：
产品功能 13 条 / 免费节改写 / 专业版段 / IARC 改答 / add-on 字段）/ 后台改动
对照表（`ms-store-copy.md:345-352`）/ 提交步骤（检查单阶段 3）。**父应用先发布
的硬顺序早已满足**（v1.0 09-21 上架，`ms-store-copy.md:299`）。

**缺口清单（按兑现顺序）**：

| # | 项 | 状态 | 锚 |
|---|---|---|---|
| 1 | **add-on 价格档**——微软价格档 vs Paddle $29/$59 如何对齐 | **未裁** | `ms-store-copy.md:411` |
| 2 | add-on 在 Partner Center **创建**（Offer ID 须 = `danqing-log-full`） | 未做·**用户动作** | `v1x-release-checklist.md:13` |
| 3 | 「能否**不随应用版本单独提交**」（add-on 是否要搭 v1.5 应用提交） | **待核** | `v1x-release-checklist.md:13` |
| 4 | Store trial 时长（建议 30 天，LogViewPlus 同档） | 未裁 | `SPEC-v1x-licensing.md:135` |
| 5 | IARC 改答「是」+ 重走（分级标签全变） | 未做 | `ms-store-copy.md:408` |
| 6 | **文案三处**「不联网」「无内购」「零网络请求」 | ⚠️未改（add-on 上线即变假） | `ms-store-copy.md:298-299` |
| 7 | 隐私政策新版文本重贴 Partner Center | 提交时核对 | `ms-store-copy.md:414` |
| 8 | add-on entitlement 链路**真机验收**（真实购买，用户本人执行） | 未做 | `v1x-release-checklist.md:46` |

**一条内部张力（待消解）**：`ms-store-copy.md:406` 专业版段预起草**已写**
「买断 $29 个人 / $59 企业」，而 `:411` 又标「价格档【待裁】」—— 若最终裁的
不是 $29/$59，该草稿段须同改。

### 6.3 结论（2026-10-12）

- **代码零欠账**，欠的全在 Partner Center 侧 + 两个待裁（价格档 / trial 时长）+
  一个待核（能否单独提交）。
- **对「首单外检」的解耦成立**：store add-on 与 Paddle hosted checkout **零耦合**；
  且商店版**另可用本地 license key 取得付费态**（`tasks/acceptance-pending.md:23`
  明记两轨都算、B/F 组验收即走本地 key）—— 即**即便 Paddle 长期不批，商店侧
  仍能跑出前提③首单**。
- **兑现最短路径**：裁价格档 → 建 add-on（Offer ID 对齐）→ 解「单独提交」之核 →
  改三处文案 + IARC + 隐私 → 提交 → 用户真机买一单验 entitlement。

---

## 七、等钟空窗清单（2026-10-16，hosted checkout 权限静默期可做）

> **2026-10-23 权限已批，本清单作废归档**——①sandbox 预演、②QQ 白名单、
> ④banner 选项随钟失去紧迫性（sandbox 仍可选做，非必须）；**仅③ API key
> 轮换仍有效**（key 经聊天暴露过，与权限无关，见用户级记忆交接协议）。
> 正文保留作流程档案，不删。

**为什么能做**：Hosted checkouts 黄条原文 = 「**In live accounts**, hosted
checkouts are only available for an app-to-web sales funnel…」——限制只锁
live 账号，**sandbox 不设此闸**（sandbox 即开发者搭流程用）。等钟期间把
上线日的机械环节提前验通，届时只剩真金白银一小步。

### 7.1 Sandbox 预演购买链（最优先）

入口（**2026-10-17 官方文档勘误**）：sandbox **不是 live 后台的模式开关，
是独立账号**——`https://sandbox-vendors.paddle.com/signup` 另注册，与
live 分数据集/分凭据/分后台（API key 含 `_sdbx`、client token 前缀
`test_`；sandbox 凭据打 live API 直接 `forbidden`）。照 §一 建品流程在
sandbox 建 Personal / Business 两产品 → 生成 hosted checkout → 假支付
走一遍。逐项验：

- [ ] **结账页渲染 + 支付方式徽标**——尤其 **WeChat Pay**（中国买家唯一
      通路），顺带看 overlay 品牌色 `#0F766E` 生效与否
- [ ] **订单落 sandbox Sales**——熟悉后台读单（transaction 状态流；
      WeChat 延迟捕获 ~10 分钟的观感）
- [ ] **订单通知邮件到 QQ 邮箱**——手动签发 SOP（§二）起点 = 「通知邮件到」，
      **这环若被 QQ 规则吞掉，整个 SOP 聋了**；**QQ 收件规则白名单
      `sellers@paddle.com` 现在就建**（不等 sandbox/真单，免费消险；
      与静默期「先翻垃圾箱」同族，见 §一·1），建完此项验信即可
- [ ] **keygen + 发货邮件演练**——以 sandbox 单号走一遍 §二 SOP
      （keygen 生成 key → 套 §二 邮件模板发自己 → 激活流验通）

**边界**：sandbox 预演**替代不了** 03「Test and go live」的真购买验证
（要 live 真单）；sandbox 单不进 live Sales，纯流程彩排。

**与 onboarding 步骤的映射（2026-10-16 核, 23:55 界面定型）**：Get
started 页「Sandbox path」五步 = 01 Create catalog / 02 Build pricing
page & checkout / 03 Handle fulfillment（**webhooks**）/ 04 Verify /
05 Test and go live；完成条件 = 五步毕 + 切回 live——**但 03 是给
Paddle.js / API 集成者的**，我们手动签发（§二）不接 webhooks / API /
Paddle.js，**不追该页 100% 勾满**；本节四项勾完即我们的「sandbox
path 完成」。其「Install our plugins… your coding agent」插件引导
同理不进路线（§7.4）。

### 7.1.1 逐步点菜单（2026-10-17 整理，官方文档实证）

**第 0 步 · QQ 白名单（动手前，2 分钟）**：收件规则白名单
`sellers@paddle.com`——③格验信与将来真单通知的共同前置。

**第 1 步 · 开 sandbox 账号（~10 分钟）**：
`sandbox-vendors.paddle.com/signup` 注册（即便已有 live 也要另开；
注册问询个人/企业信息照 live 填法，不涉真钱）。**邮箱建议用同一 QQ
邮箱**——让订单通知直落 QQ，顺带验第 0 步白名单。

**第 2 步 · 镜像配置（~5 分钟）**：live 的配置**不跟过来**（独立数据
集），关键项重配——Checkout configuration → Payment methods 照 §一·6
开（**WeChat Pay** + PayPal + Apple/Google Pay；**sandbox 若无 WeChat
Pay 选项 = sandbox 通道限制非配置错**，记一笔留 live 首单验）；Styling
→ Brand color (overlay) = `#0F766E`（①格渲染对照用）；default payment
link 随手填 `https://14uncle.github.io/checkout`（sandbox 不验域名）。

**第 3 步 · 建品（~5 分钟）**：Catalog → Products → New product，照
§一·2 建 Personal $29 / Business $59 one-time（tax category 默认
Standard digital goods 不动）。

**第 4 步 · 生成 hosted checkout 链接**：官方差异表实证——**Hosted
checkouts：sandbox「No approval required」，live「Approval
required」**（等钟闸的文档出处）。入口找法：**My account → Settings →
Hosted checkouts**（live 黄条同页，sandbox 生成入口应直接可见）；若
UI 改版入口挪走，退 Catalog → Products 点产品找 Checkout links /
Payment links（名字漂移按同义入口认）。生成后看链接域名 = 
`sandbox-pay.paddle.io/…`（顺带扫一眼 **Subdomains** 页签——custom
subdomain 早鸟功能（`loglens.paddle.io` 之类）日后美化链接的候选，
待后议不急）。

**第 5 步 · 假支付走单（~10 分钟）**：无痕窗开链接，**Test Mode 水印
属正常**（sandbox 特征）。①格渲染：账单国家切 **China** → 找
**WeChat Pay 徽标**（桌面端 + USD + one-time 应现身；通道本身 sandbox
多半扫不了真码，**徽标在场即过**）+ 看品牌色渗透。付款用官方测试卡
（sandbox 拒真卡；持卡人随便、有效期填未来）：

| 用途 | 卡号 |
|---|---|
| 成功·免 3DS | `4242 4242 4242 4242` |
| 成功·带 3DS | `4000 0038 0000 0446` |
| 成功·Visa debit | `4000 0566 5566 5556` |
| 拒付 | `4000 0000 0000 0002` |

②格读单：sandbox Sales 看落单 + transaction 状态流（卡单即时；WeChat
真单才有 ~10 分钟延迟捕获观感，记差异即可）。

**第 6 步 · 验信（③格）**：QQ 收件箱找 Paddle 来信——sandbox 邮件
**主题带 `[TEST]` 前缀、发件 @paddle.com 域**（2024-12 起），买家收据
与卖家订单通知都看。没到：先翻垃圾箱 → 对照白名单 → 仍无 = 记
「sandbox 邮件不达 QQ」缺陷，下次 Paddle 往来捎带问。此格过 = §二
SOP 第 1 步「通知邮件到」链路活。

**第 7 步 · keygen 演练（④格，agent 半截）**：把 sandbox 单号/买家
邮箱交 agent → keygen sign（私钥仓库外）出 key + 套 §二 模板出草稿 →
用户发自己 QQ → LogLens 设置 → 许可 → 粘 key → 激活流验通。

（来源：Paddle 官方 [Sandbox 文档](https://developer.paddle.com/sdks/sandbox) ·
[测试卡](https://developer.paddle.com/concepts/payment-methods/credit-debit-card#test-payment-method) ·
[sandbox 邮件 [TEST] 前缀](https://developer.paddle.com/changelog/2023/prefix-sandbox-emails) ·
[sandbox 邮件域](https://developer.paddle.com/changelog/2024/sandbox-emails-domain-revert) ·
[hosted checkout 自定义子域](https://developer.paddle.com/changelog/2025/custom-subdomains-hosted-checkout)。
另勘误 §一·2「buy.paddle.com」：文档 2026-10 实证 hosted checkout
域名族 = `pay.paddle.io` / `sandbox-pay.paddle.io`，上线前国内可达性
以生成链接实样为准两族都测。）

### 7.2 onboarding 五步视图对照（2026-10-16 三连点开后界面定型）

Get started 进度 40%（2 tasks / 30 min）。界面几经翻新，最终定型为
「Sandbox path」五步视图（取代早前 01「Set up your live account」的
AI/Manual 迁移引导视图——那个是给「sandbox 已集成 + 双 MCP server」
的代码集成者的，我们直接在 live 建品无从迁移，已作废不追）。五步读法：

| 步骤 | 状态 | 我们路线的读法 |
|---|---|---|
| 01 Create your catalog | ✅ Complete | 10-07 live 建品已被计入 |
| 02 Build your pricing page and checkout | 🔵 In progress | **= hosted checkout 权限钟本体**（**2026-10-23 权限已批**，链接生成入口解锁）；链接上架落地页/README + 回填 `PURCHASE_URL` 后翻 Complete |
| 03 Handle fulfillment and provisioning | ⚪ Not started | **正确状态非欠账**——webhooks 步（Listen to notifications in your app），我们手动签发（§二）不接 webhook |
| 04 Verify your account | ✅ Complete | — |
| 05 Test and go live | ⚪ Not started | 真购买验证 = 发布后首单外检，现在动不了 |

**唯一动作**：sandbox 推荐横幅三选一 → **「I want to integrate in my
live account only」**（live-only 形态就是它）；不挡 §7.1 预演（sandbox
独立账号，想彩排另注册）。**判读：onboarding 无我们欠的账**——未完成
三项分别被权限闸着（02）、不进路线（03）、发布后才能做（05）。

**02 Manual 视图实勘（2026-10-17 00:07）**：说明文整段 Paddle.js-first
（`Paddle.Initialize` / `PricePreview` / `Checkout.open` + client-side
token），不进路线。真货 = 「Do this in Paddle」五卡：client-side token
**跳过**（自述「authenticate Paddle.js in your frontend」，印证 §7.4）/
default payment link ✅ 10-08 已配（卡述「不设即报 Something went
wrong」= 在册 `transaction_default_checkout_url_not_set`）/ payment
methods ✅ 10-08 已配（含 WeChat Pay）/ sales tax ⬜ / balance currency
⬜（后两张 = §7.3 巡检项）。**实勘要点：全单无「生成 hosted checkout
链接」条目**——清单默认 Paddle.js overlay，我们的链接入口等权限批下
才现身，故 **02 不按此单追 Complete**；完成判据不变 = 权限批下 →
生成链接 → 挂落地页/README → 回填 `PURCHASE_URL`。

### 7.3 Settings 巡检（都能现在配; 2026-10-17 与 02 卡组对账后收窄）

- [x] **Sales tax**——✅ 2026-10-17 保持默认 **Automatic based on location**
      （美国结账加税/欧洲含税 = 各区买家预期, 标价 $29/$59 两头成立;
      excludes = 欧洲账单上浮伤转化, includes = 我方吸收欧盟 VAT ~20%）
- [x] **Currencies**——✅ 2026-10-17 两处均保持现状：①Balance currency
      **留 USD**（页面「recommend same as bank」不跟——保住 USD 兜底
      通路零换汇直出，余额改 CNY 兜底退化二次换汇；定价锚定 USD 换汇
      只在 payout 一次，限费差异待**首笔试走**拿真数字再裁）；②Active
      conversions **留 0**（定价故事 $29/$59 全链统一；主市场 WeChat Pay
      本币由支付通道换算；转换价含 Paddle 换汇加成未必划算；发布后
      30 天校准钟按真买家分布再裁）
- [x] **Taxable categories**——✅ 2026-10-17 保持现状零改动：两产品均挂
      **Standard digital goods（默认档）= 正确**（一次性买断桌面软件 =
      标准数字商品；SaaS 是订阅服务税率语义，部分辖区与软件下载税率
      不同，挂错 = 分类不实；eBook = 欧盟图书减税率，不沾；「Digital
      goods」不申请——与默认档差异无证据，不为动而动）。SaaS 显示
      Approved 但未挂载产品，无害不动。产品 ID 已录 §一·2
- [x] **Customer portal**——✅ 2026-10-17 无配置项，只有托管链接
      `https://customer-portal.paddle.com/cpl_01m49sdngv9g6vhtc1qday33mg`
      （买家自助收据/发票/订单）。放两处：**§二 发货邮件模板已加行** +
      README 购买节/落地页购买区（§五 待办「README 购买节补 Paddle
      链接」届时一并带上）。分享型链接，买家只见己单，公开放无风险

（payment methods / default payment link 两项 10-08 已配齐见 §一·6，
从本清单销掉。）

### 7.4 明确不做（写了防手痒）

- **Authentication / API keys**——手动签发模式用不到，留空最小攻击面
  （02 卡片自述 client-side token = 「authenticate Paddle.js in your
  frontend」，我们无前端集成，实证用不到）。**日后开的两个条件**
  （2026-10-17 用户问「给 agent 开 key」后裁定）：①权限批下后若
  checkout link 可 API 生成；②发布后 SOP 对账（Sales 单 ↔ 已签 key
  防漏发，只读 Transactions: Read）。届时 = scoped 最小权限 + 密钥
  落仓库外 + 永不进对话/仓库，用完可 revoke。**API 绕不过 hosted
  checkout 权限**（那是账号审核闸不是技术闸），此刻开 key 零收益
  - **2026-10-17 update：live key 已开并交接 agent**——首发直接贴进
    会话 = **按已暴露处理**（转录存本机）；轮换协议 = 用户 revoke +
    新建（勾最小 scope）后**记事本直接覆盖密钥文件**，新 key 永不过
    聊天。密钥落盘**仓库外本地目录**（全路径不入公开仓，记用户级
    记忆）。**使用纪律：读操作 agent 可自主；钱路写操作（建单/退款/
    改价/改产品）须用户明确点头**。冒烟已过（HTTP 200；两产品 ID 与
    §一·2 吻合、API 层 `tax_category: standard` 印证 §7.3、
    transactions=0）。**能力面实测（同日）**：读 7 项 200（products/
    prices/customers/subscriptions/transactions/adjustments/events；
    `GET /api-keys` 空列表 = 密钥管理仍在后台 UI）；写探针（空 body，
    构造上不可变异）3 项全 400 = **宽写权限在**（POST transactions
    建单/adjustments 退款/products 改目录）——scoped 只读 key 轮换
    因此是正经事非纯卫生。**旧 key revoke 在后台 UI 点不动**（secret
    只显示一次已成定局），兜底 = 换浏览器/进详情找 Delete，仍不行则
    下次 Paddle 往来捎带代撤
- **Connectors / Retain / Events**——订阅向，我们纯买断
- **为「生成链接」造轮子**——live 侧被权限闸着，等批后走正常生成入口

- Paddle 个人卖家与审核: [Paddle 官方 Identity Verification](https://www.paddle.com/help/start/account-verification/what-is-identity-verification) ·
  [Paddle 官方 Account Verification](https://www.paddle.com/help/start/account-verification) ·
  [Boathouse: Do You Need to Be Incorporated to Sell via Paddle](https://www.boathouse.co/paddle-video-series-episode/3-do-you-need-to-incorporate-to-sell-with-paddle) ·
  [dev.to: How to Get Your Paddle Account Approved in 48 Hours (2025)](https://dev.to/danteisshipping/2025-how-to-get-your-paddle-account-approved-in-48-hours-277a) ·
  [paas.build: Paddle rejected your account](https://paas.build/paddle-account-rejected)
- Paddle 费率: [dodopayments Paddle Fees 2026](https://dodopayments.com/blogs/paddle-fees-explained) ·
  [StackScored Paddle Pricing 2026](https://www.stackscored.com/pricing/saas-billing/paddle/) ·
  [saasfeecalculator](https://saasfeecalculator.com/paddle-fee-calculator/)
- Paddle 国家支持: [Paddle 官方帮助中心](https://www.paddle.com/help/start/intro-to-paddle/which-countries-are-supported-by-paddle) ·
  [Paddle developer docs](https://developer.paddle.com/concepts/sell/supported-countries-locales)
- Creem 备选: [Creem payout 文档](https://docs.creem.io/merchant-of-record/finance/payouts) ·
  [Creem supported countries](https://docs.creem.io/merchant-of-record/supported-countries)
- Gumroad 被墙与存档: [知乎](https://www.zhihu.com/question/268348643) ·
  [greatfire](https://zh.greatfire.org/https/nikolaykononov.gumroad.com) ·
  [Swell 2026](https://www.swell.is/content/gumroad-pricing) ·
  [grey.co](https://grey.co/blog/how-to-receive-payments-from-gumroad-as-an-international-creator)
- Lemon Squeezy 出局: [shipkit.sh](https://shipkit.sh/blog/stripe-vs-creem-vs-paddle-vs-lemon-squeezy) ·
  [LS 官方文档](https://docs.lemonsqueezy.com/help/checkout/payment-methods)
- D2 核销与先例: [LicenseSeat Gumroad 分析](https://licenseseat.com/alternative-to-gumroad) ·
  [Bases Power Pack](https://community.obsidian.md/plugins/bases-power-pack) ·
  [Mac Mouse Fix 离线校验实录](https://github.com/noah-nuebling/mac-mouse-fix/releases)
- Payhip 出局: [MoR Finder Payhip review](https://www.merchantofrecordfinder.com/providers/payhip)
