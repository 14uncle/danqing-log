# SPEC: 更新角标 (update-badge) — 让「有新版本」在设置按钮上可见 (双渠道)

> 作者: 十四叔 · 日期: 2026-09-22 · 状态: **评审双路闭环 (REQUEST CHANGES + BLOCK → 4+4 Required 全修复测) · 待 code-simplify**
> 流水线: spec → plan → build → review → code-simplify (逐段推进)
> 范围裁决: 单能力**三腿** (Phase 0 判定不拆多模块 —— 三腿共享同一验收时刻「实机角标亮」)

## 0. 背景 (为什么做)

2026-09-22 诊断结论 —— 「设置按钮角标不亮」是**两层独立根因**, 各自都足以让它不亮:

1. **角标 UI 从未实现**: 框架 `danqing/src/update.rs:161` 写着「返回 Some = 设置按钮亮角标」,
   `danqing/docs/specs/SPEC-update-check.md:97` 那条 checkbox 至今 `[ ]` 未勾;
   产品侧 `view.rs:1804-1822` 设置按钮只画文本 + hover 变色, 全仓无任何角标绘制。
   已实现的只有设置卡「关于」页提示行 (`settings.rs:592` VersionRow)。
2. **更新检查在 TLS 中间人环境下必败**: 本机 hosts 把 `api.github.com` 劫持到 `127.0.0.1`
   (SteamTools 加速, hosts mtime 2026-09-22 08:58), 443 出示 `CN=SteamTools Certificate` 私有 CA;
   而 ureq 用内置 `webpki-roots` 根证书包 (`Cargo.lock` 实证), 不读 Windows 系统信任存储
   → TLS 拒绝 → 日志 `WARN [danqing::update] 更新检查失败` (11:59 实录) → 不落缓存 → `hint()` 恒 None。
   对照: 同机 PowerShell/gh (系统证书存储) 200 通, curl schannel 吊销检查败, ureq 直接拒。

第三层在用户问询中暴露: **商店轨整个缺席** —— danqing-log 2026-09-13 决定「商店版整条关掉
更新检查」, 而家族成稿 `danqing-pomodoro/src/update.rs` (2026-09-05) 早已双轨齐备
(GitHub 轨委托框架 + 商店轨 StoreContext 查/拉更新 + 设置按钮小圆点角标, `main.rs:947-970`)。
**2026-09-22 用户裁决翻案 09-13**, 腿 C 进本次范围 (见 D5)。

背景事实: GitHub 已有 v1.0.1 (2026-09-22 发布, 用户跑 v1.0.0) —— **该亮的时候恰好检查不通**;
商店侧 MS Store 上架的是 1.0.0, 商店用户同样没有任何应用内更新感知。

## 1. Objective

**用户故事**: 作为用户 (便携版或商店版), 当有新版时, 我在底栏「⚙ 设置」上看到一个小圆点;
点开设置卡「关于」页得知「有新版本」, 一键更新:
- 便携版 → 「前往下载」跳 GitHub Releases (开着加速器/企业代理的机器上也成立);
- 商店版 → 「更新」拉起系统商店更新对话框 (不离店)。

**成功的样子**: 开着 SteamTools (hosts 劫持态) 的便携版实机上角标亮起;
商店版在有待装更新时角标亮起并可应用内更新。

## 2. 范围

### 腿 A: 角标 UI (danqing-log, `src/view.rs`)

- 底栏「⚙ 设置」文字右上角画 **6px accent 小圆点** (D1), 绝对定位, 天然零布局位移
  (自绘路径无布局参与 —— 不需要 pomodoro 的「常占槽位」对策, 见 D1 偏差说明)。
- 每帧查 `crate::app_update::hint()`, `Some` 才画; `None` **零痕迹** (与现状一致)。
- **两轨通用** (只读 `hint()`, 不知运输)。点击行为不变 (开设置卡); 不加 tooltip; 托盘不动。

### 腿 B: TLS 根证书走系统证书库 (danqing, `src/update.rs` + `Cargo.toml`)

- ureq 加 feature `platform-verifier` (`rustls-platform-verifier`, ureq 3.4.0 实证
  `platform-verifier = ["dep:rustls-platform-verifier"]`)。
- `fetch_update_status` 的 Agent 配 `TlsConfig::builder().root_certs(RootCerts::PlatformVerifier)`
  (默认是 `RootCerts::WebPki`; getter `TlsConfig::root_certs()` 存在, 可断言)。
