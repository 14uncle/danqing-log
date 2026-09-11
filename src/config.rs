//! @author 十四叔
//! @date 2026/09/11
//!
//! 配置文件读写: 主题持久化。
//!
//! 配置路径: `dirs::config_dir()` / `danqing-log/config.toml`
//! 格式: `[theme] mode = "light"` 或 `"dark"`

use std::fs;
use std::path::PathBuf;

use danqing::theme::{DarkTheme, LightTheme, Theme};
use danqing::{Color, Shadow, Easing};

/// 主题模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppTheme {
    Light,
    Dark,
}

impl AppTheme {
    /// 从配置文件加载，默认 Light。
    pub(crate) fn load() -> Self {
        let path = config_path();
        if let Ok(content) = fs::read_to_string(&path) {
            if content.contains("mode = \"dark\"") {
                return Self::Dark;
            }
        }
        Self::Light
    }

    /// 保存到配置文件。
    pub(crate) fn save(self) {
        let path = config_path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let mode = match self {
            Self::Light => "light",
            Self::Dark => "dark",
        };
        let content = format!("[theme]\nmode = \"{mode}\"\n");
        let _ = fs::write(&path, content);
    }

    /// 选项索引 (Dropdown 用)。
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Light => 0,
            Self::Dark => 1,
        }
    }

    /// 从索引构造 (Dropdown 用)。
    pub(crate) fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Light,
            _ => Self::Dark,
        }
    }

    /// 选项列表 (Dropdown 用)。
    pub(crate) fn options() -> Vec<String> {
        vec!["浅色".into(), "深色".into()]
    }

    /// 获取框架 Theme 实现。
    pub(crate) fn theme(self) -> LogTheme {
        match self {
            Self::Light => LogTheme::Light,
            Self::Dark => LogTheme::Dark,
        }
    }
}

/// 产品主题枚举 (包装框架 Theme)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogTheme {
    Light,
    Dark,
}

impl Theme for LogTheme {
    fn background(&self) -> Color {
        match self {
            Self::Light => LightTheme.background(),
            Self::Dark => DarkTheme.background(),
        }
    }
    fn surface(&self) -> Color {
        match self {
            Self::Light => LightTheme.surface(),
            Self::Dark => DarkTheme.surface(),
        }
    }
    fn surface_input(&self) -> Color {
        match self {
            Self::Light => LightTheme.surface_input(),
            Self::Dark => DarkTheme.surface_input(),
        }
    }
    fn surface_variant(&self) -> Color {
        match self {
            Self::Light => LightTheme.surface_variant(),
            Self::Dark => DarkTheme.surface_variant(),
        }
    }
    fn accent(&self) -> Color {
        match self {
            Self::Light => LightTheme.accent(),
            Self::Dark => DarkTheme.accent(),
        }
    }
    fn text_primary(&self) -> Color {
        match self {
            Self::Light => LightTheme.text_primary(),
            Self::Dark => DarkTheme.text_primary(),
        }
    }
    fn text_secondary(&self) -> Color {
        match self {
            Self::Light => LightTheme.text_secondary(),
            Self::Dark => DarkTheme.text_secondary(),
        }
    }
    fn divider(&self) -> Color {
        match self {
            Self::Light => LightTheme.divider(),
            Self::Dark => DarkTheme.divider(),
        }
    }
    fn border(&self) -> Color {
        match self {
            Self::Light => LightTheme.border(),
            Self::Dark => DarkTheme.border(),
        }
    }
    fn selection(&self) -> Color {
        match self {
            Self::Light => LightTheme.selection(),
            Self::Dark => DarkTheme.selection(),
        }
    }
    fn caret(&self) -> Color {
        match self {
            Self::Light => LightTheme.caret(),
            Self::Dark => DarkTheme.caret(),
        }
    }
    fn danger(&self) -> Color {
        match self {
            Self::Light => LightTheme.danger(),
            Self::Dark => DarkTheme.danger(),
        }
    }
    fn traffic_close(&self) -> Color {
        match self {
            Self::Light => LightTheme.traffic_close(),
            Self::Dark => DarkTheme.traffic_close(),
        }
    }
    fn traffic_minimize(&self) -> Color {
        match self {
            Self::Light => LightTheme.traffic_minimize(),
            Self::Dark => DarkTheme.traffic_minimize(),
        }
    }
    fn traffic_maximize(&self) -> Color {
        match self {
            Self::Light => LightTheme.traffic_maximize(),
            Self::Dark => DarkTheme.traffic_maximize(),
        }
    }
    fn scrim(&self) -> Color {
        match self {
            Self::Light => LightTheme.scrim(),
            Self::Dark => DarkTheme.scrim(),
        }
    }
    fn font_size_small(&self) -> u16 { LightTheme.font_size_small() }
    fn font_size_body(&self) -> u16 { LightTheme.font_size_body() }
    fn font_size_heading(&self) -> u16 { LightTheme.font_size_heading() }
    fn control_height(&self) -> f32 { LightTheme.control_height() }
    fn spacing_xs(&self) -> f32 { LightTheme.spacing_xs() }
    fn spacing_sm(&self) -> f32 { LightTheme.spacing_sm() }
    fn spacing_md(&self) -> f32 { LightTheme.spacing_md() }
    fn spacing_lg(&self) -> f32 { LightTheme.spacing_lg() }
    fn spacing_xl(&self) -> f32 { LightTheme.spacing_xl() }
    fn radius_sm(&self) -> f32 { LightTheme.radius_sm() }
    fn radius_md(&self) -> f32 { LightTheme.radius_md() }
    fn radius_lg(&self) -> f32 { LightTheme.radius_lg() }
    fn radius_xl(&self) -> f32 { LightTheme.radius_xl() }
    fn shadow_sm(&self) -> Shadow { LightTheme.shadow_sm() }
    fn shadow_md(&self) -> Shadow { LightTheme.shadow_md() }
    fn shadow_lg(&self) -> Shadow { LightTheme.shadow_lg() }
    fn easing_standard(&self) -> Easing { LightTheme.easing_standard() }
    fn easing_accelerate(&self) -> Easing { LightTheme.easing_accelerate() }
}

/// 配置文件路径。
fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("danqing-log")
        .join("config.toml")
}
