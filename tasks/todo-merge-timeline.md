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
- [ ] T6 req_id 追踪（选中值 → 跨源过滤 + 命中导航; `trace_field_value_builds_filter`）
- [ ] T7 live-tail 合流（per-source append + 增量进索引 + 跟随钉尾 + 轮转重建 + 断流降级标记）
- [ ] T8 sessions 载荷 merge group（is_recognizable 认新段 M1 守卫 + roundtrip + 源缺失明示跳过; 超支 → D4 退路裁 Open Q2 不烂尾）
- [ ] T9 logbench --merge 四组数字进 PERFORMANCE_REPORT + spec 实现记 + 人工验收九条 (a–i) 记账（需付费态 key）

## 另案同窗口

- [ ] `todo-gate-trio.md`（三连接门, D8）—— T3–T4 窗口内顺手
