//! @author 十四叔
//! @date 2026/09/08
//!
//! 文本选区纯逻辑: token 边界 / 选区规范化 / 复制文本拼装。
//!
//! 偏移一律为「解码后行文本」的字节偏移 —— 渲染 (measure 前缀)、命中测试、
//! 复制走同一路径, GBK/Latin-1 等编码不存在文件字节 ↔ 显示字符的映射歧义。
//! 不做: 选区渲染 (view.rs)、鼠标事件 (view.rs)、剪贴板写入 (danqing 焦点路径)。

/// 复制行数上限 (R3, 2026-09-08 评审 + 用户拍板 10 万行): 滚轮甩底可造出
/// 全文件选区, 无上限复制 = 逐行解码 + 逐行分配, UI 冻结分钟级。
/// 10 万行 ≈ 16MB 文本 (按 160B/行), 剪贴板与内存都安全。
pub const COPY_MAX_LINES: u64 = 100_000;

/// 文本选区: 锚点 (按下处) 与光标点 (拖动当前处), 均为 (显示行, 解码行内字节偏移)。
///
/// 事件热路径只写不排序; 规范化 ([`TextSelection::ordered`]) 只在渲染/复制读取时做。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    /// 锚点 (按下处)。
    pub anchor: (u64, usize),
    /// 光标点 (拖动当前处 / 双击词尾)。
    pub caret: (u64, usize),
}

impl TextSelection {
    pub fn new(anchor: (u64, usize), caret: (u64, usize)) -> Self {
        Self { anchor, caret }
    }

    /// 空选区 (锚点 == 光标点): 不渲染, Ctrl+C 忽略。
    pub fn is_empty(&self) -> bool {
        self.anchor == self.caret
    }

    /// 规范化为 (起, 止), 起 <= 止 (行号先比, 行内偏移后比)。
    /// 反向拖动 (caret 在 anchor 前) 在此归一, 复制内容两向一致。
    pub fn ordered(&self) -> ((u64, usize), (u64, usize)) {
        if self.anchor <= self.caret {
            (self.anchor, self.caret)
        } else {
            (self.caret, self.anchor)
        }
    }
}

/// 空白分隔 token: 返回包含 `off` 的同类字符连续段 [start, end)。
///
/// 词界只有两类字符: 空白 / 非空白 (Rust `char::is_whitespace`, Unicode 感知)。
/// 日志场景下双击能一把选中整段时间戳 / IP / `level=ERROR`。
/// `off` 落在空白上时选中该连续空白段 (与编辑器惯例一致);
/// `off` 在行尾 (== len) 时归属最后一个字符。偏移必落在 UTF-8 字符边界
/// (char_indices 扫描产生, 永不劈字符)。
pub fn token_at(line: &str, off: usize) -> (usize, usize) {
    // 归属字符: 首个起点 >= off 的字符; 行尾/超界 (off >= len) 归最后一个字符。
    let target = off.min(line.len());
    let mut class_at = None; // (字符起点, 是否空白)
    for (i, ch) in line.char_indices() {
        class_at = Some((i, ch.is_whitespace()));
        if i >= target {
            break;
        }
    }
    let Some((pos, ws)) = class_at else {
        return (0, 0); // 空行
    };
    // 由归属字符向两端扩同类连续段 (char_indices 扫描, 偏移必在字符边界)。
    let mut start = pos;
    for (i, ch) in line[..pos].char_indices().rev() {
        if ch.is_whitespace() != ws {
            break;
        }
        start = i;
    }
    let mut end = pos;
    for (i, ch) in line[pos..].char_indices() {
        if ch.is_whitespace() != ws {
            break;
        }
        end = pos + i + ch.len_utf8();
    }
    (start, end)
}

