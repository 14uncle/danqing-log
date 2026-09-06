# TODO: v1 — 过滤/搜索栏重构为真 TextInput

> 裁决: 2026-09-05 review 后用户定为 v1 任务; 焦点模型 = **方案 A**(默认无焦点→app 导航,
> 点栏/Tab 进栏, 搜索栏打开自动聚焦; 进表格模式自动聚焦过滤栏保留「打字即过滤」).
> 参考: danqing-clipboard `src/ui/bottom_bar.rs` (TextInput 容器 + 焦点/IME/命中转发 + clear-revision)。

## [danqing] (库层, 分别提交)

- [x] **T0: TextInput 加 caret_color / selection_color setter** ✅ 提交 6929898
  - 深色 UI 里玉色光标对比度不足; chromeless + 显式色匹配暗色栏

## [danqing-log] (产品层)

- [x] **T1: Bar widget** ✅ 提交 c561816
- [x] **T2: App::view → Column[Bar, LogView]**; LogView 去掉栏绘制, chrome_top 只留 HEADER_H
- [x] **T3: App 侧**: 删 search_input/filter_input 实时字段, 加 clear revision;
  App::event 删栏输入路由 + IME + paste; 加 Msg 处理 (ApplyFilter/ApplySearch/CloseSearch/ClearFilter)
- [x] **T4: 删三补丁**: wants_ime override / Event::Ime 分支 / paste+read_clipboard
- [x] **T5: 焦点** focus_request flag (open_search → search-bar; 进表格 → filter-bar), focus_restored 清标志
- [x] **T6: 三件套 + review + 人工验收** ✅ GUI 通过 (中文 IME 候选窗贴框/真光标/粘贴/导航不冲突)
  fmt + clippy -D warnings + 56 lib/6 bin 测试全绿

## 实现备注

- 焦点: 框架首帧 `first_rebuild_auto_focuses_first_focusable` 自动聚焦首个 focusable=过滤栏;
  `focus_bar` flag 补中途 Ctrl+T 进表格/开搜索的聚焦
- Esc: Bar `handle_escape` 返回 Ignored ⇒ 框架未消费 Esc 清焦 ⇒ 用户可继续用键导航 (方案 A)
- 深色栏: TextInput chromeless + `.color(filter_fg)` + `.caret_color(浅)` + `.selection_color(暗)`
- 栏为 sibling: `Column[Bar(32px 或 0) + LogView(fill)]`, LogView 顶=表头(表格)/列表(原始)
