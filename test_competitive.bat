@echo off
echo ========================================
echo 丹青日志 LogLens 竞品对比测试
echo ========================================

echo.
echo [1/5] 构建 Release 版本...
cargo build --release --bin logbench
if errorlevel 1 (
    echo 构建失败！
    pause
    exit /b 1
)

echo.
echo [2/5] 测试1GB 明文日志（冷启动）...
echo ----------------------------------------
cargo run --release --bin logbench -- test-data/test-1g.log

echo.
echo [3/5] 测试1GB 明文日志（热启动）...
echo ----------------------------------------
cargo run --release --bin logbench -- test-data/test-1g.log

echo.
echo [4/5] 测试1GB JSONL（冷启动）...
echo ----------------------------------------
cargo run --release --bin logbench -- test-data/test-1g.jsonl

echo.
echo [5/5] 测试1GB JSONL（热启动 + 字段过滤）...
echo ----------------------------------------
cargo run --release --bin logbench -- test-data/test-1g.jsonl --filter "level=ERROR status=500"

echo.
echo ========================================
echo 测试完成
echo ========================================
echo.
echo 结果说明：
echo - 冷启动：首次打开，受磁盘 I/O 影响
echo - 热启动：文件已在缓存，性能稳定
echo - 字段过滤：JSONL 独家功能
echo.
pause
