//! @author 十四叔
//! @date 2026/09/11
//!
//! 配置文件读写: 主题持久化。
//!
//! 配置路径: `dirs::config_dir()` / `danqing-log/config.toml`
//! 格式: `[theme] mode = "light"` 或 `"dark"`

use std::fs;
use std::path::PathBuf;

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

    /// 切换主题。
    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
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
}

/// 配置文件路径。
fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("danqing-log")
        .join("config.toml")
}
