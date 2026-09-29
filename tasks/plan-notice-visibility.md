# Plan: notice-visibility (Warn toast 浮层 + Info 底栏强化)

> @author 十四叔 · @date 2026/09/28
> 上游: `docs/specs/SPEC-notice-visibility.md` (2026-09-28 用户「go」批准, Open Q1–Q3 按推荐收)
> 下游: `tasks/todo-notice-visibility.md`

## 核实记录 (plan 阶段, spec §5 委托项)

1. **Stack 事件分发 = 反序** (`danqing/src/widget/layout/stack.rs:87` `children.iter_mut().rev()`):
   末位子项**最先收事件 + 最后 paint (最上层)** —— toast 挂 Stack 末位即同时满足
   「弹层之上仍收点击」与「画在模态弹层之上」, **构造保证, 写锁即可, 零框架改动**。
2. **`STATUS_HEIGHT` 是 view.rs 私有常量** (`view.rs:72`) —— toast 几何要「贴状态栏顶」
   需同源, 改 `pub(crate)` (一字改动)。
3. **框架 Overlay 模态** (`overlay.rs`: scrim + modal_barrier + 吞事件), 不可用作
   toast —— 产品侧 `src/toast.rs` 自绘 (spec D5 既定)。
4. **清 notice = 两处一组** (`notice=None` + `notice_until=None`, main.rs:2349-2350 与
   tick_notice 各一份) —— 抽 `dismiss_notice()` 单点收口, tick 到点分支与
   DismissNotice 同调 (符合 simplify 精神, 行为零变化)。
5. **Tick 通路现成**: `tick_notice` 已在 tick 链上 (T18 既有锁
   `notice_expires_when_its_deadline_passes`), toast 消退零新机制 —— 状态不变,
   只是 Warn 的呈现位变了。

## 任务流 (依赖序)

```
T1 toast 组件本体 ──→ T2 main 接线 (Msg/挂载/收口) ──→ T3 view 分派 + Info 色块 ──→ T4 端锁 + 三件套
```

**为什么 T3 必须在 T2 后**: T3 让 Warn 不再画在底栏 —— 若先于 T2 落地,
Warn 会短暂「底栏不画 + toast 未挂」= 彻底看不见 (比现状更坏)。T1↔T2 同方向依赖。

## 风险与对策

| 风险 | 对策 |
|---|---|
| toast 几何依赖 `STATUS_HEIGHT` 漂移 | 同源 `pub(crate)` 引用, 不抄数值 (token 同源家法) |
| Info 色块 token 浅色对比度不足 (面阶梯教训) | build 时实测两主题色块 vs 底栏底对比度, 回填 spec Q2; 不足走备选 `accent()` 低透明 |
| 超长文案顶穿浮层 | D8 上限 `min(窗口宽×0.6, 520px)` + 省略号截断 + 锁 |
| toast 命中矩形与绘制矩形分叉 (同源家法) | paint 时缓存矩形, 命中读缓存 (hit 同源先例: SubmitInput/col_spans) |

## 验证检查点

- 每 T 后: 该 T 新增锁全绿 + 既有 495 基线不破
- T4 后: 三件套 (fmt / clippy 0 / 全测试) + 人工验收 H 组五条记账
  `tasks/acceptance-pending.md` (spec §7 原文)
