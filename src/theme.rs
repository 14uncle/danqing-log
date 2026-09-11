//! @author 十四叔
//! @date 2026/09/11
//!
//! 主题系统: 浅色/深色双主题, 所有颜色函数统一入口。

use danqing::Color;

/// 主题模式 (复用 config::AppTheme)。
pub(crate) type ThemeMode = crate::config::AppTheme;

// ============ 通用颜色 ============

/// 背景色。
pub(crate) fn bg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgb(0.98, 0.98, 0.98),
        ThemeMode::Dark => Color::rgb(0.12, 0.12, 0.14),
    }
}

/// 默认文本色。
pub(crate) fn text_default(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgb(0.12, 0.12, 0.12),
        ThemeMode::Dark => Color::rgb(0.88, 0.88, 0.90),
    }
}

/// 行号色。
pub(crate) fn gutter_fg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgb(0.45, 0.45, 0.45),
        ThemeMode::Dark => Color::rgb(0.50, 0.50, 0.55),
    }
}

/// 选区背景色。
pub(crate) fn selection_bg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgba(0.24, 0.42, 0.66, 0.24),
        ThemeMode::Dark => Color::rgba(0.35, 0.55, 0.80, 0.30),
    }
}

/// 状态栏文本色。
pub(crate) fn status_fg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgb(0.25, 0.25, 0.25),
        ThemeMode::Dark => Color::rgb(0.70, 0.70, 0.75),
    }
}

/// 滚动条轨道色。
pub(crate) fn scrollbar_track(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgba(0.0, 0.0, 0.0, 0.05),
        ThemeMode::Dark => Color::rgba(1.0, 1.0, 1.0, 0.05),
    }
}

/// 滚动条拇指色。
pub(crate) fn scrollbar_thumb(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgba(0.0, 0.0, 0.0, 0.18),
        ThemeMode::Dark => Color::rgba(1.0, 1.0, 1.0, 0.18),
    }
}

/// 过滤栏背景色。
pub(crate) fn filter_bar_bg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgba(0.0, 0.0, 0.0, 0.04),
        ThemeMode::Dark => Color::rgba(1.0, 1.0, 1.0, 0.06),
    }
}

/// 过滤栏文本色。
pub(crate) fn filter_fg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgb(0.20, 0.20, 0.20),
        ThemeMode::Dark => Color::rgb(0.85, 0.85, 0.88),
    }
}

/// 表头文本色。
pub(crate) fn header_fg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgb(0.35, 0.35, 0.38),
        ThemeMode::Dark => Color::rgb(0.65, 0.65, 0.70),
    }
}

/// 搜索命中背景色 (琥珀)。
pub(crate) fn hit_bg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgba(0.95, 0.75, 0.10, 0.35),
        ThemeMode::Dark => Color::rgba(0.95, 0.75, 0.10, 0.25),
    }
}

/// 表格模式命中行背景色。
pub(crate) fn hit_row_bg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgba(0.95, 0.75, 0.10, 0.12),
        ThemeMode::Dark => Color::rgba(0.95, 0.75, 0.10, 0.10),
    }
}

/// 斑马纹背景色。
pub(crate) fn zebra_bg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgba(0.0, 0.0, 0.0, 0.025),
        ThemeMode::Dark => Color::rgba(1.0, 1.0, 1.0, 0.03),
    }
}

/// 鼠标悬停行背景色。
pub(crate) fn hover_bg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgba(0.0, 0.0, 0.0, 0.045),
        ThemeMode::Dark => Color::rgba(1.0, 1.0, 1.0, 0.06),
    }
}

/// 文本选区背景色。
pub(crate) fn text_sel_bg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgba(0.24, 0.42, 0.66, 0.32),
        ThemeMode::Dark => Color::rgba(0.35, 0.55, 0.80, 0.35),
    }
}

/// 强调色 (选中行竖条)。
pub(crate) fn accent(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgb(0.24, 0.42, 0.66),
        ThemeMode::Dark => Color::rgb(0.40, 0.60, 0.85),
    }
}

/// 表头背景色。
pub(crate) fn header_bg(theme: ThemeMode) -> Color {
    match theme {
        ThemeMode::Light => Color::rgba(0.0, 0.0, 0.0, 0.04),
        ThemeMode::Dark => Color::rgba(1.0, 1.0, 1.0, 0.06),
    }
}

