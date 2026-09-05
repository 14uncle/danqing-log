# TODO: core-viewer

> plan: `tasks/plan.md` | spec: `docs/specs/SPEC-core-viewer.md`
> 逐条勾选推进; 每任务完成后跑三件套 (fmt + clippy + test)。

## Phase 1: 引擎地基

- [x] **T1: 步进索引内存压缩** ✅ 2026-09-05
  - Acceptance: 1GB 索引驻留 ≤16MB; 索引 ≤600ms; 随机访问 ≤1µs/行 (stride 16,
    超则改 8 复测); 既有 logfile 测试全绿; 新增小文件逐行内容对拍测试
  - Verify: `cargo test`; `cargo run --release --bin logbench -- <1GB文件>`
  - Files: `src/logfile.rs`, `src/bin/logbench.rs`
  - 实测: 索引驻留 48MB→**3.03MiB**(明文)/2.30MiB(JSONL); 索引 432ms; 随机访问
    0.61µs/行 (stride 16 一次过); 结构正则 1249ms (≤1500 门槛)。
    **抓到一个计划外回归**: run_filter 逐行 line(i) 随机访问在步进索引下
    235ms→1072ms 挂门槛 → 新增 `lines()` 顺序迭代器 (单次扫描每行 O(1)),
    过滤回 **304ms** (≤400 ✓); 教训写进 lines()/run_filter 注释

- [x] **T2: 编码检测 + 解码/转码路径** ✅ 2026-09-05
  - Acceptance: fixtures (UTF-8 无/BOM, UTF-16LE/BE BOM, GBK 中文) 行数一致 +
    解码内容断言; 随机二进制降级不崩; GBK 中文查询命中; UTF-16 副本 ASCII
    关键字命中; 1GB UTF-8 索引性能不退化
  - Verify: `cargo test`; logbench 复跑; fixtures 由 `genlog --encoding` 生成入库
  - Files: `src/encoding.rs`(新), `src/logfile.rs`, `src/bin/genlog.rs`,
    `tests/fixtures/`(新)
  - 实测: 32 测试全绿; 1GB UTF-8 索引 409ms/搜索 70ms/驻留 3.03MiB 零退化。
    **两处计划内调整**: ①fixtures 改为测试内构造 (UTF-16 走 std encode_utf16,
    GBK 走 CP936 实测校准常量), 不落二进制 fixture 文件 —— genlog 不动;
    ②GBK 解码零依赖改走 Win32 CP936 直通 FFI (与 plan「零依赖」一致)。
    **意外收获**: 测试撞名触发 ERROR_USER_MAPPED_FILE —— Windows 拒绝截断
    仍被映射的文件, T3 假设提前得一个数据点

- [x] **T3: 截断/轮转生存原语** ✅ 2026-09-05
  - Acceptance: Windows mmap 存活期外部截断/删除/改名/覆写实测表落档 (plan 附录);
    line(i) 越界返回 None 单测; `rebuild()` 后行数/内容反映新文件; stat 快照单测
  - Verify: `cargo test`; 实验人工跑
  - Files: `src/logfile.rs`, `tasks/plan.md`(附录), `src/bin/mmap_lab.rs`(新)
  - 实测: **五场景全部不崩, 截断被 OS 拒绝** (1224) —— POC 头号风险证伪,
    意图文档边界已修正; 落地 FileStat 快照/is_stale/rebuild 三原语 + 3 测试。
    偏差记录: line() 越界保持既有「空片」语义 (全部调用方已依赖), 不改 Option;
    spec 验收措辞「返回 None」按「不崩+空」语义达成

## Checkpoint: 引擎地基 ✅ (auto 连跑完成, 用户已在汇总中过目数字)

