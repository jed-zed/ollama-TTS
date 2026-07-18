//! 角色配置管理器
//!
//! 负责加载、管理和查询角色配置。
//! 对应 Python 后端的 character_manager.py。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use tracing::{debug, error, info, warn};

use crate::models::character::{
    guess_gender_from_name, CharacterConfig, CharacterInfo, EmotionConfig, SpeakerType,
};
use crate::services::atomic_file;

/// 角色配置管理器
///
/// 负责加载、管理和查询角色配置。
pub struct CharacterManager {
    /// 配置文件目录
    config_dir: PathBuf,
    /// 串行化角色写操作，避免多把内部锁交叉获取和文件提交乱序
    mutation_lock: Mutex<()>,
    /// 角色配置映射 {character_id: CharacterConfig}
    characters: RwLock<HashMap<String, CharacterConfig>>,
    /// 别名映射 {alias_lowercase: character_id}
    alias_map: RwLock<HashMap<String, String>>,
    /// 默认角色 ID
    default_character_id: RwLock<Option<String>>,
    /// 当前选中的角色 ID
    current_character_id: RwLock<Option<String>>,
    /// 是否已加载
    loaded: RwLock<bool>,
}

impl CharacterManager {
    fn validate_character_id(character_id: &str) -> Result<(), String> {
        let valid = !character_id.is_empty()
            && character_id.len() <= 128
            && character_id
                .chars()
                .all(|character| character.is_alphanumeric() || matches!(character, '_' | '-'));
        if valid {
            Ok(())
        } else {
            Err("角色 ID 只能包含字母、数字、下划线或连字符，且长度不能超过 128".to_string())
        }
    }

    /// 创建新的角色管理器
    pub fn new<P: AsRef<Path>>(config_dir: P) -> Self {
        Self {
            config_dir: config_dir.as_ref().to_path_buf(),
            mutation_lock: Mutex::new(()),
            characters: RwLock::new(HashMap::new()),
            alias_map: RwLock::new(HashMap::new()),
            default_character_id: RwLock::new(None),
            current_character_id: RwLock::new(None),
            loaded: RwLock::new(false),
        }
    }

    /// 使用默认配置目录创建
    pub fn with_default_dir() -> Self {
        let config_dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("character_configs");
        Self::new(config_dir)
    }

    /// 设置配置目录
    pub fn set_config_dir<P: AsRef<Path>>(&self, path: P) {
        let mut dir = self.config_dir.clone();
        dir.clone_from(&path.as_ref().to_path_buf());
    }