/// 复制文本拼装: 规范化后首行取 `[start.1..]`, 末行取 `[..end.1]`, 中间整行,
/// `\n` 拼接。`line` 回调 = 显示行 → 解码行文本 (filtered/展开映射由调用方负责)。
/// 偏移超行长防御性回钳 (文件外部变更后旧选区不炸)。空选区返回空串。
pub fn copy_text(sel: &TextSelection, line: &dyn Fn(u64) -> String) -> String {
    if sel.is_empty() {
        return String::new();
    }
    let ((r0, _), (r1, _)) = sel.ordered();
    let mut out = String::new();
    for row in r0..=r1 {
        let text = line(row);
        if let Some((from, to)) = row_slice(sel, row, &text) {
            out.push_str(&text[from..to]);
        }
        if row < r1 {
            out.push('\n');
        }
    }
    out
}

/// 选区在某行的切片 `[from, to)`: 首行取锚点侧, 末行取光标侧, 中间整行;
/// 偏移超行长防御回钳到字符边界 (渲染与复制共用的唯一权威实现)。
/// row 不在选区行域内, 或切片为零宽 → None。
pub fn row_slice(sel: &TextSelection, row: u64, line: &str) -> Option<(usize, usize)> {
    let ((r0, c0), (r1, c1)) = sel.ordered();
    if !(r0..=r1).contains(&row) {
        return None;
    }
    let from = if row == r0 {
        floor_char_boundary(line, c0.min(line.len()))
    } else {
        0
    };
    let to = if row == r1 {
        floor_char_boundary(line, c1.min(line.len()))
    } else {
        line.len()
    };
    (from < to).then_some((from, to))
}

