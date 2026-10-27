# todo: v1.x 腿一 merge-timeline

- @author 十四叔
- @date 2026/09/27
- plan: `plan-merge-timeline.md`（依赖图/验收标准/风险以它为准）; spec: `docs/specs/SPEC-v1x-merge-timeline.md`

## Phase 0 — 原型拆雷（引擎 + bin，不动产品）

- [x] T0a genlog 多源 fixture（`--merge <目录> <每源 MiB> <源数>`: 交错时间戳/每源偏移/乱序率/无 ts 率/格式轮换/req_id 跨源/JSONL+log 混合; 确定性; 真值表打印 + 对拍测试; genlog 8 基线不破）
      —— **2026-09-27 落地**: genlog 11 绿 (+3: parse_merge_mode / 拒收 / deterministic_and_truthful);
      真值表四对拍 (首行 ts 区间/偏移差 ±37/.log ISO 可解/相关 req ≥2 源); A/B 变异红 (skew 公式窜改→偏移差炸)
- [x] T0b timestamp.rs 雏形（ISO/log4j/epoch s/ms 手写）+ merge.rs 雏形（k-way 归并单调物化）+ logbench `--merge`
      —— **2026-09-27 落地**: logfile 83 绿 (+15: ts 格式矩阵/probe/JSONL 取值/归并三态/tie-break/继承回填/16B);
      A/B 两红 (摘继承→continuation 炸; 反转 tie-break→序炸); 两个真发现: **probe 顺序必须 log4j 先于 ISO**
      (ISO 无小数口径会吃掉逗号毫秒) / `lines_from` 产 (行号,内容) 元组
- [x] T0c 实测 3×1GB + 8×200MB 两组 → 校准 D5/D10 → **回填 spec §7 (i) 验收线**
      —— **2026-09-27 实测 (release)**: 3×1GiB 17.0M 行总墙钟 **1525ms** (开 92×3 + 提取 1025 + 归并 215);
      8×200MiB 9.3M 行 **896ms**; 索引 **16B/行实测精确** (259.2 MiB); 提取 2.6-3.4 GiB/s (R6 关闭,
      不需步进缓存); 详见下条 CP0 呈报
- [ ] **CP0 检查点: 用户确认实测数字**（不合预期 → 回 spec 谈分块，不硬推）—— **已过 (2026-09-27「go」)**;
      spec D5/§7(i)/Q5 已按实测回填 (红线 3×1GB ≤1.6s / 8×200MB ≤1.0s; 显式乱序窗口退役=min-head 钉住语义)

## Phase 1 — 引擎完整（danqing-logfile）

- [x] T1 timestamp.rs 完整（格式矩阵 + JSONL 字段发现 + 采样探测 + 失败原因类型 + 源时区/偏移 API; `ts_detect_*`/`ts_jsonl_field_discovery` 先红后绿; 68 基线不破）
      —— **2026-09-27 落地 (90 绿, +7)**: `detect_route` 采样探测 (512 行/4MiB 预算/≥50% 命中/continuation 不参评)
      + `TsRoute`(LogPrefix|JsonlField) + `ProbeFailure` 三因 label; 浮点 epoch 钉; log4j 先于 ISO 探测顺序锁;
      A/B 红: 字段优先序反转 → priority 测试炸
- [x] T2 merge.rs 完整（乱序窗口 [T0 数据裁定] + 无 ts 继承 + tie-break + 增量 append + 源掩码重归并 + 内存报告; 六组 merge_* 锁 A/B 留痕; 内存 ≤ 红线断言）
      —— **2026-09-27 落地 (95 绿, +5)**: `extract_append` (跳行断言锁) / `insert_rows` 尾端回找
      (排序键 (ts,src,line) 字典序, WALK_CAP 兜底, 不用二分=索引乱序下不全局有序) / `build_index_masked`
      **源序号不漂移**; A/B 红: 摘回找 → walkback 测试炸。归并核重构 `merge_from` (tie-break 比原源序号)