- 只动 update 模块 (框架内唯一联网路径); 失败语义/重试语义/缓存策略**一律不动**。
- **只服务 GitHub 轨** (商店轨走 WinRT, 不经 ureq)。
- 联动链: danqing 三件套 → commit+push → danqing-log **关 patch** `cargo update -p danqing`
  复钉 → 两仓分别提交, message 注明关联。**patch 开着时 lock 是 path 态, 不许提交**。

### 腿 C: 商店轨更新检查 + 应用内「更新」(danqing-log, `src/app_update.rs`)

照 pomodoro `src/update.rs` `mod store` **成稿移植**, 三处分轨出口统一进 `app_update.rs`:

- **分轨判据 (D6)**: 运行时 `danqing::platform::is_packaged()`, **单二进制不变**
  (09-13 架构决定保留, 只翻「关掉」那半)。**不学** pomodoro 的编译期 `feature = "store"`
  (那是它三版本构建的历史形态; 本产品不留「手上这个包是哪个构建」的隐患)。
- **版本号来源**: GitHub 轨 `env!("CARGO_PKG_VERSION")`; 商店轨 MSIX 包身份版本
  (pomodoro `package_version()` 同款: `Package::Current().Id().Version()`, 三段丢 Revision,
  OnceLock 缓存, 无包身份回退编译期 + warn)。用途仅缓存换版作废闸门。
- **检查运输**: `StoreContext::GetDefault()` → `GetAppAndOptionalStorePackageUpdatesAsync`
  (RoInitialize 多线程, 成败都继续); 待装 0 个 → `UpToDate`, >0 → `UnknownVersion`
  (商店拿不到新版本号, 侧载实测见 pomodoro 注释); 任何失败 → None 静默 (家族约定不变)。
  流程与框架 `spawn_check` 同构: 缓存闸门 → 后台线程 → `publish`。
- **hint 覆写**: 商店轨 action 文案 **"更新"** (框架只产「前往下载」); status 无版本号
  「有新版本」(`UnknownVersion` 语义)。
- **动作**: `RequestDownloadAndInstallStorePackageUpdatesAsync` + `IInitializeWithWindow`
  挂主窗口属主 + `AtomicBool` 防重入 (pomodoro `request_update` 同款纪律);
  0 更新时空返回。`go_download` 更名 `perform_action` (名字随双轨语义), 调用点
  `settings.rs:676` 一行跟进。
- **隐私披露**: `docs/privacy-policy.md` 「唯一的联网动作」表加商店轨一行 (系统 Store 服务
  查询待装更新, WinRT broker, 不发 HTTP 无参数无请求体); `docs/ms-store-copy.md`
  「上架时必须改什么」清单挂「同步商店字段副本」。v1.0 的历史表述按版本钉不动。

### 不做 (裁决在案)

- 不动托盘图标 / 不加 tooltip / 不改角标点击行为 (D3)。
- 不做代理设置 / 镜像源 / 检查重试 UI (超范围)。
- **不含发布链** (D4): 打包 / GitHub Release / Partner Center 提交另行点头。
- 不动 pomodoro (框架改动自然惠及, 各自复钉 rev 时生效)。

## 3. 决策 (2026-09-22 用户裁决 + 实现裁定)

| # | 决策 | 内容 |
|---|------|------|
| D1 | 角标形态 | **小圆点**: 6px, `th.accent()` token (不自定义色), 挂「⚙ 设置」文字右上角, 绝对定位零布局位移。**锚点 (评审修订)**: Y 与**文字顶**齐平 (锚文字不锚行顶 —— 行顶会把点浮到标签上方), X 在文字右缘外 2px (验收预览形态); 命中矩形**常算吞并角标** (显隐零位移 + 整个角标点得到)。与 pomodoro 成稿同形同色; 偏差仅占位策略 (它是 widget 树常占槽位, 我们是自绘绝对定位 —— 后者天然无位移, 不需要槽位) |
| D2 | TLS 信任 | **系统证书库** (rustls + `RootCerts::PlatformVerifier`), 不换 native-tls 后端 (curl 实测 schannel 有吊销检查坑)。安全权衡已裁: 最坏代价是攻击者**伪造或压制**「有新版」提示、并观察检查时机 (固定 UA 无 PII); 下载页 URL 是编译期常量改不动。边界靠「远端 JSON 永不作定位符」成立 (框架 `fetch_update_status` 不变量注释, 评审补全最坏情形) |
| D3 | 行为不变量 | 无新版零痕迹 / 点击照旧开设置卡 / 无 tooltip / 托盘不动 |
| D4 | 发布 | 不含本次范围, 走到 code-simplify 收口为止 |
| D5 | 商店轨 | **进 C 完整双轨** (翻案 2026-09-13「商店版整条关掉更新检查」)。三理由现状: 「别把用户导向站外」被商店轨本身消解 (动作在店内); 「商店代管更新自查多余」pomodoro 用行动否了; 「零网络主张」走披露 (Store broker 非应用 HTTP, 隐私表如实加行) |
| D6 | 分轨判据 | 运行时 `is_packaged()` **单二进制** (09-13 架构保留); 判据取反 = 商店版走 GitHub 轨发 HTTP = 隐私主张变假 —— **这是本批最大风险, 双向锁死** (见 §5) |

