//! @author 十四叔
//! @date 2026/09/11
//!
//! 配置文件读写: 主题 + 视图开关。
//!
//! 配置路径: `dirs::config_dir()` / `danqing-log/config.toml`
//! 格式: `[theme] mode = "light"` + `[view] histogram = true`
//!
//! 解析是**朴素子串判定** (不引 toml 依赖), 与既有风格一致; 写入由
//! [`Config::save_to`] 一次性覆盖整文件 —— 所以两个键**必须同源写**,
//! 各存各的会让「改主题」顺手抹掉侧栏开关。
//!
//! 缺键一律取默认值, 故旧版只有 `[theme]` 的配置文件仍可读 (向后兼容)。

use std::fs;
use std::path::{Path, PathBuf};

use danqing::theme::{DarkTheme, LightTheme, Theme};
use danqing::{Color, Easing, Shadow};

/// 整个 `config.toml` 的唯一真身。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) theme: AppTheme,
    /// 级别计数侧栏是否显示 (`Ctrl+L` 切换)。
    pub(crate) histogram: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: AppTheme::Light,
            histogram: true,
        }
    }
}

impl Config {
    /// 从真实配置路径加载。
    pub(crate) fn load() -> Self {
        Self::load_from(&config_path())
    }

    /// 存到真实配置路径。
    pub(crate) fn save(self) {
        self.save_to(&config_path());
    }

    /// 从指定路径加载 (测试用: 不碰用户真配置)。
    pub(crate) fn load_from(path: &Path) -> Self {
        let mut c = Self::default();
        if let Ok(content) = fs::read_to_string(path) {
            if content.contains("mode = \"dark\"") {
                c.theme = AppTheme::Dark;
            }
            if content.contains("histogram = false") {
                c.histogram = false;
            }
        }
        c
    }

    /// 存到指定路径 (测试用)。
    pub(crate) fn save_to(self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let mode = match self.theme {
            AppTheme::Light => "light",
            AppTheme::Dark => "dark",
        };
        let content = format!(
            "[theme]\nmode = \"{mode}\"\n\n[view]\nhistogram = {}\n",
            self.histogram
        );
        let _ = fs::write(path, content);
    }
}

/// 主题模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppTheme {
    Light,
    Dark,
}

impl AppTheme {
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
    fn font_size_small(&self) -> u16 {
        LightTheme.font_size_small()
    }
    fn font_size_body(&self) -> u16 {
        LightTheme.font_size_body()
    }
    fn font_size_heading(&self) -> u16 {
        LightTheme.font_size_heading()
    }
    fn control_height(&self) -> f32 {
        LightTheme.control_height()
    }
    fn spacing_xs(&self) -> f32 {
        LightTheme.spacing_xs()
    }
    fn spacing_sm(&self) -> f32 {
        LightTheme.spacing_sm()
    }
    fn spacing_md(&self) -> f32 {
        LightTheme.spacing_md()
    }
    fn spacing_lg(&self) -> f32 {
        LightTheme.spacing_lg()
    }
    fn spacing_xl(&self) -> f32 {
        LightTheme.spacing_xl()
    }
    fn radius_sm(&self) -> f32 {
        LightTheme.radius_sm()
    }
    fn radius_md(&self) -> f32 {
        LightTheme.radius_md()
    }
    fn radius_lg(&self) -> f32 {
        LightTheme.radius_lg()
    }
    fn radius_xl(&self) -> f32 {
        LightTheme.radius_xl()
    }
    fn shadow_sm(&self) -> Shadow {
        LightTheme.shadow_sm()
    }
    fn shadow_md(&self) -> Shadow {
        LightTheme.shadow_md()
    }
    fn shadow_lg(&self) -> Shadow {
        LightTheme.shadow_lg()
    }
    fn easing_standard(&self) -> Easing {
        LightTheme.easing_standard()
    }
    fn easing_accelerate(&self) -> Easing {
        LightTheme.easing_accelerate()
    }
}

/// 配置文件路径。
fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("danqing-log")
        .join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 临时配置路径 (不碰用户真配置 —— 测试往那儿写会抹掉人家的设置)。
    fn temp_cfg(content: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "danqing-log-cfg-{}-{}.toml",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        if !content.is_empty() {
            let mut f = std::fs::File::create(&p).unwrap();
            f.write_all(content.as_bytes()).unwrap();
        }
        p
    }

    /// 往返: 存下来的**两个键**都要能读回来。
    /// 少写一个键 = 用户改了主题、侧栏开关被静默重置 (或反过来)。
    #[test]
    fn round_trip_preserves_both_keys() {
        let p = temp_cfg("");
        for cfg in [
            Config {
                theme: AppTheme::Dark,
                histogram: false,
            },
            Config {
                theme: AppTheme::Light,
                histogram: true,
            },
        ] {
            cfg.save_to(&p);
            assert_eq!(Config::load_from(&p), cfg, "往返不等: {cfg:?}");
        }
        let _ = std::fs::remove_file(&p);
    }

    /// 旧版只有 `[theme]` 的配置文件仍可读, 缺键取默认值。
    #[test]
    fn legacy_file_without_view_section_still_loads() {
        let p = temp_cfg("[theme]\nmode = \"dark\"\n");
        let c = Config::load_from(&p);
        assert_eq!(c.theme, AppTheme::Dark, "旧键要认");
        assert!(c.histogram, "缺键取默认: 显示");
        let _ = std::fs::remove_file(&p);
    }

    /// 无配置文件 → 全默认, 不 panic。
    #[test]
    fn missing_file_yields_defaults() {
        let p = temp_cfg("");
        let _ = std::fs::remove_file(&p);
        assert_eq!(Config::load_from(&p), Config::default());
        assert_eq!(Config::default().theme, AppTheme::Light);
        assert!(Config::default().histogram);
    }
}