- [x] **CP1 联动**（届时单独点头）: logfile 三件套 → push → danqing-log 关 patch cargo check 复钉 → 两仓分别提交注关联
      —— **2026-09-28 落账**: logfile `d298937` 已推; 本仓关 patch 复钉
      (danqing `b9620db` = source_palette CP2 / logfile `d298937`),
      452 绿 (168+270+11+3) 后两仓分别提交注关联 —— danqing `b9620db` /
      本仓 `8dd06d1` (T3-T5+gate-trio 攒批同车), 均已推 dev。
      幽灵坑备查: 切 patch 后 clippy 陈旧 rmeta
      报 unresolved import → `cargo clean -p danqing-logfile` 解决

## Phase 2 — 产品（danqing-log）

- [x] T3 merge_view.rs 视图模型（MergeSource + MergeLines 二态 + 展开键 (src,line) + Workspace 双模式 + 归并 AsyncJob + 三栏绘制 + 滚动/选中/书签复刻; Single 零触碰; `merge_view_lines_contract` 先红后绿; 413 基线不破）
      —— **2026-09-27 落地 (432 绿 = 160 lib + 258 main + 11 genlog + 3 keygen)**:
      lib `merge_view.rs` (MergeState/window 窗口拷贝纪律/pack_key/position_of/
      next_bookmark_pos/fmt_time_of_day + build_merge worker, 8 锁); main.rs
      `Workspace::Single|Merge` 双模式 + cur_top/cur_selected 访问器路由 (~30 触点)
      + 生命周期 (start_merge/pickup/apply) + 守卫组 (poll_growth 冻结/toggle_follow
      告示 P24/apply_session 切 Single/reload_file 回 Single) + 7 行为锁; view.rs
      合并三栏 paint (时间|源|消息, row_y 同源) + sync 快照 + Bar/底栏钮/侧栏 merge
      门禁 + 真 paint 锁 (A/B 摘快照字形减)。**真发现**: ①小追加走同步通路不开
      open_job (测试断言改行为口径) ②`#[expect(dead_code)]` 在非 test 构建才成立、
      test 构建反报 unfulfilled → 用 allow+注释 ③探测 <3 行文件必拒 (继承
      jsonl::detect 证据不足不判) —— 人工验收须知。**已知边界 (T3 明言)**: 合并内
      无嵌套展开/无搜索过滤栏/无侧栏/无导出会话钮/无合并态复制 (待 T4-T7 波;
      spec 实现记收录)
      **勘误 (2026-09-28 家法核对)**: 本条括注里的「MergeLines 二态 / Lines 第四实现」
      是 plan 的措辞 —— 落地**没**扩 `expand::Lines` 枚举, 实为 `MergeState.filtered:
      Option<Vec<u32>>` + `row_at`/`window` 自成一路 (意图兑现、抽象分叉, 见 spec §9.9)。
- [x] **CP2 检查点: D11 源色板提请用户批准**（框架 token or 退路单色 chip）—— **已过 (2026-09-27「A」= 框架 token)**;
      框架 `Theme::source_palette()` 默认实现按 `background()` 亮度自动选明/暗两套
      (SceneTheme/LogTheme 零成本继承), 8 色 = 色相环均分逐支压/提亮度解出 WCAG AA
      (常驻面 4.5 / 瞬时面 3.0, 与产品侧语义色板同一把尺); 三锁 (AA 两档 / 两两可辨
      ≥25 / 亮度自动选板) + A/B 变异红 (亮橙→1.82 炸 AA 锁); 框架 630 绿 (+3)。
      产品侧: 合并源名着色 `source_palette()[src]` + 真 paint 锁 (字形色流找两支,
      A/B 统一 accent 红); **433 绿** (+1)。联动提交攒批待批 (patch 现开, lock path 态)
