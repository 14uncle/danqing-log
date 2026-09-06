//! @author 十四叔
//! @date 2026/09/05
//!
//! JSONL 列化引擎: 检测 / 列发现 / 字段提取 / 字段过滤。
//!
//! 开枪前提②的主炮: 列化视图 + `level=ERROR status=50*` 字段过滤,
//! 对标 VS Code 扩展 daucloud.json-viewer (独立窗口、秒开、不占编辑器)。
//!
//! 显示路径 (T1): 逐可见行 serde_json parse (真 parser, 恒定成本), 嵌套值紧凑显示;
//! 过滤路径: 扁平等值/前缀走 memmem 粗筛零 parse (性能), 点路径/比较算子才
//! parse 验证 (T2 两段架构)。memmem 提取 (`extract_field`) 仅供过滤粗筛, 不再进显示。

use crate::logfile::LogFile;

/// 列描述: 字段名 + 展示宽度 (字符数, 采样最大值, [4, 32] 截断)。
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub width_chars: usize,
}

/// 列化模式: 按首见顺序排列的列集合。
#[derive(Debug, Clone)]
pub struct Schema {
    pub columns: Vec<Column>,
}

/// 检测行数采样上限。
const DETECT_SAMPLE: u64 = 64;
/// 列发现采样上限。
const SCHEMA_SAMPLE: u64 = 512;
/// 列数上限。
const MAX_COLUMNS: usize = 16;
/// 判定为 JSONL 的 object 行占比下限。
const DETECT_THRESHOLD: f64 = 0.9;

/// JSONL 检测: 采样前 DETECT_SAMPLE 行, 非空行中 ≥90% 能 parse 成 JSON object。
/// 采样不足 3 行不判 (证据不足), 空文件/全坏行 → false。
pub fn detect(file: &LogFile) -> bool {
    let n = file.line_count().min(DETECT_SAMPLE);
    let mut nonempty = 0u64;
    let mut objects = 0u64;
    for i in 0..n {
        let line = file.line(i);
        if line.is_empty() {
            continue;
        }
        nonempty += 1;
        if serde_json::from_slice::<serde_json::Value>(line).is_ok_and(|v| v.is_object()) {
            objects += 1;
        }
    }
    nonempty >= 3 && objects as f64 >= nonempty as f64 * DETECT_THRESHOLD
}

/// 列发现: 采样前 SCHEMA_SAMPLE 行, 按首见顺序收顶层 key (≤MAX_COLUMNS 列)。
/// 宽度 = max(字段名长度, 采样值展示长度), 截到 [4, 32]。
/// 非 JSONL / 采样零列 → None。
pub fn discover_schema(file: &LogFile) -> Option<Schema> {
    let n = file.line_count().min(SCHEMA_SAMPLE);
    let mut columns: Vec<Column> = Vec::new();
    for i in 0..n {
        let Ok(serde_json::Value::Object(map)) =
            serde_json::from_slice::<serde_json::Value>(file.line(i))
        else {
            continue;
        };
        for (key, value) in &map {
            let shown_len = match value {
                serde_json::Value::String(s) => s.chars().count(),
                other => other.to_string().chars().count(),
            };
            match columns.iter_mut().find(|c| c.name == *key) {
                Some(col) => col.width_chars = col.width_chars.max(shown_len),
                None => {
                    if columns.len() < MAX_COLUMNS {
                        columns.push(Column {
                            name: key.clone(),
                            width_chars: key.chars().count().max(shown_len),
                        });
                    }
                }
            }
        }
    }
    if columns.is_empty() {
        return None;
    }
    for c in &mut columns {
        c.width_chars = c.width_chars.clamp(4, 32);
    }
    Some(Schema { columns })
}

/// 构造字段定位针: `"key":`。
fn field_needle(key: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(key.len() + 3);
    v.push(b'"');
    v.extend_from_slice(key.as_bytes());
    v.extend_from_slice(b"\":");
    v
}

/// 扁平字段提取: 返回值 token 切片 (字符串去引号, 数字/bool/null 原样)。
/// 前缀校验: key 之前 (跳过空白) 必须是 `{` 或 `,`, 挡住行内裸文本误配的一半。
/// 仅供**过滤粗筛** (T2 两段架构); 显示路径已换真 parser (见 [`parse_line`])。
pub fn extract_field<'a>(line: &'a [u8], key: &str) -> Option<&'a [u8]> {
    extract_with_needle(line, &field_needle(key))
}

