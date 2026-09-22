# todo-v1x-licensing: 授权与收银台 任务清单

- @author 十四叔
- @date 2026/09/19
- Spec: `docs/specs/SPEC-v1x-licensing.md` · Plan: `tasks/plan-v1x-licensing.md`
- 状态: **build + review 双闭环** —— 2026-09-19 双路评审（代码 REQUEST CHANGES 1C+2R /
  安全 PASS）修复全落地，231 测试绿。**待: 用户实机验收 + code-simplify + push**

## Phase 1: 纯逻辑核心

- [x] **T1: key 格式 + 离线校验核心** —— `src/license.rs`（lib 侧纯逻辑）。
  新依赖 ed25519-dalek 2 + base64 0.22（getrandom 0.2 为 T2 keygen）。
  13 条测试：有效两档 / 篡改 payload 报签名错（先验签再解 JSON 的顺序红线）/
  篡改签名 / 截断 / 错前缀 / 错产品 / 未知版本 / 缺字段 / 占位公钥全拒 / 垃圾输入。
  **实现中发现**: base64 尾字符只有 2 个有效位，翻它被解码头尾位检查拦成格式错
  （那种报 Malformed 是对的）——签名错用例改翻签名段首字符。
- [x] **T2: keygen 工具** —— `src/bin/keygen.rs`（generate/pubkey/sign 三子命令，
  私钥只从仓库外路径读，已存在拒绝覆盖）。payload 构造与校验共用
  `license::build_payload`（同一真身，roundtrip 测试锁）。hex 手自保（零新依赖）。
  e2e 测试: 临时私钥文件 → 签发 → verify_key 验回。

## Checkpoint A（T1–T2）✅

- [x] 校验断言全绿；基线不破；全仓 grep 64-hex 私钥形态零命中

## Phase 2: 状态机与两条渠道

- [x] **T3: Entitlement 状态机 + 持久化** —— `Entitlement = Free | Trial{expires_epoch} |
  Paid{source}`；`Feature` 三枚 + `allows_at`（口径单点）；license.key 独立文件
  （config 目录，不进 config.toml）；`load_from`/`activate` 接注入路径；
  激活先建父目录（全新机器连配置目录都可能没有 —— 实写时想到的边界）。
- [x] **T4: 便携版启动接线** —— LogApp 挂 `entitlement` + `license_pubkey`（测试可注入）；
  `initial_entitlement` 测试构建恒 Free 不读真文件（hermetic）；`license_path()`
  与 save_config 同款硬拦（test 下 None 路径直接 panic）；activate 即时翻转（D4）+
  落盘失败照样激活只警示。3 条应用层测试（起手 Free / 激活翻转+持久化 / 坏 key 不落盘）。
- [x] **T5: 商店版授权查询 + 购买拉起** —— `src/store_license.rs`（pomodoro 成稿移植:
  IsActive 陷阱注释原样写明 / IInitializeWithWindow 属主绑定 / EnumWindows 找主窗 /
  fail-open 纪律）。**API 实证**: windows 0.61.3 源码核实 `StoreLicense` **没有 IsTrial**
  —— trial 与 durable 买断靠 `ExpirationDate` 有限性区分（阈值 ≈ 公元 9900，
  `DURABLE_THRESHOLD_EPOCH`）。映射抽纯函数 `map_store_snapshot`（lib 侧）四态测试齐。
  main 侧: `store_license_job`/`purchase_job` 走 AsyncJob + tick 拾取；
  `adopt_store_entitlement` **只许 Free→其他**（D4 不踢人，测试锁）；
  `PURCHASE_URL: Option<&str>` 单点常量（None = 未开业）。

## Checkpoint B（T3–T5）✅

- [x] 便携版端到端（激活 → Paid → 持久化验回 → 删文件 → Free）由 T3/T4 测试覆盖
- [x] 商店版编译通过、映射四态测试齐；broker 人工验收挂 Checkpoint C（等 add-on 硬顺序）

## Phase 3: UI

- [x] **T6: 设置卡「许可」页签** —— 第三页（常规/快捷键/**许可**/关于；关于居末惯例），
  `LICENSE_TAB_INDEX` 常量与 `.tab()` 链同文件相邻 + 一致性测试。内容: 当前层行
  （五态文案，trial 剩余天数零日历依赖）/ key 输入框（`base_input` 范式 bind_theme）/
  激活 + 获取付费层按钮（`Button::bind_color` 每帧跟随 accent —— Button 无 bind_theme
  是已知框架缺口，hover 自动提亮绑定色 1.2 倍故不需框架改动）/ 页内反馈行
  （**改道**: 底栏 notice 会被模态卡遮住，反馈落页内 `license_feedback`）。
  购买按钮 `show_purchase_button()` = PURCHASE_URL 回填 || 商店版（D8 占位隐藏）。
  面板超高守卫自动覆盖新页（实测通过，PANEL_CONTENT_H 180 不动）。
- [x] **T7: 统一升级提示对话框** —— `upgrade_overlay`（与设置卡同族）；双闸防付费态
  出现（点位查 + handler 再兜）；「去激活」跳设置卡许可页（引常量不抄数字）；
  Esc 次序 = 升级提示 > 设置卡 > 栏；模态守卫扩展为两卡（原长注保留）。
  **测试自己先错一次**: 忘了第一步设的提示没清就测第二道闸 —— 修的是测试不是代码。

## Phase 4: 联动收口

- [x] **T8: 文档联动** —— 隐私政策升 1.x（版本行 + 顶部变更注 + 第二节分渠道重写 +
  第三节购买跳转 + 第四节 license.key 行）；README 下载节（商店「零网络请求」钉 v1.0 +
  付费层在建 bullet）；ms-store-copy 清单**标注执行状态**（代码/政策/README ✅，
  商店铺文案三处 ⚠️ 未改 —— 线上 listing 还是 1.0，等 add-on 提交那次一起改；
  商店后台 ⏳ 等硬顺序）。**清单未删**（它的自查要求是全完才删）。

## Checkpoint C（T8 后）

- [x] spec §成功判据机器部分逐条过（见 spec「实现记」）
- [ ] **人工验收（用户实机）**: 便携版（设置卡许可页: 免费态 → 贴 key → 翻转 → 重启保持 →
  删 key 文件降级）+ 升级提示对话框观感 —— **等真公钥回填后才能真激活**
  （当前 PRODUCT_PUBKEY 占位全零，任何 key 验不过 = 收银台未开业的安全默认）
- [ ] 商店版真购买（add-on 进目录后用户本人执行，pomodoro 09-04 同款）
- [ ] 进 review 阶段（`/agent-skills:code-review-and-quality`）

## 遗留 / 待用户动作（plan 用户侧清单原样有效）

1. 注册 Lemon Squeezy / Gumroad + 提现；核实「自带 key 列表分发」（定 D2 逃逸舱去留）
2. `cargo run --bin keygen -- generate <仓库外路径>` 生成真密钥对 → `pubkey` 子命令
   输出贴回 `license.rs` 的 `PRODUCT_PUBKEY`
3. ~~v1.0 商店过审后~~ **前置已解锁（v1.0 过审上架 2026-09-21）**: trial 时长（建议 30 天, 待裁）+ add-on 创建（Offer ID 须 =
   `danqing-log-full`, 图标 300×300 超采样工艺）+ IARC 改答「是」+ `PURCHASE_URL` 回填
