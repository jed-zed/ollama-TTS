//! GPT-SoVITS TTS 引擎数据类型

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// 情感音频配置（从 JSON 文件加载）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmotionAudioConfig {
    pub emotions: HashMap<String, EmotionAudioEntry>,
}

/// 单个情感的音频配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmotionAudioEntry {
    pub emotion_name: Option<String>,
    pub total_samples: Option<i32>,
    pub primary: Option<RefAudioInfo>,
    #[serde(default)]
    pub secondary: Vec<RefAudioInfo>,
}

/// 参考音频信息
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RefAudioInfo {
    pub audio_file: String,
    pub text: String,
    pub full_path: Option<String>,
    pub confidence: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub quality_score: Option<f64>,
}

impl RefAudioInfo {
    /// 从构建好的路径和提示文本创建（用于情感音频选择）
    pub fn from_path(audio_file: String, text: String) -> Self {
        Self {
            audio_file,
            text,
            ..Default::default()
        }
    }
}