/// 解析一行 JSON (显示路径, 只 parse 可见行, 恒定成本)。
/// 淘汰 memmem 提取出显示路径 —— 消除 POC「`,"key":"` 内嵌误判」已知边界。
pub fn parse_line(raw: &[u8]) -> Option<serde_json::Value> {
    serde_json::from_slice(raw).ok()
}

/// 单元格紧凑显示: 字符串裸值, 其余 (对象/数组/数字/bool/null) `to_string` 紧凑。
pub fn cell_display(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// 展开子行: 嵌套结构的一条 (深度, 路径段, 紧凑值)。
#[derive(Debug, Clone, PartialEq)]
pub struct SubRow {
    /// 缩进深度 (1 = 顶层字段, 递增)。
    pub depth: usize,
    /// 路径段: 对象 key 或数组索引 `[i]`。
    pub label: String,
    /// 值紧凑显示 (容器 = `{...}`/`[...]`, 叶子 = [`cell_display`])。
    pub value: String,
}

/// 展开一行 JSON: 顶层对象/数组字段 → 递归子行序列 (顶层叶子 = 列, 不进子行)。
/// 数组段显示为 `[i]` (spec Open Question 已答)。根为数组非 JSONL (spec Out)。
pub fn flatten(value: &serde_json::Value) -> Vec<SubRow> {
    let mut out = Vec::new();
    if let serde_json::Value::Object(map) = value {
        for (k, v) in map {
            if v.is_object() || v.is_array() {
                flatten_into(k, v, 1, &mut out);
            }
            // 顶层叶子 = 列, 不重复为子行
        }
    }
    out
}

/// 该 JSON 值是否可展开 (顶层有对象/数组字段)。等价 `!flatten(v).is_empty()`, 但更省。
pub fn is_expandable(value: &serde_json::Value) -> bool {
    matches!(value, serde_json::Value::Object(map) if map.values().any(|v| v.is_object() || v.is_array()))
}

/// 递归拍平一个 (路径段, 值) 对: 先推容器/叶子子行, 容器再递归子节点。
fn flatten_into(label: &str, val: &serde_json::Value, depth: usize, out: &mut Vec<SubRow>) {
    out.push(SubRow {
        depth,
        label: label.to_string(),
        value: cell_display(val),
    });
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                flatten_into(k, v, depth + 1, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                flatten_into(&format!("[{i}]"), v, depth + 1, out);
            }
        }
        _ => {}
    }
}

/// 找下一个带合法前缀的字段值 token: 返回 (值 token, 下一搜索起点)。
/// 前缀校验: key 之前 (跳过空白) 必须是 `{` 或 `,`。字符串去引号, 数字/bool/null 原样。
fn next_field_value<'a>(
    line: &'a [u8],
    needle: &[u8],
    mut from: usize,
) -> Option<(&'a [u8], usize)> {
    loop {
        let pos = memchr::memmem::find(&line[from..], needle)? + from;
        let mut i = pos;
        let prefix_ok = loop {
            if i == 0 {
                break false;
            }
            i -= 1;
            match line[i] {
                b' ' | b'\t' => continue,
                b'{' | b',' => break true,
                _ => break false,
            }
        };
        let next = pos + needle.len();
        if !prefix_ok {
            from = next;
            continue;
        }
        let mut s = next;
        while s < line.len() && matches!(line[s], b' ' | b'\t') {
            s += 1;
        }
        if s >= line.len() {
            return None;
        }
        let token = if line[s] == b'"' {
            // 字符串: 到未转义的收尾引号
            let mut e = s + 1;
            let mut closed = false;
            while e < line.len() {
                match line[e] {
                    b'\\' => e += 1,
                    b'"' => {
                        closed = true;
                        break;
                    }
                    _ => {}
                }
                e += 1;
            }
            if !closed {
                return None;
            }
            &line[s + 1..e]
        } else {
            let mut e = s;
            while e < line.len() && !matches!(line[e], b',' | b'}' | b' ' | b'\t') {
                e += 1;
            }
            &line[s..e]
        };
        return Some((token, next));
    }
}

