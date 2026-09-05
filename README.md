# 丹青日志 (danqing-log)

大文件日志 / JSONL 查看分析器 —— 丹青第四件产品。**当前为 POC 阶段**。

意图与开枪前提: [`../danqing/docs/intent/log-viewer-poc.md`](../danqing/docs/intent/log-viewer-poc.md)

## 用法

```bash
# GUI: 打开日志文件 (JSONL 自动检出 → 列化表格)
cargo run --release -- <日志文件路径>

# 无窗口基准 (打开/索引/搜索/过滤/随机访问实测表)
cargo run --release --bin logbench -- <日志文件> [--filter "level=ERROR status=50*"] [正则...]

# 生成测试数据 (确定性)
cargo run --release --bin genlog -- out.log 1024          # 1 GiB 明文日志
cargo run --release --bin genlog -- out.jsonl 1024 --jsonl # 1 GiB JSONL
```

GUI 操作: 滚轮/方向键滚动, PageUp/PageDown/空格翻页 (Shift 反向), Home/End 跳头尾,
Shift+滚轮横滚, 点击选中行。
搜索: `/` (原始模式) 或 Ctrl+F (任意模式) 开搜索栏 → 输入正则 Enter 应用,
栏空后 Enter/Shift+Enter 下/上一命中, Esc 关闭。
书签: `b`/`'` (原始模式) 或 Ctrl+B/Ctrl+G (任意模式) 切换/跳下一书签 (金色行号)。
JSONL 检出后开局即表格模式: 直接打字进过滤框 (`level=ERROR status=50*`, 空格分词 AND,
尾缀 `*` 前缀通配, 裸词整行子串), Enter 应用 (工作线程过滤不冻界面), Esc 清除,
Ctrl+T 原始/表格互切。

编码: UTF-8 (含 BOM) / UTF-16 LE·BE (有无 BOM 均可, 打开时转码副本) /
GBK (原字节索引 + 行级 CP936 解码, 中文查询可搜) / 其余单字节编码 Latin-1 兜底显示。

## POC 边界 (core-viewer 后仍成立的部分)

- UTF-16 转码副本内存翻倍 (1GB→500MB); >2GB 文档化边界
- tail 跟随未实现 (live-tail 模块, spec 已备); 引擎侧截断/轮转生存原语已备
  (FileStat 快照/is_stale/rebuild; Windows 实测 OS 拒绝截断被映射文件, 见 tasks/plan.md 附录)
- JSONL 列化只认扁平顶层字段 (嵌套展开 = jsonl-table 模块, spec 已备);
  GBK/Latin-1 文件搜索退化为字面量 (无正则语法)
- 表格模式列无拖拽/显隐; 水平滚动范围为「边探索边长」估计 (max_seen, 视图态)

## 许可

MIT OR Apache-2.0
