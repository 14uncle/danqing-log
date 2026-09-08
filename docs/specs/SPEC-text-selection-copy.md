# Spec: 文本选区与复制 (text-selection-copy)

@author 十四叔
@date 2026/09/08

## Objective

给日志视图加上「看得见、拖得动、Ctrl+C 拿得走」的文本选区:

- **双击选词**: 双击行文本选中一个 token (空白分隔的连续非空白段), 日志场景下能一把选中整个时间戳 / IP / `level=ERROR`。
- **拖动框选**: 按下左键拖动, 字符级精确框选任意区间, 可跨行。
- **Ctrl+C 复制**: 选区内容以纯文本写入系统剪贴板, 多行以 `\n` 拼接。

用户故事: 排查日志时看到一条 ERROR, 想把它贴给同事/贴进搜索 —— 现在只能看着, 一个字也拿不出来。klogg/LogViewPlus/VS Code 都有, 这是查看器的底线能力, 也是付费层「专业感」的地基。

### 已拍板的决策 (2026-09-08 用户裁决)

| 决策点 | 结论 |
|--------|------|
| 选区粒度 | **字符级** (编辑器式, measure 前缀命中) |
| 表格模式 | 原始模式全功能; **表格模式 Ctrl+C 复制当前选中行的完整原文行** (含被截断省略的部分) |
| 双击词界 | **空白分隔 token** (连续非空白 = 一个词) |
| 与选中行关系 | **并存各司其职**: 单击=选中行(现状); 拖动/双击=文本选区, 出现时取代行选中视觉; Ctrl+C 只认文本选区 |

### 边界

**In**: 原始模式字符级框选/双击选词/Ctrl+C; 表格模式选中行原文复制; 选区高亮渲染; Esc 清除选区

**Out**: Shift+Click 扩展选区; 拖动出视口自动滚动; Ctrl+A 全选; 右键菜单复制; RTF/带格式复制; 表格模式单元格级框选; 行号槽/表头的选择交互

## Tech Stack

- Rust 1.85+, edition 2024, stable-x86_64-pc-windows-gnu (rustup override 已设)
- danqing (git 依赖 + 本地 `[patch]`): 剪贴板链路已现成 —— `handler.rs` 拦截 Ctrl+C → 焦点路径发 `Event::Copy` → `selected_text_at_path` 读回 → arboard 写入; **本特性理论上零引擎改动** (LogView 实现 `focusable`/`focus_id`/`selected_text` + 消费 `Event::Copy` 即可接入)
- 双击检测: 组件内时间戳自判, 沿用 `danqing/src/widget/title_bar.rs:597` 的先例 (300ms / 4px), 引擎不加事件类型
- 命中测试: `TextBatch::measure(&str, px)` 前缀测量, 纯逻辑无 GPU 依赖 (view.rs 现有测试已这么用)

## Commands

- 构建: `cargo build`
- 测试: `cargo test`
- 静态检查: `cargo clippy --all-targets -- -D warnings`
- 提交前: `cargo fmt` + clippy 零警告 + 测试全绿
- 性能回归抽查: `cargo run --release --bin logbench` (数字不得劣于 SPEC.md 实测基线)

## Project Structure

- `src/view.rs` — 主战场: 选区状态 (锚点/光标点)、事件处理 (按下/拖动/抬起/双击)、选区渲染、命中测试、`focusable`/`selected_text`/`Event::Copy` 接入
- `src/main.rs` — 仅当需要新 Msg (如 `CopySelection`/`ClearSelection`) 时动
- `docs/specs/SPEC-text-selection-copy.md` — 本文档
- `tasks/plan.md` + `tasks/todo.md` — plan/tasks 阶段产物 (沿用流水线输出约定)

## Code Style

沿袭全仓基准 (`src/logfile.rs` 风格): 中文 doc comment 说明做什么+不做什么+为什么; 新 `.rs` 文件头 `//! @author 十四叔` + `//! @date yyyy/MM/dd`; 颜色/尺寸常量集中在 view.rs 头部同族函数区。