    /// 加载所有角色配置
    pub fn load_all_characters(&self) -> bool {
        let _mutation = match self.mutation_lock.lock() {
            Ok(lock) => lock,
            Err(_) => {
                error!("角色变更锁已损坏");
                return false;
            }
        };
        if !self.config_dir.exists() {
            warn!("角色配置目录不存在: {:?}", self.config_dir);
            if let Err(e) = fs::create_dir_all(&self.config_dir) {
                error!("创建目录失败: {}", e);
                return false;
            }
            info!("已创建角色配置目录: {:?}", self.config_dir);
        }

        let mut characters = self.characters.write().unwrap();
        let mut alias_map = self.alias_map.write().unwrap();

        characters.clear();
        alias_map.clear();

        let json_files: Vec<_> = match fs::read_dir(&self.config_dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "json")
                        .unwrap_or(false)
                })
                .collect(),
            Err(e) => {
                error!("读取配置目录失败: {}", e);
                return false;
            }
        };

        if json_files.is_empty() {
            warn!("角色配置目录为空: {:?}", self.config_dir);
            return false;
        }

        let total_files = json_files.len();
        let mut loaded_count = 0;
        let mut first_character_id: Option<String> = None;

        for entry in json_files {
            let filepath = entry.path();
            match self.load_character_file(&filepath) {
                Ok(character) => {
                    let char_id = character.character_id.clone();

                    for alias in &character.name_aliases {
                        alias_map.insert(alias.to_lowercase(), char_id.clone());
                    }
                    alias_map.insert(character.name.to_lowercase(), char_id.clone());

                    if first_character_id.is_none() {
                        first_character_id = Some(char_id.clone());
                    }

                    info!("加载角色: {} ({})", character.name, char_id);
                    characters.insert(char_id, character);
                    loaded_count += 1;
                }
                Err(e) => {
                    error!("加载角色配置失败 {:?}: {}", filepath, e);
                }
            }
        }

        if let Some(id) = first_character_id {
            let mut default_id = self.default_character_id.write().unwrap();
            *default_id = Some(id.clone());
            let mut current_id = self.current_character_id.write().unwrap();
            if current_id.is_none() {
                *current_id = Some(id);
            }
        }

        *self.loaded.write().unwrap() = loaded_count > 0;
        info!("角色加载完成: {}/{} 个角色", loaded_count, total_files);
        loaded_count > 0
    }

    /// 加载单个角色配置文件
    fn load_character_file(&self, filepath: &Path) -> Result<CharacterConfig, String> {
        let content = fs::read_to_string(filepath).map_err(|e| format!("读取文件失败: {}", e))?;

        let data: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| format!("JSON 解析失败: {}", e))?;

        let required_fields = ["character_id", "name", "gender", "ref_audio_dir"];
        for field in required_fields {
            if data.get(field).is_none() {
                return Err(format!("缺少必需字段: {}", field));
            }
        }

        let mut emotions = HashMap::new();
        if let Some(emotions_obj) = data.get("available_emotions").and_then(|v| v.as_object()) {
            for (emotion_id, emotion_data) in emotions_obj {
                emotions.insert(
                    emotion_id.clone(),
                    EmotionConfig {
                        emotion_id: emotion_id.clone(),
                        file: emotion_data
                            .get("file")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        prompt: emotion_data
                            .get("prompt")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        keywords: emotion_data
                            .get("keywords")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    },
                );
            }
        }

        let name = data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let name_aliases = data
            .get("name_aliases")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_else(|| vec![name.clone()]);

        let character_id = data
            .get("character_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Self::validate_character_id(&character_id)?;

        Ok(CharacterConfig {
            character_id,
            name,
            name_aliases,
            gender: data
                .get("gender")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            ref_audio_dir: data
                .get("ref_audio_dir")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            description: data
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            available_emotions: emotions,
        })
    }

    /// 确保角色已加载
    fn ensure_loaded(&self) {
        let loaded = *self.loaded.read().unwrap();
        if !loaded {
            self.load_all_characters();
        }
    }

    /// 根据 speaker 获取角色配置
    pub fn get_character(&self, speaker: &str) -> Option<CharacterConfig> {
        self.ensure_loaded();
        let characters = self.characters.read().unwrap();

        if let Some(char) = characters.get(speaker) {
            return Some(char.clone());
        }

        let alias_map = self.alias_map.read().unwrap();
        let speaker_lower = speaker.to_lowercase();

        if let Some(char_id) = alias_map.get(&speaker_lower) {
            return characters.get(char_id).cloned();
        }

        for (alias, char_id) in alias_map.iter() {
            if alias.contains(&speaker_lower) || speaker_lower.contains(alias) {
                return characters.get(char_id).cloned();
            }
        }

        None
    }

    /// 根据别名获取角色配置
    pub fn get_character_by_alias(&self, alias: &str) -> Option<CharacterConfig> {
        self.ensure_loaded();
        let alias_lower = alias.to_lowercase();
        let char_id = self.alias_map.read().unwrap().get(&alias_lower).cloned()?;

        self.characters.read().unwrap().get(&char_id).cloned()
    }

    /// 获取后备角色
    pub fn get_fallback_character(
        &self,
        speaker: &str,
        fallback_gender: Option<&str>,
    ) -> Option<CharacterConfig> {
        self.ensure_loaded();
        let characters = self.characters.read().unwrap();

        if characters.is_empty() {
            return None;
        }

        if let Some(gender) = fallback_gender {
            for char in characters.values() {
                if char.gender.to_lowercase() == gender.to_lowercase() {
                    info!("使用后备角色 (性别匹配): {} -> {}", speaker, char.name);
                    return Some(char.clone());
                }
            }
        }

        if let Some(guessed_gender) = guess_gender_from_name(speaker) {
            for char in characters.values() {
                if char.gender.to_lowercase() == guessed_gender.to_lowercase() {
                    info!("使用后备角色 (猜测性别): {} -> {}", speaker, char.name);
                    return Some(char.clone());
                }
            }
        }

        drop(characters);
        self.get_default_character()
    }

    /// 获取默认角色
    pub fn get_default_character(&self) -> Option<CharacterConfig> {
        self.ensure_loaded();
        // 先复制 ID 再取角色表，避免与加载路径（角色表 -> 默认 ID）锁序反转。
        let default_id = self.default_character_id.read().unwrap().clone();
        let characters = self.characters.read().unwrap();

        if let Some(id) = default_id.as_ref() {
            if let Some(char) = characters.get(id) {
                return Some(char.clone());
            }
        }

        characters.values().next().cloned()
    }

    /// 获取当前选中的角色
    pub fn get_current_character(&self) -> Option<CharacterConfig> {
        self.ensure_loaded();
        // 先复制 ID 再取角色表，避免与加载路径（角色表 -> 当前 ID）锁序反转。
        let current_id = self.current_character_id.read().unwrap().clone();
        let characters = self.characters.read().unwrap();

        if let Some(id) = current_id.as_ref() {
            if let Some(char) = characters.get(id) {
                return Some(char.clone());
            }
        }

        drop(characters);
        self.get_default_character()
    }

    /// 获取当前角色 ID
    pub fn get_current_character_id(&self) -> Option<String> {
        self.current_character_id.read().unwrap().clone()
    }

    /// 设置默认角色
    pub fn set_default_character(&self, character_id: &str) -> bool {
        let characters = self.characters.read().unwrap();
        if characters.contains_key(character_id) {
            let mut default_id = self.default_character_id.write().unwrap();
            *default_id = Some(character_id.to_string());
            info!("默认角色设置为: {}", character_id);
            true
        } else {
            false
        }
    }

    /// 切换当前角色
    pub fn switch_character(&self, character_id: &str) -> bool {
        let characters = self.characters.read().unwrap();
        if characters.contains_key(character_id) {
            let mut current_id = self.current_character_id.write().unwrap();
            *current_id = Some(character_id.to_string());
            info!("当前角色切换为: {}", character_id);
            true
        } else {
            false
        }
    }

    /// 列出所有可用角色
    pub fn list_available_characters(&self) -> Vec<CharacterInfo> {
        self.ensure_loaded();
        let characters = self.characters.read().unwrap();
        let current_id = self.current_character_id.read().unwrap();

        characters
            .values()
            .map(|char| {
                let mut info = CharacterInfo::from(char);
                info.is_current = current_id
                    .as_ref()
                    .map(|id| id == &char.character_id)
                    .unwrap_or(false);
                info
            })
            .collect()
    }

    /// 重新加载角色配置
    pub fn reload_characters(&self) -> bool {
        info!("重新加载角色配置...");
        *self.loaded.write().unwrap() = false;
        self.load_all_characters()
    }

    /// 添加角色
    pub fn add_character(&self, config: CharacterConfig) -> Result<(), String> {
        Self::validate_character_id(&config.character_id)?;
        let _mutation = self
            .mutation_lock
            .lock()
            .map_err(|_| "角色变更锁已损坏".to_string())?;
        if self
            .characters
            .read()
            .unwrap()
            .contains_key(&config.character_id)
        {
            return Err(format!("角色 ID 已存在: {}", config.character_id));
        }

        let char_id = config.character_id.clone();
        let char_name = config.name.clone();
        let content =
            serde_json::to_string_pretty(&config).map_err(|e| format!("序列化失败: {}", e))?;
        self.save_character_content_to_file(&char_id, &content)?;

        let mut characters = self.characters.write().unwrap();
        let mut alias_map = self.alias_map.write().unwrap();
        for alias in &config.name_aliases {
            alias_map.insert(alias.to_lowercase(), config.character_id.clone());
        }
        alias_map.insert(config.name.to_lowercase(), config.character_id.clone());
        characters.insert(char_id, config);

        info!("添加角色: {}", char_name);
        Ok(())
    }

    /// 更新角色
    pub fn update_character(
        &self,
        character_id: &str,
        updates: crate::models::character::UpdateCharacterRequest,
    ) -> Result<(), String> {
        Self::validate_character_id(character_id)?;
        let _mutation = self
            .mutation_lock
            .lock()
            .map_err(|_| "角色变更锁已损坏".to_string())?;
        let mut updated = self
            .characters
            .read()
            .unwrap()
            .get(character_id)
            .cloned()
            .ok_or_else(|| format!("角色不存在: {}", character_id))?;

        if let Some(name) = updates.name {
            updated.name = name;
        }

        if let Some(gender) = updates.gender {
            updated.gender = gender;
        }

        if let Some(ref_audio_dir) = updates.ref_audio_dir {
            updated.ref_audio_dir = ref_audio_dir;
        }

        if let Some(description) = updates.description {
            updated.description = description;
        }

        if let Some(name_aliases) = updates.name_aliases {
            updated.name_aliases = name_aliases;
        }

        if let Some(available_emotions) = updates.available_emotions {
            updated.available_emotions = available_emotions;
        }

        let content =
            serde_json::to_string_pretty(&updated).map_err(|e| format!("序列化失败: {}", e))?;
        self.save_character_content_to_file(character_id, &content)?;

        let mut characters = self.characters.write().unwrap();
        let mut alias_map = self.alias_map.write().unwrap();
        let previous = characters
            .get(character_id)
            .ok_or_else(|| format!("角色不存在: {}", character_id))?;
        for alias in &previous.name_aliases {
            alias_map.remove(&alias.to_lowercase());
        }
        alias_map.remove(&previous.name.to_lowercase());
        for alias in &updated.name_aliases {
            alias_map.insert(alias.to_lowercase(), character_id.to_string());
        }
        alias_map.insert(updated.name.to_lowercase(), character_id.to_string());
        characters.insert(character_id.to_string(), updated);

        info!("更新角色: {}", character_id);
        Ok(())
    }

    /// 删除角色
    pub fn delete_character(&self, character_id: &str) -> Result<(), String> {
        Self::validate_character_id(character_id)?;
        let _mutation = self
            .mutation_lock
            .lock()
            .map_err(|_| "角色变更锁已损坏".to_string())?;
        let char = self
            .characters
            .read()
            .unwrap()
            .get(character_id)
            .cloned()
            .ok_or_else(|| format!("角色不存在: {}", character_id))?;

        let filepath = self.config_dir.join(format!("{}.json", character_id));
        if filepath.exists() {
            fs::remove_file(&filepath).map_err(|e| format!("删除配置文件失败: {}", e))?;
        }

        let mut characters = self.characters.write().unwrap();
        let mut alias_map = self.alias_map.write().unwrap();
        characters.remove(character_id);
        for alias in &char.name_aliases {
            alias_map.remove(&alias.to_lowercase());
        }
        alias_map.remove(&char.name.to_lowercase());

        info!("删除角色: {}", character_id);
        Ok(())
    }

    fn save_character_content_to_file(
        &self,
        character_id: &str,
        content: &str,
    ) -> Result<(), String> {
        Self::validate_character_id(character_id)?;
        let filepath = self.config_dir.join(format!("{}.json", character_id));
        atomic_file::write(&filepath, content.as_bytes())
            .map_err(|e| format!("写入文件失败: {}", e))?;

        debug!("保存角色配置: {:?}", filepath);
        Ok(())
    }

    /// 判断是否为特殊说话者
    pub fn is_special_speaker(&self, speaker: &str) -> bool {
        SpeakerType::from_speaker(speaker).is_special()
    }

    /// 获取说话者类型
    pub fn get_speaker_type(&self, speaker: &str) -> SpeakerType {
        let speaker_type = SpeakerType::from_speaker(speaker);
        if speaker_type == SpeakerType::Unknown {
            if self.get_character(speaker).is_some() {
                return SpeakerType::Character;
            }
        }
        speaker_type
    }

    /// 获取角色数量
    pub fn character_count(&self) -> usize {
        self.characters.read().unwrap().len()
    }
}

