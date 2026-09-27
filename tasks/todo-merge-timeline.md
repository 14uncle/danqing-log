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
- [ ] **CP1 联动**（届时单独点头）: logfile 三件套 → push → danqing-log 关 patch cargo check 复钉 → 两仓分别提交注关联
      —— **就绪待批**: logfile 95 绿/clippy 零警告; logbench 已切 `detect_route` 正式通路
      (三路线端到端复测 1524/1530ms ≤ 红线 1.6s); 幽灵坑备查: 切 patch 后 clippy 陈旧 rmeta
      报 unresolved import → `cargo clean -p danqing-logfile` 解决

## Phase 2 — 产品（danqing-log）

- [ ] T3 merge_view.rs 视图模型（MergeSource + MergeLines 二态 + 展开键 (src,line) + Workspace 双模式 + 归并 AsyncJob + 三栏绘制 + 滚动/选中/书签复刻; Single 零触碰; `merge_view_lines_contract` 先红后绿; 413 基线不破）
- [ ] **CP2 检查点: D11 源色板提请用户批准**（框架 token or 退路单色 chip）
- [ ] T4 源管理弹层 + 「合并…」入口 + `Feature::MergeTimeline` 门控两道闸 + 并集列（上限 24 首见截断）+ 混合源
- [ ] T5 时钟偏移/时区（弹层编辑 + 解析边界单源施加 + 重归并; `offset_applied_at_parse_boundary`）
- [ ] T6 req_id 追踪（选中值 → 跨源过滤 + 命中导航; `trace_field_value_builds_filter`）
- [ ] T7 live-tail 合流（per-source append + 增量进索引 + 跟随钉尾 + 轮转重建 + 断流降级标记）
- [ ] T8 sessions 载荷 merge group（is_recognizable 认新段 M1 守卫 + roundtrip + 源缺失明示跳过; 超支 → D4 退路裁 Open Q2 不烂尾）
- [ ] T9 logbench --merge 四组数字进 PERFORMANCE_REPORT + spec 实现记 + 人工验收九条 (a–i) 记账（需付费态 key）

## 另案同窗口

- [ ] `todo-gate-trio.md`（三连接门, D8）—— T3–T4 窗口内顺手