选区状态建议形状 (plan 阶段定稿, 此处锚定语义):

```rust
/// 文本选区: 锚点 (按下处) 与光标点 (拖动当前处) 均为 (显示行, 字节偏移)。
/// 规范化 (anchor <= caret) 只在读取/复制时做, 事件热路径不排序。
struct TextSelection {
    anchor: (u64, usize),
    caret: (u64, usize),
}
```

## Testing Strategy

- **纯逻辑单元测试** (view.rs `#[cfg(test)]`, 现有 TextBatch::measure 测试同法):
  - token 边界: 空白分隔取词 (行首/行尾/连续空白/全空白行/UTF-8 多字节不劈字符)
  - 选区规范化: 反向拖动 (caret < anchor) 复制内容一致
  - 跨行拼接: 多行选区 `\n` 连接, 首行取后缀/末行取前缀/中间整行
  - 命中测试: 坐标→(行, 偏移) 映射, 含 gutter/展开符区偏移扣除
- **集成验证**: `cargo test` 全绿; 人工验收 = 实机双击/框选/Ctrl+C 贴进记事本比对
- **性能**: 命中几何随 paint 逐帧重建但只覆盖可见窗口字符 (event 无 TextBatch, TextInput char_offsets 同法), 成本 O(可见字符) 有界; 超长行不全测; logbench 数字不回归

## Boundaries

- **Always**: 提交三件套; 行锚定 f64/显示行→文件行经 filtered 映射的现有语义不动; 复制内容=原文 (不经截断省略); **复制上限 10 万行** (2026-09-08 评审 R3 + 用户拍板: 超限 Ctrl+C 不动作 + 底栏提示, 防全文件选区逐行解码冻结 UI)
- **Ask first**: 任何 danqing 引擎改动 (评审后实际两例: opt-in 键回退 + 指针捕获, 均经用户裁决); 新增依赖; 水平滚动/自动滚动等 Out 项的拉入
- **Never**: 未获用户指示 commit/push; 改 genlog 数据格式; 选区文本进日志/遥测; 热路径全文件 JSON parse

## Success Criteria

1. 原始模式: 双击行文本选中所在 token (空白分隔), 高亮可见; 实测双击 `2026-09-08T12:34:56.789Z` 一次选中整段时间戳
2. 原始模式: 按下拖动出字符级选区, 支持反向拖、跨行拖; 高亮跟随
3. Ctrl+C 后剪贴板内容 = 选区原文 (多行 `\n` 拼接), 贴进记事本逐字节一致 (含被显示截断行 —— 但原始模式行不截断, 此条主要约束表格模式)
4. 表格模式: 存在选中行时 Ctrl+C 复制该行完整原文行 (JSONL 整行), 不受单元格截断省略影响
5. 单击 = 维持现状选中行; 拖动/双击产生文本选区后, 行选中视觉让位; Esc 或新单击清除文本选区
6. 焦点行为: 点击日志区后 Ctrl+C 直达 LogView (不被过滤栏 TextInput 截胡); 焦点在过滤/搜索栏时 Ctrl+C 仍是 TextInput 原语义
7. `cargo test` 全绿 (含新增选区单测), clippy 零警告, logbench 基线不回归
8. 实机人工验收: 双击选词 / 框选 / 跨行 / 表格行复制 / 焦点切换五种姿势各过一遍
9. 拖出组件区域 (标题栏/窗外) 释放后选区不粘滞 (指针捕获, danqing 引擎 R1); 选区超 10 万行 Ctrl+C 不复制 + 底栏提示

## Open Questions

- **拖出视口自动滚动**: v1 不做 (框选大区间需分次拖)。若验收时体感明显跛脚, 升级为 follow-up 而非当场加
- **选区高亮与搜索命中高亮交叠**: 同一区间同时是命中词和选区时的叠色规则, 建议选区优先 (命中色让位), plan 阶段定稿
- **Shift+Click 扩展选区**: 编辑器惯例, 成本极低但不在用户需求里, 默认不做, 验收时用户若要再加
