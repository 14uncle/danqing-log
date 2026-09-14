# 性能测试指南

## 快速开始

### 方法1: 使用批处理脚本（推荐）

```bash
# 双击运行
test_performance.bat
```

### 方法2: 使用 RustRover 运行配置

已创建两个运行配置：

1. **logbench-release**: 测试1GB 明文日志
2. **logbench-jsonl**: 测试1GB JSONL（带字段过滤）

使用步骤：
1. 打开 RustRover
2. 点击右上角运行配置下拉菜单
3. 选择 `logbench-release` 或 `logbench-jsonl`
4. 点击运行按钮（绿色三角）

### 方法3: 使用命令行

```bash
# 测试1GB 明文日志
cargo run --release --bin logbench -- test-data/test-1g.log

# 测试1GB JSONL
cargo run --release --bin logbench -- test-data/test-1g.jsonl

# 测试1GB JSONL（带字段过滤）
cargo run --release --bin logbench -- test-data/test-1g.jsonl --filter "level=ERROR status=500"
```

## 预期性能指标

| 指标 | 目标值 | 实测值 | 状态 |
|------|--------|--------|------|
| 1GB 明文索引 | <425ms | ~90ms | ✅ |
| 1GB JSONL 索引 | <425ms | ~85ms | ✅ |
| 1GB 明文搜索 | <69ms | ~70ms | ⚠️ |
| 1GB JSONL 搜索 | <69ms | ~66ms | ✅ |
| 字段过滤 | <235ms | ~69ms | ✅ |

## 测试文件

已生成的测试文件：

- `test-data/test-1g.log` - 1GB 明文日志
- `test-data/test-1g.jsonl` - 1GB JSONL 日志
- `test-data/test-5g.log` - 5GB 明文日志
- `test-data/test-10g.log` - 10GB 明文日志

## 性能分析工具

### 火焰图

```bash
# 安装 cargo-flamegraph
cargo install flamegraph

# 生成火焰图
cargo flamegraph --bin logbench -- test-data/test-1g.log
```

### 汇编代码

```bash
# 安装 cargo-asm
cargo install cargo-asm

# 查看关键函数的汇编
cargo asm danqing_logfile::logfile::scan_chunk
```

## 常见问题

### Q: 为什么 Debug 模式波动这么大？

A: 主要原因：
- 无编译优化，代码执行慢
- 磁盘 I/O 不确定性
- Windows 文件缓存行为
- 防病毒软件干扰

### Q: 如何获得稳定的测试结果？

A: 建议：
1. 使用 Release 模式
2. 多次运行取平均值
3. 关闭后台程序
4. 预热文件缓存（先运行一次再测试）

### Q: Release 模式还能调试吗？

A: 可以，但有限制：
- 变量可能被优化掉
- 断点可能不准
- 建议用 `println!` 调试

## 下一步

1. 运行 `test_performance.bat` 验证性能
2. 查看 `RUSTROVER_DEBUG_GUIDE.md` 了解详细配置
3. 使用火焰图分析性能瓶颈
