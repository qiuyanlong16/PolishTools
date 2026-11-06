//! 配置读写：全部放在 exe 同级目录，绿色便携。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

fn default_repeat() -> u8 {
    3
}
fn default_interval() -> u64 {
    400
}
fn default_temperature() -> f32 {
    0.2
}
fn default_max_chars() -> usize {
    6000
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    /// 连续按下的次数
    pub repeat: u8,
    /// 相邻两次按下的最大间隔（毫秒）
    pub interval_ms: u64,
    /// 是否吞掉触发用的那一次按键，避免多余空格进入输入框
    pub swallow: bool,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            repeat: default_repeat(),
            interval_ms: default_interval(),
            swallow: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    /// 超过该长度的文本拒绝润色，避免误触发超大文档
    pub max_chars: usize,
    /// 润色完成后是否恢复用户原来的剪贴板内容
    pub restore_clipboard: bool,
    /// 暂停热键
    pub paused: bool,
    /// 开机自动启动（写 HKCU Run，可随时关闭）
    pub autostart: bool,
    pub hotkey: HotkeyConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_base: "https://api.deepseek.com".to_string(),
            api_key: String::new(),
            model: "deepseek-chat".to_string(),
            temperature: default_temperature(),
            max_chars: default_max_chars(),
            restore_clipboard: true,
            paused: false,
            autostart: false,
            hotkey: HotkeyConfig::default(),
        }
    }
}

pub fn app_dir() -> PathBuf {
    if let Ok(p) = std::env::var("POLISH_TOOLS_HOME") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|x| x.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_dir() -> PathBuf {
    app_dir().join("data")
}

pub fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

pub fn db_path() -> PathBuf {
    data_dir().join("polish.db")
}

pub fn log_path() -> PathBuf {
    data_dir().join("app.log")
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir())
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Config>(&text) {
                Ok(cfg) => cfg,
                Err(e) => {
                    crate::log_line(&format!("配置文件解析失败，使用默认配置: {e}"));
                    let cfg = Config::default();
                    let _ = cfg.save();
                    cfg
                }
            },
            Err(_) => {
                let cfg = Config::default();
                let _ = cfg.save();
                cfg
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        std::fs::write(config_path(), text)
    }
}

pub type SharedConfig = Arc<RwLock<Config>>;