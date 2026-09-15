# Plan: 搜索/过滤默认大小写不敏感 (case-insensitive)

> @author 十四叔 · @date 2026/09/15
> spec: `docs/specs/SPEC-case-insensitive.md` (**已批** 2026-09-15 用户「go」)
> intent: `docs/intent/case-insensitive.md` (四项裁定 + 实测弹药)
> todo: `tasks/todo-case-insensitive.md`
> 时机: 进 v1.0, 阻塞发布 (裁定 3)

---

## Overview

五处接线点同口径改为默认大小写不敏感: 搜索 (UTF-8 / 非 UTF-8 两分支) / Bare 裸词 /
Flat 字段 (值 + 键规范化) / 直方图字段口径分类器 / 高亮 (已同源, 只加锁)。
两仓: `danqing-logfile` (过滤编译层) → 本仓 (接线)。danqing 与 danqing-encoding 零改动。

**前置状态 (plan 阶段实测)**:
- logfile 基线 **51 绿** (2026-09-15 实测);
- 本仓基线 **暂不可得** —— 工作树里有 interaction-polish **M2 未提交在途改动**
  (view.rs +347, 测试模块 11 处机械编译错误), 非本会话产物。
  **T0 以树恢复可编译为前提** (M2 处置方式待用户裁, 见 todo T0)。最后已知基线
  133 绿 = 51 lib + 74 main + 8 genlog (2026-09-14 记档)。

## Architecture Decisions

1. **新原语 `contains_ascii_ci` (logfile jsonl.rs, 私有)**: 首字节双变体
   `memchr2` 驱动 + 窗口 `eq_ignore_ascii_case` 验证, ~20 行, 服务 Bare 子句。
   选手写不选 per-line regex: 与本仓 memchr 系惯用法一致, 无 regex VM 每行开销;
   正确性由 **regex oracle 对拍**锁死 (随机字节串 vs `(?i-u)+escape` 逐组一致)。
2. **键规范化单点在产品侧**: 新增 `LogApp::parse_filter(query) -> Vec<Clause>`
   (parse + 调 logfile 纯函数 `normalize_clause_keys`), 三处调用点
   (`apply_filter` :945 / 追加作业构造 :525 / live-tail 重滤 :798) 全部走它;
   open.rs worker 的 clauses 来自 :525, 自动继承。**状态栏仍显示用户原始输入**
   (不改 `filter_applied` 存储串, 避免显示层惊讶)。
3. **`normalize_clause_keys` (logfile jsonl.rs, pub)**: 仅**单段** path 做不敏感
   查表改写; 查无此列保持原样 (自然 0 命中); 多段 path (Verify) 不动 (spec D6)。
4. **spec 修正 (plan 读码发现, 2026-09-15, 待用户随 plan 一并点头)**:
   - **D7 修正: 行口径分类器 `classify_level` 保持敏感** —— `levels.rs:110` 注释
     记着这是**既定设计**: 不敏感会让 `"no errors found"` 这类正文污染计数
     (柱条数字必须可信)。.log 桶只读、无点选, 不受 D2 逐桶相等约束。
     真正被 D2 强制的是**字段口径** (可点 → 子句 → 过滤), 只有它必须跟随不敏感。
     推论边界: .log 桶计数 (敏感) 与小写裸词过滤 (不敏感) 在散文行上数字可不同
     —— 设计如此, 落 spec §5。
   - **`find_level_column` 已不敏感** (`levels.rs:170` `eq_ignore_ascii_case`) ——
     spec §2 现状表写错了, 已修正; 该处零改动。
5. **高亮零改动**: `view.rs:915-921` 编译的就是 `app.search_pattern` 同一串
   (plan 阶段查证), `(?i)` 前缀自动跟随。加同源注释 + 一条「pattern 带 (?i)」
   回归锁, 防未来第二处拼 pattern。
6. **逃逸舱零代码**: UTF-8 搜索查询本是裸正则, `(?i)` 前缀后用户写 `(?-i)` 即
   局部恢复敏感 (regex 组内 flag 覆盖) —— 加测试锁, README 补一句。

## 依赖与顺序

```
T1 contains_ascii_ci ─┬─ T2 Bare ─────────────┐
                      └─ T3 值比较+键规范化 ───┴─ Checkpoint A (logfile 三件套)
                                                        │
T4 搜索 (?i) 两分支 (不依赖 logfile, 可与 Phase1 并行) ──┤
T5 parse_filter 单点 (依赖 T3 的 normalize) ────────────┤
T6 字段口径分类器 (依赖 T3 的 Flat 不敏感, 对拍才成立) ──┴─ Checkpoint B (本仓三件套)
                                                        │
T7 性能复测 + 文档刷新 ── Checkpoint C (预算达标)         │
T8 review → code-simplify ─ T9 联动收口 (push→复钉→分别提交)
                                                        │
                              Checkpoint D (用户人工验收, spec §7 六条)
```

**按仓分层不按特性竖切** (与 interaction-polish 的 M1 先行同理): logfile 必须先
push 才能复钉 lock, 跨仓竖切会产生多次 push/复钉往返。T4 与 Phase 1 无依赖可并行。

## 风险登记

| # | 风险 | 对策 |
|---|------|------|
| R1 | `(?i)` 与 `(?-u)` 字节转义交互不成立 | **已实测关闭** (GBK 7884 行对拍逐字节一致, 见 intent) |
| R2 | 敏感时代旧断言翻红被误认真回归 | 每任务带「口径翻转 vs 真回归」核对步; 翻转清单落 todo |
| R3 | 手写 `contains_ascii_ci` 正确性 | regex oracle 对拍 + 空 needle/非 ASCII 首字节/贴边用例 |
| R4 | Bare 性能超预算 (≤150ms/1GB) | T7 复测兜底; 备选 = per-line `(?i-u)` regex (spec D4 留有选型) |
| R5 | patch/lock 陷阱 | T9 走既有 SOP: 关 patch → `cargo check` 重解 → `cargo update -p` 复钉 → `--locked` 验证 |
| R6 | 树内 M2 半成品与本模块文件面重叠 (view.rs) | **T0 前置闸门**: M2 处置完毕 (提交或 stash) 才开工; 处置方式用户裁 |