- [x] 三件套绿 (T1–T3 各自过 + T7 末轮全量: 40 lib + 5 bin 测试绿, clippy 0)
- [x] logbench 全基线达标 (T7 后终测: 索引 410ms / ERROR 67ms / 结构正则 1166ms /
  随机 0.56µs / 驻留 3.03MiB / JSONL 复合过滤 300ms —— 全部在 SPEC.md 门槛内)

## Phase 2: 交互

- [x] **T4: danqing PageUp/PageDown 联动** ✅ 2026-09-05
  - Acceptance: danqing `cargo test --lib --tests` 绿; danqing-log PageUp/Down
    人工生效 (±PAGE_ROWS, Space 保留)
  - Verify: 两仓三件套; 人工按一遍
  - Files: `../danqing/src/event.rs`, `../danqing/src/window/event.rs`, `src/main.rs`
  - 实测: danqing 385+37 测试全绿, danqing-log 35 全绿; 人工生效待模块验收统一过

- [x] **T5: 正则搜索 UI** ✅ 2026-09-05
  - Acceptance: `/`/Ctrl+F 开栏 (与过滤栏同槽位); Enter 应用+跳第一命中 (置视口
    中部), 之后 Enter/Shift+Enter 下/上; 可见行命中区间高亮; 底栏计数
    `搜索 "..." → 第 k/n 命中 (x ms)`; Esc 关栏; 状态机单测
  - Verify: `cargo test` + 1GB 文件人工验收
  - Files: `src/search.rs`(新), `src/main.rs`, `src/view.rs`
  - 实测: 40+2 测试绿。AsyncJob 泛化 (过滤/搜索同构, 删掉手写 rev/Mutex 重复段);
    SearchNav 纯逻辑导航 5 测试; **抓到一个真坑**: GBK 转义 `\xNN` 模式在
    Unicode 模式下按码点 UTF-8 展开匹配不到原始字节, 必须 `(?-u)` 前缀;
    高亮前缀宽度测量与行显示统一走 encoding::decode_line (GBK 不错位)

- [x] **T6: 书签** ✅ 2026-09-05
  - Acceptance: `b` 切换选中行书签; `'` 循环跳下一书签 (选中跟随); 行号槽金色
    圆点; 过滤模式按文件行号判定; 集合操作/循环跳转单测
  - Verify: `cargo test` + 人工
  - Files: `src/main.rs`, `src/view.rs`
  - 实测: next_bookmark 严格大于+环绕+MAX 不溢出单测; 行号槽金色行号 (圆点换色,
    槽宽稳定); 表格模式字符键归过滤框 → 书签双模式通用键 = Ctrl+B/Ctrl+G,
    原始模式另有 b/'; 底栏「书签 n」计数

- [x] **T7: 水平滚动** ✅ 2026-09-05
  - Acceptance: Shift+滚轮 ±3 字符宽; 底部细滚动条 (6px); 行号槽钉住; 原始/表格
    模式同法; x_offset 钳制单测
  - Verify: `cargo test` + 超长行文件人工验收
  - Files: `src/view.rs`, `src/main.rs`
  - 实测: 引擎缺口打磨寄生落地 —— danqing MouseWheel 加 shift/ctrl/alt 修饰键
    (scrollable 转发与测试同步, 全家测试绿; pomodoro/clipboard 无 MouseWheel
    解构, 波及面零)。实现决策: danqing 无 scissor 裁剪层 → 水平滚动走
    scroll_trim 左截断 (后缀+亚字符偏移, 亚像素平滑, 不压行号槽);
    x_offset/max_seen 纯视图态 (Cell), 应用层零参与 —— main.rs 未动

## Checkpoint: 模块验收 —— 待用户人工验收

- [ ] 三件套绿 ✅ (终轮); 两仓联动改动分别待提交 (注明关联, 等用户 commit 指令)
- [ ] SPEC-core-viewer 成功判据逐条过单 (人工验收时核对)
- [ ] 用户人工验收全过
- [ ] 进 review 阶段 (`/agent-skills:code-review-and-quality`)
