# RustRover 调试配置指南

## 配置 Release 模式测试

### 方法1: 修改 Run 配置

1. 打开 RustRover
2. 点击右上角的运行配置下拉菜单
3. 选择 "Edit Configurations..."
4. 找到 `logbench` 配置（或创建新的）
5. 修改以下设置：
   - **Build profile**: 选择 `release`
   - **Program arguments**: `test-data/test-1g.log`
   - **Working directory**: `F:\github\farm01\danqing-log`

### 方法2: 使用 Cargo 命令

在 RustRover 的 Terminal 中运行：

```bash
# Release 模式测试
cargo run --release --bin logbench -- test-data/test-1g.log

# 带字段过滤的 JSONL 测试
cargo run --release --bin logbench -- test-data/test-1g.jsonl --filter "level=ERROR status=500"
```

### 方法3: 创建专用测试脚本

创建 `test_performance.bat`:

```batch
@echo off
echo ========================================
echo danqing-log 性能测试 (Release 模式)
echo ========================================

echo.
echo [1/3] 构建 Release 版本...
cargo build --release --bin logbench

echo.
echo [2/3] 测试1GB 明文日志...
cargo run --release --bin logbench -- test-data/test-1g.log

echo.
echo [3/3] 测试1GB JSONL (带字段过滤)...
cargo run --release --bin logbench -- test-data/test-1g.jsonl --filter "level=ERROR status=500"

echo.
echo ========================================
echo 测试完成
echo ========================================
pause
```

## 性能对比

| 模式 | 索引耗时 | 搜索耗时 | 说明 |
|------|---------|---------|------|
| Debug (冷启动) | ~1700ms | ~200ms | 无优化，首次磁盘 I/O |
| Debug (热启动) | ~200ms | ~50ms | 无优化，内存缓存 |
| **Release** | **~90ms** | **~70ms** | **优化后，稳定** |

## 为什么 Release 模式快这么多？

1. **编译优化**：`lto = "fat"` + `codegen-units = 1` + `opt-level = "z"`
2. **SIMD 指令**：memchr 使用 SIMD 优化换行符查找
3. **内联优化**：函数调用开销减少
4. **循环优化**：循环展开和向量化

## 调试技巧

### 查看优化效果

```bash
# 对比 Debug vs Release
cargo run --bin logbench -- test-data/test-1g.log
cargo run --release --bin logbench -- test-data/test-1g.log
```

### 查看汇编代码

```bash
# 安装 cargo-asm
cargo install cargo-asm

# 查看关键函数的汇编
cargo asm danqing_logfile::logfile::scan_chunk
```

### 性能分析

```bash
# 安装 cargo-flamegraph
cargo install flamegraph

# 生成火焰图
cargo flamegraph --bin logbench -- test-data/test-1g.log
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

配置完成后，运行以下命令验证：

```bash
cargo run --release --bin logbench -- test-data/test-1g.log
```

预期结果：
- 索引：<100ms
- 搜索：<70ms
- 字段过滤：<100ms
