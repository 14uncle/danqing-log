# SPEC-v1x-map: v1.x 付费层能力地图

- @author 十四叔
- @date 2026/09/19
- 状态: **已批准**（2026-09-19 用户「go」）—— 本文件是 v1.x 各模块 spec 的唯一索引；模块边界、依赖方向、构建顺序以本图为准，不猜文件名

范围权威源：`../ROADMAP-v1x.md`（付费层唯一权威清单）。本图不发明需求，只把 ROADMAP 切成可独立验收的模块。

## 前置裁决（2026-09-19 用户四裁，全按推荐项）

1. **GitHub 渠道收费轨**：License key + Lemon Squeezy / Gumroad 代销（MoR 代办全球增值税；key 离线签名校验，不联网）。商店版照 ROADMAP §三走 Store trial + IAP —— **双轨**
2. **便携版无试用钟**：免费层本身就是 demo（「看懂」全功能）；纯离线便携包的试用钟防不住（删配置即重置），不写假安全。商店版 Store 托管 trial 是平台强制执行的真试用，保留
3. **首波范围**：基建（licensing）+ 腿二（字段分析）+ 腿三（导出）+ 腿四（会话持久化）。腿一（多文件时间戳合并）是 intent 三大技术风险之一、引擎 + UI 双改，**第二波单独起 spec**
4. **免费层欠账搭车**：列配置三件套（拖拽/显隐/重排）+ 书签持久化 + 免语法字段查询 UI —— 与腿四共用状态持久化基建，成本协同。**行多选**（前置 = 框架 `Event::MouseInput` 加修饰键）与 **.log 行首过滤通路**各是 M5 级，另排

## 前置裁决翻案（2026-09-27 用户裁决，四路竞品调研后；调研落盘 `../research-v1x-paid-tier-2026-09-27.md`）

1. **翻 09-19 #3**：腿一（`merge-timeline`）从「第二波单独起 spec」提前为 **v1.x 发布前置**——
   下一件即起 spec。证据：合并 = 品类付费入场券；现三腿买家实锤≈零；腿一是其余三腿的承重梁
   （合并工作区的会话 / 合并时间线的导出 / 跨文件统计才是真「批量·留存·交付」）。
2. **翻 09-19 #4**：免费层欠账三连（`table-column-config` / `bookmark-persist` / `field-picker-ui`）
   **改判付费层**——功能本体与人工验收已按免费层建成（**零返工**），**接门已建成**（09-28
   `todo-gate-trio` G1–G5：`Feature::ColumnConfig/BookmarkPersist/FieldPicker` 三枚举 + 主/视/
   设置三处调用点齐，弹层入口 + 表头手势起点 + state.json 读写通路 + 「字段…」按钮 + 合并入口
   全接）；会话内书签与 .log 直用场景保持免费（v1.0 既有行为不动）。
3. **新增**：腿五候选 = 远程日志源，挂 ROADMAP 待裁（本轮不建）；EULA 身份门收；更新窗 = 永久买断。
4. **v1.x 发布延期等腿一**；发布链动作（merge/tag/打包/商店提交）另行点头。

## 模块表

| 模块 id | 职责 | 依赖 |
|---|---|---|
| `licensing` | 收银台：便携版 license key 离线签名校验 + 商店版 Store trial/IAP 接线 + 未授权降级免费层（不变砖）+ 设置卡许可区 | — |
| `field-analytics` | 腿二：字段聚合——数值列分布/分位数（p50/p99/max）、枚举列取值分布 | `licensing`（付费门） |
| `export` | 腿三：过滤结果导出 / JSON 美化导出 / CSV | `licensing` |
| `table-column-config` | ~~免费层欠账~~ **付费层**（09-27 翻案）：列宽拖拽 / 列显隐 / 列重排；**接门已建（09-28）** | `licensing`（门已接） |
| `bookmark-persist` | ~~免费层欠账~~ **付费层**（09-27 翻案）：书签跨重启持久化（会话内书签仍免费）；**接门已建（09-28）** | `licensing`（门已接） |
| `field-picker-ui` | ~~免费层欠账~~ **付费层**（09-27 翻案）：免语法字段查询 UI（.log 直用场景仍免费）；**接门已建（09-28）** | `licensing`（门已接） |
| `workspace-sessions` | 腿四：命名工作台会话（过滤器组合 + 搜索历史 + 列配置 + 展开态）跨会话保存/切换 | `licensing`, `table-column-config` |
| `merge-timeline` | 腿一：多文件时间戳合并 + req_id 跳转——**v1.x 发布前置（09-27 翻案），下一件起 spec** | （live-tail 增量索引，已存在） |

依赖方向单向，无环。`licensing` 只造机制（授权状态 + 门控查询 + 升级提示），被门控的功能本体在各自模块。

## 构建顺序

