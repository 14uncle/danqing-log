//! @author 十四叔
//! @date 2026/09/19
//!
//! 字段分析 (SPEC-v1x-field-analytics, 腿二): 对字段取值做聚合 ——
//! 数值列分布/分位数, 枚举列取值分布。
//!
//! 引擎是 `danqing-logfile::scan::scan_field` (顶层字段扫描器, D1) ——
//! 本模块只消费它, 不碰 memmem 切取 (D1 红线)。语义决策见 spec D2/D3/D6。

use std::collections::HashMap;

use danqing_logfile::logfile::LogFile;
use danqing_logfile::scan::{FieldKind, FieldValue, scan_field};

/// 分位数 reservoir 上限 (D2): 100 万值 ≈ 8 MB, 超限转采样并标注。
const RESERVOIR_CAP: usize = 1_000_000;
/// 枚举 distinct 上限: 超出并入「其他」桶并标注。
const ENUM_CAP: usize = 10_000;
/// 类型判定采样行数 (D6)。
const TYPE_VOTE_SAMPLE: usize = 100;
/// 枚举展示条数 (spec Open Question: build 实测定; 先按 20)。
pub const ENUM_TOP: usize = 20;

/// 列型判定 (D6: 目标行集前 100 行采样投票)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Numeric,
    Enum,
}

/// 数值聚合结果。count/min/max/mean 流式精确; p* 在「未超 reservoir
/// 上限」时精确, 超限时是采样估计且 `sampled = true`。
#[derive(Debug, Clone, PartialEq)]
pub struct NumericStats {
    pub count: u64,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    /// true = 分位数基于 reservoir 采样 (总数见 `Analysis::scope_rows`)。
    pub sampled: bool,
}

/// 枚举聚合结果: `top` 按计数降序 (同计数按值字典序, 确定性)。
#[derive(Debug, Clone, PartialEq)]
pub struct EnumStats {
    pub top: Vec<(String, u64)>,
    /// 「其他」桶计数 (Top N 之外的行数 + distinct 超限被并入的行数)。
    pub others: u64,
    /// true = distinct 值超上限 (超出的取值没进 map, 其行数已并入 `others`)。
    pub capped: bool,
    pub total: u64,
}

/// 一次分析的结果。
#[derive(Debug, Clone, PartialEq)]
pub enum AnalysisResult {
    Numeric(NumericStats),
    Enum(EnumStats),
}

/// 分析产物全量: 结果 + 作用域 + 跳过计数。
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub field: String,
    /// 作用域行数 (D3: 有过滤 = 过滤行集大小, 无过滤 = 文件行数)。
    pub scope_rows: u64,
    /// 跳过行数 (取不到该字段 / 嵌套值 / 数值列里的非数值行, D6)。
    pub skipped: u64,
    pub result: AnalysisResult,
}

/// 字段分析主入口。`rows` = 过滤行集 (升序文件行号), None = 全文件 (D3)。
pub fn analyze_field(file: &LogFile, field: &str, rows: Option<&[u64]>) -> Analysis {
    let scope_rows = rows.map_or_else(|| file.line_count(), |r| r.len() as u64);
    let ctype = detect_column_type(file, field, rows);
    match ctype {
        ColumnType::Numeric => analyze_numeric(file, field, rows, scope_rows),
        ColumnType::Enum => analyze_enum(file, field, rows, scope_rows),
    }
}

/// 类型判定: 前 [`TYPE_VOTE_SAMPLE`] 行采样。数值占比 ≥ 一半 → 数值列
/// (平票归数值 —— 混合列按多数类型分析, 少数派行跳过); 否则枚举列。
pub fn detect_column_type(file: &LogFile, field: &str, rows: Option<&[u64]>) -> ColumnType {
    let mut num = 0u32;
    let mut seen = 0u32;
    for_each_row(file, rows, Some(TYPE_VOTE_SAMPLE), |line| {
        seen += 1;
        if scan_field(line, field).is_some_and(|v| v.kind == FieldKind::Num) {
            num += 1;
        }
    });
    if seen == 0 {
        return ColumnType::Enum; // 空集: 走枚举 (top 为空, 语义干净)
    }
    if num * 2 >= seen {
        ColumnType::Numeric
    } else {
        ColumnType::Enum
    }
}

