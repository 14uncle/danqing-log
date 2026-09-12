//! @author 十四叔
//! @date 2026/09/12
//!
//! 日志级别分类与计数 (level-histogram 模块, spec: docs/specs/SPEC-level-histogram.md):
//! 把「级别分布」从渲染期逐可见行提前到打开期一趟算清。
//!
//! 本模块只做纯逻辑 (分类 + 计数), 不含任何 UI; 侧栏呈现见 `src/histogram.rs`。
//!
//! 为什么与 `view.rs` 的 `level_color` 分桶不同: 后者是**整行着色**, INFO 走默认色
//! 是有意的降噪策略 (用户 2026-09-06 验收); 而计数必须有 INFO 桶当基线 —— 没有基线
//! 就看不出错误有多稀少。两者有意不共用, 详见 spec 的 Never 项。

/// 级别桶 (严重度降序, 顺序即侧栏显示序)。
///
/// `DebugTrace` 合并 DEBUG 与 TRACE: 两者在既有配色里同为灰, 拆开无信息增量。
/// `FATAL` 与 `ERROR` 拆开: FATAL 才是「一眼」要抓的那一行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Fatal,
    Error,
    Warn,
    Info,
    DebugTrace,
    /// 未识别出级别的行 (无级别词, 或词在判据区间之外)。
    Other,
}

impl Level {
    /// 全部桶, 严重度降序 —— 侧栏按此序渲染。
    pub const ALL: [Level; 6] = [
        Level::Fatal,
        Level::Error,
        Level::Warn,
        Level::Info,
        Level::DebugTrace,
        Level::Other,
    ];

    /// 侧栏显示的短名。
    pub fn label(self) -> &'static str {
        match self {
            Level::Fatal => "FATAL",
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::DebugTrace => "DEBUG",
            Level::Other => "其他",
        }
    }
}

/// 判据区间: 行首字节数。日志级别几乎都在行首; 200 沿用 `view.rs::level_color`
/// 的既定区间。区间之外出现的级别词不认 —— 避免把正文里的 "error" 当成级别。
const LEVEL_HEAD_BYTES: usize = 200;