## 4. Commands / Structure / Style (增量, 其余沿用两仓 CLAUDE.md)

- 命令不变: `cargo test` / `cargo clippy --all-targets -- -D warnings` / `cargo fmt`。
- 改动文件:
  - 腿 A: `danqing-log/src/view.rs`
  - 腿 B: `danqing/Cargo.toml` + `danqing/src/update.rs`
  - 腿 C: `danqing-log/src/app_update.rs` (双轨分派 + `mod store` 移植) + `danqing-log/src/settings.rs` (调用点一行) + `docs/privacy-policy.md` + `docs/ms-store-copy.md`
- 无新 `.rs` 文件 (文件头规则不触发)。注释一律中文, 决策处写「为什么」, 09-13 翻案处留翻案记录 (不抹旧理由, 改写为「为何不再成立」)。
- 计划/任务文档按农场惯例: `danqing-log/tasks/plan-update-badge.md` + `tasks/todo-update-badge.md`。

## 5. Testing Strategy

**注入惯例 (照 VersionRow 成例, `settings.rs:937`)**: 构造注入状态字段, **不碰全局 `publish`**
(框架注释明说全局静态测试会并行互踩 —— 唯一例外见下), **不触网不触商店**。

腿 A 新增 (danqing-log main 测试族):
- `settings_button_shows_update_dot_only_when_hint_exists` —— 有提示画圆点 / 无提示零痕迹,
  两态对拍 (A/B: 摘掉绘制须精确红, 先红后绿才算锁住)。
- `update_dot_sits_at_the_label_corner_and_outside_layout` —— 圆点几何 (6px, 文字右上,
  整体落在命中矩形内), 与 paint 同源的纯几何函数, 拒绝两处各算一遍。
- `log_view_paint_wires_the_update_dot` —— **接线锁** (评审补): `LogView::paint` 真画出来,
  字段注入两态 (注入惯例), 删调用行/写死 false 必红 —— 锁不能停在 helper 层。
- 颜色按 token 断言 (`th.accent()`), 不钉色号 —— 与 `cell_highlight_colors` 同款抽函数锁法。

腿 B 新增 (danqing lib 测试族):
- `update_agent_trusts_platform_root_certs` —— Agent 的 `TlsConfig::root_certs()` 断言为
  `RootCerts::PlatformVerifier` (编译+配置面保障; 真网络行为无法单测, 归人工验收)。

腿 C 新增 (danqing-log main 测试族):
- `dispatch_polarity_is_never_inverted` —— 分轨极性**双模型双臂锁死** (评审后加固):
  `track_of` (派轨) + `github_http_allowed` (GitHub 臂体内的隐私双保险, 直读包标识
  独立于分派)。**本批最大风险是判据取反** (商店版发 GitHub HTTP = 隐私主张变假) ——
  只锁谓词恒等函数不够 (评审 PoC: 接线上取反测试全绿), 故臂体另有闸, 破主张需
  **两次独立错误**。原 `updates_are_enabled_in_a_plain_binary` 的「取反风险」语义并入。
- `store_track_hint_overrides_action_to_update` —— 注入 `UnknownVersion` 缓存 →
  hint = 「有新版本」+ action「**更新**」; `KnownVersion` + GitHub 轨保持「前往下载」;
  注入 `UpToDate` → None (照 pomodoro `store_track_hint_action_overridden` 成稿,
  改运行时分轨的纯函数形态可两臂都测)。
- `store_cache_file_name_derives_from_repo_slug` —— `-msix` 缓存文件名从 repo slug 派生
  (评审 Nit: 防手工副本漂移)。
- 商店运输本身 (StoreContext) 不进测试 (WinRT 不可单测, 沿袭 store_license 惯例), 归人工验收。

基线纪律: danqing-log **253** 不许破 (09-20 实测值; spec 初稿误抄 CLAUDE.md 冻结的
184, 2026-09-22 对账时发现并改); danqing lib 默认 **617** / `--features update` **646**
不许破。真实网络/商店调用一律不进测试 (tests-must-not-touch-real-desktop 精神)。