impl Default for CharacterManager {
    fn default() -> Self {
        Self::with_default_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{mpsc, Arc},
        time::Duration,
    };
    use tempfile::tempdir;

    #[test]
    fn test_speaker_type() {
        assert_eq!(SpeakerType::from_speaker("narrator"), SpeakerType::Narrator);
        assert_eq!(SpeakerType::from_speaker("旁白"), SpeakerType::Narrator);
        assert_eq!(SpeakerType::from_speaker("player"), SpeakerType::Player);
        assert_eq!(SpeakerType::from_speaker("玩家"), SpeakerType::Player);
        assert_eq!(SpeakerType::from_speaker("system"), SpeakerType::System);
        assert_eq!(
            SpeakerType::from_speaker("SomeCharacter"),
            SpeakerType::Unknown
        );
    }

    #[test]
    fn test_guess_gender() {
        assert_eq!(
            guess_gender_from_name("女仆小姐"),
            Some("female".to_string())
        );
        assert_eq!(guess_gender_from_name("骑士大人"), Some("male".to_string()));
        assert_eq!(guess_gender_from_name("某角色"), None);
    }

    #[test]
    fn add_character_does_not_reenter_character_lock() {
        let dir = tempdir().unwrap();
        let manager = Arc::new(CharacterManager::new(dir.path()));
        let worker = Arc::clone(&manager);
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let result = worker.add_character(CharacterConfig {
                character_id: "lock-test".to_string(),
                name: "Lock Test".to_string(),
                name_aliases: vec!["lock-test".to_string()],
                gender: "neutral".to_string(),
                ref_audio_dir: String::new(),
                description: String::new(),
                available_emotions: HashMap::new(),
            });
            let _ = tx.send(result);
        });

        let result = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("add_character timed out, likely due to lock re-entry");
        result.unwrap();
        assert!(dir.path().join("lock-test.json").exists());
        assert_eq!(manager.character_count(), 1);
    }

    #[test]
    fn character_id_rejects_path_traversal() {
        for invalid in ["", "..", "../escape", "folder\\escape", "name.json"] {
            assert!(
                CharacterManager::validate_character_id(invalid).is_err(),
                "{invalid}"
            );
        }
        assert!(CharacterManager::validate_character_id("角色_01-test").is_ok());
    }
}