/// 行级别分类: 读行首 [`LEVEL_HEAD_BYTES`] 字节, 按优先级首个命中即定。
///
/// 优先级 `FATAL` > `ERROR` > `WARN` > `INFO` > `DEBUG`/`TRACE` > `其他`,
/// 故一行含多个级别词时**只归一个桶**。
///
/// **大小写敏感** (与 `view.rs::level_color` / `level_cell_color` 一致): 否则
/// `"no errors found"` 这类正文会污染计数, 而柱条数字必须可信。代价是
/// `"error: ..."` 这类小写级别不识别, 归入 `其他`。
pub fn classify_level(line: &[u8]) -> Level {
    let head = &line[..line.len().min(LEVEL_HEAD_BYTES)];
    // memmem (SIMD) 逐个试; 关键词短且常命中, 长词优先的顺序按优先级排好,
    // 命中即返回 —— 级别词几乎总在行首, 实际每行试 1–2 次就定。
    let has = |pat: &[u8]| memchr::memmem::find(head, pat).is_some();
    if has(b"FATAL") {
        Level::Fatal
    } else if has(b"ERROR") {
        Level::Error
    } else if has(b"WARN") {
        Level::Warn
    } else if has(b"INFO") {
        Level::Info
    } else if has(b"DEBUG") || has(b"TRACE") {
        Level::DebugTrace
    } else {
        Level::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 桶的全集恰好 6 个且互不相同 —— 计数不漏不重的结构前提。
    #[test]
    fn level_all_is_complete_and_unique() {
        assert_eq!(Level::ALL.len(), 6, "恰好 6 桶");
        let mut seen = std::collections::HashSet::new();
        for l in Level::ALL {
            assert!(seen.insert(l), "{l:?} 在 ALL 里重复");
        }
    }

    /// 分类是全函数: 任何输入都落到恰好一个桶 (返回单值即结构上保证唯一)。
    #[test]
    fn classification_is_total() {
        let cases: &[&[u8]] = &[
            b"",
            b"   ",
            b"2026-09-05 12:00:01 INFO ok",
            b"\xff\xfe\x00 bad utf8",
            &[0u8; 300],
        ];
        for c in cases {
            let _ = classify_level(c); // 不 panic 即通过
        }
    }

    /// FATAL 优先于 ERROR: 两者同现时归 FATAL (一眼要抓的是它)。
    #[test]
    fn fatal_beats_error() {
        assert_eq!(classify_level(b"FATAL during ERROR handling"), Level::Fatal);
        assert_eq!(classify_level(b"ERROR caused FATAL"), Level::Fatal);
    }

    /// 优先级全序: WARN 先于 INFO, ERROR 先于 WARN。
    #[test]
    fn precedence_is_ordered() {
        assert_eq!(classify_level(b"INFO WARN both here"), Level::Warn);
        assert_eq!(classify_level(b"WARN ERROR both here"), Level::Error);
        assert_eq!(classify_level(b"INFO DEBUG both here"), Level::Info);
    }

    /// INFO 单独成桶 —— 这是与 `level_color` 分桶的关键分歧 (那里 INFO 降噪到默认色)。
    #[test]
    fn info_has_its_own_bucket() {
        assert_eq!(classify_level(b"2026-09-05 12:00:01 INFO ok"), Level::Info);
    }

    /// DEBUG 与 TRACE 合并同桶。
    #[test]
    fn debug_and_trace_share_a_bucket() {
        assert_eq!(classify_level(b"DEBUG cache miss"), Level::DebugTrace);
        assert_eq!(classify_level(b"TRACE entering fn"), Level::DebugTrace);
    }

    /// 无级别词 → 其他。
    #[test]
    fn unrecognised_falls_back_to_other() {
        assert_eq!(
            classify_level(b"2026-09-05 12:00:01 hello world"),
            Level::Other
        );
        assert_eq!(classify_level(b""), Level::Other);
    }

    /// 小写级别词不识别 (大小写敏感的代价, 有意为之): 正文里的 "errors" 不得
    /// 污染 ERROR 桶, 而柱条数字必须可信。
    #[test]
    fn lowercase_prose_does_not_pollute_counts() {
        assert_eq!(
            classify_level(b"finished with no errors found"),
            Level::Other
        );
        assert_eq!(classify_level(b"informational message"), Level::Other);
        assert_eq!(classify_level(b"error: disk full"), Level::Other);
    }

    /// 大小写敏感的直接断言 (与上一条同源, 分开钉住两个方向)。
    #[test]
    fn matching_is_case_sensitive() {
        assert_eq!(classify_level(b"ERROR disk full"), Level::Error);
        assert_eq!(classify_level(b"error disk full"), Level::Other);
        assert_eq!(
            classify_level(b"WARM up complete"),
            Level::Other,
            "WARM 不含 WARN"
        );
    }

    /// 只扫行首 200 字节: 级别词在区间之外不认。
    #[test]
    fn only_head_window_is_scanned() {
        let mut far = vec![b'x'; LEVEL_HEAD_BYTES + 10];
        far.extend_from_slice(b"ERROR disk full");
        assert_eq!(
            classify_level(&far),
            Level::Other,
            "级别词在第 {LEVEL_HEAD_BYTES} 字节之后, 不认"
        );

        // 边界内 (恰在区间内起) 应认出
        let mut near = vec![b'x'; LEVEL_HEAD_BYTES - 6];
        near.extend_from_slice(b"ERROR!");
        assert_eq!(classify_level(&near), Level::Error, "区间内应认出");
    }

    /// label 与桶一一对应, 无重复 (侧栏显示不串行)。
    #[test]
    fn labels_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for l in Level::ALL {
            assert!(seen.insert(l.label()), "{:?} 的 label 重复", l);
        }
    }
}
