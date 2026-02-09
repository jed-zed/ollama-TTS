//! GPT-SoVITS TTS 引擎模块
//!
//! 拆分为子模块：
//! - types: 数据类型定义
//! - emotion: 情感音频选择逻辑
//! - synthesizer: 合成请求构建与发送
//! - text_splitter: 长文本智能切分

pub mod types;
mod emotion;
mod synthesizer;
mod text_splitter;

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use reqwest::Url;
use tracing::{info, warn, error};

use crate::models::config::{Config, GsvCharacterConfig};
use crate::models::tts_types::{TtsRequest, TtsResult};

pub use types::{EmotionAudioConfig, EmotionAudioEntry, RefAudioInfo};

pub struct GPTSoVITSEngine {
    api_url: String,
    timeout: Duration,
    default_character: String,
    default_male_character: String,
    default_female_character: String,
    gsv_characters: HashMap<String, GsvCharacterConfig>,
    current_gsv_character: String,
    emotion_audio_cache: HashMap<String, EmotionAudioConfig>,
    use_server_path: bool,
    server_path_prefix: String,
    top_k: i32,
    top_p: f32,
    temperature: f32,
    client: reqwest::Client,
    model_switch_failed: std::collections::HashSet<String>,
    current_gpt_model: String,
    current_sovits_model: String,
    min_ref_audios: i32,
    max_ref_audios: i32,
    emotion_threshold: f32,
}

impl GPTSoVITSEngine {
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

    pub fn get_api_url(&self) -> &str {
        &self.api_url
    }

    pub fn set_api_url(&mut self, url: &str) {
        self.api_url = url.to_string();
        info!("临时设置 GSV API URL: {}", url);
    }

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
        ].into_iter().collect();

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

        if !self.current_gsv_character.is_empty() && self.gsv_characters.contains_key(&self.current_gsv_character) {
            info!("🎭 使用GUI全局默认角色: {}", self.current_gsv_character);
            return self.current_gsv_character.clone();
        }

        self.get_default_character()
    }

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

    pub fn select_ref_audios_by_emotion_vector(
        &self,
        char_id: &str,
        emotion_vector: &HashMap<String, f32>,
    ) -> Vec<RefAudioInfo> {
        emotion::select_ref_audios_by_emotion_vector(
            char_id,
            emotion_vector,
            &self.emotion_audio_cache,
            &self.gsv_characters,
            self.min_ref_audios,
            self.max_ref_audios,
            self.emotion_threshold,
            &|audio, dir| self.build_audio_path(audio, dir),
        )
    }

    pub fn split_text(&self, text: &str, max_length: usize) -> Vec<String> {
        text_splitter::split_text(text, max_length)
    }

    pub async fn synthesize(&mut self, request: TtsRequest) -> TtsResult {
        if request.text.trim().is_empty() {
            return synthesizer::fail_result("文本为空");
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

        if target_character.is_empty() {
            return synthesizer::fail_result("没有配置任何 TTS 角色。请在语音设置中添加角色配置。");
        }

        let mut ref_audios = if let Some(emotion_vector) = &request.emotion_vector {
            self.select_ref_audios_by_emotion_vector(&target_character, emotion_vector)
        } else {
            emotion::select_audio_by_emotion(
                &target_character, &request.emotion,
                &self.emotion_audio_cache, &self.gsv_characters,
                &|audio, dir| self.build_audio_path(audio, dir),
            ).map(|a| vec![a]).unwrap_or_default()
        };

        if ref_audios.is_empty() {
            info!("🎵 情感音频匹配失败，尝试随机选择参考音频");
            if let Some(random_audio) = self.get_random_ref_audio(&target_character) {
                info!("🎵 使用随机参考音频: {}", random_audio.audio_file);
                ref_audios = vec![random_audio];
            }
        }

        if ref_audios.is_empty() {
            return synthesizer::fail_result(format!(
                "未找到匹配的参考音频。请检查角色 '{}' 的配置：1) emotion_audio_json 路径是否正确 2) ref_audio_dir 目录是否存在且包含音频文件",
                target_character
            ));
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

            let payload = synthesizer::build_payload(
                segment_text, primary_audio, is_v2_api,
                self.top_k, self.top_p, self.temperature, request.speed,
            );

            if let Some(data) = synthesizer::send_synthesis_request(
                &self.client, &self.api_url, &payload,
                self.timeout, i, text_segments.len(),
            ).await {
                audio_segments.push(data);
            }
        }

        if audio_segments.is_empty() {
            return synthesizer::fail_result("所有段落合成失败");
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

    pub async fn synthesize_streaming(
        &mut self,
        request: TtsRequest,
        dialogue_id: u64,
        tx: tokio::sync::mpsc::Sender<crate::models::tts_types::TtsSegmentAudio>,
    ) -> TtsResult {
        use crate::models::tts_types::TtsSegmentAudio;

        if request.text.trim().is_empty() {
            return synthesizer::fail_result("文本为空");
        }

        info!("🎤 TTS 流式合成请求:");
        info!("   📝 文本: {}...", request.text.chars().take(40).collect::<String>());
        info!("   👤 speaker: {}", if request.speaker.is_empty() { "(空)" } else { &request.speaker });
        info!("   🚻 gender: {}", if request.gender.is_empty() { "(空)" } else { &request.gender });

        let target_character = self.resolve_target_character(&request.speaker, &request.gender);

        if target_character.is_empty() {
            return synthesizer::fail_result("没有配置任何 TTS 角色。请在语音设置中添加角色配置。");
        }

        let mut ref_audios = if let Some(emotion_vector) = &request.emotion_vector {
            self.select_ref_audios_by_emotion_vector(&target_character, emotion_vector)
        } else {
            emotion::select_audio_by_emotion(
                &target_character, &request.emotion,
                &self.emotion_audio_cache, &self.gsv_characters,
                &|audio, dir| self.build_audio_path(audio, dir),
            ).map(|a| vec![a]).unwrap_or_default()
        };

        if ref_audios.is_empty() {
            info!("🎵 情感音频匹配失败，尝试随机选择参考音频");
            if let Some(random_audio) = self.get_random_ref_audio(&target_character) {
                ref_audios = vec![random_audio];
            }
        }

        if ref_audios.is_empty() {
            return synthesizer::fail_result(format!(
                "未找到匹配的参考音频。请检查角色 '{}' 的配置",
                target_character
            ));
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

            let payload = synthesizer::build_payload(
                segment_text, primary_audio, is_v2_api,
                self.top_k, self.top_p, self.temperature, request.speed,
            );

            if let Some(audio_data) = synthesizer::send_synthesis_request(
                &self.client, &self.api_url, &payload,
                self.timeout, i, total_segments,
            ).await {
                let data_len = audio_data.len();
                let is_last = i == total_segments - 1;

                let segment_audio = TtsSegmentAudio {
                    dialogue_id,
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
        }

        if success_count == 0 {
            synthesizer::fail_result("所有段落合成失败")
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