- [x] T4 源管理弹层 + 「合并…」入口 + `Feature::MergeTimeline` 门控两道闸 + 并集列（上限 24 首见截断）+ 混合源
      —— **2026-09-28 落地 (444 绿 = 165 lib + 265 main + 11 genlog + 3 keygen)**:
      `license.rs` Feature::MergeTimeline+label; `merge_view.rs` 并集列模型 (union_columns
      首见序/去重/截断 + union_cells 异源缺列留空(判据=schema 列集, 杂散字段不填) +
      route_label + **carry_view_state**(加/减源重建按路径搬书签/显隐/选中, 序号漂移仍找回);
      `settings.rs` 源管理弹层 (弹层族第七员: RowList 第四消费者+色块 with_swatch 加法 builder+
      并集列截断提示 merge_union_hint); `main.rs` 门控两道闸 (入口 OpenMergeMenu + 动作兜底
      merge_gate) + 加/减源/显隐/ToggleMergeWorkspace 臂 + apply_merge_outcome 接 carry;
      `view.rs` 底栏「合并…」+ Ctrl+M。**真发现**: rebuild_masked 后选中位漂到别的行
      (位置语义 vs 行身份) —— 修 = 选中按 (源,行) 锚定, 键不漂原则同书签 (修复前
      carry 锁红 = 天然 A/B)。锁: 门控两态/上限拒绝+去重/弹层互斥+Esc 插层/并集断言×3/
      carry 行为/色块×1/hint 三态。**T3 注释兑现**: StartMerge/ExitMerge 的
      allow(dead_code) 已删 (构造点接通)
- [x] T5 时钟偏移/时区（弹层编辑 + 解析边界单源施加 + 重归并; `offset_applied_at_parse_boundary`）
      —— **2026-09-28 落地 (452 绿 = 168 lib + 270 main + 11 genlog + 3 keygen)**:
      `merge_view.rs` set_time_params (只重提该源 ts + 重归并, 文件行索引不动;
      书签/展开键 = (源,行) 不含时间 → 天然不受扰) + parse_tz_ms (±hh:mm/小时数,
      双符号/越界拒收) + local_tz_offset_ms (Win32 GetTimeZoneInformation 裸 extern
      —— danqing-encoding GBK FFI 同款范式, 不扩 windows feature; 夏令时取当前
      生效档不回溯) + build_merge 默认 tz=本地 (腿 D「无 tz 格式必填, 默认本地」);
      `settings.rs` 弹层时间编辑区 (快捷档 ±1s/±1min/±1h op_btn 同款 + 偏移 ms 手输
      + 时区手输, 作用选中源)。**真发现**: 默认本地时区使测试涉及时区依赖机器 ——
      锁一律先 set_time_params 归一到 tz=0 再断言 (时钟字面量 00:00:10.000 级断言,
      施加两遍必红 23:59:56.000); tie-break 复核 = 等时刻源序号小者先。锁: 主锁
      `offset_applied_at_parse_boundary` (排序+显示两侧同红) / tz 只动无 tz 行 /
      parse 矩阵 / 应用层选中指针+门控
- [x] T6 req_id 追踪（选中值 → 跨源过滤 + 命中导航; `trace_field_value_builds_filter`）
      —— **2026-09-28 落地 (464 绿 = 172 lib + 278 main + 11 genlog + 3 keygen)**:
      lib —— `trace_clause` (**单条 Bare 字面量, 不经 parse_query**: 值里空白会被
      切碎/算子会误读成字段子句, .log 源零命中 = 静默落空) / `snap_jsonl_string`
      (选区落在字符串字面量内 → 放大整个字段值; 转义保持原文 = 行里真实字节) /
      `apply_trace` (逐源**两指针**走查, 归并保持文件内行序故可单调比对; fixture
      造单源双命中逼指针推进) / `clear_trace` / `trace_hits` / `TraceOutcome`;
      MergeState 加 `trace` (与 filtered 同生同灭) + `sel_rev` (**仅源集合变化**
      才升 —— (源,行) 键在显隐/偏移/追踪下稳定)。
      view —— 合并消息列文本选区复刻 (**单行选区**: 跨行不做, 合并相邻行可能来自
      不同文件, 跨行复制字节语义无诚实答案): geom 缓存复用 `measure_row_geom` +
      `caret_at_geom` 收单点 / `merge_msg_x` paint 缓存 (event 无 TextBatch) /
      双击 token / 拖选跨行冻结 caret / 选区带真画锁 (rect_painted 断言产出) /
      `merge_files` 每帧 Arc 同步 (选中行滚出窗仍复制得到, 单文件 file 常驻同义)。
      **修活洞**: 合并态 Ctrl+C 行兜底原先静默复制**单文件文件的错行**
      (copy_source 无合并守卫, line_at/file 在合并位置空间是垃圾值) → 现在
      无选区 = 不消费 + 出声指路; 整行复制挂 T7 波 (T3 边界清单同项)。
      手势链: Ctrl+R (LogView 持焦消费出 `TraceValue`(源,行,字节) / 应用层兜底
      指路两态); Esc 升级序列 = 选区 → 追踪过滤 (`ClearTrace`) → 清焦;
      底栏常驻「追踪 "v" → N 行 (Esc 清除)」; 快捷键页 +Ctrl+R 行,
      `PANEL_CONTENT_H` 208→211 (实测 211 就地盖住, 不预留)。
      R5 族在途作废: 显隐切换 / 时间参数 / 加减源 / 退出合并 四处 invalidate。
      「命中导航复用搜索链」口径落账: 过滤态可视集 = 命中集, 锚定回发起行 +
      既有行导航即命中导航 (零新机制); 合并搜索栏不属本波。
      A/B 三红留痕: 摘两指针推进 → 命中序断言红 / snap 失活 → 字段值断言红 /
      trace_clause 改走 parse_query → 段① 红。
      锁: spec 指定 `trace_field_value_builds_filter` (串生成+过滤链两段) +
      snap 矩阵 + rebuild 作废 + sel_rev 代次 + view 四锁 + main 四锁 (端到端/
      字段值放大/在途作废/Ctrl+R 指路)。
- [x] T7 live-tail 合流（per-source append + 增量进索引 + 跟随钉尾 + 轮转重建 + 断流降级标记）
      —— **2026-09-28 落地 (472 绿 = 176 lib + 282 main + 11 genlog + 3 keygen; logfile 96 绿)**:
      引擎 (danqing-logfile) —— `MergeIndex::remove_row` 尾端回找摘除 (超帽 false =
      调用方兜底重建, 不静默留幽灵) + 锁 `remove_row_then_reinsert_corrects_tail_ts`;
      **联动攒批待批** (patch 现开, lock path 态, 复钉随下批)。
      lib —— `append_source`: 末行补全**退一行重提** (ts pop+extract_append + 索引
      先摘后插, 否则旧 ts 幽灵双条); 隐藏源 ts 照长索引不插 (掩码语义); 摘除失败
      兜底掩码重建; **trace 随行** (增量补滤退一行摘补对称 + 过滤行集全量重推);
      选中锚定 / follow `pin_tail`。`trace_hits` 缓存进 MergeState (apply_trace
      收归持有, 增量补滤不重扫全源); `filtered_from_hits` 两指针走查收单点。
      main —— `poll_growth_merge`: per-source stat 轮询 (同 250ms 节流); 分流 =
      小增量同步 append_source / 巨量追平·UTF-16 副本·轮转缩容 → rebuild_merge
      (worker, 旧 bundle 保持可见); stat 失败 = 断流标记, stat 成功即清
      (弹层行内「断流」+ 底栏「断流 N 源」); `merge_job_live` 标志补 AsyncJob
      无 in-flight 查询 (launch/pickup 配对, 门禁在途不叠加)。**trace 在途缝**
      (T6 伏笔收口): TraceOutcome 带 `scanned` 快照行数, 落地时缺口 [scanned-1,当前)
      退一行补滤 —— 不许追踪永久缺在途窗口里进来的行。
      toggle_follow 合并接通 (T3 告示锁翻案为 toggle 锁, D4 单文件不动); f 键合并
      放行, `/` 不放行出声指路 (合并搜索栏属后续波次)。
      真发现: ①Windows `ERROR_USER_MAPPED_FILE` —— 被 mmap 持有的文件不能就地
      覆写, 删除后 delete-pending 同名重建被拒 (句柄不关) —— 轮转测试走**改名+新建**
      (真轮转形态); ②A/B 刀1 初版锁辨不出「摘除 vs 兜底重建」(结果等价) → 补强为
      **路径锁** (追踪存活 = 走小路), 变异立红 —— 结果锁辨不出路径时拿状态差当判官。
      A/B 三红留痕: 摘除目标改错行 → 追踪存活锁红 / 强制末行完整 → 幽灵双条红 /
      摘恢复自清 → stale 卡死红。T3 冻结锁注释更新 (单文件侧冻结保留, 合并侧接通)。
      **留档**: 重归并在途时 ExitMerge, 交卷会把工作区拉回 Merge (T4 既有边界,
      本轮未动 —— 撞到再裁); 追踪态下持续追加重推过滤集 = O(合并行数)/次
      (17M 行 ~30ms 级, 有界可接受, T9 实测复核)。
- [x] T8 sessions 载荷 merge group（is_recognizable 认新段 M1 守卫 + roundtrip + 源缺失明示跳过; 超支 → D4 退路裁 Open Q2 不烂尾）
      —— **2026-09-28 落地 (484 绿 = 182 lib + 288 main + 11 genlog + 3 keygen)**:
      **D4 承重梁兑现, 退路未动用** (Open Q2 不裁)。
      lib (merge_view) —— `MergeSourceState`/`MergeGroup` 载荷类型 (源路径+偏移/
      时区+显隐, **保序 = 源序号**; **探测结论 route 不落盘** —— 恢复重探测,
      文件内容可能已变) + `MergeState::snapshot_group` (保存侧) +
      `apply_saved_params` (恢复侧: **按路径对源** = 序号漂移免疫, 时间参数变了
      才重提该源 ts, 全套完**一次**重归并, 返回变化源数)。
      lib (columns) —— `SessionEntry.merge: Option<MergeGroup>` (None **省略键**,
      单文件会话不被新键刷屏); `merge_group_from_value` 容错解析 (坏段丢段不丢条
      / 源缺 path 跳源不丢组 / 重复源收编 / 超 MAX_SOURCES 截断 / **天文 offset
      ±10 年 · tz ±23h 钳制** —— 账本外部数据家规, 提取侧裸加法不溢出);
      `is_recognizable` 认段注释 + M1 复发锁 (merge 段寄生 sessions 条目内)。
      main —— save_session 合并态快照旧带单文件侧四样 (冻结现场) + merge 段;
      `apply_session` 拆 `apply_session_payload` (单文件侧四样共用, 行为零变化)
      + 合并分支: **先验源** (FileStat 缺失明示跳过不拒全体; 现存不足两个 =
      整体不应用零副作用, 会话留着) → 载荷应用 → `pending_merge_apply` 挂上 →
      后台归并; `apply_merge_outcome` 交卷套回 (carry 先跑 = 书签/展开零触碰,
      载荷殿后 = 显隐以保存值为准); start_merge/rebuild_merge 清理 pending
      (防旧载荷错嫁新归并); R5 族: 恢复起归并前 trace_job 作废。
      view/settings —— 底栏「会话」钮合并态**照画** (原「合并不画」旧规翻案,
      注释更新); 弹层行合并会话后缀「· 合并 N 源」可辨 (载荷仍按名)。
      **真发现 (T7 遗留错形, T8 测试撞出)**: `pickup_merge_job` 原**先清 live
      后 poll** —— 首帧空转即清标记 = 「重归并在途不叠加」门禁飞行中提前开门,
      且 pump_merge 首拾取即返回 (时序侥幸才绿)。修为**交付才清** + 确定性锁
      (50ms 慢作业, 在途空转不清/交付清双断言)。
      **plan 偏差注记**: ①「格式手改记录」无物可存 —— route 手改功能首版未建
      (D2「可手改」只兑现了时间参数), 载荷故无 route 字段, spec 实现记 (T9)
      回写; ②合并态 expands 仍是**单文件侧**行号 (合并展开未建 = T3 边界),
      「(src,line) 键形态」钉在 SessionEntry::merge 注释备用; ③合并追踪串
      不落盘 (首版) —— MergeGroup 注释 + spec 实现记。
      A/B 五红留痕: 摘 hidden 套回 → replay 锁红 / 摘 offset 钳制 → clamps 锁红 /
      摘载荷套回 → restore 锁红 (精确落「保存偏移套回」) / 摘跳过逻辑 → 缺失
      明示锁红 / 摘「交付才清」→ live 标记锁确定性红。
      锁: spec 指定 `session_merge_group_roundtrip` (三源混合逐项回 + None 不写键
      + 同账不互扰) + 坏段丢段 + M1 认段 + 钳制截断 + snapshot 保序 +
      apply_saved_params 双证 + main 四锁 (保存捕获/端到端恢复/缺失跳过/不足
      两源拒绝) + live 标记锁 + view 翻案锁。
      **留档**: 恢复在途时 ExitMerge, 交卷仍拉回 Merge 并套回载荷 (T7 同一边界
      的延伸, 撞到再裁); 合并会话应用不查 merge_gate (与 sessions 同一张付费
      门票, session_gate 已过)。
- [x] T9 logbench --merge 四组数字进 PERFORMANCE_REPORT + spec 实现记 + 人工验收九条 (a–i) 记账（需付费态 key）
      —— **2026-09-28 落地 (494 绿 = 186 lib + 294 main + 11 genlog + 3 keygen;
      引擎 **99 绿)**。**实测 (产品路径, 3×1GiB/16,985,344 行, release 热缓存, 三跑)**:
      ① 合并就绪 **947/967/962 ms** (串行版 1676–1731 → 用户裁「源级并行提取」后
      **1.78×**, 红线 1.6s 内; D5 非红线目标 0.8s 差 ~17%, 记档不追);
      ② 重归并 **947–1344 ms**; ③ 隐藏重建 **159–206 ms** → 1198.4 万行;
      ④ 跟随追加 **27–50 ms/4MiB(26,026 行)**, 滞后源(深度 198 万行) **38–46 ms**,
      追踪态 **47–55 ms** (+过滤集重推 66–89 ms)。索引 259.2 MiB (16B/行精确);
      探针 1GiB mmap **建立 95–106 / 释放 42–58 ms**。数字全表进
      `PERFORMANCE_REPORT.md` 新增「合并」节 (含竞品锚注)。
      **源级并行** (`build_merge` 逐源一线程 + 结果**按源序回填**; 归并仍串行
      min-head): 串行时代 1525ms 里提取占 1025ms 且源间零依赖 —— 并行后 ≈
      max(单源流水线), 顺带吸收 JSONL 列发现的口径差 (~71ms×2);
      锁 `build_merge_keeps_source_order_under_parallel_build` (完成序与源序相反
      的构造; A/B 倒序回填 → 红) + `build_merge_cancel_skips_without_rejecting`
      (取消 ≠ 拒收: 拒收会走「N 个源未加入」告示; A/B 摘取消检查 → 红)。
      **T9 实测揪出并当场修掉三条追加成本缺陷** (用户裁「引擎修 + 产品闸 + D3/D4
      同窗口」): **D1** 逐行 `Vec::insert` 代价 = Σ(回找深度) —— 批量追尾 k²/2
      (4MiB/2.6 万行 **792–953 ms**), 慢时钟源触帽每行常数 2.45 ms (2 万行 **48.0 s**,
      且落近似位); **D2** 容量重分配 (翻倍 → 拷 259 MiB); **D3** 旧 1 GiB 映射释放
      50–158 ms/次压在 UI 线程; **D4** 追加后锚定走 `position_of` 从头线性扫
      (17M 行 **158–250 ms/次**)。
      修法: 引擎 **整批单遍归并插入** (`insert_rows` 重写: 批内 (ts,line) 稳定序 +
      一次 `slot_of` 定位 + **反向单遍合并**, 写指针恒 ≥ 读指针原地安全; 不变式
      w == ti + bi) + **回找去帽** (精确位, Q5 回写) + **容量留位**
      (`APPEND_SLACK_DIV=16`, ≤6.25% 超募, `index_bytes` 口径不变); 产品 **单轮增长
      > `MERGE_SYNC_MAX_ROWS`(20,000) 行交 worker 全量重归并**; `dispose_snapshot`
      一次性线程释放旧快照; `append_source` 锚定改**有界窗口重定位**
      (`[原位−1, 原位+k]`, 证明: 批行只插原位之后 + 末行补全至多先摘 1 条)。
      **修后 ④ = 50 ms / 46 ms (滞后源 1045×)**。
      A/B 五红留痕: 反转合并方向 → 差分锁红 / 摘容量留位 → 留位锁红 / 摘帽 →
      深回找精确位锁红 / 摘产品闸 → 分流锁红 / 塌锚定窗口 → 选中行锁红。
      **真发现 (方法论)**: 三次「数字与模型不符」都靠**插桩实测**拆开 —— 先怀疑
      回找深度, 实测出 25 行也花 271 ms ⇒ 揪出与行数无关的两笔 (mmap 释放 +
      容量重分配); 「对照组比推理可靠」又中一次。
      **已知残留**: ①`position_of` 仍是 O(合并行数) 线性扫 (追踪落地/书签跳转/
      掩码重建各一次/用户动作, 17M 行 ~158–250 ms/次) —— 低频, 记档不动;
      ②源级并行后 D5 非红线目标 0.8s 尚差 ~17% (带宽争用 + 归并串行段 215ms),
      红线已过, 记档不追; ③D3 异步释放使旧映射短暂仍在 —— 基准/测试一律
      独立路径 (家法「不许时序侥幸」)。
      **联动攒批** (patch 现开, lock path 态, 复钉随批): logfile 本轮新增
      `insert_rows` 重写 / `APPEND_SLACK_DIV` / 4 条锁 (差分对拍 · 深回找精确位 ·
      容量留位 + 既有 12) —— 与 T7 的 `remove_row` 同批 push。
      spec `SPEC-v1x-merge-timeline.md` 加 **§9 实现记** (8 条) + Q5 回写;
      人工验收九条记账 `tasks/acceptance-pending.md` **G 组** (需付费态 key)。

- [x] **T9 后: 模块级双路评审 + 并账全修** (2026-09-28, 零 commit 惯例)
      —— 代码评审 (`code-review-and-quality`) **REQUEST CHANGES** (Critical ×2 +
      Required ×4) + 安全审计 **0 Critical / Required ×1 / Optional ×4 / Nit ×1**
      (Required 与 C1 同源)。**全部并账修毕, +7 锁 → 494 绿** (引擎 99)。
      - **C1 (两路独立同指, 最重)**: 在途追踪 + 源集合换过 → `filtered_from_hits`
        按 `src` 索引命中表**越界** (release `panic=abort` = 整进程死); 等长重排时
        **静默把命中贴到别源**。两个漏点: `apply_merge_outcome` (换源落地点) /
        `apply_session` 单文件分支 (离场); 另有「换源期间新起追踪」绕过旧作废。
        修: 两处 `invalidate` + `apply_trace_outcome` 到点校验 (不符→丢弃+出声) +
        `apply_trace` **运行期闸** + `filtered_from_hits` 改 `get` 纵深。
        **A/B: 摘运行期闸 → 越界 panic 精确红** (与评审描述逐字一致)。
      - **C2**: 我的锚定窗口**证明前提写错** —— 「批行只插原位之后」被自己的测试
        反证; 锚行**自身**是被补全改判的残行时键变了、可任意远。评审给了反例
        ((0,2)@位3 → 补全后@位10, 窗口 [2,5] 搜不到 → 静默跳到 (0,1))。
        修: 窗口外回落 `position_of`。**A/B: 摘回落 → 「实得位 2 的 (0,1)」精确红**
        —— 两条 Critical 都**先复现再修**。
      - **R1** worker panic 卡死 `merge_job_live` → 源线程内 catch_unwind 兜成拒收
        + 释放线程 `Builder` 化; **R2** 「释放挪出 UI 线程」原先**没达成**
        (`LogView.merge_files` 同持 Arc, 真 munmap 落在 view sync) → view 退役整表
        同走释放通道 + 退出合并松手 (报告口径已改正, 并注明 ④ 绕过了产品闸);
        **R3** `apply_saved_params` 曾在 UI 线程重提 3×1GiB → **参数进 `build_merge`
        (worker 提取时就施加偏移/时区 + 建索引时掩码显隐)**, 落点零重提零重建;
        **R4** 补 C1/C2 两条复现锁。
      - **评审未抓、自查抓到**: `carry_view_state` **不搬 offset/tz** —— 加/减源会
        把用户 T5 校准的时钟偏移**静默重置** (R3 改法一并解决, 锁
        `rebuild_merge_keeps_time_params_by_path`)。
      - **家法核对抓出 spec 三条锁名漂移** (§5 勘误已回写): `merge_view_lines_contract`
        (落地**没**扩 `expand::Lines`, 走 `MergeState.filtered` + `row_at`/`window` ——
        意图兑现、抽象分叉记 §9 实现记 9) / `source_hide_rebuilds_index` (产品级缺,
        **本轮补** `merge_source_hide_rebuilds_and_restores_order`) /
        `gate_free_state_opens_upgrade_dialog` (纯命名, 实际是
        `merge_gate_blocks_free_tier_and_never_prompts_paid`)。
      - **评审证伪一条**: 「每帧 N 次 FileStat」不成立 —— `last_stat_poll`
        250ms 节流在 (main.rs:278/3789), 4Hz×8 源 ≈ 32 syscall/s。
      - 其余收口: UI 手输偏移/时区与账本**同一条钳制** (`set_time_params` 收口 +
        `saturating_add`); 会话恢复拒 `\\.\` 设备命名空间 (源路径其余按「用户自己的
        文件」= 已信任假设, UNC 合法但**是本应用唯一触网路径**); `start_trace` 两处
        静默 return 改出声 (P24); 补「无文件在手时恢复合并会话」锁; 基准
        `extreme_ts_source` 取负溢出修。
      - **记档不修 (spec §9.10)**: `position_of` 线性扫 / 增量不重探 route·schema /
        门控漏点 (今日不可达) / 追踪态多源同帧重推累乘 / 引擎 `timestamp.rs` 极值回绕
        (既有代码)。

- [x] **code-simplify 收口 → 腿一五段全闭** (2026-09-28)
      —— **7 项行为零变化** (产品 494 绿 / 引擎 99 绿, **测试零改动**, 连跑 5 遍稳):
      ①钳制单一真身 (columns.rs 私有常量与 merge_view 重复 → 只留一处) ②释放策略
      单一 spawn 点 (`dispose_offthread`) ③在途追踪作废收口 `discard_trace_job()`
      (7 个带注释的调用点 → 1 方法) ④`anchor_at`/`file_handles` 概念命名 (生产侧 7 处)
      ⑤合并复制链单一解析点 `merge_sel_slice` (「有无可复制」与「取出什么」由构造同源
      —— T6 曾在这条链上漏出「说有选中却复制错行」; 唯一语义收紧: 源句柄缺失的
      **不可达**分支由静默改为出声) ⑥过期前向引用修正 (notice「整行复制随 T7」→「未接」)
      ⑦**顺带修一条既有 flaky** (bookmark-persist 09-23 起潜伏, 非本模块):
      `toggle_says_truth_when_save_fails` 占位目录里的 sentinel 让 `remove_dir` 删不掉
      → 目录泄漏 → **pid 复用时**下次 `create_dir().unwrap()` 炸 (约 1/5); 修 =
      开头整树防御清理 + 结尾 `remove_dir_all`。**教训: 单跑绿不算数, 连跑多遍才抓得到
      flaky** (同族: 2026-09-14 并行临时文件 flake)。
      **不动清单** (想过不改, spec §10 有理由): `apply_saved_params` (网 + 有独立单测) /
      `position_of` 线性扫 (低频入口, 不为它建反向结构) / 双模式位置数学分叉 (动单文件侧,
      属设计决定) / 引擎注释密度 + logbench 参数个数 / 测试内联形状 (零 churn)。
      **五阶段 (spec→plan→build→review→code-simplify) 至此全闭**; 余:
      **联动提交批** (logfile `insert_rows` 重写 + `remove_row` push → 关 patch 复钉 →
      两仓分别提交注关联, 待用户点头) + 人工验收 G 组九条 (需付费态 key) +
      v1.x 发布链。

## 另案同窗口

- [ ] `todo-gate-trio.md`（三连接门, D8）—— T3–T4 窗口内顺手
