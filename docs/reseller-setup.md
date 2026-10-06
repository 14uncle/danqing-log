# 代销收银台：选型 + 注册提现流程（GitHub 轨 · 中国大陆个人卖家）

- @author 十四叔
- @date 2026/10/07
- 状态: **终选 = Paddle**（2026-10-07 用户实测 paddle.com 可达；接受个人卖家、
  CNY payout 不经 PayPal）。**执行前置 = EULA 落盘**（Paddle 域名审核要 ToS 页面，
  与 09-27 EULA 身份门裁决合并落地，见 §一·0）。平台政策时效性强，执行时以官网为准。
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
- website 就填 **GitHub 仓库 URL**（独立开发者过审先例存在；若域名审核被驳回，
  退路 = GitHub Pages 起一页落地页，零成本，届时再裁）

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

## 二、每单 key 签发 SOP（手动模式）

```
1. Paddle 订单通知邮件到（或每天看一次后台 Sales）
2. 本地跑 keygen（私钥在仓库外）:
     cargo run --bin keygen -- sign --email <买家邮箱> --tier <personal|enterprise>
   （keygen 的 sign 子命令如尚未实现，属收银台联动小件，发布前补）
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

1. **手动签发模式**（推荐）：key 邮件人工交付、24h 内 —— 确认则 spec 回写销案；
   否决则回 spec 逃逸舱（激活时一次在线校验），隐私政策第二节要跟着改
2. ~~终选平台~~ **已定 Paddle**（2026-10-07 实测可达）
3. **邮件模板**与产品描述措辞：照用 §二 草稿还是要改
4. ~~EULA 措辞~~ **已核销（2026-10-07 用户过目通过）**：[`eula.md`](eula.md) 落盘 ——
   双语以中文为准；分界 = 谁出钱（**个人自购 $29 可商用**，用户裁定推翻 ROADMAP
   「商用请购 $59」字面）；**30 天无理由退款**（用户裁定）；评估例外 30 天（我推，获通过）；
   README 下载节身份门口径 + 许可节双轨链接已同步
5. **是否 MS Store 轨先行**：若 Paddle 审核意外拖长，先开 Store 轨把前提③
   首单外检跑起来、GitHub 轨后补，是合法选项；代价 = 两轨付费信号先后到，
   首单外检判读口径要注明渠道
6. 店铺/产品链接：注册后把个人档 checkout link 给我回填 `PURCHASE_URL`

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
- [ ] checkout 域名（buy.paddle.com）国内可达性实测（买家侧，发布前）
- [ ] 真购买链路验证（前提③前置）：自己下一单 $29 → 走通 签发→激活→升级提示消失

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
