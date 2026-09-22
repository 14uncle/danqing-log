# plan-v1x-licensing: 授权与收银台 实施计划

- @author 十四叔
- @date 2026/09/19
- Spec: `docs/specs/SPEC-v1x-licensing.md`（已含 D1–D8 设计决策，本计划不重复论证，只排任务）
- 任务清单: `tasks/todo-v1x-licensing.md`（全家按模块命名惯例，不用通用 todo.md）

## Overview

双轨授权机制：便携版 license key 离线校验 + 商店版 Store trial/IAP + 降级免费层 + 设置卡许可区 + 统一升级提示。只造机制，不造被门控的功能本体。

## Architecture Decisions（spec 已有，此处只留指针）

- D1 运行时分流 `is_packaged()`，同一二进制
- D2 离线 Ed25519 key，公钥 embed / 私钥仓库外；测试密钥对入库无害（公钥注入设计）
- D3 门控 = `Feature` 枚举 + `allows()`，点位归各功能 spec
- D4 降级不变砖、会话内不踢人、启动时判定失效
- D5 商店侧照 pomodoro 成稿，IsActive 陷阱写明

## 依赖图与切片

```
T1 校验核心(纯逻辑) ──┬── T2 keygen 工具
                     └── T3 状态机+持久化 ── T4 便携版接线 ── T6 设置卡许可页
                              │                              ↑
                              └── T5 商店版查询 ─────────────┘
                              └────── T7 升级提示对话框
T8 文档联动（最后，发布时机与 v1.x 整体对齐）
```

纵向切片原则：T1→T3→T4 走完，便携版「贴 key → 机制翻转」即端到端可验（无 UI 也可测）；UI（T6/T7）在其后。

## 新依赖（Ask-first，spec Boundaries 已预告）

- `ed25519-dalek`（verify-only 用法）+ `base64` —— 版本 plan 阶段不定死，build T1 时取当前稳定版并在 commit message 注明
- `open = "5"` **已在依赖里**（框架 update.rs 同款原语），不新增
- payload 序列化用现有 `serde_json`（config 的朴素解析风格只用于 config.toml，不套用到 key）

## 任务

### Phase 1: 纯逻辑核心

- **T1** key 格式 + 离线校验核心（`src/license.rs` 骨架）
- **T2** keygen 工具（`src/bin/keygen.rs`，私钥从仓库外路径读入）

### Checkpoint A（T1–T2 后）

- [ ] 校验核心全套断言绿（有效/篡改/截断/错产品/错前缀/未知版本）
- [ ] 184 测试基线不破；三件套绿
- [ ] 全仓 grep 私钥零命中

### Phase 2: 状态机与两条渠道

- **T3** Entitlement 状态机 + license 文件持久化（config 目录独立 `license.key`，接路径参数供测试）
- **T4** 便携版启动接线（启动加载 → 校验 → 状态；激活 API `activate(key_str)`）
- **T5** 商店版授权查询（is_packaged 分流 + WinRT broker + AddOnLicenses 遍历；映射层抽纯函数可测）

### Checkpoint B（T3–T5 后）

- [ ] 便携版端到端（无 UI）：激活 → Paid → 重启保持 → 删文件 → Free
- [ ] 商店版编译通过、映射纯函数有测试（broker 不可单测，人机分工写明）
- [ ] 三件套绿

### Phase 3: UI

- **T6** 设置卡「许可」页签（第四页，序号单点；三态错误文案；「获取付费层」按钮 —— `PURCHASE_URL` 占位时隐藏）
- **T7** 统一升级提示对话框（文案 + 两入口；免费态出现/付费态消失）

### Phase 4: 联动收口

- **T8** 文档联动：隐私政策 1.0→1.x + 第二节重写；README 下载节付费层说明；ms-store-copy 三处 ⚠️。**push/发布时机与 v1.x 整体发布对齐**（仓库公开，文档 push 即公开；dev 先行）

### Checkpoint C（T8 后）

- [ ] 成功判据全过（spec §成功判据逐条勾）
- [ ] 人工验收：便携版全流程用户实机；商店版待 add-on 进目录后用户真购买
- [ ] 进 review 阶段

## Risks and Mitigations

| 风险 | 级别 | 缓解 |
|---|---|---|
| 代销商不支持「自带 key 列表分发」 | 中 | D2 逃逸舱（在线激活）——启用须用户裁决 + 改隐私政策；低量期手动签发兜底 |
| Store trial API 面未实测过（pomodoro 只有买断 add-on，没用过 trial） | 中 | T5 build 时以微软官方文档为准核实 API；映射层抽纯函数保证可测部分全覆盖；最坏情况商店半边首波只上买断 add-on 不上 trial |
| WinRT broker 不可单测 | 低 | 纯函数抽离 + 用户人工验收（pomodoro 09-04 同款流程） |
| 设置卡第四页签超高（`PANEL_CONTENT_H` 180 教训，09-13「量了再写」） | 低 | 许可页内容量实测后定稿；沿用溢出守卫模式 |
| 公开仓库私钥泄漏 | 高（红线） | 私钥永不入库；keygen 只读外部路径；Checkpoint A 加 grep 闸 |

## Open Questions（带进 build，不阻塞）

- Store trial 时长：建议 30 天（LogViewPlus 同档）——**待用户裁**；注意时长是 Partner Center 后台配置，代码只读 broker 返回的过期时间，不写死
- key payload 预留 `expires` 可选字段：T1 按「预留」实现（买断 key 不含此字段，格式层面预留表达能力）
- `PURCHASE_URL` / add-on Offer ID / Store ID：用户侧动作后回填

## 用户侧并行清单（不挡 T1–T7，挡开业）

1. 注册 Lemon Squeezy / Gumroad + 打通提现；顺手核实「自带 key 列表分发」能力（决定 D2 要不要走逃逸舱）
2. T2 完成后：本地跑 keygen 生成真实密钥对，私钥存仓库外安全位置，公钥贴回 `license.rs` 常量占位处
3. v1.0 商店过审后（硬顺序）：trial 时长配置 + add-on 创建（图标 300×300 按 pomodoro SVG 4× 超采样工艺）+ IARC 改答「是」
