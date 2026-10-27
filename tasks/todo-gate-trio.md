# todo: 三连接门（gate-trio）—— 免费层欠账三连改判付费的接门活

- @author 十四叔
- @date 2026/09/27
- 性质: **无 spec 轻 todo**（SPEC-v1x-merge-timeline D8 裁决: 照 export/sessions 先例纯接线,
  与腿一同窗口）; 权威 = 2026-09-27 用户裁决②（地图翻案块② / ROADMAP §一 翻案块）
- 前提: 三连功能本体与人工验收已按免费层建成（**零返工**）; 本清单只做门控接线与提示位

## G0 门控口径钉死（动手前先核这张表）

| 功能 | Feature 枚举 | 门控点 | 免费态行为（裁决在案） |
|---|---|---|---|
| 列配置三件套 | `ColumnConfig` | **入口**: 「列管理」弹层入口 + 表头拖拽手势起点（拖宽/换位） | 默认列摆法照用（v1.0 行为）; 手势不启动, 弹升级对话框; state.json 列配置段不读不写 |
| 书签持久化 | `BookmarkPersist` | **通路**: state.json bookmarks 段的读与写 | **会话内书签照用**（v1.0 行为不动, toggle 不拦截不弹窗）; 跨重启不恢复; 提示位 = 设置卡许可页一行, **不在 toggle 时弹对话框** |
| 免语法字段查询 | `FieldPicker` | **入口**: 过滤栏「字段…」按钮 | 手输迷你语法照用; 按钮点击弹升级对话框（.log 本无此按钮 = 自然免费直用） |

共同纪律: 两道闸（数据永在: 已付费生成过的 state.json 数据在免费态不删不改）;
升级对话框复用 `ShowUpgradePrompt(Feature::X)` 先例; 注入惯例两态锁（免费弹/付费不弹）。

## 任务

- [x] G1 `license.rs` Feature 加三枚举 + `label()`（「列配置」「书签持久化」「字段查询」）+ 既有三锁跟进
- [x] G2 列配置接门: 弹层入口 + 手势起点双拦截; 免费态 state.json 列配置段不读不写（付费期已写的保留不删）; 两态锁先红后绿
- [x] G3 书签持久化接门: 载入/落盘通路按 `entitlement.allows(BookmarkPersist)` 收口（toggle/会话内行为零变化 —— 既有 413 内相关锁不许动语义）; 设置卡许可页提示行; 两态锁
- [x] G4 字段点选接门: 「字段…」按钮入口拦截; 两态锁
- [x] G5 三件套复核（fmt/clippy/test 全绿）+ 各 spec 人工验收节加「免费态门控」补验注记（A/D/E 组已过的条目不动, 增补门控两条待实机: 免费态拦截可见 / 付费态功能如旧）

      —— **2026-09-28 落地 (448 绿 = 165 lib + 269 main + 11 genlog + 3 keygen)**:
      G1 三枚举 (ColumnConfig/BookmarkPersist/FieldPicker) + label + 两枚举锁跟进;
      G2 入口 (OpenColMenu) + 手势起点 (view.rs sync 快照 col_gesture_free, 拖宽/换位
      双拦) + 动作兜底 (ColumnWidthSet/Clear/MoveBefore) + 通路段「不读不写」
      (免费期运行态不上账, **付费期已写保留不删** = 降级不毁数据);
      G3 通路型 (load/save 两段同规, toggle 不拦不弹窗) + 许可页提示行
      (免费态才显示; PANEL_CONTENT_H 192→208 = 实测 207.5 就地盖住, 不预留);
      G4 「字段…」入口拦 (手输迷你语法照用); G5 三件套 + 三 spec 验收节补验注记。
      **既有锁 10 处按注入惯例注付费态, 断言零改动** (「不许动语义」的正确读法);
      真发现: 既有 `toggle_bookmark_persists_and_reloads` 验的就是付费通路语义
      (增即落盘) —— 功能收付费后锁的运行态必须跟着走, 语义不动。

**窗口**: 腿一 T3–T4 期间顺手; 完成即在本文件勾账, 并进当次联动提交（message 注明 D8 另案）。