/// 按行集遍历每行字节。`rows` 升序 —— 单迭代器前向跳行, 不回退。
/// `limit` = 最多消费的行数 (类型判定采样用), None = 全部。
fn for_each_row(
    file: &LogFile,
    rows: Option<&[u64]>,
    limit: Option<usize>,
    mut f: impl FnMut(&[u8]),
) {
    let mut done = 0usize;
    let mut hit = |bytes: &[u8], done: &mut usize| {
        if limit.is_none_or(|l| *done < l) {
            f(bytes);
            *done += 1;
        }
    };
    match rows {
        None => {
            for (_ln, bytes) in file.lines_from(0) {
                if limit.is_some_and(|l| done >= l) {
                    break;
                }
                hit(bytes, &mut done);
            }
        }
        Some(rows) => {
            let mut it = file.lines_from(0);
            let mut cur = it.next();
            for &r in rows {
                if limit.is_some_and(|l| done >= l) {
                    break;
                }
                loop {
                    match cur {
                        Some((ln, bytes)) if ln == r => {
                            hit(bytes, &mut done);
                            cur = it.next();
                            break;
                        }
                        Some((ln, _)) if ln < r => cur = it.next(),
                        // ln > r (行集越出当前文件代次, 如截断后) 或迭代耗尽:
                        // 该行取不到, 跳过 —— 不因此 panic (live-tail 下的体面)。
                        _ => break,
                    }
                }
            }
        }
    }
}

/// 数值列聚合 (T2)。
fn analyze_numeric(file: &LogFile, field: &str, rows: Option<&[u64]>, scope_rows: u64) -> Analysis {
    let mut count = 0u64;
    let mut sum = 0.0f64;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut skipped = 0u64;
    let mut reservoir: Vec<f64> = Vec::with_capacity(RESERVOIR_CAP.min(1024));
    let mut rng = Rng(0x2545_f491_4f6c_dd1d); // 定种子: 采样结果可复现

    for_each_row(file, rows, None, |line| {
        let v = scan_field(line, field)
            .filter(|v| v.kind == FieldKind::Num)
            .and_then(|v| std::str::from_utf8(v.raw).ok()?.parse::<f64>().ok());
        let Some(x) = v else {
            skipped += 1;
            return;
        };
        count += 1;
        sum += x;
        min = min.min(x);
        max = max.max(x);
        // Vitter R 法 reservoir
        if reservoir.len() < RESERVOIR_CAP {
            reservoir.push(x);
        } else {
            let j = rng.below(count);
            if (j as usize) < RESERVOIR_CAP {
                reservoir[j as usize] = x;
            }
        }
    });

    reservoir.sort_by(f64::total_cmp);
    let pick = |p: f64| {
        // nearest-rank: 语义直白, 与「采样估计」标注配套 (不做插值伪装精确)
        let n = reservoir.len();
        if n == 0 {
            return f64::NAN;
        }
        reservoir[((n - 1) as f64 * p).round() as usize]
    };
    Analysis {
        field: field.to_string(),
        scope_rows,
        skipped,
        result: AnalysisResult::Numeric(NumericStats {
            count,
            min: if count == 0 { f64::NAN } else { min },
            max: if count == 0 { f64::NAN } else { max },
            mean: if count == 0 {
                f64::NAN
            } else {
                sum / count as f64
            },
            p50: pick(0.50),
            p95: pick(0.95),
            p99: pick(0.99),
            sampled: count > RESERVOIR_CAP as u64,
        }),
    }
}

/// 枚举列聚合 (T3)。数值/布尔的文本形态也入桶 (混合列里它们是合法取值);
/// 嵌套值与取不到字段的行跳过。
fn analyze_enum(file: &LogFile, field: &str, rows: Option<&[u64]>, scope_rows: u64) -> Analysis {
    let mut counts: HashMap<String, u64> = HashMap::new();
    let mut total = 0u64;
    let mut skipped = 0u64;
    let mut capped = false;
    // distinct 超限后被丢值**本身**但行数照计 —— 并入「其他」桶,
    // 否则高基数列的分布数字凭空少一大截 (评审: request_id 类 400 万 distinct
    // 时侧栏只显示得出 1 万)。
    let mut overflow = 0u64;

    for_each_row(file, rows, None, |line| {
        let Some(v) = scan_field(line, field) else {
            skipped += 1;
            return;
        };
        let Some(s) = value_text(&v) else {
            skipped += 1; // Nested
            return;
        };
        total += 1;
        // (先查再插, 别在 match 守卫里读 counts —— get_mut 的可变借用那时还活着)
        if let Some(c) = counts.get_mut(&s) {
            *c += 1;
        } else if counts.len() < ENUM_CAP {
            counts.insert(s, 1);
        } else {
            capped = true; // distinct 超限: 丢弃值本身, 行数并入其他
            overflow += 1;
        }
    });

    let mut all: Vec<(String, u64)> = counts.into_iter().collect();
    all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let others_count: u64 = all.iter().skip(ENUM_TOP).map(|(_, c)| c).sum::<u64>() + overflow;
    all.truncate(ENUM_TOP);
    Analysis {
        field: field.to_string(),
        scope_rows,
        skipped,
        result: AnalysisResult::Enum(EnumStats {
            top: all,
            others: others_count,
            // 「distinct 超限」与「Top N 之外还有值」是两个语义, 不压成一位 ——
            // 面板对二者措辞不同 (「取值过多已合并」只在真超限时出现)。
            capped,
            total,
        }),
    }
}

