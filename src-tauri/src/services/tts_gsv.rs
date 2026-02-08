//! GPT-SoVITS TTS 引擎模块
//!
//! 功能：
//! - 调用本地 GPT-SoVITS API
//! - 使用参考音频克隆语音和情感
//! - 支持多角色动态切换
//! - 智能情感向量匹配
//! - 长文本切分

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use regex::Regex;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};

use crate::models::config::{Config, GsvCharacterConfig};
use crate::models::tts_types::{TtsRequest, TtsResult, emotion_vector_to_json};

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

/// GPT-SoVITS TTS 引擎
pub struct GPTSoVITSEngine {
    /// API 地址
    api_url: String,
    /// 请求超时时间
    timeout: Duration,
    /// 默认角色
    default_character: String,
    /// 默认男性角色
    default_male_character: String,
    /// 默认女性角色
    default_female_character: String,
    /// GSV 角色配置
    gsv_characters: HashMap<String, GsvCharacterConfig>,
    /// 当前选中的角色
    current_gsv_character: String,
    /// 情感音频配置缓存
    emotion_audio_cache: HashMap<String, EmotionAudioConfig>,
    /// 是否使用服务器路径
    use_server_path: bool,
    /// 服务器路径前缀
    server_path_prefix: String,
    /// 推理参数
    top_k: i32,
    top_p: f32,
    temperature: f32,
    /// HTTP 客户端
    client: reqwest::Client,
    /// 模型切换失败缓存
    model_switch_failed: std::collections::HashSet<String>,
    /// 当前加载的 GPT 模型
    current_gpt_model: String,
    /// 当前加载的 SoVITS 模型
    current_sovits_model: String,
    /// 最少参考音频数量
    min_ref_audios: i32,
    /// 最多参考音频数量
    max_ref_audios: i32,
    /// 情感占比阈值
    emotion_threshold: f32,
}

