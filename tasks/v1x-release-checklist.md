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
- [ ] 早鸟折扣 expiry 设为发布日 +1 个月（「首月或前 50 单先到为准」，2026-10-07 裁定；
      Uses left 50 已设）—— **发布日 10-28 设 `2026-11-27`**（用户后台动作：
      Catalog → Discounts 两个各设）
- [x] 三件套：`cargo fmt` / clippy 0 / 521 测试绿（基线见 CLAUDE.md，别拿旧数）——
      **10-27 复跑绿（含版本号 1.5.0 同步后）**；发布打包前再跑一次
- [x] 手动签发运营链自检：邮件模板（`docs/specs/SPEC-v1x-licensing.md` §二定稿）+
      **WeChat Pay 延迟捕获 ~10 分钟 → 以 transaction 完成态为准再签 key**（`docs/reseller-setup.md` §一·6）

## 阶段 2：打包与自验

- [ ] 便携 zip：`powershell -NoProfile -File tools/package_portable.ps1` + sha256
- [ ] MSIX：`build_msix.ps1` → `sign_msix_local.ps1`（sidecar 签名后重算逻辑已有）
- [ ] 两包 exe 与 `target/release` 三方同哈希核对
- [ ] 本机侧载 + 便携版实机：购买入口显隐（D8）、key 激活、付费门控三道

## 阶段 3：发布

- [ ] dev → master 合并 + tag（发布点打，不提前 —— 死标签教训）
- [ ] GitHub Release（正文 = release notes 去文件头元信息段）
- [ ] **README 购买节补双链接 + 落地页购买按钮解注挂链**（本单原漏；文案与按钮
      占位均已备——README「购买链接开业后回填此处」、pages 仓 `index.html`
      HTML 注释占位 `PADDLE_CHECKOUT_PERSONAL/BUSINESS`；注意 dev = 默认分支，
      push 即公开 = 开业钟起算，与早鸟 expiry 同窗口）
- [ ] MS Store 提交：照 `docs/ms-store-copy.md`「v1.x 上架时必须改什么」逐项核
      （IARC 改答「是」→ 分级标签全变 / 隐私政策贴**新版**文本 / 文案三处 / add-on 图标 300×300）
- [ ] 商店 add-on 提交

## 阶段 4：发布后外检（前提③开门）

- [ ] 真付费链路验收：代销 key 真购买 → 激活（便携版通路）
- [ ] 商店 add-on entitlement 链路验收
- [ ] **触发分发推送**（`docs/distribution-prep.md`，两波）
- [ ] 校准钟回填：10-18 GitHub / 10-21 商店（`tasks/calibration-30d.md`）
- [ ] round8 复活条件检查：首单外检落地 or 10-21 校准钟 → 选型机器合法开机评估
