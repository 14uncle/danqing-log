# 丹青日志 LogLens 最终用户许可协议（EULA）

- @author 十四叔
- @date 2026/10/07
- 双职：本文件既是最终用户许可协议（EULA 身份门，2026-09-27 用户裁决「收」），
  也是代销平台（Paddle）审核要求的 Terms of Service 页面。中英双语同义，
  **如有歧义以中文为准**。
- 生效：随 v1.x 付费层销售开始生效（起草 2026-10-07）。版本钉死原则同隐私政策：
  本协议适用于 v1.x，大版本变更时走更新，不写「及以后」。

---

## 中文版

### 1. 这份协议管什么

LogLens 有两条授权路径，互不覆盖：

- **源代码**：以 MIT OR Apache-2.0 双许可证开源（见仓库 `LICENSE-MIT` /
  `LICENSE-APACHE`）。你可以按该许可证自由使用、修改、再分发**源代码**——
  那条路不受本协议约束。
- **官方二进制与 license key**：你从 GitHub Releases 或 Microsoft Store 下载的
  可执行文件，以及付费层的 license key，适用本协议。

### 2. 免费层

免费层（未激活付费层的官方二进制）授权你**免费**用于**个人、非商业用途**：
你自己的设备、非职务目的。

「职务/商业用途」指在任何组织内履行工作职责、或服务于任何营利活动的使用。
职务/商业用途请购买付费层（个人档或企业档，见 §3）。

**例外**：为评估是否购买而进行的试用不受身份门限制（合理期限，例如 30 天）——
先上手，再决定。

### 3. 付费层（买断制）

付费层解锁批量/留存/交付能力（字段分析、导出、工作台会话、多文件时间线合并等，
以发布说明为准）。两档的分界是**谁出钱、给谁用**，不是「在哪用」：

| | 个人档（$29） | 企业档（$59 / seat） |
|---|---|---|
| 购买者 | 本人自购 | 组织出资（含报销、统一部署） |
| 授权对象 | 购买者**本人一个自然人** | 每份 seat = 组织内**一名指定成员** |
| 用途 | **不限**——含职务/商业用途 | 不限 |
| 设备 | 本人任意多台设备，不限台数 | 指定成员的设备，不限台数；成员变动可重新分配 |

两档共同条款：

- **买断**：一次性付款，永久使用 v1.x 大版本，含该大版本**全部更新**，
  不设更新订阅费。未来的 v2 大版本另行定价。
- **换机自由**：不绑设备指纹，不设激活次数上限，换机不需要联系我们。
- **license key**：绑定你的购买邮箱，离线校验、激活不联网。
  请像保管密码一样保管它；不得转售、不得公开分享。
  key 泄漏或遗失，联系我们核实购买记录后注销重签。

### 4. 君子协定（明说）

我们信任你按 §2/§3 的分界自觉购买。license key 的离线校验证明「你拿到过真
key」，它在技术上挡不住分享——我们不打算为此引入联网验证、设备指纹或
激活次数锁，那是用全体正当用户的体验换少数违规者的麻烦。你选择怎么对待
这份信任，我们尊重并记录。

### 5. 退款

购买后 **30 天内无理由退款**：回复购买确认邮件、或来信说明订单邮箱即可，
由销售平台（Paddle）原路退回。退款后 license key 注销，软件回落到免费层，
你的数据与配置不受影响。

### 6. 禁止事项

- 转售、分许可、公开发布你的 license key
- 修改或重新打包**官方二进制**后冒充官方版本分发（源代码的修改与再分发
  请走 MIT/Apache-2.0 路径，那条路是自由的）
- 将软件用于违法用途

### 7. 免责声明与责任限制

软件按「现状」提供。我们不担保它无错误、不中断、或适配你的特定用途；
分析结果仅供参考。在法律允许的最大范围内，我们不对任何间接、附带、
后果性损失（含数据丢失、利润损失、生产事故）承担责任；
我们的累计责任上限为你为本软件实际支付的金额（免费层用户为零）。

### 8. 隐私

见[隐私政策](privacy-policy.md)。license key 的激活与校验全程离线，
不向我们发送任何数据。

### 9. 协议变更与适用法律

本协议可能随版本更新，变更会在发布说明中注明；继续使用新版本即视为接受
更新后的协议。本协议适用中华人民共和国法律；争议先友好协商，
协商不成由开发者所在地有管辖权的人民法院处理。

