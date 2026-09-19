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

## 模块表

| 模块 id | 职责 | 依赖 |
|---|---|---|
| `licensing` | 收银台：便携版 license key 离线签名校验 + 商店版 Store trial/IAP 接线 + 未授权降级免费层（不变砖）+ 设置卡许可区 | — |
| `field-analytics` | 腿二：字段聚合——数值列分布/分位数（p50/p99/max）、枚举列取值分布 | `licensing`（付费门） |
| `export` | 腿三：过滤结果导出 / JSON 美化导出 / CSV | `licensing` |
| `table-column-config` | 免费层欠账：列宽拖拽 / 列显隐 / 列重排 | — |
| `bookmark-persist` | 免费层欠账：书签持久化 | — |
| `field-picker-ui` | 免费层欠账：免语法字段查询 UI（列发现采样结果做成下拉点选） | — |
| `workspace-sessions` | 腿四：命名工作台会话（过滤器组合 + 搜索历史 + 列配置 + 展开态）跨会话保存/切换 | `licensing`, `table-column-config` |
| ~~`merge-timeline`~~ | 腿一：多文件时间戳合并 + req_id 跳转——**本波只占位，不起 spec** | （live-tail 增量索引，已存在） |

依赖方向单向，无环。`licensing` 只造机制（授权状态 + 门控查询 + 升级提示），被门控的功能本体在各自模块。

## 构建顺序

`licensing` → `field-analytics` → `export` → `table-column-config` → `bookmark-persist` → `field-picker-ui` → `workspace-sessions`

- 先装收银台再摆商品：licensing 可暗发（免费层行为零变化）
- `workspace-sessions` 排最后：它要持久化的列配置得先存在
- 三个免费层欠账模块插在付费腿之间——它们不依赖 licensing，可被任何优先级调整，但不得插到 licensing 之前（收银台是这波存在的理由）

## 地图备注（随图一并批准）

1. **腿二有引擎前置**：字符串值切取换真 parser 的边界问题（`SPEC-jsonl-table.md:16` 记录在案）须先收——聚合是提取结果的二次消费，边界错误会被放大。动 `danqing-logfile`，一次联动
2. **商店侧外部硬顺序**：add-on/trial 只能等 v1.0 父应用过审发布后提交（pomodoro 实测）。licensing 商店半边的**提交动作**排在认证后，**代码**可先备
3. **离线 key 是君子协定**：签名校验只证明「拿到过真 key」，挡不住分享。$29 开发者工具按 Sublime 模式接受此性质，不搞假安全

## 用户侧并行动作（不挡 spec，挡收银台开业）

注册 **Lemon Squeezy**（或 Gumroad）+ 打通提现——Partner Center 同类活。模块 spec 中代销商相关一节做成可替换。

## 模块 spec 索引

| 模块 | spec 文件 | 状态 |
|---|---|---|
| `licensing` | `SPEC-v1x-licensing.md` | review 完成（2026-09-19, 231 测试绿）, 待 code-simplify |
| `field-analytics` | `SPEC-v1x-field-analytics.md` | review 完成（2026-09-19, 6 Required 全修, 250+68 绿）, 待 code-simplify |
| `export` | `SPEC-v1x-export.md` | 未开工 |
| `table-column-config` | `SPEC-v1x-table-column-config.md` | 未开工 |
| `bookmark-persist` | `SPEC-v1x-bookmark-persist.md` | 未开工 |
| `field-picker-ui` | `SPEC-v1x-field-picker-ui.md` | 未开工 |
| `workspace-sessions` | `SPEC-v1x-workspace-sessions.md` | 未开工 |

每模块按 spec → plan → build → review → code-simplify 五段推进，spec 写完不立即编码。
