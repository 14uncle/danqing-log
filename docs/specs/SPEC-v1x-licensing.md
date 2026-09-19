# SPEC-v1x-licensing: 授权与收银台

- @author 十四叔
- @date 2026/09/19
- 状态: 待 review
- 所属: 能力地图 `SPEC-v1x-map.md` 模块 `licensing`（构建顺序第 1 位）

## Objective

给 v1.x 付费层装收银台。没有购买路径，分发实验跑通了也读不出首单外检的数——本模块是这波存在的理由。

**做什么**：双轨授权——便携版（GitHub 渠道）license key 离线签名校验；商店版（MSIX）Store trial + add-on 内购。未授权 = 免费层全功能照常，付费功能入口给统一升级提示。**本模块只造机制**：授权状态模型、校验、门控查询、设置卡许可区、升级提示。被门控的三个功能本体（字段分析 / 导出 / 会话持久化）在各自 spec。

**成功长什么样**：便携版用户贴 key → 付费层解锁、重启保持、断网可用；商店版用户 trial 期内全开、过期降级免费层不变砖、买 add-on 永久解锁；免费用户在全部路径上**零行为变化**（暗发）。

## 范围

**In**：

- `src/license.rs` 新模块——授权状态机 + `Feature` 门控查询（对外唯一入口 `license::allows(feature)`）
- 便携版：key 解析 + Ed25519 校验（公钥嵌入常量）+ key 持久化（config 目录独立文件，不混进 `config.toml`）+ 激活/错误反馈
- 商店版：`is_packaged()` 运行时分流 → WinRT `StoreContext` 授权查询 + trial 状态 + 购买拉起（pomodoro 成稿模式）
- 设置卡新增「许可」页签：当前层 / 激活输入 / 购买入口 / 试用状态
- 付费功能入口的**统一升级提示**（机制层，点位在各功能 spec 定义）
- `tools/keygen/` 密钥签发工具（私钥从仓库外文件读入）
- 文档联动清单（见 §联动）：隐私政策升版、README、ms-store-copy 清单执行项

**Out**：

- 被门控的功能本体（field-analytics / export / workspace-sessions 各自 spec）
- Lemon Squeezy / Gumroad 店铺配置与提现（用户侧动作）
- Store 后台 add-on / trial 创建（用户侧动作；硬顺序：父应用 v1.0 过审发布后）
- 腿一 merge-timeline（第二波）

## 设计决策

### D1: 双轨授权模型，运行时分流，同一二进制

```
Entitlement = Free | Trial { expires } | Paid { source }
source = License (便携版) | StoreAddOn (商店版)
```

分流判据 = `danqing::platform::is_packaged()`（09-13 已落地的**运行时** Win32 判据），**不用编译期 feature**——商店版与便携版是同一个二进制，不留「手上这个包是哪个构建」的隐患（pomodoro freemium 三构建是前车之鉴）。

### D2: 便携版 key = 自签名离线校验

- 格式：`loglens1.<payload_b64url>.<sig_b64url>`；payload 为 JSON：`{v:1, product:"danqing-log", tier:"personal"|"enterprise", email, issued_at, nonce}`
- Ed25519：公钥 embed 为常量；**私钥只存仓库外**（仓库是公开的）
- 校验纯本地，**零网络请求**——这是商店版隐私姿态向便携版的延伸，不是可选项
- 校验函数签名注入公钥（`verify(payload, sig, pubkey) -> Result`），产品侧常量只是注入值——测试用**自己的密钥对**，测试私钥入库无害
- **待验证项**（不挡 build，挡开业）：代销商是否支持「自带 key 列表分发」——Gumroad 可上传自有 key 列表（待核）；Lemon Squeezy 自定义 key 走 webhook 需服务端（待核）。**逃逸舱**：两家都不行 → 退回「激活时一次在线校验」，届时隐私政策第二节同步改（写明：这是最后手段，启用需用户裁决）
- 低量期兜底：购买通知邮件 → 本地 keygen 签发 → 回邮，手动流程可接受（真实首单量级预期 = 个位数）

### D3: 门控机制，不发明点位

`Feature` 枚举（`FieldAnalytics` / `Export` / `WorkspaceSessions`）+ `license::allows(feature) -> bool`。功能模块各自查询。**升级提示统一为一个对话框组件**：文案 + 两入口（「获取付费层」→ 购买页 / 「已有 key？去激活」→ 设置卡许可区）。各功能的门控**点位**（哪个按钮/菜单拦）在各功能 spec 里定，本模块只给对话框与查询。

打开浏览器用框架现成原语 `open::that`（`danqing/src/update.rs:232` 在用），不新造。

### D4: 降级不变砖，会话内不踢人

- trial 过期 / key 删除 → 回免费层，应用全程可用
- 授权**获取**即时生效（贴 key 即解锁，不要求重启）
- 授权**失效**以启动时判定为准，**会话中不踢人**——排障途中收回功能是「试用期撞生产事故」的 mini 版，09-12 已否决过同款（intent §定价锚与渠道三条裁因）
- 付费功能产出的既有数据（已导出的文件、已保存的会话）在降级后**可读不删**——具体保全语义在各功能 spec 细化，本模块只立原则

