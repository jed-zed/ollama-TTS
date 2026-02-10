//! 角色配置数据模型
//!
//! 定义角色相关的数据结构，对应 Python 后端的 character_manager.py。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 情感配置
///
/// 定义角色的单个情感选项，包含对应的音频文件和提示词。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmotionConfig {
    /// 情感 ID
    pub emotion_id: String,
    /// 音频文件路径
    pub file: String,
    /// 提示文本
    pub prompt: String,
    /// 关键词列表
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// 角色配置
///
/// 定义完整的角色配置，包含角色信息和可用情感。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterConfig {
    /// 角色唯一 ID
    pub character_id: String,
    /// 角色显示名称
    pub name: String,
    /// 角色别名列表（用于模糊匹配）
    #[serde(default)]
    pub name_aliases: Vec<String>,
    /// 性别: "male" | "female" | "neutral"
    pub gender: String,
    /// 参考音频目录
    pub ref_audio_dir: String,
    /// 角色描述
    #[serde(default)]
    pub description: String,
    /// 可用情感映射 {emotion_id: EmotionConfig}
    #[serde(default)]
    pub available_emotions: HashMap<String, EmotionConfig>,
}

impl CharacterConfig {
    /// 获取指定情感配置
    pub fn get_emotion(&self, emotion: &str) -> Option<&EmotionConfig> {
        self.available_emotions.get(emotion)
    }

    /// 列出所有可用情感
    pub fn list_emotions(&self) -> Vec<String> {
        self.available_emotions.keys().cloned().collect()
    }

    /// 检查是否有指定情感
    pub fn has_emotion(&self, emotion: &str) -> bool {
        self.available_emotions.contains_key(emotion)
    }
}

/// API 响应用的角色信息（简化版）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterInfo {
    /// 角色 ID
    pub character_id: String,
    /// 角色名称
    pub name: String,
    /// 性别
    pub gender: String,
    /// 描述
    pub description: String,
    /// 情感数量
    pub emotion_count: usize,
    /// 可用情感列表
    pub emotions: Vec<String>,
    /// 是否为当前选中角色
    #[serde(default)]
    pub is_current: bool,
}

impl From<&CharacterConfig> for CharacterInfo {
    fn from(config: &CharacterConfig) -> Self {
        Self {
            character_id: config.character_id.clone(),
            name: config.name.clone(),
            gender: config.gender.clone(),
            description: config.description.clone(),
            emotion_count: config.available_emotions.len(),
            emotions: config.list_emotions(),
            is_current: false,
        }
    }
}

/// 添加角色请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddCharacterRequest {
    /// 角色 ID
    pub character_id: String,
    /// 角色名称
    pub name: String,
    /// 性别
    pub gender: String,
    /// 参考音频目录
    pub ref_audio_dir: String,
    /// 描述（可选）
    #[serde(default)]
    pub description: String,
    /// 别名列表（可选）
    #[serde(default)]
    pub name_aliases: Vec<String>,
    /// 可用情感（可选）
    #[serde(default)]
    pub available_emotions: HashMap<String, EmotionConfig>,
}

/// 更新角色请求
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateCharacterRequest {
    /// 角色名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 性别
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    /// 参考音频目录
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_audio_dir: Option<String>,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 别名列表
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_aliases: Option<Vec<String>>,
    /// 可用情感
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_emotions: Option<HashMap<String, EmotionConfig>>,
}

/// 切换角色请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchCharacterRequest {
    /// 目标角色 ID
    pub character_id: String,
}

/// 特殊说话者类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeakerType {
    /// 角色
    Character,
    /// 旁白
    Narrator,
    /// 玩家
    Player,
    /// 系统
    System,
    /// 未知
    Unknown,
}

impl SpeakerType {
    /// 从说话者名称判断类型
    pub fn from_speaker(speaker: &str) -> Self {
        let speaker_lower = speaker.to_lowercase();
        match speaker_lower.as_str() {
            "narrator" | "旁白" | "narration" => SpeakerType::Narrator,
            "player" | "玩家" | "user" => SpeakerType::Player,
            "system" | "系统" | "assistant" => SpeakerType::System,
            _ => SpeakerType::Unknown,
        }
    }

    /// 判断是否为特殊说话者（非角色）
    pub fn is_special(&self) -> bool {
        !matches!(self, SpeakerType::Character | SpeakerType::Unknown)
    }
}

/// 常见男性关键词
pub const MALE_KEYWORDS: &[&str] = &[
    "guard", "soldier", "knight", "king", "prince", "lord", "sir",
    "守卫", "士兵", "骑士", "国王", "王子", "领主", "先生", "老爷",
    "将军", "大人", "老头", "老伯", "大叔", "少爷",
];

/// 常见女性关键词
pub const FEMALE_KEYWORDS: &[&str] = &[
    "maid", "princess", "queen", "lady", "miss", "girl",
    "女仆", "公主", "女王", "小姐", "夫人", "女孩", "少女",
    "姐姐", "妹妹", "阿姨", "婆婆", "女士",
];

/// 从名字猜测性别
pub fn guess_gender_from_name(name: &str) -> Option<String> {
    let name_lower = name.to_lowercase();

    for keyword in MALE_KEYWORDS {
        if name_lower.contains(keyword) {
            return Some("male".to_string());
        }
    }

    for keyword in FEMALE_KEYWORDS {
        if name_lower.contains(keyword) {
            return Some("female".to_string());
        }
    }

    None
}