### 10. 联系

- GitHub Issues：<https://github.com/14uncle/danqing-log/issues>
- 邮箱：gwhun@qq.com

---

## English Version

### 1. Scope

LogLens is distributed under two separate licenses:

- **Source code**: open source under MIT OR Apache-2.0 (see `LICENSE-MIT` /
  `LICENSE-APACHE`). That path is not governed by this Agreement.
- **Official binaries and license keys**: executables downloaded from GitHub
  Releases or the Microsoft Store, and paid-tier license keys, are governed
  by this Agreement.

### 2. Free Tier

The free tier (an official binary without paid-tier activation) is licensed to
you at no cost for **personal, non-commercial use**: your own devices, for
non-professional purposes.

"Professional/commercial use" means use in the course of employment duties
within any organization, or in service of any for-profit activity. For
professional/commercial use, please purchase a paid tier (Personal or
Business, see §3).

**Exception**: evaluation for the purpose of deciding whether to purchase is
not restricted (a reasonable period, e.g., 30 days).

### 3. Paid Tiers (Perpetual, One-Time Payment)

The paid tier unlocks batch/persistence/delivery capabilities (field
analytics, export, workspace sessions, multi-file timeline merge, etc., per
the release notes). The two tiers differ by **who pays and who uses**, not
by where the software is used:

| | Personal ($29) | Business ($59 / seat) |
|---|---|---|
| Purchaser | The individual, out of pocket | An organization (incl. reimbursement, centralized rollout) |
| Licensed to | The purchasing **individual only** | **One designated member** per seat |
| Usage | **Unrestricted** — including professional/commercial use | Unrestricted |
| Devices | Any number of the individual's own devices | The designated member's devices; seats may be reassigned when members change |

Common terms for both tiers:

- **Perpetual**: one-time payment; permanent use of the v1.x major version,
  including **all updates within that major version**, no update subscription.
  A future v2 major version will be priced separately.
- **Machine freedom**: no device fingerprinting, no activation-count limits,
  no need to contact us when switching machines.
- **License key**: bound to your purchase email; verified offline, activation
  requires no network. Keep it like a password; do not resell or publicly
  share it. If your key is leaked or lost, contact us — after verifying your
  purchase record we will revoke and reissue it.

### 4. The Honor Agreement (Plainly Stated)

We trust you to purchase according to the §2/§3 boundary. Offline key
validation proves "you once obtained a genuine key"; it cannot technically
prevent sharing — and we deliberately refuse to add online activation,
device fingerprinting, or activation-count locks, which would punish all
legitimate users to inconvenience a few bad actors. How you treat this
trust is your choice, and we respect that.

### 5. Refunds

**30-day no-questions-asked refund**: reply to your purchase confirmation
email, or write to us with your order email. Refunds are processed via the
sales platform (Paddle) to the original payment method. After a refund the
license key is revoked and the software falls back to the free tier; your
data and configuration are unaffected.

### 6. Prohibited

- Reselling, sublicensing, or publicly posting your license key
- Redistributing modified or repackaged **official binaries** as if they were
  official (modifying and redistributing the *source code* is free under
  MIT/Apache-2.0 — that path stays open)
- Using the software for unlawful purposes

### 7. Disclaimer and Limitation of Liability

The software is provided "as is". We do not warrant that it is error-free,
uninterrupted, or fit for your particular purpose; analysis results are for
reference only. To the maximum extent permitted by law, we are not liable
for any indirect, incidental, or consequential damages (including data loss,
lost profits, or production incidents); our aggregate liability is capped
at the amount you actually paid for the software (zero for free-tier users).

### 8. Privacy

See the [Privacy Policy](privacy-policy.md). License-key activation and
validation are fully offline and transmit no data to us.

### 9. Changes and Governing Law

This Agreement may be updated with new versions; changes will be noted in
the release notes, and continued use of a new version constitutes acceptance.
This Agreement is governed by the laws of the People's Republic of China.
Disputes shall first be resolved through friendly negotiation; failing that,
they shall be submitted to the people's court with jurisdiction at the
developer's location.

### 10. Contact

- GitHub Issues: <https://github.com/14uncle/danqing-log/issues>
- Email: gwhun@qq.com