/// 回钳到 <= off 的最近字符边界 (Rust 1.9+ `str::floor_char_boundary`
/// 尚未稳定的等效实现, 稳定后替换)。渲染侧 (view.rs) 与复制拼装共用。
pub fn floor_char_boundary(s: &str, off: usize) -> usize {
    let mut i = off.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- token_at ----

    #[test]
    fn token_covers_timestamp() {
        // 时间戳 "2026-09-08T12:34:56.789Z" = 24 字节, 空白分隔保证 -/T/: 不切开
        let line = "2026-09-08T12:34:56.789Z INFO boot";
        assert_eq!(token_at(line, 0), (0, 24));
        assert_eq!(token_at(line, 10), (0, 24));
        assert_eq!(token_at(line, 23), (0, 24));
    }

    #[test]
    fn token_middle_word() {
        let line = "2026-09-08 INFO boot";
        assert_eq!(token_at(line, 11), (11, 15)); // INFO
        assert_eq!(token_at(line, 16), (16, 20)); // boot
    }

    #[test]
    fn token_on_whitespace_selects_space_run() {
        let line = "ab   cd";
        assert_eq!(token_at(line, 3), (2, 5));
    }

    #[test]
    fn token_multibyte_never_splits_char() {
        // 中=0..3 文=3..6 空=6 日=7..10 志=10..13
        let line = "中文 日志";
        assert_eq!(token_at(line, 1), (0, 6));
        assert_eq!(token_at(line, 8), (7, 13));
        assert_eq!(token_at(line, 13), (7, 13)); // 行尾归属最后字符
    }

    #[test]
    fn token_empty_and_all_space() {
        assert_eq!(token_at("", 0), (0, 0));
        assert_eq!(token_at("   ", 1), (0, 3));
    }

    // ---- 规范化 ----

    #[test]
    fn empty_selection_detected() {
        assert!(TextSelection::new((3, 5), (3, 5)).is_empty());
        assert!(!TextSelection::new((3, 5), (3, 7)).is_empty());
        assert!(!TextSelection::new((3, 5), (4, 0)).is_empty());
    }

    #[test]
    fn ordered_normalizes_reversed_drag() {
        let sel = TextSelection::new((5, 10), (2, 3));
        assert_eq!(sel.ordered(), ((2, 3), (5, 10)));
        // 同行反向
        let sel = TextSelection::new((2, 10), (2, 3));
        assert_eq!(sel.ordered(), ((2, 3), (2, 10)));
        // 正向不变
        let sel = TextSelection::new((1, 2), (3, 4));
        assert_eq!(sel.ordered(), ((1, 2), (3, 4)));
    }

    // ---- copy_text ----

    fn lines3(row: u64) -> String {
        match row {
            0 => "aaa bbb".into(),
            1 => "ccc".into(),
            2 => "ddd eee".into(),
            _ => panic!("越界行 {row}"),
        }
    }

    #[test]
    fn copy_single_line_slice() {
        let sel = TextSelection::new((0, 4), (0, 7));
        assert_eq!(copy_text(&sel, &lines3), "bbb");
    }

    #[test]
    fn copy_cross_two_lines() {
        let sel = TextSelection::new((0, 4), (1, 2));
        assert_eq!(copy_text(&sel, &lines3), "bbb\ncc");
    }

    #[test]
    fn copy_cross_three_lines_takes_full_middle() {
        let sel = TextSelection::new((0, 2), (2, 3));
        assert_eq!(copy_text(&sel, &lines3), "a bbb\nccc\nddd");
    }

    #[test]
    fn copy_reversed_drag_equals_forward() {
        let fwd = TextSelection::new((0, 4), (2, 3));
        let rev = TextSelection::new((2, 3), (0, 4));
        assert_eq!(copy_text(&fwd, &lines3), copy_text(&rev, &lines3));
    }

    #[test]
    fn copy_empty_selection_is_empty_string() {
        let sel = TextSelection::new((1, 1), (1, 1));
        assert_eq!(copy_text(&sel, &lines3), "");
    }

    #[test]
    fn copy_clamps_offset_beyond_line_len() {
        // 文件外部变短后的陈旧选区: 回钳不炸
        let sel = TextSelection::new((1, 0), (1, 100));
        assert_eq!(copy_text(&sel, &lines3), "ccc");
    }

    #[test]
    fn copy_clamps_mid_multibyte_char() {
        // 偏移落在多字节字符中间 → 回钳到字符边界, 不劈字符 (floor 的存在理由)
        let line = |_: u64| "中文".to_string(); // 中=0..3 文=3..6
        let sel = TextSelection::new((0, 1), (0, 4));
        assert_eq!(copy_text(&sel, &line), "中");
    }

    #[test]
    fn floor_boundary_never_splits_multibyte() {
        assert_eq!(floor_char_boundary("中文", 0), 0);
        assert_eq!(floor_char_boundary("中文", 1), 0);
        assert_eq!(floor_char_boundary("中文", 2), 0);
        assert_eq!(floor_char_boundary("中文", 3), 3);
        assert_eq!(floor_char_boundary("中文", 4), 3);
        assert_eq!(floor_char_boundary("中文", 6), 6);
        assert_eq!(floor_char_boundary("中文", 99), 6);
        assert_eq!(floor_char_boundary("", 5), 0);
    }

    // ---- row_slice (渲染/复制共用) ----

    #[test]
    fn row_slice_boundaries() {
        let sel = TextSelection::new((0, 2), (2, 3));
        // 行域外 → None
        assert_eq!(row_slice(&sel, 5, "abcde"), None);
        // 首行取后缀 / 中间整行 / 末行取前缀
        assert_eq!(row_slice(&sel, 0, "abcde"), Some((2, 5)));
        assert_eq!(row_slice(&sel, 1, "abc"), Some((0, 3)));
        assert_eq!(row_slice(&sel, 2, "abcde"), Some((0, 3)));
        // 零宽 → None (行内空切不渲染)
        let z = TextSelection::new((1, 2), (1, 2));
        assert_eq!(row_slice(&z, 1, "abcde"), None);
        // 超行长回钳 + 中劈回钳
        let c = TextSelection::new((0, 1), (0, 99));
        assert_eq!(row_slice(&c, 0, "中文"), Some((0, 6)));
    }
}
