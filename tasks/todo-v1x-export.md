# todo-v1x-export: 导出（腿三） 任务清单

- @author 十四叔
- @date 2026/09/23
- Spec: `docs/specs/SPEC-v1x-export.md` · Plan: `tasks/plan-v1x-export.md`
- 状态: **plan 完成待用户过目** → build（零 commit 惯例）
- 测试基线: **262**（2026-09-22 实测; T1 三件套复核回填）

## Phase 1: 纯逻辑核心（`src/export.rs`, 零 UI）

- [x] **T1: 行集快照 + 原始行 writer + 流式写出循环骨架** —— `src/export.rs`（新文件头
  `//! @author 十四叔` + `//! @date 2026/09/23`）。
  **先核实**: `LogFile::open` 对 UTF-16 的行为（拒开 ⇒ 导出天然不适用, 记已知局限;
  能开 ⇒ 行尾探测宽字符语义单独立项再定, 不混进本任务）。
  内容:
  - `ExportSet` 行集快照: 过滤命中 / 搜索命中 / 全集三来源收口成一个升序 `Vec<u64>`（或
    `Arc<[u64]>`）+ `line_count` clamp（live-tail 冻结, spec D1）
  - `LineEnd` 策略: **全集 raw 走 mmap 字节整拷**（保真满分）; 稀疏 raw 走行内容 +
    文件级行尾探测（采样前 8 个行尾定 LF/CRLF, 统一写出; 无行尾文件回退 `\n`）
  - 流式写出循环骨架: `lines()` 单遍 + 行集游标（**禁止**全量 `line(i)` —
    `logfile.rs:410` 的 235→1072ms 教训）; 泛型 `W: Write` 注入可测; 每 N 行查
    `AtomicBool` 取消 + 累进 `AtomicU64` 进度
  Acceptance: ①CRLF 源稀疏导出行尾保真 ②全集导出与源文件**逐字节相等** ③GBK 源不解码
  逐字节相同 ④稀疏行集内容正确（跳跃/首行/末行/空集）⑤取消后写出中止（半成品删除
  属 T4 语义, 此处只锁「循环停」）⑥摘掉行尾策略精确红（A/B 记录）。
  Verify: `cargo test` + clippy 0。
- [x] **T2: CSV writer** —— RFC4180 手写转义（零新依赖）: 字段含逗号/引号/换行整体加引号、
  内部 `"` → `""`; 文件头 UTF-8 BOM（`EF BB BF`）+ 行尾 CRLF; 表头 = schema 首见序列名行;
  行内缺字段 = 空串; schema 外字段忽略; 值 = **提取值源字节全文**（`FieldExtractor`
  同源取值, `danqing_encoding` 解码, 不逐行 serde parse —— D9 性能红线）。
  Acceptance: ①BOM 头字节断言 ②引号/逗号/换行/CJK/空串/嵌套源字节各形态与手算期望
  逐字节对拍 ③schema 列序 = 首见序 ④缺字段/skip 字段口径 ⑤GBK 值解码进 UTF-8 输出。
  Verify: `cargo test` + 对拍样例入测试夹具（不引外部文件）。
- [x] **T3: pretty writer** —— 逐行 `serde_json::from_slice` → `to_string_pretty`（indent 2）,
  对象之间**空一行**; 解析失败行原样写出并计数; 输出 UTF-8 + `\n`。
  Acceptance: ①合法行 pretty 与 serde_json 原生 pretty 全等对拍 ②混合文件失败行
  原样 + 计数正确 ③空行/空对象边界。

## Checkpoint A（T1–T3）✅

- [x] 三格式纯逻辑测试全绿 (**281** = 104 lib + 166 main + 8 genlog + 3 keygen; 基线
      265→281, +16 export 各锁); 三处 A/B 精确红记录在案: ①摘行尾策略 → `aa\ncc\n` ≠
      `aa\r\ncc\r\n` ②摘 RFC4180 转义 → 裸字段 vs 引号字段逐字节红 ③摘 pretty 空行
      分隔 → 缺分隔 `10` 精确红; fmt / clippy 0 (含 same_item_push 一处修正)。
      **基线实测 265** (09-23 开工复核, plan 记 262 为 09-22 旧值)。
      兄弟 crate `danqing-logfile` 68 绿 (含联动: 新增 `LogFile::bytes()` 访问器)。
      **T2 口径修正回写**: CSV 单元格走 `parse_line`+`cell_display` (显示口径构造保证),
      推翻 plan 原案「不逐行 parse」红线 —— 理由: ①字符串反转义缺口 (extract token
      保留 JSON 转义, `say \"hi\"` 进 Excel 是错的) ②一次 parse 供全部 K 列
      ③与 pretty 共享 ≤30s 预算, T6 实测说话。

## Phase 2: 作业与全链路

- [x] **T4: ExportJob 语义** —— `AsyncJob` 范式包装（`Arc<LogFile>` 共享, `Arc::clone`）:
  在途标志（入口变「取消导出」的依据）+ `Arc<AtomicU64>` 进度 + `Arc<AtomicBool>`
  取消（协作退出）+ **取消删半成品**（删除失败降级提示路径）+ 完成回调（总行数/
  输出路径/pretty 失败计数）。
  Acceptance: ①取消后半成品文件**不存在** ②在途不产生第二作业 ③完成回调数据完整
  ④换文件（async-open）时在途导出 invalidate（旧结果不贴新文件, AsyncJob 代次语义）。
