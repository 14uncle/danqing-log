//! @author 十四叔
//! @date 2026/09/05
//!
//! JSONL 列化引擎: 检测 / 列发现 / 字段提取 / 字段过滤。
//!
//! 开枪前提②的主炮: 列化视图 + `level=ERROR status=50*` 字段过滤,
//! 对标 VS Code 扩展 daucloud.json-viewer (独立窗口、秒开、不占编辑器)。
//!
//! 提取策略: 不做全量 JSON parse, 用 memmem 定位 `"key":` 后切值 token。
//! 已知边界 (demo 范围, 正式版换真 parser):
//! - 只认扁平顶层字段; 嵌套对象同名字段会撞名 (取最左命中);
//! - 字符串值内含 `,"key":"` 形态会误判 (前缀校验只能挡一半)。

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
pub fn extract_field<'a>(line: &'a [u8], key: &str) -> Option<&'a [u8]> {
    extract_with_needle(line, &field_needle(key))
}

/// 值 token 切取 (needle 预编译版, 供过滤循环复用)。
fn extract_with_needle<'a>(line: &'a [u8], needle: &[u8]) -> Option<&'a [u8]> {
    let mut from = 0usize;
    while let Some(pos) = memchr::memmem::find(&line[from..], needle).map(|p| p + from) {
        // 前缀校验
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
        if !prefix_ok {
            from = pos + needle.len();
            continue;
        }
        // 值 token
        let mut s = pos + needle.len();
        while s < line.len() && matches!(line[s], b' ' | b'\t') {
            s += 1;
        }
        if s >= line.len() {
            return None;
        }
        if line[s] == b'"' {
            // 字符串: 到未转义的收尾引号
            let mut e = s + 1;
            while e < line.len() {
                match line[e] {
                    b'\\' => e += 1,
                    b'"' => return Some(&line[s + 1..e]),
                    _ => {}
                }
                e += 1;
            }
            return None;
        }
        let mut e = s;
        while e < line.len() && !matches!(line[e], b',' | b'}' | b' ' | b'\t') {
            e += 1;
        }
        return Some(&line[s..e]);
    }
    None
}

/// 过滤子句。
#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    /// 字段匹配: `key=value` / `key=前缀*`。
    Field { key: String, pattern: String },
    /// 裸词: 整行子串。
    Bare(String),
}

/// 解析查询: 空白分词, 含 `=` 为字段子句, 否则裸词。AND 语义。
pub fn parse_query(q: &str) -> Vec<Clause> {
    q.split_whitespace()
        .map(|tok| match tok.split_once('=') {
            Some((key, pattern)) if !key.is_empty() => Clause::Field {
                key: key.to_string(),
                pattern: pattern.to_string(),
            },
            _ => Clause::Bare(tok.to_string()),
        })
        .collect()
}

/// 值匹配: 尾部 `*` = 前缀通配, 否则全等。
fn value_matches(value: &[u8], pattern: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => value.len() >= prefix.len() && &value[..prefix.len()] == prefix.as_bytes(),
        None => value == pattern.as_bytes(),
    }
}

/// 预编译子句 (needle 只造一次, 过滤循环零分配)。
enum Compiled {
    Field { needle: Vec<u8>, pattern: String },
    Bare(Vec<u8>),
}

/// 单行判定: 全部子句命中 (AND)。
fn line_matches(line: &[u8], clauses: &[Compiled]) -> bool {
    clauses.iter().all(|c| match c {
        Compiled::Bare(w) => memchr::memmem::find(line, w).is_some(),
        Compiled::Field { needle, pattern } => {
            extract_with_needle(line, needle).is_some_and(|v| value_matches(v, pattern))
        }
    })
}

/// 全文字段过滤: 顺序走一遍 (lines() 迭代器, 每行 O(1)), 返回命中行号 (升序)。
/// 单线程: 实测 1GB/483 万行量级 < 1s 则不并行 (简洁优先)。
/// 禁用 line(i) 逐行随机访问 —— 步进索引下每次定位带段内前扫, 全量遍历会
/// 把成本乘进行数 (T1 实测回归 235ms → 1072ms 的教训, 见 logfile.rs::lines 注释)。
pub fn run_filter(file: &LogFile, clauses: &[Clause]) -> Vec<u64> {
    let compiled: Vec<Compiled> = clauses
        .iter()
        .map(|c| match c {
            Clause::Field { key, pattern } => Compiled::Field {
                needle: field_needle(key),
                pattern: pattern.clone(),
            },
            Clause::Bare(w) => Compiled::Bare(w.as_bytes().to_vec()),
        })
        .collect();
    if compiled.is_empty() {
        return (0..file.line_count()).collect();
    }
    let mut hits = Vec::new();
    for (i, line) in file.lines() {
        if line_matches(line, &compiled) {
            hits.push(i);
        }
    }
    hits
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
    fn parse_query_splits_clauses() {
        let clauses = parse_query("level=ERROR status=50* slow");
        assert_eq!(
            clauses,
            vec![
                Clause::Field {
                    key: "level".into(),
                    pattern: "ERROR".into()
                },
                Clause::Field {
                    key: "status".into(),
                    pattern: "50*".into()
                },
                Clause::Bare("slow".into()),
            ]
        );
    }

    #[test]
    fn value_wildcard_prefix() {
        assert!(value_matches(b"500", "50*"));
        assert!(value_matches(b"502", "50*"));
        assert!(!value_matches(b"200", "50*"));
        assert!(value_matches(b"ERROR", "ERROR"));
        assert!(!value_matches(b"ERRORS", "ERROR"));
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