/// 值 token 切取 (needle 预编译版, 供扁平过滤直通): 返回首个带合法前缀的字段值。
fn extract_with_needle<'a>(line: &'a [u8], needle: &[u8]) -> Option<&'a [u8]> {
    next_field_value(line, needle, 0).map(|(v, _)| v)
}

/// 行内是否存在某字段值命中 (点路径粗筛): 遍历所有 `"leaf":` 出现, 任一值匹配即 true。
/// 无假阴性 (值命中行必过), 把候选压到「值命中量级」再 parse 验证 —— 避免全量 parse。
fn field_value_matches(line: &[u8], needle: &[u8], op: Op, value: &str) -> bool {
    let mut from = 0usize;
    while let Some((token, next)) = next_field_value(line, needle, from) {
        if token_matches(token, op, value) {
            return true;
        }
        from = next;
    }
    false
}

/// 比较算子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `=` 全等。
    Eq,
    /// `=...*` 前缀通配。
    Prefix,
    /// `>` 数值大于。
    Gt,
    /// `>=` 数值大于等于。
    GtEq,
    /// `<` 数值小于。
    Lt,
    /// `<=` 数值小于等于。
    LtEq,
}

/// 过滤子句。
#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    /// 字段匹配: `key=value` / `a.b.c=value` / `status>=500` / `level=ERR*`。
    Field {
        /// 字段路径 (点分; 单段 = 顶层)。
        path: Vec<String>,
        op: Op,
        value: String,
    },
    /// 裸词: 整行子串。
    Bare(String),
}

/// 解析查询: 空白分词, 含算子 (`= >= <= > <`) 为字段子句, 否则裸词。AND 语义。
pub fn parse_query(q: &str) -> Vec<Clause> {
    q.split_whitespace().map(parse_clause).collect()
}

/// 单 token → 子句。
fn parse_clause(tok: &str) -> Clause {
    let Some((path_str, op, value)) = split_operator(tok) else {
        return Clause::Bare(tok.to_string());
    };
    let path: Vec<String> = path_str
        .split('.')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if path.is_empty() {
        return Clause::Bare(tok.to_string());
    }
    // `=` + 值尾缀 `*` → 前缀通配 (比较算子不识别 `*`)
    let (op, value) = if op == Op::Eq {
        match value.strip_suffix('*') {
            Some(prefix) => (Op::Prefix, prefix.to_string()),
            None => (Op::Eq, value.to_string()),
        }
    } else {
        (op, value.to_string())
    };
    Clause::Field { path, op, value }
}

/// 找算子: 返回 (路径串, 算子, 值串)。长算子 (`>=`/`<=`) 先于短算子匹配。
fn split_operator(tok: &str) -> Option<(&str, Op, &str)> {
    for (pat, op) in [
        (">=", Op::GtEq),
        ("<=", Op::LtEq),
        (">", Op::Gt),
        ("<", Op::Lt),
        ("=", Op::Eq),
    ] {
        if let Some(pos) = tok.find(pat) {
            return Some((&tok[..pos], op, &tok[pos + pat.len()..]));
        }
    }
    None
}

/// 预编译子句 (needle 只造一次, 过滤循环零分配)。
enum Compiled {
    /// 裸词: 整行子串 (memmem)。
    Bare(Vec<u8>),
    /// 扁平等值/前缀: memmem 提取值 token 直通, 零 parse (POC 性能路径)。
    Flat {
        needle: Vec<u8>,
        op: Op,
        value: String,
    },
    /// 需 parse 验证 (点路径或比较算子): 粗筛最内层 key + serde_json 导航比较。
    Verify {
        needle: Vec<u8>,
        path: Vec<String>,
        op: Op,
        value: String,
    },
}

/// 值 token 匹配: Eq 全等, Prefix 前缀, 比较算子按数值 (token 解析, 非数值不匹配)。
/// 扁平直通与点路径粗筛共用。比较按 token 解析成 f64 —— 字符串 "500" 也会当数值 500,
/// 与 parse 路径 (`compare_val`) 的严格字符串/数值区分有差异 (有意边界: 扁平比较是性能直通)。
fn token_matches(token: &[u8], op: Op, target: &str) -> bool {
    match op {
        Op::Eq => token == target.as_bytes(),
        Op::Prefix => token.starts_with(target.as_bytes()),
        Op::Gt | Op::GtEq | Op::Lt | Op::LtEq => {
            let Some(n) = std::str::from_utf8(token)
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
            else {
                return false;
            };
            let Ok(t) = target.parse::<f64>() else {
                return false;
            };
            match op {
                Op::Gt => n > t,
                Op::GtEq => n >= t,
                Op::Lt => n < t,
                Op::LtEq => n <= t,
                _ => unreachable!(),
            }
        }
    }
}

