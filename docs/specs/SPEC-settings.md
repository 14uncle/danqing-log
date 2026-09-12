# Spec: 设置入口 + 轻量设置卡 (settings)

> @author 十四叔 · @date 2026/09/06
> 前置: interview-me + 用户裁决「轻」 + **移除只读快捷键对照表** + 设置入口放**底部状态栏**。
> 依赖框架 `danqing::update`(`../danqing/docs/specs/SPEC-update-check.md`) 做版本检查。
> 复用 clip 的 `Stack + Box(ui) + Center` 浮层卡片模式(`../danqing-clipboard/src/ui/settings.rs`)。

## Objective

danqing-log 无任何设置/关于入口。补一个**轻量**设置卡:

- 状态栏**右下** ⚙/「关于」入口(参考 clip `BottomBar` 设置入口)。
- 点开是全窗 scrim + 居中玻璃卡(非 tab 单页, 因已移除快捷键表无需 MultiPanel):
  - **关于**: 产品名(`丹青日志`) + `vX.Y.Z` + 设计一句话;
  - **版本行**: 来自 `danqing::update::current_hint()`, 有新版显示「有新版本 vN」+ 「前往下载」(跳 GitHub 发布页);
  - **问题反馈**: 链接跳 GitHub Issues。
- 关闭: ✕ 按钮 / Esc / 点遮罩。

## Tech Stack

- 产品 Cargo.toml: `danqing = { features = ["update"] }`(开启版本检查核心); 不新增其他产品侧依赖
  (设置卡用框架 `Box/Center/Column/Row/Stack/CloseButton`; 打开链接用框架已引的 `open` 或产品 `open`)。
- 版本检查: 直接调 `danqing::update::{spawn_check, current_hint, UpdateSpec, perform_action}`, 不抄后端。

## Commands

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --release -- <日志>   # 人工: 状态栏入口 → 设置卡 → 版本行/反馈链接
```

## Project Structure

```
src/status.rs(或并入 main.rs/view.rs) ← 状态栏右下设置入口, 或复用现有 STATUS_HEIGHT 区
src/settings.rs   ← settings_card(): Stack[scrim, Center[Box(card)]]; about/版本行/反馈 Link
src/app_update.rs ← 薄接线: UpdateSpec(repo=14uncle/danqing-log, user_agent, releases_page,
                    current_version=env!("CARGO_PKG_VERSION")); startup spawn_check()
src/main.rs       ← Msg: OpenSettings/CloseSettings/OpenUrl/UpdateAction; focus/settings 态;
                    布局: Column[TitleBar, LogView.fill] 的 status 区放入口; settings 卡 overlay
```

## Code Style

沿用 clip `settings.rs` 范式:

- `settings_card()` = `Stack::new().child(Box(scrim)).child(Center::new(card).fill_max())`。
- `card` = `Box(card_bg).radius(lg).child(Padding[…, Column[…]] )`。
- `Link`(可点文字 + hover 下划线 + 打开 URL)、`ghost_button`、收起/关闭行(CloseButton)。
- 版本行: `Text::bind(|app| update::current_hint() 的状态文案)` + 按钮(`perform_action`)绑定, 只读 UI。

## Testing Strategy

- 设置卡布局/事件单测(参照 clip settings.rs 的 `#[cfg(test)]`): 入口 hover/点击 → `OpenSettings`;
  遮罩/Esc → 关闭; Link 点击 → `OpenUrl`。
- 版本行(纯逻辑已在框架测过): log 侧只验证「有 cache 时渲染提示」绑定, 不触网; 无 cache → 无角标。
- 人工: 状态栏入口点开 → 卡显示名称/版本/设计/反馈链接; 链接能开浏览器; 有新版显示「前往下载」;
  遮罩/Esc/✕ 关闭; 焦点限制在卡内(焦点陷阱)。
- 三件套绿。

## Boundaries

- **Always**: 复用框架组件与 `danqing::update`; 设置入口在**状态栏**(不在标题栏);
  版本行/反馈只读; 卡用 scrim 浮层(不发明新原语)。
- **Ask first**: 做可编辑设置项(本期只读: 只有关于/版本/反馈, 无任何可配置开关);
  引入 MS Store 轨更新/授权(暂无 store 轨, 留 v1.x); 加 tab/MultiPanel(仅当未来有真设置项)。
- **Never**: 不把快捷键表放进设置(用户已移除); 不抄一份 update 后端(必须走框架 `danqing::update`)。
- **~~Never~~ 修订 (2026-09-12)**: ~~不在设置卡放付费墙(v1 全功能免费)~~ —— 原措辞与新分层冲突:
  **免费层 ≠ 全功能**(免费层 = 看懂, 付费层 = 批量/留存/交付, 见 `../ROADMAP-v1x.md`)。
  本期的真实约束是 **v1 不含任何付费/授权 UI**(v1 只有免费层)。MS Store 的 trial/授权状态展示
  归 v1.x —— 届时设置卡会新增授权行, 那时再放开这条。

## Success Criteria

- [ ] 状态栏右下有 ⚙/「关于」入口; 点开浮层卡, 点遮罩/Esc/✕ 关闭。
- [ ] 卡显示: 产品名 + `vX.Y.Z` + 设计一句话 + 问题反馈链接(GitHub Issues)。
- [ ] 版本行: `danqing::update` 查到新版 → 显示「有新版本 vN」+「前往下载」跳发布页;
  无新版 → 不显示角标; 网络失败 → 静默。
- [ ] 焦点限制在卡内(不影响底层表格模式); 三件套绿。人工「通过」。

## Open Questions

- 「设计一句话」的文案(如「大文件日志/JSONL 查看分析器」) → 发布命名定稿时一并定, 本期先用占位。
- `open::that`(反馈/发布页)由框架 `danqing::update` 引的 `open` 提供, 还是产品直接引 → 沿用框架
  已引的那份, 避免产品重复依赖。
