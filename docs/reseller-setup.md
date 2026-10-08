# 代销收银台：选型 + 注册提现流程（GitHub 轨 · 中国大陆个人卖家）

- @author 十四叔
- @date 2026/10/07
- 状态: **终选 = Paddle**（2026-10-07 用户实测 paddle.com 可达；接受个人卖家、
  CNY payout 不经 PayPal）。**执行中**（进展见 §一·1 末「执行进展」：注册 / 产品 /
  折扣 / 落地页 / 身份验证已完成；**域名审核 10-08 已过**（Checkout +
  Apple Pay 双徽标）；hosted checkout 权限在审；payout 绑卡已完成
  （10-08 CNY 电汇））。平台政策时效性强，执行时以官网为准。
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
- ⏳ **Hosted checkouts 权限**：live 账号的 hosted checkout 只开放给
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

| 产品 | 定价 | 说明 |
|---|---|---|
| LogLens 完整版 · 个人 | **$29** one-time | EULA 限个人/非商业（君子协定） |
| LogLens 完整版 · 企业 | **$59**/seat one-time | 商用；座位数不技术强制 |

- 类型选 one-time（Paddle 支持一次性/lifetime 售卖；$29 单价在 $0.50 固定费甜区）
- **不开**任何平台自家 license key 功能（手动签发，§二）
- 生成 checkout / payment link —— **个人档那条 = 回填 `license.rs` 的
  `PURCHASE_URL`**；发布前实测 checkout 域名（buy.paddle.com）国内可达性，
  国内买家兜底 = MS Store 轨
- 描述写清两句：① 买断制永久使用、含本大版本全部更新；② **key 经邮件人工
  交付，通常 24 小时内**

### 3. Payout

- 方式：**电汇（支持 CNY payout 货币）** 或 Payoneer；最低 $100
- 成本：FX margin ~1.5–3%（USD→CNY）+ 某些组合 $15 SWIFT 费 —— **首笔走 CNY
  电汇，看实际到帐金额再定长期路线**；银行账户开户名与 Paddle 注册名一致
- 放款节奏与持有期以 Paddle Billing 后台当前口径为准

### 4. 早鸟折扣（2026-10-07 用户裁定：标价不动 + 首发早鸟价）

标价 $29/$59 不动，开业首月**或前 50 单**（先到为准）早鸟 **$19/$39**。
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

## 二、每单 key 签发 SOP（手动模式）

```
1. Paddle 订单通知邮件到（或每天看一次后台 Sales）
2. 本地跑 keygen（私钥在仓库外）:
     cargo run --bin keygen -- sign <私钥路径> <买家邮箱> <personal|enterprise>
   （工具 09-19 licensing 已交付: generate/pubkey/sign 三件套齐, 无需补造）
3. 把 key 粘进邮件发给买家（平台后台可直接给该 customer 发消息，购买记录留痕）
4. 每月导出一次 Sales CSV 存档（对账 + 退款复查）
```

邮件模板（草稿）:

> 感谢购买丹青日志 LogLens 完整版（个人档）。
> 你的 license key：`<key>`
> 激活：打开 LogLens → 设置 → 许可 → 粘贴 key → 激活。离线校验，无需联网。
> 本 key 绑定你的邮箱，买断制永久有效，含 v1.x 全部更新；换机自由。
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
6. 店铺/产品链接：产品 ×2 已建成（2026-10-07）；**checkout link 生成受 hosted
   checkout 权限闸**（§一·1 执行进展，10-07 已申请）——获批后把个人档 link
   给我回填 `PURCHASE_URL`

## 五、终选落地后的仓库联动（我来做，逐项点头）

- [x] **`docs/eula.md` 起草**（身份门 + Paddle ToS 双职）→ 你过目 → README 加链接
      —— **已落（2026-10-07）**，README 下载节/许可节同步
- [ ] `license.rs` `PURCHASE_URL = Some("<Paddle 个人档 checkout link>")` 回填
      —— 便携版设置卡「获取付费层」按钮随之现身（settings.rs:341 D8 显隐判据）
- [ ] spec `SPEC-v1x-licensing.md` 回写：D2 待核项核销（均不支持 → 手动签发）、
      §8「代销商终选」核销（Paddle；LS 大陆不可行 / Gumroad 被墙）、
      逃逸舱段标注「未启用 · 保留」
- [ ] `tasks/todo-v1x-licensing.md` 末节用户侧清单打勾
- [ ] README 购买节补 Paddle 链接（EULA 身份门口径同窗口）
- [x] checkout 域名国内可达性实测（买家侧）：**10-08 已测**——`buy.paddle.com`
      HTTP 200 / `pay.paddle.com` 405 / `checkout.paddle.com` 404（后两个根
      路径无 GET 是预期，TLS 握手全部成功未被墙）；真 checkout 页带表单的
      终验随真购买验证一并
- [ ] 真购买链路验证（前提③前置）：自己下一单 $29 → 走通 签发→激活→升级提示消失
- [x] Paddle 账号资料两项顺手改（Account settings 页标注「Paddle sets these
      details，改须发邮件」）：**Product website** `github.com/14uncle/danqing-log`
      → `https://14uncle.github.io`；**Company display name** Not set → Danqing
      —— **已随 Ivan 工单线程发出（10-08）**，同线程捎带、未单发邮件；
      落实结果待 Paddle 确认（改没改回后台 Account settings 核一眼即可）

---

## 来源（2026-10-07 检索）

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
