# TODO: v1 — 过滤/搜索栏重构为真 TextInput

> 裁决: 2026-09-05 review 后用户定为 v1 任务; 焦点模型 = **方案 A**(默认无焦点→app 导航,
> 点栏/Tab 进栏, 搜索栏打开自动聚焦; 进表格模式自动聚焦过滤栏保留「打字即过滤」).
> 参考: danqing-clipboard `src/ui/bottom_bar.rs` (TextInput 容器 + 焦点/IME/命中转发 + clear-revision)。

## [danqing] (库层, 分别提交)

- [ ] **T0: TextInput 加 caret_color / selection_color setter** (字段已存在, 只加 builder)
  - 深色 UI 里玉色光标对比度不足; chromeless + 显式色匹配暗色栏
  - Verify: `cargo clippy -D warnings` + 测试 (danqing 仓)

## [danqing-log] (产品层)

- [ ] **T1: Bar widget** (view.rs): 托管 filter_ti + search_ti 两个 TextInput (chromeless)
  - sync: mode/search_open/filter_applied/search_query + 两个 clear revision
  - layout: 可见 FILTER_BAR_H, 否则 0; paint: 外壳(bg+前缀label) + active TextInput
  - event: 拦截 Enter/Esc/PageUp/PageDown, 转发其余给 active TextInput
  - focusable(true) + focus_id("log-bar") + 转发 wants_ime/ime_area/selected_text/hit_area/reset_focus/animate
- [ ] **T2: App::view → Column[Bar, LogView]**; LogView 去掉栏绘制, chrome_top 只留 HEADER_H
- [ ] **T3: App 侧**: 删 search_input/filter_input 实时字段, 加 clear revision;
  App::event 删栏输入路由 + IME + paste; 加 Msg 处理 (ApplyFilter/ApplySearch/CloseSearch/ClearFilter)
- [ ] **T4: 删三补丁**: wants_ime override / Event::Ime 分支 / paste+read_clipboard
- [ ] **T5: 焦点** focus_request flag (open_search → search-bar; 进表格 → filter-bar), focus_restored 清标志
- [ ] **T6: 三件套 + review + 人工验收** (中文 IME 候选窗贴框/粘贴/编辑/导航不冲突)

## 风险备注

- 深色 UI: TextInput 用 chromeless + 显式 caret/selection (依赖 T0)
- 「边输边滚」丢失: 栏聚焦时方向键移光标; PageUp/Down 由 Bar 拦截转滚动
- 树持久 (mod.rs:207 app.view() 只调一次), TextInput 保活焦点/IME/光标
