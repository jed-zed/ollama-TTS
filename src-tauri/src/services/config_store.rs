//! 配置存储服务
//! 
//! 提供配置的读取、保存和管理功能。

use std::path::PathBuf;
use std::fs;

use parking_lot::RwLock;
use tracing::{info, warn};

use crate::models::Config;

/// 配置存储服务
/// 
/// 使用 RwLock 保护配置状态，支持并发读取和独占写入。
pub struct ConfigStore {
    /// 配置文件路径
    config_path: PathBuf,
    /// 当前配置（受 RwLock 保护）
    config: RwLock<Config>,
}

impl ConfigStore {
    /// 创建新的配置存储服务实例
    /// 
    /// # Arguments
    /// * `config_path` - 配置文件路径
    /// 
    /// # Returns
    /// 配置存储服务实例
    pub fn new(config_path: PathBuf) -> Self {
        let config = Self::load_from_file(&config_path);
        Self {
            config_path,
            config: RwLock::new(config),
        }
    }
    
    /// 从文件加载配置
    /// 
    /// 如果文件不存在或解析失败，返回默认配置。
    fn load_from_file(path: &PathBuf) -> Config {
        if !path.exists() {
            info!("Config file not found at {:?}, using defaults", path);
            return Config::default();
        }
        
        match fs::read_to_string(path) {
            Ok(content) => {
                match serde_json::from_str::<Config>(&content) {
                    Ok(config) => {
                        info!("Config loaded from {:?}", path);
                        config
                    }
                    Err(e) => {
                        warn!("Failed to parse config file: {}, using defaults", e);
                        Config::default()
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read config file: {}, using defaults", e);
                Config::default()
            }
        }
    }
    
    /// 获取当前配置的克隆
    /// 
    /// # Returns
    /// 配置的克隆副本
    pub fn get(&self) -> Config {
        self.config.read().clone()
    }
    
    /// 更新配置
    /// 
    /// 更新内存中的配置并保存到文件。
    /// 
    /// # Arguments
    /// * `new_config` - 新的配置
    /// 
    /// # Returns
    /// 如果保存成功返回 Ok(())，否则返回错误信息
    pub fn update(&self, new_config: Config) -> Result<(), String> {
        // 先保存到文件
        self.save_to_file(&new_config)?;
        
        // 再更新内存
        let mut config = self.config.write();
        *config = new_config;
        
        info!("Config updated and saved");
        Ok(())
    }
    
    /// 部分更新配置
    /// 
    /// 使用 JSON 合并的方式更新配置，只更新提供的字段。
    /// 
    /// # Arguments
    /// * `partial_config` - 部分配置 JSON
    /// 
    /// # Returns
    /// 更新后的完整配置
    pub fn merge_update(&self, partial_config: serde_json::Value) -> Result<Config, String> {
        let mut config = self.get();
        
        // 将当前配置转换为 JSON
        let mut config_json = serde_json::to_value(&config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        
        // 合并部分配置
        if let (Some(base), Some(update)) = (config_json.as_object_mut(), partial_config.as_object()) {
            for (key, value) in update {
                base.insert(key.clone(), value.clone());
            }
        }
        
        // 反序列化回 Config
        config = serde_json::from_value(config_json)
            .map_err(|e| format!("Failed to deserialize merged config: {}", e))?;
        
        // 保存并更新
        self.update(config.clone())?;
        
        Ok(config)
    }
    
    /// 保存配置到文件
    fn save_to_file(&self, config: &Config) -> Result<(), String> {
        // 确保父目录存在
        if let Some(parent) = self.config_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create config directory: {}", e))?;
            }
        }
        
        // 序列化为格式化的 JSON
        let json = serde_json::to_string_pretty(config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        
        // 写入文件
        fs::write(&self.config_path, json)
            .map_err(|e| format!("Failed to write config file: {}", e))?;
        
        info!("Config saved to {:?}", self.config_path);
        Ok(())
    }
    
    /// 重新从文件加载配置
    /// 
    /// 丢弃内存中的更改，重新从文件加载。
    pub fn reload(&self) {
        let config = Self::load_from_file(&self.config_path);
        let mut current = self.config.write();
        *current = config;
        info!("Config reloaded from file");
    }
    
    /// 获取配置文件路径
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }
}

/// 获取默认的配置文件路径
/// 
/// 返回可执行文件同目录下的 config.json 路径。
pub fn get_default_config_path() -> PathBuf {
    // 获取可执行文件目录
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    
    exe_dir.join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.main_api_port, 8765);
        assert!(config.tts_enabled);
        assert_eq!(config.gsv_api_url, "http://localhost:9880/tts");
    }
    
    #[test]
    fn test_config_store_new() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let store = ConfigStore::new(path.clone());
        
        // 应该返回默认配置
        let config = store.get();
        assert!(config.tts_enabled);
    }
    
    #[test]
    fn test_config_store_save_and_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let store = ConfigStore::new(path.clone());
        
        // 修改配置
        let mut config = store.get();
        config.api_url = "http://test.api".to_string();
        config.tts_enabled = false;
        store.update(config).unwrap();
        
        // 重新加载
        let store2 = ConfigStore::new(path);
        let loaded = store2.get();
        assert_eq!(loaded.api_url, "http://test.api");
        assert!(!loaded.tts_enabled);
    }
}