/// 值 → 展示文本。Str 含转义时把 token 丢回 serde 解真值 (低频路径,
/// 引擎头注释); 数字保留原文本 (「1.0」与「1」是两个取值, 不规整掉)。
fn value_text(v: &FieldValue<'_>) -> Option<String> {
    match v.kind {
        FieldKind::Num | FieldKind::Bool | FieldKind::Null => {
            Some(String::from_utf8_lossy(v.raw).into_owned())
        }
        FieldKind::Str if !v.has_escapes => Some(String::from_utf8_lossy(v.raw).into_owned()),
        FieldKind::Str => {
            let mut token = Vec::with_capacity(v.raw.len() + 2);
            token.push(b'"');
            token.extend_from_slice(v.raw);
            token.push(b'"');
            serde_json::from_slice::<String>(&token).ok()
        }
        FieldKind::Nested => None,
    }
}

/// xorshift64 (reservoir 用; 与 logfile/genlog 测试同款, 定种子可复现)。
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use std::path::PathBuf;

    /// 临时 JSONL 文件 (并行 flake 教训: pid + 用例名)。
    fn temp_jsonl(tag: &str, lines: &[&str]) -> (PathBuf, LogFile) {
        let path = std::env::temp_dir().join(format!(
            "danqing-log-analysis-{}-{tag}.jsonl",
            std::process::id()
        ));
        let mut f = std::fs::File::create(&path).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        drop(f);
        let lf = LogFile::open(&path).unwrap();
        (path, lf)
    }

    #[test]
    fn numeric_small_file_hand_computed() {
        let (p, lf) = temp_jsonl("num", &[r#"{"d":10}"#, r#"{"d":20}"#, r#"{"d":30}"#]);
        let a = analyze_field(&lf, "d", None);
        let AnalysisResult::Numeric(s) = a.result else {
            panic!("应判为数值列")
        };
        assert_eq!(s.count, 3);
        assert_eq!(s.min, 10.0);
        assert_eq!(s.max, 30.0);
        assert_eq!(s.mean, 20.0);
        // nearest-rank (n-1)*p: p50 → idx 1 = 20
        assert_eq!(s.p50, 20.0);
        assert_eq!(s.p95, 30.0);
        assert_eq!(s.p99, 30.0);
        assert!(!s.sampled);
        assert_eq!(a.scope_rows, 3);
        assert_eq!(a.skipped, 0);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn numeric_mixed_column_skips_non_numeric() {
        let (p, lf) = temp_jsonl(
            "mixed",
            &[
                r#"{"d":1}"#,
                r#"{"d":2}"#,
                r#"{"d":3}"#,
                r#"{"d":"oops"}"#,  // 字符串: 跳过
                r#"{"d":{"x":1}}"#, // 嵌套: 跳过
            ],
        );
        let a = analyze_field(&lf, "d", None);
        let AnalysisResult::Numeric(s) = a.result else {
            panic!("数值过半 (3/5) 应判数值列")
        };
        assert_eq!(s.count, 3);
        assert_eq!(a.skipped, 2);
        assert_eq!(s.mean, 2.0);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn enum_majority_string_column() {
        let (p, lf) = temp_jsonl(
            "enum",
            &[
                r#"{"level":"INFO"}"#,
                r#"{"level":"ERROR"}"#,
                r#"{"level":"INFO"}"#,
                r#"{"level":null}"#,
                r#"{"level":true}"#,
                r#"{"level":"INFO"}"#,
            ],
        );
        let a = analyze_field(&lf, "level", None);
        let AnalysisResult::Enum(s) = a.result else {
            panic!("字符串多数应判枚举列")
        };
        assert_eq!(s.top[0], ("INFO".to_string(), 3));
        assert_eq!(s.total, 6);
        assert_eq!(a.skipped, 0);
        // bool/null 以文本形态入桶
        assert!(s.top.iter().any(|(v, _)| v == "true"));
        assert!(s.top.iter().any(|(v, _)| v == "null"));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn enum_top_ordering_is_count_desc_then_value_asc() {
        let (p, lf) = temp_jsonl(
            "order",
            &[
                r#"{"s":"b"}"#,
                r#"{"s":"a"}"#,
                r#"{"s":"c"}"#,
                r#"{"s":"b"}"#,
                r#"{"s":"a"}"#,
                r#"{"s":"b"}"#,
            ],
        );
        let AnalysisResult::Enum(s) = analyze_field(&lf, "s", None).result else {
            panic!()
        };
        assert_eq!(s.top[0], ("b".to_string(), 3));
        assert_eq!(s.top[1], ("a".to_string(), 2));
        assert_eq!(s.top[2], ("c".to_string(), 1));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn enum_scoped_rows_follow_filter_set() {
        // D3: 作用域 = 行集; 且行集走法与全扫在「全集」上等价
        let (p, lf) = temp_jsonl(
            "scope",
            &[
                r#"{"s":"x","keep":1}"#,
                r#"{"s":"y"}"#,
                r#"{"s":"x","keep":1}"#,
            ],
        );
        let all = analyze_field(&lf, "s", None);
        let via_rows = analyze_field(&lf, "s", Some(&[0, 1, 2]));
        assert_eq!(all, via_rows, "行集走法 ≡ 全文件走法");
        let scoped = analyze_field(&lf, "s", Some(&[0, 2]));
        let AnalysisResult::Enum(s) = scoped.result else {
            panic!()
        };
        assert_eq!(s.top, vec![("x".to_string(), 2)]);
        assert_eq!(scoped.scope_rows, 2, "作用域 = 过滤行集大小");
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn reservoir_below_cap_is_exact() {
        // 不超限: 分位数必须等于排序精确值 (采样标注 = false)
        let lines: Vec<String> = (1..=999).map(|i| format!(r#"{{"d":{i}}}"#)).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let (p, lf) = temp_jsonl("exact", &refs);
        let AnalysisResult::Numeric(s) = analyze_field(&lf, "d", None).result else {
            panic!()
        };
        assert!(!s.sampled);
        assert_eq!(s.p50, 500.0);
        assert_eq!(s.count, 999);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn analysis_is_deterministic_across_runs() {
        // 定种子 reservoir: 同一文件两次分析逐位相等
        let (p, lf) = temp_jsonl(
            "det",
            &[r#"{"d":1}"#, r#"{"d":2}"#, r#"{"d":3}"#, r#"{"d":"x"}"#],
        );
        assert_eq!(analyze_field(&lf, "d", None), analyze_field(&lf, "d", None));
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn enum_beyond_top20_merges_into_others_without_capped() {
        // 21 个 distinct (未超 ENUM_CAP): 「其他」= Top20 之外那 1 个取值的行数,
        // capped = false —— 「Top N 之外还有值」不等于「取值过多已合并」。
        let lines: Vec<String> = (0..21).map(|i| format!(r#"{{"s":"v{i}"}}"#)).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let (p, lf) = temp_jsonl("top20", &refs);
        let AnalysisResult::Enum(s) = analyze_field(&lf, "s", None).result else {
            panic!()
        };
        assert_eq!(s.top.len(), 20);
        assert_eq!(s.others, 1, "第 21 个取值的那 1 行进其他桶");
        assert!(!s.capped, "distinct 未超上限不得标「取值过多已合并」");
        assert_eq!(s.total, 21);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn enum_distinct_overflow_merges_rows_into_others() {
        // distinct 超 ENUM_CAP: 超出的取值不进 map, 但其**行数**必须并入其他桶,
        // 且 capped = true (评审抓的分叉: 原先超限行被静默丢弃)。
        let cap = 10_000usize;
        let extra = 37usize;
        let lines: Vec<String> = (0..cap + extra)
            .map(|i| format!(r#"{{"s":"v{i}"}}"#))
            .collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let (p, lf) = temp_jsonl("overflow", &refs);
        let AnalysisResult::Enum(s) = analyze_field(&lf, "s", None).result else {
            panic!()
        };
        assert!(s.capped, "distinct 超限必须标注");
        let top_sum: u64 = s.top.iter().map(|(_, c)| c).sum();
        assert_eq!(
            top_sum + s.others,
            s.total,
            "top + 其他 必须等于总数 —— 一行都不许凭空消失"
        );
        assert_eq!(s.others, (cap - 20 + extra) as u64);
        std::fs::remove_file(p).ok();
    }
}