### D5: 商店版照抄 pomodoro 成稿，IsActive 陷阱写明

`StoreContext::GetDefault() → GetAppLicenseAsync() → 遍历 AddOnLicenses()`（`danqing-pomodoro/src/license.rs:440` 起为第一手参考）。**必须遍历 add-on 许可证**：`StoreAppLicense::IsActive` 是应用级许可证，免费上架时所有安装者都为 `true`，拿它判断内购永远为真（pomodoro 实测坑，照抄时不许简化掉这段）。

商店版仍不自发 HTTP（broker 进程外调用），但隐私政策措辞按 `ms-store-copy.md` 文末清单 §2 重写——「零网络请求」对 1.x 不再成立。

### D6: keygen 入库，私钥永不入库

`tools/keygen/` 小工具，私钥从**命令行指定的仓库外路径**读入，生成器本身入库可审计。公钥派生后贴进 `src/license.rs` 常量。回归闸：全仓 grep 私钥零命中。

### D7: 企业档同格式，seat 不强制

$tier$ 字段区分 personal/enterprise，$59/seat 的座位数不技术上强制——君子协定，与 D2 同性质，写明即可。

### D8: 设置卡「许可」页签

第四页签（常规 / 快捷键 / 关于 / **许可**）。内容：当前层（免费 / 试用中·到期日 / 已激活·邮箱尾缀遮蔽显示）+ key 激活输入框 + 分态错误文案（格式错 / 签名错 / 产品不匹配——三态分开，不合一）+ 「获取付费层」按钮（便携版开购买页 URL；商店版拉起购买对话框）。

**页签序号单点定义在 `settings.rs` 的 `.tab()` 处**——09-13 教训在档：两处 main.rs 注释各抄一份序号，双双漂了。本 spec 写明：序号只在 `.tab()` 调用链出现，注释不抄数字。

购买页 URL 为单点常量 `PURCHASE_URL`，待用户开店后回填；回填前常量值为占位且「获取付费层」按钮在便携版**隐藏**（不给死链接）。

## 联动（文档与后台清单）

发布前必须执行的文档联动（源自 `docs/ms-store-copy.md` 文末「v1.x 上架时必须改什么」，本模块落地时逐项过）：

1. **隐私政策**（`docs/privacy-policy.md`）：顶部适用版本 `1.0` → `1.x` + 生效日期；第二节重写（授权查询经 Microsoft Store 服务；购买付款由 Microsoft 处理，我们不接触支付信息）
2. **README**：下载节付费层说明（$29/$59 买断、免费层永久免费口径不变——**别把「全部功能免费」改成「全部功能收费」**）
3. **商店后台**（用户侧）：IARC 改答「是」（分级标签全变，问卷可能重走）；新建 add-on（Offer ID + Store 一览 + **图标下限 300×300**——`assets/logo/` 最大 256，届时按 pomodoro 的 SVG 几何 4× 超采样工艺重渲）；add-on 只能在父应用发布后提交
4. **文案三处 ⚠️**（ms-store-copy）：「不联网」「无内购」「零网络请求」
5. 该清单整段过完后从 ms-store-copy.md **删掉它**（它自己的要求）

## 成功判据

机器可测：

- 便携版无 key：`allows()` 全 `false`，免费层全部功能行为与 v1.0 逐点一致（现有 **184 测试全绿** + 新增守卫）
- 有效 key → `allows()` 三腿全 `true`；重启后保持（持久化）；**断网**状态下激活成功（纯本地校验）
- 坏 key 三态（格式错 / 签名错 / 产品不匹配）各自明确报错，不崩、不静默、不误解锁
- 篡改一字节的 key 必失败；测试密钥对签的 key 在产品公钥下必失败
- 升级提示对话框：免费态出现、付费态消失；「去激活」跳转设置卡许可页
- 全仓 grep 私钥零命中；`PURCHASE_URL` 单点定义
- 页签序号单点守卫（09-13 教训的回归锁）

人工验收（用户实机）：

- 便携版：免费 → 贴 key → 解锁 → 重启保持 → 删 key 文件降级免费层可用
- 商店版（add-on 进目录后，硬顺序在 v1.0 过审后）：trial 活跃全开 → 购买拉起真对话框 → 购买后永久解锁；窗口期「购买未完成 · 重试」属预期（pomodoro 实测）
- 商店版**真实购买测试由用户本人执行**（pomodoro 09-04 同款），不做自动化

## 已知局限

- 离线 key 是君子协定：挡不住分享，seat 不强制——接受（Sublime 模式，地图备注 3）
- 代销商 key 分发能力未核实，逃逸舱（在线激活）牵隐私政策第二节
- 商店半边受 v1.0 认证硬顺序阻塞，代码可先备、提交在后
- 会话内授权失效不踢人（D4 有意取舍）
- 本模块验收时三腿未建，「解锁」只验机制（`allows()` 翻转 + 提示显隐），真功能解锁随各腿 spec 验收