## 6. Boundaries

- **Always**: 提交前三件套全绿; 中文注释; 颜色/间距走 token; 基线不破; 联动两仓分别提交;
  文档宣称的测试名写完顺手 grep 确认真存在 (`settings.rs:953` 教训); 隐私表与行为同步改。
- **Ask first**: 加/换依赖 feature (D2 已批, 在案); 改 hint 语义/失败语义; 动托盘; 发布链;
  改动超过 §4 文件清单。
- **Never**: 自定义色/魔法值绕开 Theme; 测试触网/触商店/真实桌面副作用; patch 开着时提交 lock;
  无 hint 时留任何角标残留; 判据取反让商店版走 GitHub 轨; 提前勾 `SPEC-update-check.md:97`
  (落地后才勾)。

## 7. Success Criteria

机器部分 (2026-09-22 build 收口 + 评审修复后复测):
1. [x] 两仓 `cargo test` 全绿: danqing-log **258** (基线 253 − 1 并锁 + 4 新增 + 评审补 2 锁:
   88 lib + 159 main + 8 genlog + 3 keygen) / danqing `--features update` **646 过 + 1 ignored**
   (默认面 617 过 + 1 ignored)。新增锁先红后绿, A/B 留痕**四**条:
   `实际: WebPki` / `打包 (MSIX) 必须走商店轨` / `有提示恰画一个圆点 left: 0 right: 1` /
   `角标恰添一个矩形 … left: 0 right: 1` (接线锁, 评审 PoC 验真)。
2. [x] 两仓 `cargo fmt` + `cargo clippy` (`--features update` 与默认双模式) 零警告。
3. [x] `danqing/docs/specs/SPEC-update-check.md:97` checkbox 已勾 (带日期注)。
4. [x] 隐私表 + 商店检查单 + listing 文案已改 (腿 C 披露不落账 = 流出仓库边界事故,
   `ms-store-copy` 教训; 评审补抓 §三「商店版不会有这个提示」/ 缓存表「仅便携版」/
   listing「不联网」三处漏网, 已一并清)。

人工验收 —— **便携版** (用户实机, **开着 SteamTools = hosts 劫持态**; **2026-09-22 全部通过**):
- [x] a) 启动后日志**无**「更新检查失败」; `%APPDATA%\danqing\update-check-14uncle-danqing-log.json` 落盘。
- [x] b) 底栏「⚙ 设置」右上出现小圆点; 设置卡「关于」页见「有新版本」行 (实机植缓存出 v1.0.2, 观感反馈催生 SPEC-update-hint-ui)。
- [x] c) 零痕迹对照: 缓存手改 `UpToDate` → 圆点消失, 底栏与现状无异。
- [x] d) 点击照旧开设置卡; 「前往下载」跳 `https://github.com/14uncle/danqing-log/releases/latest`。
- [x] e) 圆点观感过目 (6px/accent 是拟态值, **位置**含「文字顶齐平」锚点 —— 偏小/偏色/偏位当场裁, 不预设翻案)。

人工验收 —— **商店轨** (侧载 MSIX 构建; StoreContext 运输侧载下不可全验, 如实分两截):
- [ ] f) 侧载启动: 日志出现商店更新查询条目 (pomodoro 同款「商店更新查询: 待装更新 N 个」),
  **零** GitHub HTTP 相关日志 (判据正向的实机证据)。
- [ ] g) 待装 0 个 → 零痕迹 (与 c 同款对照)。
- [ ] h) **植缓存出 UI**: 手写 `update-check-14uncle-danqing-log-msix.json` (`UnknownVersion` + 新鲜时间戳) → 重启 →
  角标亮 + 「有新版本」(无版本号) + 按钮「**更新**」; 点击空返回不炸 (侧载无待装更新)。
- [ ] i) **真商店更新流** (有待装更新 → 点「更新」→ 系统对话框装完) **不在本次可验范围**
  (侧载包商店不推送) —— 最终验收发生在下次商店提交、旧版见新之后, 到时按此单补勾。

## 8. Open Questions

- (小) 圆点最终直径/颜色若实机观感不佳, 以验收 (e) 当场裁决为准, spec 回写。
- (defer) 下次商店提交的版本号 (1.0.2 / 1.1.0) 未定 (D4 发布不在本次); 隐私表生效版本
  表述届时按提交版本钉。
- (defer) pomodoro 是否跟进复钉 rev (框架腿 B 自然惠及) —— 超范围。
- (记录) 验收 (i) 挂起至下次商店提交后补勾, 属**已知的验收欠账**, 不是遗漏。