/// 验证路径比较: 等值/前缀按显示串 ([`cell_display`]), 比较按数值 (非数值不匹配)。
fn compare_val(val: &serde_json::Value, op: Op, target: &str) -> bool {
    match op {
        Op::Eq => cell_display(val) == target,
        Op::Prefix => cell_display(val).starts_with(target),
        Op::Gt | Op::GtEq | Op::Lt | Op::LtEq => {
            let Some(n) = val.as_f64() else {
                return false;
            };
            let Ok(t) = target.parse::<f64>() else {
                return false;
            };
            match op {
                Op::Gt => n > t,
                Op::GtEq => n >= t,
                Op::Lt => n < t,
                Op::LtEq => n <= t,
                _ => unreachable!(),
            }
        }
    }
}

/// 沿路径导航: 对象取 key, 数组取索引。中途非对象/数组 = 无。
fn navigate<'a>(mut val: &'a serde_json::Value, path: &[String]) -> Option<&'a serde_json::Value> {
    for seg in path {
        val = match val {
            serde_json::Value::Object(map) => map.get(seg)?,
            serde_json::Value::Array(arr) => {
                let i: usize = seg.parse().ok()?;
                arr.get(i)?
            }
            _ => return None,
        };
    }
    Some(val)
}

/// 单行判定: 全部子句命中 (AND)。
fn line_matches(line: &[u8], clauses: &[Compiled]) -> bool {
    clauses.iter().all(|c| match c {
        Compiled::Bare(w) => memchr::memmem::find(line, w).is_some(),
        Compiled::Flat { needle, op, value } => {
            extract_with_needle(line, needle).is_some_and(|v| token_matches(v, *op, value))
        }
        Compiled::Verify {
            needle,
            path,
            op,
            value,
        } => {
            // 粗筛: 任一 leaf 字段值命中才 parse 验证 (把候选压到值命中量级, 免全量 parse)
            if !field_value_matches(line, needle, *op, value) {
                return false;
            }
            let Some(parsed) = parse_line(line) else {
                return false;
            };
            let Some(val) = navigate(&parsed, path) else {
                return false;
            };
            compare_val(val, *op, value)
        }
    })
}

/// 全文字段过滤: 顺序走一遍 (lines() 迭代器, 每行 O(1)), 返回命中行号 (升序)。
/// 单线程: 实测 1GB/483 万行量级 < 1s 则不并行 (简洁优先)。
/// 禁用 line(i) 逐行随机访问 —— 步进索引下每次定位带段内前扫, 全量遍历会
/// 把成本乘进行数 (T1 实测回归 235ms → 1072ms 的教训, 见 logfile.rs::lines 注释)。
pub fn run_filter(file: &LogFile, clauses: &[Clause]) -> Vec<u64> {
    run_filter_from(file, clauses, 0)
}

/// 增量过滤: 只对 >= start_line 的行跑谓词 (live-tail 追加), 返回命中行号 (升序)。
/// 走 `lines_from` 顺序扫描, 不重扫前文 —— 与全量路径命中集一致 (单测对拍)。
pub fn run_filter_from(file: &LogFile, clauses: &[Clause], start_line: u64) -> Vec<u64> {
    let compiled: Vec<Compiled> = clauses.iter().map(compile).collect();
    if compiled.is_empty() {
        return (start_line..file.line_count()).collect();
    }
    let mut hits = Vec::new();
    for (i, line) in file.lines_from(start_line) {
        if line_matches(line, &compiled) {
            hits.push(i);
        }
    }
    hits
}

