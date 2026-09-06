//! @author 十四叔
//! @date 2026/09/06
//!
//! 展开行模型 (jsonl-table T3): 行内子行嵌套展开的显示行双向映射。
//!
//! 展开态 = `BTreeMap<文件行号, 子行数>`; 显示行 = 文件行 ∪ 展开子行。
//! 显示行 ↔ (文件行, 子行偏移) 双向映射走前缀和; 行高恒定 → 行锚定数学不破
//! (展开不改变单行高度, 只增加行数)。[`Lines`] 统一「全量」与「过滤命中」两种
//! 显示行来源, 被滤掉的展开行不计入 (过滤 × 展开叠加一致性)。

use std::collections::BTreeMap;

/// 显示行来源: 全量 (恒等) 或过滤命中表 (升序)。
#[derive(Clone, Copy)]
pub enum Lines<'a> {
    All { total: u64 },
    Filtered(&'a [u64]),
}

impl Lines<'_> {
    /// 文件行总数 (未加子行)。
    fn len(&self) -> u64 {
        match self {
            Lines::All { total } => *total,
            Lines::Filtered(s) => s.len() as u64,
        }
    }

    /// 该文件行是否在显示中。
    fn contains(&self, line: u64) -> bool {
        match self {
            Lines::All { total } => line < *total,
            Lines::Filtered(s) => s.binary_search(&line).is_ok(),
        }
    }

    /// 该文件行之前有多少个文件行 (0-based 位置)。
    fn position(&self, line: u64) -> u64 {
        match self {
            Lines::All { .. } => line,
            Lines::Filtered(s) => s.partition_point(|&l| l < line) as u64,
        }
    }

    /// 第 pos 个文件行 (0-based)。
    fn at(&self, pos: u64) -> u64 {
        match self {
            Lines::All { .. } => pos,
            Lines::Filtered(s) => s[pos as usize],
        }
    }
}

/// 展开态: 文件行号 → 该行展开产生的子行数 (0 = 未展开)。
#[derive(Default, Clone)]
pub struct ExpandMap {
    expanded: BTreeMap<u64, usize>,
}

impl ExpandMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// 展开该行 (记子行数)。sub_count == 0 时忽略。
    pub fn expand(&mut self, line: u64, sub_count: usize) {
        if sub_count > 0 {
            self.expanded.insert(line, sub_count);
        }
    }

    /// 折叠该行。
    pub fn collapse(&mut self, line: u64) {
        self.expanded.remove(&line);
    }

    pub fn is_expanded(&self, line: u64) -> bool {
        self.expanded.contains_key(&line)
    }

    /// 该行子行数 (未展开 = 0)。
    pub fn sub_count(&self, line: u64) -> usize {
        self.expanded.get(&line).copied().unwrap_or(0)
    }

    /// file_line 之前、且出现在显示中的展开行子行总数 (显示行偏移)。
    pub fn expanded_before(&self, lines: Lines, line: u64) -> usize {
        self.expanded
            .range(..line)
            .filter(|item| lines.contains(*item.0))
            .map(|item| *item.1)
            .sum()
    }
}

/// 显示行数 = 文件行数 + 其中展开行的子行数。
pub fn display_count(lines: Lines, map: &ExpandMap) -> u64 {
    let sub: usize = match lines {
        Lines::All { .. } => map.expanded.values().sum(),
        Lines::Filtered(s) => s.iter().map(|&l| map.sub_count(l)).sum(),
    };
    lines.len() + sub as u64
}

/// 文件行号 → 显示行 (该文件行的显示行号; 展开子行排在其后)。
pub fn display_row_of(file_line: u64, lines: Lines, map: &ExpandMap) -> u64 {
    lines.position(file_line) + map.expanded_before(lines, file_line) as u64
}

/// 显示行 → (文件行, 子行偏移)。偏移 0 = 文件行本身, >0 = 第 offset 条子行 (1-based)。
/// 越界返回 None。
pub fn file_line_at(display_row: u64, lines: Lines, map: &ExpandMap) -> Option<(u64, usize)> {
    if lines.len() == 0 {
        return None;
    }
    // 二分: 找最大 p 使 p + expanded_before(lines, lines.at(p)) <= display_row
    let mut lo = 0u64;
    let mut hi = lines.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        let line = lines.at(mid);
        if mid + map.expanded_before(lines, line) as u64 <= display_row {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let p = lo.checked_sub(1)?;
    let line = lines.at(p);
    let base = p + map.expanded_before(lines, line) as u64;
    let offset = (display_row - base) as usize;
    if offset > map.sub_count(line) {
        return None;
    }
    Some((line, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_collapse_and_sub_count() {
        let mut m = ExpandMap::new();
        assert!(!m.is_expanded(3));
        m.expand(3, 2);
        assert!(m.is_expanded(3));
        assert_eq!(m.sub_count(3), 2);
        m.collapse(3);
        assert!(!m.is_expanded(3));
        m.expand(5, 0);
        assert!(!m.is_expanded(5), "零子行不记展开");
    }

    #[test]
    fn display_mapping_roundtrip_all() {
        // 5 文件行, 行 1 展开 2 子行, 行 3 展开 1 子行
        let mut m = ExpandMap::new();
        m.expand(1, 2);
        m.expand(3, 1);
        let lines = Lines::All { total: 5 };
        assert_eq!(display_count(lines, &m), 8, "5 行 + 3 子行");
        let expect = [
            (0, 0),
            (1, 0),
            (1, 1),
            (1, 2),
            (2, 0),
            (3, 0),
            (3, 1),
            (4, 0),
        ];
        for (d, &e) in expect.iter().enumerate() {
            assert_eq!(file_line_at(d as u64, lines, &m), Some(e), "显示行 {d}");
        }
        assert_eq!(file_line_at(8, lines, &m), None, "越界");
        assert_eq!(display_row_of(0, lines, &m), 0);
        assert_eq!(display_row_of(1, lines, &m), 1);
        assert_eq!(display_row_of(2, lines, &m), 4);
        assert_eq!(display_row_of(3, lines, &m), 5);
        assert_eq!(display_row_of(4, lines, &m), 7);
    }

    #[test]
    fn filtered_lines_exclude_expanded_off_list() {
        // 过滤命中 = [0, 3]; 行 1 展开但被滤掉, 其子行不计
        let mut m = ExpandMap::new();
        m.expand(1, 2);
        m.expand(3, 1);
        let filtered = [0u64, 3];
        let lines = Lines::Filtered(&filtered);
        assert_eq!(
            display_count(lines, &m),
            3,
            "2 行 + 1 子行 (行 1 子行被滤掉)"
        );
        assert_eq!(file_line_at(0, lines, &m), Some((0, 0)));
        assert_eq!(file_line_at(1, lines, &m), Some((3, 0)));
        assert_eq!(file_line_at(2, lines, &m), Some((3, 1)));
        assert_eq!(file_line_at(3, lines, &m), None);
        assert_eq!(display_row_of(3, lines, &m), 1);
    }
}