`licensing` → `field-analytics` → `export` → `table-column-config` → `bookmark-persist` → `field-picker-ui` → `workspace-sessions`

- 先装收银台再摆商品：licensing 可暗发（免费层行为零变化）
- `workspace-sessions` 排最后：它要持久化的列配置得先存在
- 三个免费层欠账模块插在付费腿之间——它们不依赖 licensing，可被任何优先级调整，但不得插到 licensing 之前（收银台是这波存在的理由）

> **2026-09-27 翻案后**：`merge-timeline` 已五段全闭（09-28）；三连接门同窗口已建成
> （`todo-gate-trio` G1–G5）。**余下 = 人工验收 G 组九条（待实机）+ 收银台开业 + 发布链**。

## 地图备注（随图一并批准）

1. **腿二有引擎前置**：字符串值切取换真 parser 的边界问题（`SPEC-jsonl-table.md:16` 记录在案）须先收——聚合是提取结果的二次消费，边界错误会被放大。动 `danqing-logfile`，一次联动
2. **商店侧外部硬顺序**：add-on/trial 只能等 v1.0 父应用过审发布后提交（pomodoro 实测）。licensing 商店半边的**提交动作**排在认证后，**代码**可先备
3. **离线 key 是君子协定**：签名校验只证明「拿到过真 key」，挡不住分享。$29 开发者工具按 Sublime 模式接受此性质，不搞假安全

## 用户侧并行动作（不挡 spec，挡收银台开业）

注册 **Lemon Squeezy**（或 Gumroad）+ 打通提现——Partner Center 同类活。模块 spec 中代销商相关一节做成可替换。

## 模块 spec 索引

| 模块 | spec 文件 | 状态 |
|---|---|---|
| `licensing` | `SPEC-v1x-licensing.md` | **五段全收口 + 人工验收过**（09-19 收口; 09-20 三轮实机过, 真公钥已回填） |
| `field-analytics` | `SPEC-v1x-field-analytics.md` | **五段全收口 + 人工验收过**（09-19 收口; 09-20 三轮实机过） |
| `export` | `SPEC-v1x-export.md` | **五阶段全闭**（2026-09-23 一日走完, 314 测试绿）; **人工验收四条 2026-09-27 用户实机全过**（总清单 B 组, 含 Excel 开 CSV） |
| `table-column-config` | `SPEC-v1x-table-column-config.md` | **五段全闭**（2026-09-23 一日走完: 365 测试绿, 双路评审 Critical×1+Required×6 全修 + Nit×5 清零, 零框架/引擎改动）; **人工验收六条 2026-09-27 用户实机全过**（总清单 A 组; A3 勘误验点经 E5 兼验） |
| `bookmark-persist` | `SPEC-v1x-bookmark-persist.md` | **五段全闭**（2026-09-23 一日走完: 双路评审 Critical×1+Required×6 全修 + simplify, 376 测试绿, 零框架/引擎改动）; **人工验收五条 2026-09-27 用户实机全过**（总清单 D 组） |
| `field-picker-ui` | `SPEC-v1x-field-picker-ui.md` | **五段全闭**（2026-09-23: 含 T0 col_menu 启动快照修复; 双路评审并账 Critical×1+Required×6 全修（拼子句守卫/镜像退役回归持有者收口/载荷锚定/键路门禁）; 2026-09-24 simplify 8 项行为零变化收口, 393 测试零修改, 零框架/引擎改动; **人工验收五条 2026-09-27 用户实机全过**（E 组; E5 兼 table-column-config A3 勘误验点）） |
| `merge-timeline` | `SPEC-v1x-merge-timeline.md` | **五段全闭**（09-28 一日：T0 原型 → T1–T2 引擎时间戳解析 + 合并索引 → T3–T5 视图模型 + 源管理 + 时钟校准 → T6 req_id 追踪 → T7 live-tail 合流 → T8 会话载荷 merge group → T9 logbench 四组实测回填；双路评审 Critical×2+Required×4 全修 + simplify 7 项零变化，494 绿；联动 logfile `4511126` 已 push，本仓 `b64357a`）；**人工验收九条（G 组）待实机**（需付费态 key，见 `../../tasks/acceptance-pending.md`） |
| `workspace-sessions` | `SPEC-v1x-workspace-sessions.md` | **五段全闭**（2026-09-24 一日: 「go」×2 → /build auto T1–T4（407 绿）→ 双路评审并账 Critical×3+Required×5 全修（账本可辨谓词/上限取齐/点穿/假绿锁/护在途/拒收显式清空）→ simplify 3 项零变化收口, 413 测试零修改, 零框架/引擎改动; **人工验收五条 2026-09-27 用户实机全过**（F 组）） |

每模块按 spec → plan → build → review → code-simplify 五段推进，spec 写完不立即编码。