/// 子句 → 预编译 (needle = 最内层 key 的 `"key":` 针)。
fn compile(c: &Clause) -> Compiled {
    match c {
        Clause::Bare(w) => Compiled::Bare(w.as_bytes().to_vec()),
        Clause::Field { path, op, value } => {
            let leaf = path.last().map(String::as_str).unwrap_or("");
            let needle = field_needle(leaf);
            // 扁平字段 (任意算子) → token 直通零 parse; 点路径 → 粗筛 + parse 验证
            if path.len() == 1 {
                Compiled::Flat {
                    needle,
                    op: *op,
                    value: value.clone(),
                }
            } else {
                Compiled::Verify {
                    needle,
                    path: path.clone(),
                    op: *op,
                    value: value.clone(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 落一个临时 JSONL 文件并打开 (文件名全局唯一: Windows 拒绝截断仍被映射的文件)。
    fn open_with(content: &[u8]) -> LogFile {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "danqing-log-jsonl-test-{}-{}-{}.log",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            content.len()
        ));
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(content).unwrap();
        }
        let lf = LogFile::open(&path).unwrap();
        std::fs::remove_file(&path).ok();
        lf
    }

    #[test]
    fn detect_jsonl_true() {
        let lf = open_with(b"{\"a\":1}\n{\"a\":2,\"b\":\"x\"}\n{\"a\":3}\n{\"a\":4}\n");
        assert!(detect(&lf));
    }

    #[test]
    fn detect_plain_log_false() {
        let lf = open_with(
            b"2026-09-05 INFO hello\n2026-09-05 INFO world\n2026-09-05 WARN x\n2026-09-05 INFO y\n",
        );
        assert!(!detect(&lf));
    }

    #[test]
    fn detect_tolerates_some_bad_lines() {
        // 10 行里 1 行坏 = 90% 及格线
        let mut content = String::new();
        for i in 0..9 {
            content.push_str(&format!("{{\"a\":{i}}}\n"));
        }
        content.push_str("not json\n");
        let lf = open_with(content.as_bytes());
        assert!(detect(&lf), "90% object 占比应判 JSONL");
    }

    #[test]
    fn schema_first_seen_order_and_widths() {
        let lf = open_with(
            b"{\"level\":\"INFO\",\"msg\":\"hi\",\"duration_ms\":12}\n{\"level\":\"ERROR\",\"msg\":\"a much longer message here\",\"status\":500}\n",
        );
        let s = discover_schema(&lf).expect("应有列");
        let names: Vec<&str> = s.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["level", "msg", "duration_ms", "status"],
            "首见顺序"
        );
        let msg = &s.columns[1];
        assert_eq!(msg.width_chars, 26, "宽度取采样最大值: {msg:?}");
        let status = &s.columns[3];
        assert_eq!(status.width_chars, 6, "短值按字段名长度: {status:?}");
    }

    #[test]
    fn extract_string_number_bool() {
        let line = br#"{"level":"ERROR","duration_ms":123,"ok":true,"ref":null}"#;
        assert_eq!(extract_field(line, "level"), Some(&b"ERROR"[..]));
        assert_eq!(extract_field(line, "duration_ms"), Some(&b"123"[..]));
        assert_eq!(extract_field(line, "ok"), Some(&b"true"[..]));
        assert_eq!(extract_field(line, "ref"), Some(&b"null"[..]));
        assert_eq!(extract_field(line, "missing"), None);
    }

    #[test]
    fn extract_skips_escaped_quote_in_string() {
        let line = br#"{"msg":"say \"hi\" now","x":1}"#;
        assert_eq!(extract_field(line, "msg"), Some(&br#"say \"hi\" now"#[..]));
        assert_eq!(extract_field(line, "x"), Some(&b"1"[..]));
    }

    #[test]
    fn extract_rejects_key_without_json_prefix() {
        // 裸文本里的 "level":"X" (前面不是 { 或 ,) 不算字段
        let line = br#"log prefix "level":"ERROR" tail"#;
        assert_eq!(extract_field(line, "level"), None);
    }

    #[test]
    fn parse_line_resolves_adversarial_embedded_key() {
        // 对抗样本: msg 值内含 `,"key":"` 形态 —— memmem 提取会误判成 WRONG,
        // 真 parser 不会 (T1 显示路径换 parser 的核心回归)。
        let line = br#"{"msg":"see ,\"key\":\"WRONG\" here","key":"RIGHT"}"#;
        let v = parse_line(line).expect("应可解析");
        assert_eq!(cell_display(v.get("key").unwrap()), "RIGHT");
        assert_eq!(
            cell_display(v.get("msg").unwrap()),
            "see ,\"key\":\"WRONG\" here"
        );
    }

    #[test]
    fn cell_display_compacts_nested_values() {
        let v = parse_line(br#"{"user":{"id":42},"tags":[1,2],"ok":true,"ref":null}"#).unwrap();
        assert_eq!(cell_display(v.get("user").unwrap()), "{\"id\":42}");
        assert_eq!(cell_display(v.get("tags").unwrap()), "[1,2]");
        assert_eq!(cell_display(v.get("ok").unwrap()), "true");
        assert_eq!(cell_display(v.get("ref").unwrap()), "null");
    }

    #[test]
    fn flatten_nested_object_and_array() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"level":"INFO","user":{"id":42,"name":"bob"},"tags":[7,8]}"#)
                .unwrap();
        let rows = flatten(&v);
        // level 是顶层叶子 = 列, 不进子行; user/tags 是容器 → 展开
        assert_eq!(
            rows,
            vec![
                SubRow {
                    depth: 1,
                    label: "user".into(),
                    value: "{\"id\":42,\"name\":\"bob\"}".into()
                },
                SubRow {
                    depth: 2,
                    label: "id".into(),
                    value: "42".into()
                },
                SubRow {
                    depth: 2,
                    label: "name".into(),
                    value: "bob".into()
                },
                SubRow {
                    depth: 1,
                    label: "tags".into(),
                    value: "[7,8]".into()
                },
                SubRow {
                    depth: 2,
                    label: "[0]".into(),
                    value: "7".into()
                },
                SubRow {
                    depth: 2,
                    label: "[1]".into(),
                    value: "8".into()
                },
            ]
        );
    }

    #[test]
    fn flatten_top_level_scalars_are_columns_not_rows() {
        let v = parse_line(br#"{"level":"ERROR","msg":"x"}"#).unwrap();
        assert!(flatten(&v).is_empty(), "全顶层叶子 → 无子行");
    }

    #[test]
    fn is_expandable_detects_nested_fields() {
        let v = parse_line(br#"{"user":{"id":1},"tags":[1,2]}"#).unwrap();
        assert!(is_expandable(&v), "有嵌套对象/数组");
        let flat = parse_line(br#"{"level":"ERROR","msg":"x"}"#).unwrap();
        assert!(!is_expandable(&flat), "全顶层叶子不可展开");
    }

    #[test]
    fn parse_query_splits_clauses() {
        let clauses = parse_query("level=ERROR status=50* slow");
        assert_eq!(
            clauses,
            vec![
                Clause::Field {
                    path: vec!["level".into()],
                    op: Op::Eq,
                    value: "ERROR".into()
                },
                Clause::Field {
                    path: vec!["status".into()],
                    op: Op::Prefix,
                    value: "50".into()
                },
                Clause::Bare("slow".into()),
            ]
        );
        // 点路径 + 比较算子 (长算子 >= 先于 >)
        let clauses = parse_query("user.id=42 status>=500 duration_ms>1000");
        assert_eq!(
            clauses,
            vec![
                Clause::Field {
                    path: vec!["user".into(), "id".into()],
                    op: Op::Eq,
                    value: "42".into()
                },
                Clause::Field {
                    path: vec!["status".into()],
                    op: Op::GtEq,
                    value: "500".into()
                },
                Clause::Field {
                    path: vec!["duration_ms".into()],
                    op: Op::Gt,
                    value: "1000".into()
                },
            ]
        );
    }

    #[test]
    fn token_matches_eq_and_prefix() {
        assert!(token_matches(b"500", Op::Eq, "500"));
        assert!(!token_matches(b"500", Op::Eq, "50"));
        assert!(token_matches(b"500", Op::Prefix, "50"));
        assert!(token_matches(b"502", Op::Prefix, "50"));
        assert!(!token_matches(b"200", Op::Prefix, "50"));
        assert!(token_matches(b"ERROR", Op::Eq, "ERROR"));
        assert!(!token_matches(b"ERRORS", Op::Eq, "ERROR"));
    }

    #[test]
    fn navigate_dot_path_and_array() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"user":{"id":42,"tags":[7,8]}}"#).unwrap();
        let p = |s: &str| s.split('.').map(String::from).collect::<Vec<_>>();
        assert_eq!(navigate(&v, &p("user")).unwrap()["id"].as_i64(), Some(42));
        assert_eq!(navigate(&v, &p("user.id")).unwrap().as_i64(), Some(42));
        assert_eq!(navigate(&v, &p("user.tags.1")).unwrap().as_i64(), Some(8));
        assert!(navigate(&v, &p("user.nope")).is_none());
        assert!(navigate(&v, &p("nope.id")).is_none());
    }

    #[test]
    fn compare_val_operators_and_boundaries() {
        let num = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap();
        assert!(compare_val(&num("500"), Op::GtEq, "500"), ">= 含等");
        assert!(compare_val(&num("501"), Op::Gt, "500"));
        assert!(!compare_val(&num("500"), Op::Gt, "500"), "> 不含等");
        assert!(compare_val(&num("-5"), Op::Lt, "0"), "负数");
        assert!(compare_val(&num("1.5"), Op::Lt, "2"), "浮点");
        assert!(compare_val(&num("42"), Op::Eq, "42"), "Eq 数值按显示串");
        assert!(
            !compare_val(&serde_json::Value::String("500".into()), Op::Gt, "0"),
            "字符串不参与数值比较"
        );
    }

    #[test]
    fn filter_dot_path_and_comparison() {
        let lf = open_with(
            br#"{"user":{"id":42},"status":500}
{"user":{"id":7},"status":200}
{"user":{"id":99},"status":503}
{"user":{"id":42},"status":501}
"#,
        );
        assert_eq!(run_filter(&lf, &parse_query("user.id=42")), vec![0, 3]);
        assert_eq!(run_filter(&lf, &parse_query("status>=500")), vec![0, 2, 3]);
        assert_eq!(
            run_filter(&lf, &parse_query("user.id=42 status>=500")),
            vec![0, 3]
        );
        assert_eq!(run_filter(&lf, &parse_query("user.id=4*")), vec![0, 3]);
    }

    #[test]
    fn field_value_matches_scans_all_occurrences() {
        // order.id 在前 (值 7), user.id 在后 (值 42): 粗筛必须扫到后者, 无假阴性
        let line = br#"{"order":{"id":7},"user":{"id":42}}"#;
        let needle = field_needle("id");
        assert!(
            field_value_matches(line, &needle, Op::Eq, "42"),
            "第二处 id 命中"
        );
        assert!(!field_value_matches(line, &needle, Op::Eq, "99"), "无 99");
        // 精确验证仍走 navigate: 第一处 id=7 不代表 user.id=7
        let v = parse_line(line).unwrap();
        assert_eq!(
            navigate(&v, &["user".into(), "id".into()])
                .unwrap()
                .as_i64(),
            Some(42)
        );
    }

    #[test]
    fn run_filter_from_matches_full() {
        let lf = open_with(
            br#"{"level":"INFO","msg":"a"}
{"level":"ERROR","msg":"b"}
{"level":"INFO","msg":"c"}
{"level":"ERROR","msg":"d"}
{"level":"ERROR","msg":"e"}
"#,
        );
        let clauses = parse_query("level=ERROR");
        let full = run_filter(&lf, &clauses);
        assert_eq!(full, vec![1, 3, 4]);
        assert_eq!(run_filter_from(&lf, &clauses, 0), full, "从 0 起跑 == 全量");
        assert_eq!(run_filter_from(&lf, &clauses, 3), vec![3, 4], "中间起跑");
        assert_eq!(
            run_filter_from(&lf, &clauses, 99),
            Vec::<u64>::new(),
            "越界空"
        );
    }

    #[test]
    fn filter_field_and_bare() {
        let lf = open_with(
            br#"{"level":"INFO","msg":"fast"}
{"level":"ERROR","msg":"slow query"}
{"level":"ERROR","msg":"fast"}
{"level":"WARN","msg":"slow disk"}
"#,
        );
        let hits = run_filter(&lf, &parse_query("level=ERROR"));
        assert_eq!(hits, vec![1, 2]);
        let hits = run_filter(&lf, &parse_query("level=ERROR slow"));
        assert_eq!(hits, vec![1], "字段+裸词 AND");
        let hits = run_filter(&lf, &parse_query("level=ERR*"));
        assert_eq!(hits, vec![1, 2], "前缀通配");
        let hits = run_filter(&lf, &[]);
        assert_eq!(hits.len(), 4, "空查询 = 全量");
    }
}
