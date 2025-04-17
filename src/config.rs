use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to load config: {0}")]
    LoadError(#[from] config::ConfigError),

    #[error("OpenAI API key not found")]
    MissingApiKey,

    #[error("Failed to create config directory: {0}")]
    CreateDirError(#[from] std::io::Error),

    #[error("Failed to save config: {0}")]
    SaveError(String),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Settings {
    pub openai_api_key: Option<String>,
    pub model: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub max_retries: u32,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub timeout_seconds: u64,
}

fn deserialize_number_from_string<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: std::str::FromStr + serde::Deserialize<'de>,
    T::Err: std::fmt::Display,
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber<T> {
        String(String),
        Number(T),
    }

    match StringOrNumber::<T>::deserialize(deserializer) {
        Ok(StringOrNumber::String(s)) => s.parse::<T>().map_err(serde::de::Error::custom),
        Ok(StringOrNumber::Number(i)) => Ok(i),
        Err(e) => Err(e),
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            openai_api_key: None,
            model: String::from("gpt-4"),
            max_retries: 3,
            timeout_seconds: 30,
        }
    }
}

impl Settings {
    /// 加载配置,按以下顺序(后面的会覆盖前面的):
    /// 1. 默认值
    /// 2. 配置文件 (~/.config/git-tools/config.json 或当前目录 config.json)
    /// 3. 环境变量 (GT_* 或 OPENAI_API_KEY)
    pub fn load() -> Result<Self, ConfigError> {
        // 创建一个默认配置
        let default_settings = Settings::default();

        // 尝试读取项目目录中的配置文件
        let current_dir_config = "config.json";

        // 构建配置
        let mut builder = Config::builder()
            // 设置默认值
            .set_default("openai_api_key", default_settings.openai_api_key.clone())?
            .set_default("model", default_settings.model.clone())?
            .set_default("max_retries", default_settings.max_retries)?
            .set_default("timeout_seconds", default_settings.timeout_seconds)?
            // 如果当前目录中存在配置文件则加载
            .add_source(File::with_name(current_dir_config).required(false));

        // 如果用户目录存在则尝试加载
        if let Ok(config_path) = Self::get_config_path() {
            builder = builder.add_source(File::from(config_path).required(false));
        }

        // 加载环境变量
        builder = builder.add_source(
            Environment::with_prefix("GT")
                .try_parsing(true)
        );

        // 解析配置
        let mut config: Settings = builder.build()?.try_deserialize()?;

        // 如果没有设置 OpenAI API 密钥，则尝试从 OPENAI_API_KEY 环境变量获取
        if config.openai_api_key.is_none() {
            if let Ok(api_key) = env::var("OPENAI_API_KEY") {
                if !api_key.is_empty() {
                    config.openai_api_key = Some(api_key);
                }
            }
        }

        // 验证必需的配置项
        if config.openai_api_key.is_none() {
            return Err(ConfigError::MissingApiKey);
        }

        Ok(config)
    }

    #[allow(dead_code)] // 允许这个方法未被使用，因为它在测试中使用
    /// 保存配置到文件
    pub fn save(&self) -> Result<(), ConfigError> {
        let config_path = Self::get_config_path()?;

        // 确保目录存在
        if let Some(dir) = config_path.parent() {
            std::fs::create_dir_all(dir)?;
        }

        // 保存为 JSON 格式
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| ConfigError::SaveError(e.to_string()))?;
        std::fs::write(&config_path, json)?;

        Ok(())
    }

    /// 获取配置文件路径
    fn get_config_path() -> Result<PathBuf, ConfigError> {
        let home = dirs::home_dir().ok_or_else(|| {
            ConfigError::LoadError(config::ConfigError::NotFound(
                "Home directory not found".to_string()
            ))
        })?;
        Ok(home.join(".config/git-tools/config.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert!(settings.openai_api_key.is_none());
        assert_eq!(settings.model, "gpt-4");
        assert_eq!(settings.max_retries, 3);
        assert_eq!(settings.timeout_seconds, 30);
    }

    #[test]
    fn test_load_from_env() {
        // 设置环境变量
        env::set_var("GT_OPENAI_API_KEY", "test-key");
        env::set_var("GT_MODEL", "gpt-3.5-turbo");
        env::set_var("GT_MAX_RETRIES", "5");
        env::set_var("GT_TIMEOUT_SECONDS", "60");

        let settings = Settings::load().unwrap();

        // 修复这行，把期望的值从"openai-key"改为"test-key"
        assert_eq!(settings.openai_api_key.unwrap(), "test-key");
        assert_eq!(settings.model, "gpt-3.5-turbo");
        assert_eq!(settings.max_retries, 5);
        assert_eq!(settings.timeout_seconds, 60);

        // 清理环境变量
        env::remove_var("GT_OPENAI_API_KEY");
        env::remove_var("GT_MODEL");
        env::remove_var("GT_MAX_RETRIES");
        env::remove_var("GT_TIMEOUT_SECONDS");
    }

    #[test]
    fn test_save_and_load() -> Result<(), ConfigError> {
        // 创建临时目录
        let temp_dir = TempDir::new().unwrap();

        // 修改配置文件路径为临时路径
        env::set_var("HOME", temp_dir.path());

        // 创建测试配置
        let mut settings = Settings::default();
        settings.openai_api_key = Some("test-key".to_string());
        settings.model = String::from("gpt-3.5-turbo");
        settings.save()?;

        // 重新加载配置
        let loaded = Settings::load()?;

        assert_eq!(loaded.openai_api_key.unwrap(), "test-key");
        assert_eq!(loaded.model, "gpt-3.5-turbo");

        Ok(())
    }
}

