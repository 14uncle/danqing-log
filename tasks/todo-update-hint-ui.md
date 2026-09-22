# TODO: update-hint-ui
> Verify 栏数字 = 收口实测终值 (build 后回填)。基线: danqing-log 258, danqing 617(默认)/646+1ignored(update)。
> 2026-09-22 实机 bug 返修: 角标 y 双加 area.origin.y 跌出面板 → 修 + 补平移锁 (A/B 第 6 条)。
> 同日 review 轮 (REQUEST CHANGES→全修→APPROVE, 记录在 plan): R1 几何锚 tripwire / R2 双臂
> 动作模型 / R3 绑定上限防线 / R4 lock 流程红线 → 终值 **265** / **623** / **652**+1ignored (A/B 第 7 条)。

> 日期: 2026-09-22 · spec: `docs/specs/SPEC-update-hint-ui.md` · plan: `tasks/plan-update-hint-ui.md`
> 零 commit 推进; T5 是用户闸门, 未点头不动 git

- [x] T1: A框架 —— Tabs per-tab 角标
  - Acceptance: 「关于」页签文字右上可绘 6px accent 圆点 (顶齐平 + 右缘外 2px, token 同 D1);
    零布局位移; 不改页签命中/切换; 无角标零痕迹。API 命名面 T1 内定 (保留模式 `bind_*` 家族优先),
    语义契约以 spec §2 为准
  - Verify: danqing `cargo test` + `cargo clippy --all-targets -- -D warnings` 0; 框架锁先红后绿
    (摘掉 push 精确红: 恰一圆点实例 left:0 right:1 / 几何 / 两态对拍); `measure_tabs` 的
    `text_info.size` 是否含 icon 偏移已读源确认
  - Files: `danqing/src/widget/view/tabs.rs` (+ `lib.rs` re-export 若有新公开 API)
- [x] T2: A应用 —— 「关于」页签角标接线 (谓词经 `update_hint_override` 测试注入 —— 首版「VersionRow 保鲜 Cell」设计被 sync 自噬证伪, 已改)
  - Acceptance: `hint().is_some()` 驱动「关于」页签角标 (只标「关于」一个); 谓词与底栏同源
  - Verify: danqing-log `cargo test` 258+N; 接线锁 `about_tab_badge_follows_update_hint` 先红后绿
    (把 bind 谓词写死 false 须红)
  - Files: `danqing-log/src/settings.rs`
- [x] T3: 腿 B —— `Link` 泛化 + 版本行重排
  - Acceptance: `Link` 点击消息可定制 + 行内形 (自然宽, hover/下划线随文字块), 旧调用点零改动;
    新 `Msg::PerformUpdateAction` → `perform_action()`; 版本行 = `Center::new(Row[status, gap, link 形按钮])`
    (status text_secondary / 按钮 Link 语言); 手绘 `Cell<Rect>` + 硬编码 RGB 初值消灭; 无 hint 零高度
  - Verify: danqing-log 258+N; 四锁先红后绿 (`version_row_is_centered_with_link_action` /
    `version_row_action_button_emits_perform_update_action` / `link_open_url_behavior_is_preserved` /
    无 hint 零高度并入布局锁); A/B 留痕
  - Files: `danqing-log/src/settings.rs`, `danqing-log/src/main.rs` (Msg 变体 + 分发一行)
- [x] T4: 收口 —— 三件套 + 基线回填 (262 / 620 / 649+1ignored)
  - Acceptance: 两仓 fmt/clippy/测试全绿; plan Verify 栏数字回填; 文档宣称测试名 grep 确认真存在;
    spec §7 机器部分勾选
  - Verify: 两仓三件套 (验证落盘重定向取真退出码 + 读内容)
  - Files: plan/todo/spec 回填
- [x] T5: 联动落地 (2026-09-22 用户「push」放行)
  - Acceptance: danqing commit+push → danqing-log 关 patch 复钉 → 两仓分别提交注明关联; lock 无 path 态
  - Verify: 无 patch `cargo check --locked` 过 (实测 0); pinned `danqing#672241a` 下 265 全绿
  - Files: danqing `672241a` / danqing-log `8456e80` + `8299858`; lock 钉 `danqing#672241a67b4e98e`,
    logfile 原 rev `3f3cd03` 未动

## 人工验收 (build 后另行, spec §7) —— 2026-09-22 用户实机验收通过

- [x] a) 「关于」页签角标与底栏同显同灭 (植缓存两态)
- [x] b) 版本行居中协调 —— 裁决: **行内形通过**, 两行式不启用 (D5 收口)
- [x] c) link 形制按钮: 「前往下载」直达 GitHub release / 商店侧载「更新」拉起系统更新
- [x] d) 无 hint 对照: 页签无点、关于页无此行
