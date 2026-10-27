#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""main.rs 拆分脚本: impl LogApp 方法按功能簇搬入 app_*.rs, mod tests 搬入 tests.rs。

用法: python tools/split_main.py [--apply]
默认 dry-run 只打印分桶结果; --apply 落盘。

项切分规则 (依赖 rustfmt 保证的缩进):
- impl 内方法签名行 = 4 空格缩进的 `fn <name>(` / `pub(crate) fn <name>(`
- 自由项签名行 = 0 缩进的 `fn <name>(` / `pub(crate) fn <name>(` / `const <name>`
- 项头向前吞连续的 4 空格 doc/注释/属性行 (自由项吞 0 缩进的)
"""
import re
import sys
from pathlib import Path

MAIN = Path("src/main.rs")

# 方法名 → 目标簇 (不在表里的留 main.rs)
BUCKETS = {
    "app_persist": [
        "save_config", "license_path", "state_path", "legacy_state_path",
        "load_state_account", "load_state_for_current_file",
        "save_session", "apply_session", "apply_session_payload", "delete_session",
        "rebuild_expands", "backup_if_corrupt", "save_state",
    ],
    "app_export": [
        "export_csv_columns", "export_entry_clicked", "begin_export",
        "export_name_parts", "export_line_set", "launch_export", "handle_export_result",
    ],
    "app_license": [
        "activate_license", "adopt_store_entitlement", "adopt_purchase_outcome",
        "purchase_paid_layer", "try_begin_purchase",
    ],
    "app_merge": [
        "merge_columns", "start_merge", "pickup_merge_job", "add_merge_source",
        "remove_selected_merge_source", "edit_source_time", "rebuild_merge",
        "start_trace", "merge_cap_notice", "discard_trace_job",
        "apply_trace_outcome", "apply_merge_outcome",
    ],
    "app_open": [
        "poll_growth", "poll_growth_merge", "rebuild_file", "apply_rebuild",
        "reload_file", "apply_fresh", "apply_appended", "pickup_open_job",
    ],
    "app_filter": [
        "apply_level_filter", "append_filter_hits", "drop_filter_hits_from",
        "merge_filter_hits", "parse_filter", "apply_filter", "clear_filter",
        "toggle_mode", "open_search", "clear_search", "apply_search",
        "jump_to_file_line", "next_hit", "prev_hit",
        "toggle_bookmark", "goto_next_bookmark",
    ],
    "app_status": [
        "loading_parts", "set_notice", "dismiss_notice", "expire_notice",
        "refresh_status", "set_status", "set_status_error",
    ],
}

# 自由函数 → 目标簇 (同样不在表里的留 main.rs)
FREE_BUCKETS = {
    "app_filter": ["build_clause", "clause_reject_notice", "next_bookmark", "build_search_pattern"],
    "app_status": ["status_text"],
}

CLUSTER_DOC = {
    "app_persist": "//! LogApp · 持久化簇: config.toml / state.json 读写、命名会话、损坏备份。",
    "app_export": "//! LogApp · 导出簇: 导出入口/格式/行集冻结/作业发起与交卷。",
    "app_license": "//! LogApp · 许可簇: key 激活、商店授权与购买流程。",
    "app_merge": "//! LogApp · 合并时间线簇: 归并发起/重建/交卷、源管理、追踪过滤。",
    "app_open": "//! LogApp · 打开与 live-tail 簇: 增长轮询、轮转重建、追平、打开作业拾取。",
    "app_filter": "//! LogApp · 过滤/搜索/书签簇: 行集过滤、命中导航、书签切换与跳转。",
    "app_status": "//! LogApp · 状态栏与提示簇: notice 生命周期、底栏合成、Loading 文案。",
}

HEADER = "//! @author 十四叔\n//! @date 2026/09/29\n"

METHOD_SIG = re.compile(r"^    (?:pub\(crate\) )?fn (\w+)")
FREE_SIG = re.compile(r"^(?:pub\(crate\) )?fn (\w+)")


def split_items(lines, start, end, indent):
    """把 [start, end) 区间切成顶层项列表 [(name, [行])], 保持文件顺序。

    indent = 项签名行缩进字符串。项 = [本项头, 下一项头) ——
    项头 = 签名行 + 向前连续的注释/属性行; 尾部空行归本项。
    const 也算自由项 (免被粘进前一项)。
    """
    if indent:
        sig = re.compile(rf"^{re.escape(indent)}(?:pub\(crate\) )?fn (\w+)")
    else:
        sig = re.compile(r"^(?:pub\(crate\) )?(?:fn|const) (\w+)")
    noise = re.compile(rf"^{re.escape(indent)}(?://|#\[)")
    sigs = [i for i in range(start, end) if sig.match(lines[i])]
    heads = []
    for s in sigs:
        h = s
        while h - 1 >= start and noise.match(lines[h - 1]):
            h -= 1
        heads.append(h)
    items = []
    for k, s in enumerate(sigs):
        tail = heads[k + 1] if k + 1 < len(heads) else end
        name = sig.match(lines[s]).group(1)
        items.append((name, lines[heads[k]:tail]))
    return items


def main():
    apply = "--apply" in sys.argv
    lines = MAIN.read_text(encoding="utf-8").splitlines(keepends=True)

    # --- 定位 impl LogApp 块 ---
    impl_start = next(i for i, l in enumerate(lines) if l.startswith("impl LogApp {"))
    impl_end = next(i for i in range(impl_start + 1, len(lines)) if lines[i] == "}\n")

    items = split_items(lines, impl_start + 1, impl_end, "    ")
    names = [n for n, _ in items]
    assert len(names) == len(set(names)), "方法名重复, 分桶表无法消歧"

    method_to_cluster = {m: c for c, ms in BUCKETS.items() for m in ms}
    free_to_cluster = {f: c for c, fs in FREE_BUCKETS.items() for f in fs}

    unknown = [n for n in names if n not in method_to_cluster]
    missing = [m for m in method_to_cluster if m not in names]

    # --- 自由函数区: impl LogApp 闭括号后到 impl App 前 + status_text ---
    app_impl_start = next(i for i, l in enumerate(lines) if l.startswith("impl App for LogApp {"))
    app_impl_end = next(i for i in range(app_impl_start + 1, len(lines)) if lines[i] == "}\n")
    free_items = split_items(lines, impl_end + 1, app_impl_start, "")
    free_names = [n for n, _ in free_items]
    # status_text 在 impl App 之后、main() 之前
    main_fn = next(i for i, l in enumerate(lines) if l.startswith("fn main()"))
    free_items2 = split_items(lines, app_impl_end + 1, main_fn, "")
    free_names2 = [n for n, _ in free_items2]
    missing_free = [f for f in free_to_cluster
                    if f not in free_names and f not in free_names2]

    print(f"impl LogApp 方法 {len(names)} 个: 移走 {len(names) - len(unknown)}, 留 main {len(unknown)}")
    for c in BUCKETS:
        got = [n for n, _ in items if method_to_cluster.get(n) == c]
        print(f"  {c}: {len(got)} 方法 + 自由函数 {FREE_BUCKETS.get(c, [])}")
    print(f"  留 main: {unknown}")
    if missing:
        print(f"!! 分桶表里有但 impl 里找不到: {missing}")
    if missing_free:
        print(f"!! 自由函数找不到: {missing_free}")
    print(f"自由函数区1: {free_names}")
    print(f"自由函数区2 (impl App 后): {free_names2}")

    if not apply:
        print("\n(dry-run, 未落盘; 核对后加 --apply)")
        return

    # --- tests 模块提取 ---
    cfg_test = next(i for i, l in enumerate(lines) if l.startswith("#[cfg(test)]"))
    mod_tests = cfg_test + 1
    assert lines[mod_tests] == "mod tests {\n", f"tests 块头不符: {lines[mod_tests]!r}"
    test_body = lines[mod_tests + 1:-1]
    assert lines[-1] == "}\n", f"文件末行不是 tests 闭括号: {lines[-1]!r}"

    tests_out = (HEADER + "//!\n//! main.rs 的测试模块 (2026-09-29 从 main.rs 原样搬出, 内容零修改)。\n\n"
                 + "".join(test_body))
    Path("src/tests.rs").write_text(tests_out, encoding="utf-8", newline="")

    # --- 簇文件 ---
    for cluster, doc in CLUSTER_DOC.items():
        meths = [blk for n, blk in items if method_to_cluster.get(n) == cluster]
        frees = [blk for n, blk in free_items if free_to_cluster.get(n) == cluster]
        frees += [blk for n, blk in free_items2 if free_to_cluster.get(n) == cluster]
        parts = [HEADER, doc, "\nuse super::*;\n"]
        if meths:
            parts.append("\nimpl LogApp {\n")
            for blk in meths:
                parts.append("".join(blk).rstrip("\n") + "\n")
            parts.append("}\n")
        for blk in frees:
            parts.append("\n" + "".join(blk).rstrip("\n") + "\n")
        Path(f"src/{cluster}.rs").write_text("".join(parts), encoding="utf-8", newline="")

    # --- main.rs 重写 (保留项维持文件顺序) ---
    kept = [blk for n, blk in items if n not in method_to_cluster]
    kept_free = [blk for n, blk in free_items if n not in free_to_cluster]
    kept_free2 = [blk for n, blk in free_items2 if n not in free_to_cluster]
    out = []
    out += lines[:impl_start + 1]
    for blk in kept:
        out.append("".join(blk).rstrip("\n") + "\n\n")
    out.append("}\n\n")
    for blk in kept_free:
        out.append("".join(blk).rstrip("\n") + "\n\n")
    out += lines[app_impl_start:app_impl_end + 1]
    out.append("\n")
    for blk in kept_free2:
        out.append("".join(blk).rstrip("\n") + "\n\n")
    out += lines[main_fn:cfg_test]
    out.append("#[cfg(test)]\nmod tests;\n")

    # mod 声明按字母序插入
    new_mods = sorted(BUCKETS)
    text = "".join(out)
    anchor = "mod analysis_panel;\n"
    assert anchor in text
    text = text.replace(anchor, anchor + "".join(f"mod {m};\n" for m in new_mods), 1)
    MAIN.write_text(text, encoding="utf-8", newline="")
    print("落盘完成: 7 簇文件 + tests.rs + main.rs 重写")


if __name__ == "__main__":
    main()