impl GPTSoVITSEngine {
    /// 创建新的 GPT-SoVITS 引擎
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        
        Self {
            api_url: "http://localhost:9880/tts".to_string(),
            timeout: Duration::from_secs(120),
            default_character: String::new(),
            default_male_character: String::new(),
            default_female_character: String::new(),
            gsv_characters: HashMap::new(),
            current_gsv_character: String::new(),
            emotion_audio_cache: HashMap::new(),
            use_server_path: false,
            server_path_prefix: String::new(),
            top_k: 40,
            top_p: 0.9,
            temperature: 1.3,
            client,
            model_switch_failed: std::collections::HashSet::new(),
            current_gpt_model: String::new(),
            current_sovits_model: String::new(),
            min_ref_audios: 2,
            max_ref_audios: 3,
            emotion_threshold: 0.3,
        }
    }

    /// 获取当前 API URL
    pub fn get_api_url(&self) -> &str {
        &self.api_url
    }

    /// 设置 API URL（用于测试连接时临时覆盖）
    pub fn set_api_url(&mut self, url: &str) {
        self.api_url = url.to_string();
        info!("临时设置 GSV API URL: {}", url);
    }

    /// 从配置初始化引擎
    pub fn initialize(&mut self, config: &Config) {
        self.api_url = config.gsv_api_url.clone();
        self.default_character = config.tts_default_character.clone();
        self.top_k = config.gsv_top_k;
        self.top_p = config.gsv_top_p;
        self.temperature = config.gsv_temperature;
        self.default_male_character = config.gsv_default_male_character.clone();
        self.default_female_character = config.gsv_default_female_character.clone();
        self.gsv_characters = config.gsv_characters.clone();
        self.current_gsv_character = config.gsv_current_character.clone();
        self.use_server_path = config.gsv_use_server_path;
        self.server_path_prefix = config.gsv_server_path_prefix.clone();
        self.min_ref_audios = config.gsv_min_ref_audios;
        self.max_ref_audios = config.gsv_max_ref_audios;
        self.emotion_threshold = config.gsv_emotion_threshold;

        let configs_to_load: Vec<(String, String)> = self.gsv_characters.iter()
            .filter(|(_, char_config)| !char_config.emotion_audio_json.is_empty())
            .map(|(char_id, char_config)| (char_id.clone(), char_config.emotion_audio_json.clone()))
            .collect();

        for (char_id, json_path) in configs_to_load {
            if let Err(e) = self.load_emotion_audio_config(&char_id, &json_path) {
                warn!("加载情感音频配置失败 {}: {}", char_id, e);
            }
        }

        if !self.gsv_characters.is_empty() {
            info!("GPT-SoVITS 加载 {} 个角色配置", self.gsv_characters.len());
        }
    }

    /// 加载情感音频配置 JSON
    fn load_emotion_audio_config(&mut self, char_id: &str, json_path: &str) -> Result<(), String> {
        if !Path::new(json_path).exists() {
            return Err(format!("配置文件不存在: {}", json_path));
        }

        let content = std::fs::read_to_string(json_path)
            .map_err(|e| format!("读取文件失败: {}", e))?;
        
        let config: EmotionAudioConfig = serde_json::from_str(&content)
            .map_err(|e| format!("解析 JSON 失败: {}", e))?;
        
        self.emotion_audio_cache.insert(char_id.to_string(), config);
        info!("加载情感音频配置: {} -> {}", char_id, json_path);
        Ok(())
    }

    /// 构建音频文件路径
    fn build_audio_path(&self, audio_info: &RefAudioInfo, ref_audio_dir: &str) -> String {
        if self.use_server_path && !self.server_path_prefix.is_empty() {
            if let Some(full_path) = &audio_info.full_path {
                let normalized = full_path.replace('\\', "/");
                let parts: Vec<&str> = normalized.split('/').collect();
                if parts.len() >= 2 {
                    let char_dir = parts[parts.len() - 2];
                    let filename = parts[parts.len() - 1];
                    let prefix = self.server_path_prefix.replace('\\', "/").trim_end_matches('/').to_string();
                    return format!("{}/{}/{}", prefix, char_dir, filename);
                }
            }
            return format!("{}/{}", self.server_path_prefix.trim_end_matches('/'), audio_info.audio_file);
        }

        if !ref_audio_dir.is_empty() {
            let path = Path::new(ref_audio_dir).join(&audio_info.audio_file);
            return path.to_string_lossy().to_string();
        }

        audio_info.full_path.clone().unwrap_or_else(|| audio_info.audio_file.clone())
    }

    /// 根据 speaker 和 gender 确定目标角色
    /// 
    /// 优先级：
    /// 1. 通过 speaker 名称匹配角色
    /// 2. 通过 gender 使用性别默认角色
    /// 3. 使用 GUI 当前选中的全局默认角色
    /// 4. 使用第一个可用角色
    fn resolve_target_character(&self, speaker: &str, gender: &str) -> String {
        info!("🔍 角色解析: speaker={}, gender={}", 
              if speaker.is_empty() { "(空)" } else { speaker },
              if gender.is_empty() { "(空)" } else { gender });
        info!("   📋 可用角色: {:?}", self.gsv_characters.keys().collect::<Vec<_>>());
        info!("   ⚙️ 默认配置: 男性={}, 女性={}, GUI当前={}", 
              if self.default_male_character.is_empty() { "(未设置)" } else { &self.default_male_character },
              if self.default_female_character.is_empty() { "(未设置)" } else { &self.default_female_character },
              if self.current_gsv_character.is_empty() { "(未设置)" } else { &self.current_gsv_character });

        let character_aliases: HashMap<&str, &str> = [
            ("katixia", "卡提希娅"),
            ("卡提", "卡提希娅"),
            ("yuno", "尤诺"),
            ("yinlin", "昔涟"),
            ("吟霖", "昔涟"),
        ].into_iter().collect();

        // 1. 通过 speaker 名称匹配
        if !speaker.is_empty() {
            let speaker_lower = speaker.to_lowercase();

            if let Some(&alias_target) = character_aliases.get(speaker_lower.as_str()) {
                if self.gsv_characters.contains_key(alias_target) {
                    info!("🎭 别名匹配: {} -> {}", speaker, alias_target);
                    return alias_target.to_string();
                }
            }

            for char_id in self.gsv_characters.keys() {
                let char_lower = char_id.to_lowercase();
                if char_lower == speaker_lower || char_lower.contains(&speaker_lower) || speaker_lower.contains(&char_lower) {
                    info!("🎭 对话角色匹配: {} -> {}", speaker, char_id);
                    return char_id.clone();
                }
            }
        }

        // 2. 通过 gender 使用性别默认角色（优先于全局默认）
        if !gender.is_empty() {
            let gender_lower = gender.to_lowercase();
            if gender_lower == "male" && !self.default_male_character.is_empty() {
                if self.gsv_characters.contains_key(&self.default_male_character) {
                    info!("🎭 使用默认男性角色: {}", self.default_male_character);
                    return self.default_male_character.clone();
                }
            } else if gender_lower == "female" && !self.default_female_character.is_empty() {
                if self.gsv_characters.contains_key(&self.default_female_character) {
                    info!("🎭 使用默认女性角色: {}", self.default_female_character);
                    return self.default_female_character.clone();
                }
            }
        }

        // 3. 使用 GUI 当前选中的全局默认角色
        if !self.current_gsv_character.is_empty() && self.gsv_characters.contains_key(&self.current_gsv_character) {
            info!("🎭 使用GUI全局默认角色: {}", self.current_gsv_character);
            return self.current_gsv_character.clone();
        }

        // 4. 回退到第一个可用角色
        self.get_default_character()
    }

    /// 获取默认角色（第一个可用角色）
    fn get_default_character(&self) -> String {
        if let Some(first_char) = self.gsv_characters.keys().next() {
            info!("🎭 回退到第一个可用角色: {}", first_char);
            return first_char.clone();
        }
        if !self.default_character.is_empty() {
            return self.default_character.clone();
        }
        warn!("⚠️ 没有可用的角色配置");
        String::new()
    }

    /// 根据情感向量选择参考音频
    /// 
    /// 逻辑：
    /// 1. 按照情感向量百分比匹配
    /// 2. 占比低于阈值（默认30%）的忽略
    /// 3. 最高的当做主参考音频，其他的作为辅助参考音频
    /// 4. 如果主要情感占比超过70%则再加一段这个情感的音频作为辅助参考音频
    /// 5. 至少要 min_ref_audios 条音频，最多 max_ref_audios 条
    /// 6. 传入参考音频文本
    pub fn select_ref_audios_by_emotion_vector(
        &self,
        char_id: &str,
        emotion_vector: &HashMap<String, f32>,
    ) -> Vec<RefAudioInfo> {
        info!("🎵 情感向量匹配: 角色={}", char_id);
        info!("   📦 已缓存的角色: {:?}", self.emotion_audio_cache.keys().collect::<Vec<_>>());
        
        let emotion_config = match self.emotion_audio_cache.get(char_id) {
            Some(config) => {
                info!("   ✅ 找到角色 {} 的情感配置，包含 {} 个情感", char_id, config.emotions.len());
                config
            },
            None => {
                warn!("   ❌ 未找到角色 {} 的情感配置，可用角色: {:?}", char_id, self.emotion_audio_cache.keys().collect::<Vec<_>>());
                return Vec::new();
            }
        };

        if emotion_config.emotions.is_empty() {
            warn!("   ⚠️ 角色 {} 的情感配置为空", char_id);
            return Vec::new();
        }
        
        info!("   🎭 可用情感: {:?}", emotion_config.emotions.keys().collect::<Vec<_>>());

        let emotion_map = emotion_vector_to_json();
        let mut emotion_weights: HashMap<String, f32> = HashMap::new();

        for (vec_key, json_emotion) in &emotion_map {
            let weight = emotion_vector.get(*vec_key).copied().unwrap_or(0.0);
            emotion_weights
                .entry(json_emotion.to_string())
                .and_modify(|w| *w = w.max(weight))
                .or_insert(weight);
        }

        let mut sorted: Vec<_> = emotion_weights.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let total_weight: f32 = sorted.iter().map(|(_, w)| *w).filter(|w| *w > 0.0).sum();
        let total_weight = if total_weight == 0.0 { 1.0 } else { total_weight };

        info!("🎵 GSV 情感分析 (阈值={:.0}%, 最少{}条, 最多{}条):", 
              self.emotion_threshold * 100.0, self.min_ref_audios, self.max_ref_audios);
        let mapping_parts: Vec<String> = emotion_map.iter()
            .filter_map(|(k, v)| {
                let w = emotion_vector.get(*k).copied().unwrap_or(0.0);
                if w > 0.0 { Some(format!("{}→{}({:.2})", k, v, w)) } else { None }
            })
            .collect();
        info!("   📊 向量映射: {}", if mapping_parts.is_empty() { "无有效情感".to_string() } else { mapping_parts.join(", ") });

        let char_config = self.gsv_characters.get(char_id);
        let ref_audio_dir = char_config.map(|c| c.ref_audio_dir.as_str()).unwrap_or("");

        let mut result = Vec::new();
        let mut primary_emotion: Option<String> = None;
        let mut primary_ratio: f32 = 0.0;

        for (emotion_id, weight) in &sorted {
            if *weight <= 0.0 { continue; }
            let ratio = *weight / total_weight;

            if result.is_empty() {
                primary_emotion = Some(emotion_id.clone());
                primary_ratio = ratio;
                
                if let Some(entry) = emotion_config.emotions.get(emotion_id) {
                    if let Some(primary) = &entry.primary {
                        let full_path = self.build_audio_path(primary, ref_audio_dir);
                        result.push(RefAudioInfo {
                            audio_file: full_path,
                            text: primary.text.clone(),
                            full_path: None,
                            ..Default::default()
                        });
                        info!("   ✅ 主情感: {} ({:.0}%) - {}", emotion_id, ratio * 100.0, primary.text.chars().take(20).collect::<String>());
                    }
                }
            } else if ratio >= self.emotion_threshold && (result.len() as i32) < self.max_ref_audios {
                if let Some(entry) = emotion_config.emotions.get(emotion_id) {
                    if let Some(primary) = &entry.primary {
                        let full_path = self.build_audio_path(primary, ref_audio_dir);
                        result.push(RefAudioInfo {
                            audio_file: full_path,
                            text: primary.text.clone(),
                            full_path: None,
                            ..Default::default()
                        });
                        info!("   ✅ 次情感: {} ({:.0}%) - {}", emotion_id, ratio * 100.0, primary.text.chars().take(20).collect::<String>());
                    }
                }
            } else if ratio < self.emotion_threshold {
                info!("   ⏭️ 跳过: {} ({:.0}%) - 低于{:.0}%阈值", emotion_id, ratio * 100.0, self.emotion_threshold * 100.0);
            }
        }

        if let Some(primary_emo) = &primary_emotion {
            if primary_ratio >= 0.70 && (result.len() as i32) < self.max_ref_audios {
                if let Some(entry) = emotion_config.emotions.get(primary_emo) {
                    if let Some(secondary) = entry.secondary.first() {
                        let full_path = self.build_audio_path(secondary, ref_audio_dir);
                        result.push(RefAudioInfo {
                            audio_file: full_path,
                            text: secondary.text.clone(),
                            full_path: None,
                            ..Default::default()
                        });
                        info!("   ✅ 主情感辅助: {} (占比{:.0}%>70%) - {}", primary_emo, primary_ratio * 100.0, secondary.text.chars().take(20).collect::<String>());
                    }
                }
            }
        }

        while (result.len() as i32) < self.min_ref_audios && !result.is_empty() {
            if let Some(primary_emo) = &primary_emotion {
                if let Some(entry) = emotion_config.emotions.get(primary_emo) {
                    if let Some(secondary) = entry.secondary.first() {
                        let full_path = self.build_audio_path(secondary, ref_audio_dir);
                        let already_has = result.iter().any(|r| r.audio_file == full_path);
                        if !already_has {
                            result.push(RefAudioInfo {
                                audio_file: full_path,
                                text: secondary.text.clone(),
                                full_path: None,
                                ..Default::default()
                            });
                            info!("   ✅ 补充辅助: {} (确保至少{}条) - {}", primary_emo, self.min_ref_audios, secondary.text.chars().take(20).collect::<String>());
                            continue;
                        }
                    }
                }
            }
            
            if let Some(calm_entry) = emotion_config.emotions.get("calm") {
                if let Some(primary) = &calm_entry.primary {
                    let full_path = self.build_audio_path(primary, ref_audio_dir);
                    let already_has = result.iter().any(|r| r.audio_file == full_path);
                    if !already_has {
                        result.push(RefAudioInfo {
                            audio_file: full_path,
                            text: primary.text.clone(),
                            full_path: None,
                            ..Default::default()
                        });
                        info!("   ✅ 补充 calm: (确保至少{}条) - {}", self.min_ref_audios, primary.text.chars().take(20).collect::<String>());
                        continue;
                    }
                }
            }
            break;
        }

        if result.is_empty() {
            if let Some(calm_entry) = emotion_config.emotions.get("calm") {
                if let Some(primary) = &calm_entry.primary {
                    warn!("   无有效情感，回退到 calm");
                    let full_path = self.build_audio_path(primary, ref_audio_dir);
                    result.push(RefAudioInfo {
                        audio_file: full_path,
                        text: primary.text.clone(),
                        full_path: None,
                        ..Default::default()
                    });
                }
                if let Some(secondary) = calm_entry.secondary.first() {
                    let full_path = self.build_audio_path(secondary, ref_audio_dir);
                    result.push(RefAudioInfo {
                        audio_file: full_path,
                        text: secondary.text.clone(),
                        full_path: None,
                        ..Default::default()
                    });
                }
            }
        }

        info!("   📢 最终选择: {} 条参考音频", result.len());
        result
    }

    /// 根据情感字符串选择参考音频
    fn select_audio_by_emotion(&self, char_id: &str, emotion: &str) -> Option<RefAudioInfo> {
        let emotion_config = self.emotion_audio_cache.get(char_id)?;
        
        let emotion_lower = emotion.to_lowercase();
        let emotion_map: HashMap<&str, &str> = [
            ("happy", "happy"), ("joy", "happy"), ("joyful", "happy"),
            ("angry", "angry"), ("anger", "angry"), ("mad", "angry"),
            ("sad", "sad"), ("sorrow", "sad"), ("sorrowful", "sad"),
            ("fear", "fear"), ("afraid", "fear"), ("scared", "fear"),
            ("surprised", "surprised"), ("surprise", "surprised"),
            ("depressed", "depressed"), ("depression", "depressed"),
            ("calm", "calm"), ("neutral", "calm"), ("normal", "calm"),
            ("喜", "happy"), ("开心", "happy"), ("高兴", "happy"),
            ("怒", "angry"), ("愤怒", "angry"), ("生气", "angry"),
            ("哀", "sad"), ("悲伤", "sad"), ("难过", "sad"),
            ("惧", "fear"), ("恐惧", "fear"), ("害怕", "fear"),
            ("惊", "surprised"), ("惊讶", "surprised"),
            ("平静", "calm"), ("平和", "calm"),
        ].into_iter().collect();

        let json_emotion = emotion_map.get(emotion_lower.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if emotion_config.emotions.contains_key(&emotion_lower) {
                    Some(emotion_lower.clone())
                } else if emotion_config.emotions.contains_key("calm") {
                    info!("🎵 情感 '{}' 未找到，回退到 calm", emotion);
                    Some("calm".to_string())
                } else {
                    emotion_config.emotions.keys().next().cloned()
                }
            })?;

        let entry = emotion_config.emotions.get(&json_emotion)?;
        let primary = entry.primary.as_ref()?;
        
        let char_config = self.gsv_characters.get(char_id);
        let ref_audio_dir = char_config.map(|c| c.ref_audio_dir.as_str()).unwrap_or("");
        let full_path = self.build_audio_path(primary, ref_audio_dir);

        Some(RefAudioInfo {
            audio_file: full_path,
            text: primary.text.clone(),
            full_path: None,
            ..Default::default()
        })
    }

    /// 从角色目录随机选择一个参考音频（回退方案）
    fn get_random_ref_audio(&self, char_id: &str) -> Option<RefAudioInfo> {
        use rand::seq::SliceRandom;
        
        let char_config = self.gsv_characters.get(char_id)?;
        let ref_audio_dir = &char_config.ref_audio_dir;
        
        if ref_audio_dir.is_empty() {
            info!("🎵 角色 {} 没有配置参考音频目录", char_id);
            return None;
        }

        let path = Path::new(ref_audio_dir);
        if !path.exists() {
            info!("🎵 参考音频目录不存在: {}", ref_audio_dir);
            return None;
        }

        let audio_extensions = ["wav", "mp3", "flac", "ogg"];
        let mut audio_files: Vec<std::path::PathBuf> = Vec::new();

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_file() {
                    if let Some(ext) = entry_path.extension() {
                        if audio_extensions.iter().any(|e| ext.eq_ignore_ascii_case(e)) {
                            audio_files.push(entry_path);
                        }
                    }
                }
            }
        }

        if audio_files.is_empty() {
            info!("🎵 目录中没有找到音频文件: {}", ref_audio_dir);
            return None;
        }

        let mut rng = rand::thread_rng();
        let selected = audio_files.choose(&mut rng)?;
        let selected_str = selected.to_string_lossy().to_string();

        let final_path = if self.use_server_path && !self.server_path_prefix.is_empty() {
            let normalized = selected_str.replace('\\', "/");
            let parts: Vec<&str> = normalized.split('/').collect();
            if parts.len() >= 2 {
                let char_dir = parts[parts.len() - 2];
                let filename = parts[parts.len() - 1];
                let prefix = self.server_path_prefix.replace('\\', "/").trim_end_matches('/').to_string();
                format!("{}/{}/{}", prefix, char_dir, filename)
            } else {
                selected_str.clone()
            }
        } else {
            selected_str.clone()
        };

        let prompt_text = selected.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "默认提示".to_string());

        info!("🎵 随机选择参考音频: {} -> {}", selected_str, final_path);

        Some(RefAudioInfo {
            audio_file: final_path,
            text: prompt_text,
            full_path: Some(selected_str),
            ..Default::default()
        })
    }

    /// 智能切分长文本
    pub fn split_text(&self, text: &str, max_length: usize) -> Vec<String> {
        if text.chars().count() <= max_length {
            return vec![text.to_string()];
        }

        let primary_regex = Regex::new(r"[。！？!?]").unwrap();
        let secondary_regex = Regex::new(r"[，,；;：:]").unwrap();

        let mut segments = Vec::new();
        let mut remaining = text.trim();

        while !remaining.is_empty() {
            if remaining.chars().count() <= max_length {
                segments.push(remaining.to_string());
                break;
            }

            let search_end = remaining.char_indices()
                .take(max_length)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(remaining.len());
            
            let search_text = &remaining[..search_end];
            let mut split_pos = None;

            if let Some(m) = primary_regex.find_iter(search_text).last() {
                split_pos = Some(m.end());
            }

            if split_pos.is_none() {
                if let Some(m) = secondary_regex.find_iter(search_text).last() {
                    split_pos = Some(m.end());
                }
            }

            let pos = split_pos.unwrap_or(search_end);
            segments.push(remaining[..pos].trim().to_string());
            remaining = remaining[pos..].trim();
        }

        segments.into_iter().filter(|s| !s.is_empty()).collect()
    }

    /// 尝试切换模型
    async fn switch_model_if_needed(&mut self, char_id: &str) -> bool {
        let char_config = match self.gsv_characters.get(char_id) {
            Some(c) => c.clone(),
            None => return true,
        };

        let gpt_model = &char_config.gpt_model_path;
        let sovits_model = &char_config.sovits_model_path;

        if gpt_model.is_empty() && sovits_model.is_empty() {
            return true;
        }

        let model_key = format!("{}|{}", gpt_model, sovits_model);
        if self.model_switch_failed.contains(&model_key) {
            return true;
        }

        let need_switch = (!gpt_model.is_empty() && gpt_model != &self.current_gpt_model)
            || (!sovits_model.is_empty() && sovits_model != &self.current_sovits_model);

        if !need_switch {
            return true;
        }

        let parsed: Url = match Url::parse(&self.api_url) {
            Ok(u) => u,
            Err(_) => return false,
        };
        let base_url = format!("{}://{}", parsed.scheme(), parsed.host_str().unwrap_or("localhost"));
        let port = parsed.port().map(|p| format!(":{}", p)).unwrap_or_default();
        let base_url = format!("{}{}", base_url, port);

        info!("🔄 尝试切换模型: GPT={}, SoVITS={}", 
              if gpt_model.is_empty() { "无" } else { gpt_model },
              if sovits_model.is_empty() { "无" } else { sovits_model });

        let mut gpt_success = true;
        let mut sovits_success = true;

        if !gpt_model.is_empty() && gpt_model != &self.current_gpt_model {
            let url = format!("{}/set_gpt_weights?weights_path={}", base_url, gpt_model);
            match self.client.get(&url).timeout(Duration::from_secs(60)).send().await {
                Ok(resp) if resp.status().is_success() => {
                    self.current_gpt_model = gpt_model.clone();
                    info!("   GPT 模型切换成功");
                }
                Ok(resp) => {
                    gpt_success = false;
                    warn!("   GPT 模型切换失败: HTTP {}", resp.status());
                }
                Err(e) => {
                    gpt_success = false;
                    warn!("   GPT 模型切换异常: {}", e);
                }
            }
        }

        if !sovits_model.is_empty() && sovits_model != &self.current_sovits_model {
            let url = format!("{}/set_sovits_weights?weights_path={}", base_url, sovits_model);
            match self.client.get(&url).timeout(Duration::from_secs(60)).send().await {
                Ok(resp) if resp.status().is_success() => {
                    self.current_sovits_model = sovits_model.clone();
                    info!("   SoVITS 模型切换成功");
                }
                Ok(resp) => {
                    sovits_success = false;
                    warn!("   SoVITS 模型切换失败: HTTP {}", resp.status());
                }
                Err(e) => {
                    sovits_success = false;
                    warn!("   SoVITS 模型切换异常: {}", e);
                }
            }
        }

        if gpt_success && sovits_success {
            self.model_switch_failed.remove(&model_key);
            true
        } else {
            self.model_switch_failed.insert(model_key);
            false
        }
    }

    /// 合成语音
    pub async fn synthesize(&mut self, request: TtsRequest) -> TtsResult {
        if request.text.trim().is_empty() {
            return TtsResult {
                success: false,
                error: Some("文本为空".to_string()),
                ..Default::default()
            };
        }

        info!("🎤 TTS 合成请求:");
        info!("   📝 文本: {}...", request.text.chars().take(40).collect::<String>());
        info!("   👤 speaker: {}", if request.speaker.is_empty() { "(空)" } else { &request.speaker });
        info!("   🚻 gender: {}", if request.gender.is_empty() { "(空)" } else { &request.gender });
        info!("   😊 emotion: {}", request.emotion);
        if let Some(ref vec) = request.emotion_vector {
            let vec_parts: Vec<String> = vec.iter()
                .filter(|(_, v)| **v > 0.0)
                .map(|(k, v)| format!("{}={:.2}", k, v))
                .collect();
            info!("   🎭 emotion_vector: {}", if vec_parts.is_empty() { "无有效情感".to_string() } else { vec_parts.join(", ") });
        } else {
            info!("   🎭 emotion_vector: None");
        }

        let target_character = self.resolve_target_character(&request.speaker, &request.gender);
        
        // 检查是否有可用的角色配置
        if target_character.is_empty() {
            return TtsResult {
                success: false,
                error: Some("没有配置任何 TTS 角色。请在语音设置中添加角色配置。".to_string()),
                ..Default::default()
            };
        }
        
        let mut ref_audios = if let Some(emotion_vector) = &request.emotion_vector {
            self.select_ref_audios_by_emotion_vector(&target_character, emotion_vector)
        } else {
            self.select_audio_by_emotion(&target_character, &request.emotion)
                .map(|a| vec![a])
                .unwrap_or_default()
        };

        if ref_audios.is_empty() {
            info!("🎵 情感音频匹配失败，尝试随机选择参考音频");
            if let Some(random_audio) = self.get_random_ref_audio(&target_character) {
                info!("🎵 使用随机参考音频: {}", random_audio.audio_file);
                ref_audios = vec![random_audio];
            }
        }

        if ref_audios.is_empty() {
            return TtsResult {
                success: false,
                error: Some(format!(
                    "未找到匹配的参考音频。请检查角色 '{}' 的配置：1) emotion_audio_json 路径是否正确 2) ref_audio_dir 目录是否存在且包含音频文件",
                    target_character
                )),
                ..Default::default()
            };
        }

        let primary_audio = &ref_audios[0];

        self.switch_model_if_needed(&target_character).await;

        let text_segments = self.split_text(request.text.trim(), 40);
        
        if text_segments.len() > 1 {
            info!("🎵 GSV TTS: 长文本切分为 {} 段 | 角色={}", text_segments.len(), target_character);
        } else {
            let preview: String = request.text.chars().take(30).collect();
            info!("🎵 GSV TTS: {}... | 角色={} | 情感={}", preview, target_character, request.emotion);
        }

        let is_v2_api = self.api_url.trim_end_matches('/').ends_with("/tts");

        let mut audio_segments = Vec::new();
        for (i, segment_text) in text_segments.iter().enumerate() {
            if text_segments.len() > 1 {
                let preview: String = segment_text.chars().take(25).collect();
                info!("   📝 段落 {}/{}: {}...", i + 1, text_segments.len(), preview);
            }

            let payload = if is_v2_api {
                serde_json::json!({
                    "text": segment_text,
                    "text_lang": "zh",
                    "ref_audio_path": primary_audio.audio_file,
                    "prompt_text": primary_audio.text,
                    "prompt_lang": "zh",
                    "top_k": self.top_k,
                    "top_p": self.top_p,
                    "temperature": self.temperature,
                    "speed": request.speed
                })
            } else {
                serde_json::json!({
                    "text": segment_text,
                    "text_language": "zh",
                    "refer_wav_path": primary_audio.audio_file,
                    "prompt_text": primary_audio.text,
                    "prompt_language": "zh"
                })
            };

            match self.client
                .post(&self.api_url)
                .json(&payload)
                .timeout(self.timeout)
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        let content_type = response.headers()
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");

                        if content_type.contains("audio") || content_type.contains("wav") || content_type.contains("octet-stream") {
                            match response.bytes().await {
                                Ok(bytes) => {
                                    audio_segments.push(bytes.to_vec());
                                    if text_segments.len() > 1 {
                                        info!("   ✅ 段落 {} 合成成功: {} 字节", i + 1, bytes.len());
                                    }
                                }
                                Err(e) => {
                                    error!("   ❌ 段落 {} 读取响应失败: {}", i + 1, e);
                                }
                            }
                        } else {
                            error!("   ❌ 段落 {} 未知响应格式: {}", i + 1, content_type);
                        }
                    } else {
                        error!("   ❌ 段落 {} HTTP {}", i + 1, response.status());
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        error!("   ❌ 段落 {} 请求超时", i + 1);
                    } else if e.is_connect() {
                        error!("   ❌ 段落 {} 连接失败", i + 1);
                    } else {
                        error!("   ❌ 段落 {} 异常: {}", i + 1, e);
                    }
                }
            }
        }

        if audio_segments.is_empty() {
            return TtsResult {
                success: false,
                error: Some("所有段落合成失败".to_string()),
                ..Default::default()
            };
        }

        if audio_segments.len() == 1 {
            info!("GSV TTS 合成成功: {} 字节", audio_segments[0].len());
            TtsResult {
                success: true,
                audio_data: Some(audio_segments.remove(0)),
                speaker_used: target_character,
                emotion_used: request.emotion,
                ..Default::default()
            }
        } else {
            let total_bytes: usize = audio_segments.iter().map(|s| s.len()).sum();
            info!("GSV TTS 合成成功: {} 段, 共 {} 字节", audio_segments.len(), total_bytes);
            TtsResult {
                success: true,
                audio_data: None,
                audio_segments: Some(audio_segments),
                speaker_used: target_character,
                emotion_used: request.emotion,
                ..Default::default()
            }
        }
    }

    /// 流式合成语音（边合成边发送）
    /// 
    /// 每合成完一个段落就通过 channel 发送，实现边合成边播放。
    pub async fn synthesize_streaming(
        &mut self,
        request: TtsRequest,
        tx: tokio::sync::mpsc::Sender<crate::models::tts_types::TtsSegmentAudio>,
    ) -> TtsResult {
        use crate::models::tts_types::TtsSegmentAudio;
        
        if request.text.trim().is_empty() {
            return TtsResult {
                success: false,
                error: Some("文本为空".to_string()),
                ..Default::default()
            };
        }

        info!("🎤 TTS 流式合成请求:");
        info!("   📝 文本: {}...", request.text.chars().take(40).collect::<String>());
        info!("   👤 speaker: {}", if request.speaker.is_empty() { "(空)" } else { &request.speaker });
        info!("   🚻 gender: {}", if request.gender.is_empty() { "(空)" } else { &request.gender });

        let target_character = self.resolve_target_character(&request.speaker, &request.gender);
        
        if target_character.is_empty() {
            return TtsResult {
                success: false,
                error: Some("没有配置任何 TTS 角色。请在语音设置中添加角色配置。".to_string()),
                ..Default::default()
            };
        }
        
        let mut ref_audios = if let Some(emotion_vector) = &request.emotion_vector {
            self.select_ref_audios_by_emotion_vector(&target_character, emotion_vector)
        } else {
            self.select_audio_by_emotion(&target_character, &request.emotion)
                .map(|a| vec![a])
                .unwrap_or_default()
        };

        if ref_audios.is_empty() {
            info!("🎵 情感音频匹配失败，尝试随机选择参考音频");
            if let Some(random_audio) = self.get_random_ref_audio(&target_character) {
                ref_audios = vec![random_audio];
            }
        }

        if ref_audios.is_empty() {
            return TtsResult {
                success: false,
                error: Some(format!(
                    "未找到匹配的参考音频。请检查角色 '{}' 的配置",
                    target_character
                )),
                ..Default::default()
            };
        }

        let primary_audio = &ref_audios[0];
        self.switch_model_if_needed(&target_character).await;

        let text_segments = self.split_text(request.text.trim(), 40);
        let total_segments = text_segments.len();
        
        info!("🎵 GSV TTS 流式合成: {} 段 | 角色={}", total_segments, target_character);

        let is_v2_api = self.api_url.trim_end_matches('/').ends_with("/tts");
        let mut success_count = 0;

        for (i, segment_text) in text_segments.iter().enumerate() {
            let preview: String = segment_text.chars().take(25).collect();
            info!("   📝 合成段落 {}/{}: {}...", i + 1, total_segments, preview);

            let payload = if is_v2_api {
                serde_json::json!({
                    "text": segment_text,
                    "text_lang": "zh",
                    "ref_audio_path": primary_audio.audio_file,
                    "prompt_text": primary_audio.text,
                    "prompt_lang": "zh",
                    "top_k": self.top_k,
                    "top_p": self.top_p,
                    "temperature": self.temperature,
                    "speed": request.speed
                })
            } else {
                serde_json::json!({
                    "text": segment_text,
                    "text_language": "zh",
                    "refer_wav_path": primary_audio.audio_file,
                    "prompt_text": primary_audio.text,
                    "prompt_language": "zh"
                })
            };

            match self.client
                .post(&self.api_url)
                .json(&payload)
                .timeout(self.timeout)
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        let content_type = response.headers()
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");

                        if content_type.contains("audio") || content_type.contains("wav") || content_type.contains("octet-stream") {
                            match response.bytes().await {
                                Ok(bytes) => {
                                    let audio_data = bytes.to_vec();
                                    let data_len = audio_data.len();
                                    let is_last = i == total_segments - 1;
                                    
                                    let segment_audio = TtsSegmentAudio {
                                        seq: i,
                                        total: total_segments,
                                        audio_data,
                                        is_last,
                                    };
                                    
                                    if tx.send(segment_audio).await.is_ok() {
                                        info!("   ✅ 段落 {}/{} 已发送: {} 字节", i + 1, total_segments, data_len);
                                        success_count += 1;
                                    } else {
                                        error!("   ❌ 段落 {} 发送失败: channel 已关闭", i + 1);
                                    }
                                }
                                Err(e) => {
                                    error!("   ❌ 段落 {} 读取响应失败: {}", i + 1, e);
                                }
                            }
                        } else {
                            error!("   ❌ 段落 {} 未知响应格式: {}", i + 1, content_type);
                        }
                    } else {
                        error!("   ❌ 段落 {} HTTP {}", i + 1, response.status());
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        error!("   ❌ 段落 {} 请求超时", i + 1);
                    } else if e.is_connect() {
                        error!("   ❌ 段落 {} 连接失败", i + 1);
                    } else {
                        error!("   ❌ 段落 {} 异常: {}", i + 1, e);
                    }
                }
            }
        }

        if success_count == 0 {
            TtsResult {
                success: false,
                error: Some("所有段落合成失败".to_string()),
                ..Default::default()
            }
        } else {
            info!("GSV TTS 流式合成完成: {}/{} 段成功", success_count, total_segments);
            TtsResult {
                success: true,
                speaker_used: target_character,
                emotion_used: request.emotion,
                ..Default::default()
            }
        }
    }

    /// 测试 API 连接（使用 TCP Socket，与 Python 实现一致）
    pub async fn test_connection(&self) -> bool {
        info!("开始测试 GSV API 连接，当前 URL: {}", self.api_url);
        
        let parsed: Url = match Url::parse(&self.api_url) {
            Ok(u) => u,
            Err(e) => {
                warn!("GSV API URL 解析失败: {} - {}", self.api_url, e);
                return false;
            }
        };

        let host = match parsed.host_str() {
            Some(h) => h.to_string(),
            None => {
                warn!("GSV API URL 缺少主机名: {}", self.api_url);
                return false;
            }
        };

        let port = parsed.port().unwrap_or(80);
        let addr = format!("{}:{}", host, port);

        info!("测试 GSV API TCP 连接: {}", addr);

        for attempt in 1..=3 {
            info!("尝试 TCP 连接 ({}/3): {}", attempt, addr);
            
            match tokio::time::timeout(
                Duration::from_secs(5),
                tokio::net::TcpStream::connect(&addr)
            ).await {
                Ok(Ok(_stream)) => {
                    info!("GSV API 端口可达: {}", addr);
                    return true;
                }
                Ok(Err(e)) => {
                    if attempt < 3 {
                        info!("GSV API TCP 连接失败 ({}/3): {}", attempt, e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    } else {
                        warn!("GSV API 无法连接: {} - {}", addr, e);
                    }
                }
                Err(_) => {
                    if attempt < 3 {
                        info!("GSV API TCP 连接超时 ({}/3)", attempt);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    } else {
                        warn!("GSV API 连接超时: {}", addr);
                    }
                }
            }
        }
        warn!("GSV API 连接测试失败，已尝试 3 次");
        false
    }
}

impl Default for GPTSoVITSEngine {
    fn default() -> Self {
        Self::new()
    }
}
