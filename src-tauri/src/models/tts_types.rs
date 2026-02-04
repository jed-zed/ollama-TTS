//! TTS 相关类型定义
//! 
//! 定义 TTS 配置、对话段落等数据结构。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// TTS 配置（从 AI 响应中提取）
/// 
/// AI 返回的 JSON 中的 tts_config 字段结构。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TtsConfig {
    /// 说话者（单一 speaker 模式）
    #[serde(default)]
    pub speaker: Option<String>,
    /// 性别标识 (male/female)
    #[serde(default)]
    pub gender: Option<String>,
    /// 情感
    #[serde(default)]
    pub emotion: Option<String>,
    /// 情感描述
    #[serde(default)]
    pub emotion_description: Option<String>,
    /// 情感向量（8维）
    #[serde(default)]
    pub emotion_vector: Option<HashMap<String, f32>>,
    /// 语速
    #[serde(default)]
    pub speed: Option<f32>,
    /// 是否跳过 TTS
    #[serde(default)]
    pub skip_tts: Option<bool>,
    /// 是否为 NSFW 内容
    #[serde(default)]
    pub is_nsfw: Option<bool>,
    /// TTS 段落列表（多 speaker 模式）
    #[serde(default)]
    pub segments: Option<Vec<TtsSegment>>,
}

/// TTS 段落配置
/// 
/// 用于多 speaker 模式，每个段落可以有不同的情感和语速。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TtsSegment {
    /// 说话者
    #[serde(default)]
    pub speaker: Option<String>,
    /// 性别标识
    #[serde(default)]
    pub gender: Option<String>,
    /// 情感
    #[serde(default)]
    pub emotion: Option<String>,
    /// 情感描述
    #[serde(default)]
    pub emotion_description: Option<String>,
    /// 情感向量
    #[serde(default)]
    pub emotion_vector: Option<HashMap<String, f32>>,
    /// 语速
    #[serde(default)]
    pub speed: Option<f32>,
    /// 是否跳过 TTS（旁白）
    #[serde(default)]
    pub skip_tts: Option<bool>,
    /// 文本标记（用于匹配对话开头）
    #[serde(default)]
    pub text_marker: Option<String>,
}

/// 检测到的对话段落
/// 
/// 流式 TTS 解析器检测到对话时生成的结构。
#[derive(Debug, Clone)]
pub struct DialogueSegment {
    /// 对话文本（引号内内容）
    pub text: String,
    /// 说话者
    pub speaker: String,
    /// 性别标识 (male/female)
    pub gender: String,
    /// 情感 (neutral, happy, angry, etc.)
    pub emotion: String,
    /// 情感描述
    pub emotion_description: String,
    /// 情感向量
    pub emotion_vector: Option<HashMap<String, f32>>,
    /// 语速
    pub speed: f32,
    /// 是否为 NSFW 内容
    pub is_nsfw: bool,
    /// 是否跳过 TTS
    pub skip_tts: bool,
}

impl Default for DialogueSegment {
    fn default() -> Self {
        Self {
            text: String::new(),
            speaker: String::new(),
            gender: String::new(),
            emotion: "neutral".to_string(),
            emotion_description: String::new(),
            emotion_vector: None,
            speed: 1.0,
            is_nsfw: false,
            skip_tts: false,
        }
    }
}

impl DialogueSegment {
    /// 从 TtsSegment 创建 DialogueSegment
    pub fn from_segment(text: String, speaker: String, segment: &TtsSegment, is_nsfw: bool) -> Self {
        Self {
            text,
            speaker,
            gender: segment.gender.clone().unwrap_or_default(),
            emotion: segment.emotion.clone().unwrap_or_else(|| "neutral".to_string()),
            emotion_description: segment.emotion_description.clone().unwrap_or_default(),
            emotion_vector: segment.emotion_vector.clone(),
            speed: segment.speed.unwrap_or(1.0),
            is_nsfw,
            skip_tts: segment.skip_tts.unwrap_or(false),
        }
    }
}

/// TTS 合成请求
#[derive(Debug, Clone)]
pub struct TtsRequest {
    /// 要合成的文本
    pub text: String,
    /// 说话者
    pub speaker: String,
    /// 性别
    pub gender: String,
    /// 情感
    pub emotion: String,
    /// 情感描述
    pub emotion_description: String,
    /// 语速
    pub speed: f32,
    /// 情感向量
    pub emotion_vector: Option<HashMap<String, f32>>,
    /// 是否为 NSFW 内容
    pub is_nsfw: bool,
}

impl From<DialogueSegment> for TtsRequest {
    fn from(segment: DialogueSegment) -> Self {
        Self {
            text: segment.text,
            speaker: segment.speaker,
            gender: segment.gender,
            emotion: segment.emotion,
            emotion_description: segment.emotion_description,
            speed: segment.speed,
            emotion_vector: segment.emotion_vector,
            is_nsfw: segment.is_nsfw,
        }
    }
}

/// TTS 合成结果
#[derive(Debug, Clone, Default)]
pub struct TtsResult {
    /// 是否成功
    pub success: bool,
    /// 错误信息
    pub error: Option<String>,
    /// 音频数据（单段）
    pub audio_data: Option<Vec<u8>>,
    /// 音频数据（多段，长文本切分）
    pub audio_segments: Option<Vec<Vec<u8>>>,
    /// 使用的说话者
    pub speaker_used: String,
    /// 使用的情感
    pub emotion_used: String,
}

/// 引号配对常量
pub const QUOTE_PAIRS: &[(char, char)] = &[
    ('"', '"'),       // 英文双引号
    ('"', '"'),       // 中文左双引号 -> 中文右双引号
    ('„', '"'),       // 中文低双引号 -> 中文右双引号
    ('「', '」'),     // 日式引号
    ('『', '』'),     // 日式双引号
];

/// 获取左引号集合
pub fn left_quotes() -> std::collections::HashSet<char> {
    QUOTE_PAIRS.iter().map(|(l, _)| *l).collect()
}

/// 获取右引号集合
pub fn right_quotes() -> std::collections::HashSet<char> {
    QUOTE_PAIRS.iter().map(|(_, r)| *r).collect()
}

/// 获取引号对映射
pub fn quote_pairs_map() -> HashMap<char, char> {
    QUOTE_PAIRS.iter().cloned().collect()
}

/// 情感维度（8维情感向量）
pub const EMOTION_DIMENSIONS: &[&str] = &[
    "joy",        // 喜
    "anger",      // 怒
    "sorrow",     // 哀
    "fear",       // 惧
    "disgust",    // 厌恶
    "depression", // 低落
    "surprise",   // 惊喜
    "calm",       // 平静
];

/// 情感向量到 JSON 情感 ID 的映射
pub fn emotion_vector_to_json() -> HashMap<&'static str, &'static str> {
    [
        ("joy", "happy"),
        ("anger", "angry"),
        ("sorrow", "sad"),
        ("fear", "fear"),
        ("disgust", "angry"),
        ("depression", "depressed"),
        ("surprise", "surprised"),
        ("calm", "calm"),
    ].iter().cloned().collect()
}