## Boundaries

- Always：`cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test` 全绿；中文注释；新 `.rs` 文件头 `@author/@date`；测试无真实桌面副作用
- Ask first：**新依赖**（计划引入 `ed25519-dalek` verify-only + `base64`，plan 阶段定版）;隐私政策 / README / ms-store-copy 改动按 §联动 清单执行
- Never：私钥入库；真实购买自动化；便携版激活引入网络请求（逃逸舱启用须用户裁决）；删测试

## Open Questions

- [ ] 代销商终选：Lemon Squeezy vs Gumroad——取决于「自带 key 列表分发」核实（用户注册时确认；若可联网核实则 agent 先查）
- [ ] Store trial 时长：建议 30 天（LogViewPlus 同档）——待用户裁
- [ ] `PURCHASE_URL`——用户开店后回填
- [ ] add-on Offer ID / Store ID——用户建后回填（参考 pomodoro：`danqing-pomodoro-full` / `9P4B2MPB8HNN`）
- [ ] key payload 是否含 `expires` 字段预留（买断制当前永不过期；预留字段防将来订阅/升级策略变化时旧 key 无法表达）——倾向加，plan 阶段定

## 实现记（2026-09-19 build 收口, 与上文分叉处回本）

- **公钥占位全零 = 收银台未开业的安全默认**（任何 key 验不过）;真公钥等用户跑 keygen 后回填 `PRODUCT_PUBKEY` —— 人工验收的真激活在那之后才能做
- **StoreLicense 没有 `IsTrial`**（windows 0.61.3 crate 源码实证, 不是文档记忆）——trial 与 durable 买断靠 `ExpirationDate` 有限性区分（阈值 `DURABLE_THRESHOLD_EPOCH` ≈ 公元 9900）;商店首波 trial 可行性待 Partner Center 实看
- **激活反馈落在「许可」页内**（`license_feedback`), 不走底栏 notice —— 模态卡会遮住底栏。spec D8「错误反馈」未定位置, 此处定档
- **激活消息只有一个入口** `Msg::ActivateLicenseClicked`（读输入镜像）; 早先设计的 `ActivateLicense(String)` 在真实 UI 里无人构造, 已删
- **「许可」是第三个页签**（常规/快捷键/许可/关于 —— 「关于居末」惯例优先于 spec 起草时的「第四页」措辞）, `LICENSE_TAB_INDEX` 常量 + 一致性测试守着
- **`expires` 预留字段已加**（Open Question 末条按倾向落地）
- **D8「占位期隐藏购买按钮」原样实现**（`show_purchase_button()`), 商店版始终显示（窗口期报「购买未完成」属预期）
- 测试规模: lib 77 / main 136 / genlog 8 / keygen 3 = **224 全绿**, clippy 零警告（2026-09-19）

## 评审记（2026-09-19 双路评审 → 修复闭环, 231 测试绿）

代码评审 **REQUEST CHANGES**（1 Critical + 2 Required）+ 安全审计 **PASS**（零 Critical/Required, 5 条卫生项）—— 全部落地:

- **Critical 已修**: 模态守卫吞 Ctrl+V（许可页粘贴激活主路径断裂）—— 守卫内放行剪辑组合键 c/x/v/a/z/y 给焦点分发, 回归锁 `modal_card_passes_clipboard_keys_but_blocks_globals`
- **Required ① 已修**: 购买防重入移植（`try_begin_purchase` 闸 + 结果复位 + 按钮在途文案「购买中…」）—— AsyncJob 代次语义下重复 launch 会让晚到旧轮覆盖新轮结果被丢弃 = 付了钱会话内无感知
- **Required ② 已修**: 购买结果反馈落 `license_feedback`（底栏 notice 会被模态卡遮住, 与激活反馈同通道）
- **安全卫生已修**: `verify_key` 4096 长度闸 / 激活成功清空输入框明文（框架 `TextInput::bind_clear` 下沉, **danqing 联动一笔**）/ `.gitignore` 加 `*.secret`+`*.license.key` 兜底 / `Payload` 手写 Debug 遮蔽邮箱
- **Optional 采纳**: 已付费再点购买给明确回话（不进流程）; keygen pubkey 同时打印 Rust 数组字面量（消手工转换手滑点）; 面板高度守卫给许可页补两条绑定行高（口径对齐）
- **核实后不采纳**: ed25519-dalek `default-features` 不裁 —— lock 里的 pkcs8 是「锁住但未启用」的可选依赖（`cargo tree -i` 实证不进编译闭包）; windows 0.61 不追 0.62（与 pomodoro 实测成稿一致优先）
- **留痕**: RoInitialize 不配对 / 不带 catch_unwind（release `panic=abort`）—— 已在 store_license 模块头显性化
- **新 Open Question**: trial 会话内到期口径 —— 当前 `allows()` 用调用时刻 now（实时关闸）, 与 D4「会话内不踢人」字面有张力; trial 上线那天若要严守 D4, 把 trial 有效性固化成会话常量