- [x] **T5: 全链路 UI** —— 状态栏「导出…」（底栏, update-dot/设置按钮几何先例）+
  `Ctrl+E`（`main.rs:1809` Ctrl 链加 `e`）+ 格式小菜单**按模式收口**（JSONL: 三项;
  明文: 仅原始行; 不可用格式不出现）+ **门控点位 = 入口**（`Msg::ShowUpgradePrompt
  (Feature::Export)`, 保存对话框**之前**; 与 `main.rs:675` 同款）+ `rfd::FileDialog::
  save_file`（默认名 `<stem>[-filtered|-searched]-<yyyyMMdd-HHmmss>.<ext>`, 时间戳
  手写 civil-from-days 纯函数 + 闰年测试）+ 进度/完成/取消底栏反馈 + SHORTCUTS 表
  落 `("Ctrl+E", "导出…")` + `action_of` 一致性锁。
  Acceptance: ①免费态点导出弹升级提示且**零落盘、不开保存对话框** ②付费态三格式
  各导一次全链路 ③明文模式菜单只现原始行 ④Ctrl+E 与按钮同动作 ⑤SHORTCUTS 一致性锁
  （防 1041 行教训: 清单两处漂）⑥默认文件名含语义中缀与时间戳。

## Checkpoint B（T4–T5）✅ (机器部分)

- [x] 三件套绿: **298** (112 lib + 175 main + 8 genlog + 3 keygen; 281→298, +17 =
      2 文件名锁 + 5 作业锁 + 2 view 锁 + 6 主接线锁 + SHORTCUTS 锁 + 面板守卫随高);
      clippy 0 (含 let-尾返回 / 手工除零 两条修正)。PANEL_CONTENT_H 180→192
      (快捷键页加 Ctrl+E 行后 189.5 越界, 守卫红→按其指示就地调)。
- [ ] **实机各走一遍归人工验收 (Checkpoint C 用户闸门)** —— 测试侧已锁: 免费态零菜单 /
      付费态开菜单+作业态取消 / Ctrl+E 同消息+模态不吃穿 / Esc 次序 / 明文守门 /
      端到端过滤行集字节对拍 / 按钮几何与空态零痕迹 / 取消删半成品 (export 单测)。
      **实现口径记**: ①「JSONL 表格模式」判据取 `schema.is_some()` (文件是 JSONL),
      Ctrl+T 切原始视图不收窄可导格式 —— 若实机裁定「严格 Table 模式」再收;
      ②轮转重建**不**作废在途导出 (冻结快照自洽, 与 filter/search 贴错文件不同族);
      ③时间戳 = **UTC** (零依赖手写 civil-from-days; 文件名唯一用途, 不承担叙事)。

## Phase 3: 测量与收口

- [x] **T6: logbench --export + 实测回填** —— `logbench <file> --export <raw|pretty|csv>`
  （argv 解析照 `--filter`/`--analyze` 先例）; 与 app 同源调用导出核心（D9: 数字与
  真实路径同一代码）; 实测（热缓存注明）回填 `PERFORMANCE_REPORT.md`。
  目标: 1 GiB 全集 raw ≤ 8s / CSV·pretty ≤ 30s 量级; **未达标按数据决定优化或重订目标,
  禁止估算冒充实测**（09-21 口径家法）。
  Verify: logbench 输出 + 报告 diff。
- [x] **T7: 文档收口** —— README 付费层 bullet（导出三格式入「交付」话术）/ ROADMAP §二
  腿三勾销 / spec 实现记 + 成功判据机器部分逐条回填 / `docs/ms-store-copy.md`
  「v1.x 上架时必须改什么」清单核对（导出是付费层素材, 三处商店铺文案改动清单里
  一并列）/ CLAUDE.md 状态落账。

## Checkpoint C（T6–T7）(机器部分 ✅)

- [x] spec §成功判据机器部分逐条过 (1–8 全过; A/B 三处见 Checkpoint A)
- [x] 性能数字进 PERFORMANCE_REPORT（2026-09-23 实测, release 热缓存二跑, SSD 口径标注;
      raw 全集 **608ms**/1684MiB/s (目标 ≤8s ✅) · raw 稀疏 36k 行 129ms ·
      pretty **18.1s** · CSV **20.7s** (目标 ≤30s 量级 ✅); 产物机检: BOM/CRLF/表头/
      缩进分隔全对）
- [ ] **人工验收（用户实机, spec 四条）**: ①免费态升级提示无保存框 ②付费态三格式
  真文件（CSV **Excel** 开中文不乱码列对齐 / 美化缩进 / 原始行 diff 对应行一致）
  ③demo-1gb 导出期间 UI 可响应、取消半成品消失 ④明文菜单收口 + live-tail 快照语义
- [ ] 进 review 阶段（`/agent-skills:code-review-and-quality`）→ code-simplify

## 遗留 / 待用户动作

1. 人工验收（Checkpoint C, 需付费态 —— 便携版真 key 激活已可走, licensing 收银台就绪）
2. 商店侧 add-on / IARC / `PURCHASE_URL`（licensing 用户侧清单原样有效, 导出不新增）
3. 混合行尾文件若实机撞到: 升级引擎 `line_with_ending(i)`（一次联动, 按需再裁）
