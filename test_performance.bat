@echo off
echo ========================================
echo danqing-log 性能测试 (Release 模式)
echo ========================================

echo.
echo [1/4] 构建 Release 版本...
cargo build --release --bin logbench
if errorlevel 1 (
    echo 构建失败！
    pause
    exit /b 1
)

echo.
echo [2/4] 测试1GB 明文日志...
cargo run --release --bin logbench -- test-data/test-1g.log

echo.
echo [3/4] 测试1GB JSONL...
cargo run --release --bin logbench -- test-data/test-1g.jsonl

echo.
echo [4/4] 测试1GB JSONL (带字段过滤)...
cargo run --release --bin logbench -- test-data/test-1g.jsonl --filter "level=ERROR status=500"

echo.
echo ========================================
echo 测试完成
echo ========================================
pause
