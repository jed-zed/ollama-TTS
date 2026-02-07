//! 流式 TTS 解析器
//! 
//! 边接收数据边检测对话并触发 TTS。

use std::collections::HashMap;
use regex::Regex;
use tracing::{info, warn};

use crate::models::tts_types::{TtsConfig, TtsSegment, DialogueSegment, quote_pairs_map, left_quotes};

/// 对话检测回调类型
pub type OnDialogueDetected = Box<dyn Fn(DialogueSegment) + Send + Sync>;

/// 流式 TTS 解析器
/// 
/// 工作流程：
/// 1. 先接收 tts_config（等到 "response": 出现）
/// 2. 建立 text_marker → 情感 的映射表
/// 3. 流式接收 response，检测对话边界，立即触发回调
pub struct StreamingTTSParser {
    /// 对话检测回调
    on_dialogue_detected: Option<OnDialogueDetected>,
    
    // 阶段1: 收集 tts_config
    /// 完整缓冲区
    buffer: String,
    /// tts_config 是否已完成解析
    tts_config_complete: bool,
    /// 解析出的 tts_config
    tts_config: Option<TtsConfig>,
    /// text_marker -> segment 映射
    segments_map: HashMap<String, TtsSegment>,
    /// 是否为 NSFW 内容
    is_nsfw: bool,
    /// 默认 segment（单一 speaker 模式）
    default_segment: Option<TtsSegment>,
    /// 所有 segments 的有序列表（用于顺序匹配）
    segments_list: Vec<TtsSegment>,
    /// 当前使用的 segment 索引
    current_segment_index: usize,
    
    // 阶段2: 流式解析 response
    /// 是否在 response 字段内
    in_response: bool,
    /// response 缓冲区
    response_buffer: String,
    /// 当前对话缓冲区
    dialogue_buffer: String,
    /// 是否在引号内
    in_dialogue: bool,
    /// 当前说话者
    current_speaker: String,
    /// 已检测到的对话列表
    detected_dialogues: Vec<DialogueSegment>,
    
    /// 记录每个角色最近使用的 segment（用于回退匹配）
    last_used_segment_by_speaker: HashMap<String, TtsSegment>,
    
    /// 当前使用的引号类型
    quote_char: Option<char>,
    
    /// 引号对映射
    quote_pairs: HashMap<char, char>,
    /// 左引号集合
    left_quotes: std::collections::HashSet<char>,
    
    /// 是否在等待 response 开始
    waiting_for_response_start: bool,
    
    /// 角色格式正则表达式
    speaker_pattern: Regex,
}

impl StreamingTTSParser {
    /// 创建新的 TTS 解析器
    pub fn new() -> Self {
        Self {
            on_dialogue_detected: None,
            buffer: String::new(),
            tts_config_complete: false,
            tts_config: None,
            segments_map: HashMap::new(),
            is_nsfw: false,
            default_segment: None,
            segments_list: Vec::new(),
            current_segment_index: 0,
            in_response: false,
            response_buffer: String::new(),
            dialogue_buffer: String::new(),
            in_dialogue: false,
            current_speaker: String::new(),
            detected_dialogues: Vec::new(),
            last_used_segment_by_speaker: HashMap::new(),
            quote_char: None,
            quote_pairs: quote_pairs_map(),
            left_quotes: left_quotes(),
            waiting_for_response_start: false,
            speaker_pattern: Regex::new(r"\*\*([^*]+)\*\*[：:][\s\\n]*$").unwrap(),
        }
    }
    
    /// 设置对话检测回调
    pub fn with_callback(mut self, callback: OnDialogueDetected) -> Self {
        self.on_dialogue_detected = Some(callback);
        self
    }
    
    /// 设置对话检测回调
    pub fn set_callback(&mut self, callback: OnDialogueDetected) {
        self.on_dialogue_detected = Some(callback);
    }
    
    /// 重置解析器状态
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.tts_config_complete = false;
        self.tts_config = None;
        self.segments_map.clear();
        self.is_nsfw = false;
        self.default_segment = None;
        self.segments_list.clear();
        self.current_segment_index = 0;
        self.in_response = false;
        self.response_buffer.clear();
        self.dialogue_buffer.clear();
        self.in_dialogue = false;
        self.current_speaker.clear();
        self.detected_dialogues.clear();
        self.last_used_segment_by_speaker.clear();
        self.quote_char = None;
        self.waiting_for_response_start = false;
    }
    
    /// 喂入新的数据块
    /// 
    /// # Arguments
    /// * `chunk` - 新的数据块
    /// 
    /// # Returns
    /// 用于显示的 response 内容（与 StreamingResponseParser 兼容）
    pub fn feed(&mut self, chunk: &str) -> String {
        let mut display_content = String::new();
        
        for char in chunk.chars() {
            self.buffer.push(char);
            
            // 阶段1: 等待 tts_config 完成（检测 "response": 的出现）
            if !self.tts_config_complete {
                if self.buffer.ends_with("\"response\":") {
                    self.parse_tts_config();
                    self.tts_config_complete = true;
                    self.in_response = false;
                    self.waiting_for_response_start = true;
                    info!("tts_config 解析完成，segments: {}", self.segments_map.len());
                }
                continue;
            }
            
            // 等待 response 字符串的开始引号
            if self.waiting_for_response_start {
                if char == '"' {
                    self.in_response = true;
                    self.waiting_for_response_start = false;
                }
                continue;
            }
            
            // 阶段2: 流式解析 response 内容
            if self.in_response {
                // 处理转义字符
                if !self.response_buffer.is_empty() && self.response_buffer.ends_with('\\') {
                    let len = self.response_buffer.len();
                    match char {
                        'n' => {
                            self.response_buffer.truncate(len - 1);
                            self.response_buffer.push('\n');
                            display_content.push('\n');
                            self.process_char('\n');
                        }
                        '"' => {
                            // 转义引号处理
                            let buffer_without_backslash = self.response_buffer[..len - 1].to_string();
                            self.response_buffer = buffer_without_backslash.clone();
                            self.response_buffer.push('"');
                            display_content.push('"');
                            
                            // 检查是否是对话开始
                            if self.left_quotes.contains(&'"') && !self.in_dialogue {
                                if let Some(caps) = self.speaker_pattern.captures(&buffer_without_backslash) {
                                    if let Some(m) = caps.get(1) {
                                        self.current_speaker = m.as_str().to_string();
                                        self.in_dialogue = true;
                                        self.quote_char = Some('"');
                                        self.dialogue_buffer.clear();
                                        continue;
                                    }
                                }
                            }
                            
                            // 如果在对话中，检查是否是对话结束
                            if self.in_dialogue {
                                let expected_right = self.quote_pairs.get(&self.quote_char.unwrap_or('"')).unwrap_or(&'"');
                                if '"' == *expected_right || ('"' == '"' && self.quote_char == Some('"')) {
                                    self.emit_dialogue();
                                    self.in_dialogue = false;
                                    self.quote_char = None;
                                    self.dialogue_buffer.clear();
                                    continue;
                                }
                                self.dialogue_buffer.push('"');
                            }
                        }
                        '\\' => {
                            self.response_buffer.truncate(len - 1);
                            self.response_buffer.push('\\');
                            display_content.push('\\');
                        }
                        't' => {
                            self.response_buffer.truncate(len - 1);
                            self.response_buffer.push('\t');
                            display_content.push('\t');
                        }
                        _ => {
                            self.response_buffer.push(char);
                            display_content.push(char);
                        }
                    }
                    continue;
                }
                
                // 检测 response 字符串结束（未转义的引号）
                if char == '"' && !self.in_dialogue {
                    self.in_response = false;
                    info!("response 解析完成，共检测到 {} 段对话", self.detected_dialogues.len());
                    continue;
                }
                
                // 反斜杠先存入，等下一个字符判断是否转义
                if char == '\\' {
                    self.response_buffer.push(char);
                    continue;
                }
                
                self.response_buffer.push(char);
                display_content.push(char);
                self.process_char(char);
            }
        }
        
        display_content
    }
    
    /// 从 buffer 中解析 tts_config
    fn parse_tts_config(&mut self) {
        // 找到 tts_config 的开始位置
        let tts_start = match self.buffer.find("\"tts_config\"") {
            Some(pos) => pos,
            None => {
                warn!("❌ 未找到 tts_config 字段");
                return;
            },
        };
        
        // 找到 tts_config 后面的冒号和第一个 {
        let after_key = &self.buffer[tts_start + "\"tts_config\"".len()..];
        let colon_pos = match after_key.find(':') {
            Some(pos) => pos,
            None => {
                warn!("❌ tts_config 后未找到冒号");
                return;
            },
        };
        
        let after_colon = &after_key[colon_pos + 1..];
        let brace_offset = match after_colon.find('{') {
            Some(pos) => pos,
            None => {
                warn!("❌ tts_config 后未找到开始花括号");
                return;
            },
        };
        
        let brace_start = tts_start + "\"tts_config\"".len() + colon_pos + 1 + brace_offset;
        
        // 使用 serde_json 的流式解析器来找到正确的 JSON 边界
        // 这比手动括号匹配更可靠，因为它正确处理所有 JSON 边界情况
        let remaining = &self.buffer[brace_start..];
        
        // 显示要解析的 JSON 片段（截取前 200 字符）
        let preview_len = remaining.len().min(200);
        info!("📋 准备解析 tts_config JSON (前{}字符): {}", preview_len, &remaining[..preview_len]);
        
        // 尝试使用 serde_json 直接解析
        // serde_json::from_str 会在遇到有效 JSON 结束后停止
        let mut deserializer = serde_json::Deserializer::from_str(remaining).into_iter::<TtsConfig>();
        
        match deserializer.next() {
            Some(Ok(raw_config)) => {
                info!("📋 tts_config 原始解析结果:");
                info!("   raw.tts_config 嵌套: {:?}", raw_config.tts_config.is_some());
                info!("   raw.segments: {:?}", raw_config.segments.as_ref().map(|s| s.len()));
                
                let config = raw_config.unwrap_nested();
                
                info!("📋 tts_config 展开后:");
                info!("   segments 数量: {:?}", config.segments.as_ref().map(|s| s.len()));
                
                self.is_nsfw = config.is_nsfw.unwrap_or(false);
                
                info!("📋 tts_config 解析成功:");
                info!("   is_nsfw: {}", self.is_nsfw);
                info!("   speaker: {:?}", config.speaker);
                info!("   gender: {:?}", config.gender);
                info!("   emotion: {:?}", config.emotion);
                info!("   emotion_vector: {:?}", config.emotion_vector);
                info!("   segments 数量: {:?}", config.segments.as_ref().map(|s| s.len()));
                
                if let Some(segments) = &config.segments {
                    for (i, seg) in segments.iter().enumerate() {
                        info!("   segment[{}]: speaker={:?}, gender={:?}, text_marker={:?}", 
                              i, seg.speaker, seg.gender, seg.text_marker);
                        if let Some(ref vec) = seg.emotion_vector {
                            let vec_parts: Vec<String> = vec.iter()
                                .filter(|(_, v)| **v > 0.0)
                                .map(|(k, v)| format!("{}={:.2}", k, v))
                                .collect();
                            info!("      emotion_vector: {}", vec_parts.join(", "));
                        }
                    }
                }
                
                // 建立 text_marker -> segment 映射，同时存入有序列表
                if let Some(segments) = &config.segments {
                    for seg in segments {
                        // 存入有序列表（用于顺序匹配）
                        self.segments_list.push(seg.clone());
                        
                        // 存入 text_marker 映射（用于精确匹配）
                        if let Some(marker) = &seg.text_marker {
                            if !marker.is_empty() {
                                self.segments_map.insert(marker.clone(), seg.clone());
                                info!("   ✅ 添加 segment 映射: text_marker=\"{}\"", marker);
                            }
                        }
                    }
                    info!("   📋 segments_list 大小: {}", self.segments_list.len());
                }
                
                // 如果没有 segments，使用单一 speaker 格式作为默认配置
                if config.segments.is_none() || config.segments.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
                    self.default_segment = Some(TtsSegment {
                        speaker: config.speaker.clone(),
                        gender: config.gender.clone(),
                        emotion: config.emotion.clone(),
                        emotion_description: config.emotion_description.clone(),
                        emotion_vector: config.emotion_vector.clone(),
                        speed: config.speed,
                        skip_tts: Some(false),
                        text_marker: None,
                    });
                    info!("解析 tts_config: 单一 speaker 模式, speaker={:?}, gender={:?}", 
                          config.speaker, config.gender);
                } else {
                    info!("解析 tts_config: is_nsfw={}, segments_map={}, segments_list={}", 
                          self.is_nsfw, self.segments_map.len(), self.segments_list.len());
                }
                
                self.tts_config = Some(config);
            }
            Some(Err(e)) => {
                warn!("tts_config 解析失败: {}", e);
                // 输出更多调试信息
                let preview: String = remaining.chars().take(500).collect();
                warn!("   JSON 预览: {}...", preview);
            }
            None => {
                warn!("tts_config 解析失败: 没有有效的 JSON 数据");
            }
        }
    }
    
    /// 处理单个字符，检测对话边界
    fn process_char(&mut self, char: char) {
        // 检测引号开始 - 只有紧跟在 **角色**： 之后才识别为对话
        if self.left_quotes.contains(&char) && !self.in_dialogue {
            // 检查 response_buffer 末尾是否是 **角色名**：
            if let Some(caps) = self.speaker_pattern.captures(&self.response_buffer) {
                if let Some(m) = caps.get(1) {
                    self.current_speaker = m.as_str().to_string();
                    self.in_dialogue = true;
                    self.quote_char = Some(char);
                    self.dialogue_buffer.clear();
                    return;
                }
            }
            // 否则跳过这个引号（非角色对话）
            return;
        }
        
        // 检测引号结束
        if self.in_dialogue {
            if let Some(quote) = self.quote_char {
                let expected_right = self.quote_pairs.get(&quote).unwrap_or(&'"');
                if char == *expected_right || (char == '"' && quote == '"') {
                    // 对话结束，触发回调
                    self.emit_dialogue();
                    self.in_dialogue = false;
                    self.quote_char = None;
                    self.dialogue_buffer.clear();
                    return;
                }
            }
            
            // 累积对话内容
            self.dialogue_buffer.push(char);
        }
    }
    
    /// 发送检测到的对话
    fn emit_dialogue(&mut self) {
        let dialogue_text = self.dialogue_buffer.trim().to_string();
        if dialogue_text.is_empty() {
            return;
        }
        
        info!("🔍 对话匹配: \"{}...\"", dialogue_text.chars().take(30).collect::<String>());
        info!("   segments_map 大小: {}", self.segments_map.len());
        if !self.segments_map.is_empty() {
            for (marker, _) in &self.segments_map {
                info!("   可用 marker: \"{}\"", marker);
            }
        }
        
        // 查找匹配的 segment（通过 text_marker）
        let mut matched_segment: Option<TtsSegment> = None;
        let mut matched_marker: Option<String> = None;
        
        for (marker, seg) in &self.segments_map {
            // text_marker 是对话的开头部分
            if dialogue_text.starts_with(marker) || dialogue_text.contains(marker) {
                matched_segment = Some(seg.clone());
                matched_marker = Some(marker.clone());
                info!("   ✅ text_marker 匹配成功: \"{}\"", marker);
                break;
            }
        }
        
        // 更宽松的匹配（前几个字）
        if matched_segment.is_none() {
            for (marker, seg) in &self.segments_map {
                let min_len = marker.len().min(dialogue_text.len()).min(5);
                if min_len >= 3 {
                    let marker_prefix: String = marker.chars().take(min_len).collect();
                    let text_prefix: String = dialogue_text.chars().take(min_len).collect();
                    if marker_prefix == text_prefix {
                        matched_segment = Some(seg.clone());
                        matched_marker = Some(marker.clone());
                        break;
                    }
                }
            }
        }
        
        // 移除已使用的 segment
        if let Some(marker) = &matched_marker {
            self.segments_map.remove(marker);
        }
        
        // 使用默认配置（单一 speaker 模式）
        if matched_segment.is_none() {
            if let Some(default) = &self.default_segment {
                matched_segment = Some(default.clone());
                info!("   ✅ 使用默认 segment（单一 speaker 模式）");
            }
        }
        
        // 顺序匹配：使用 segments_list 中的下一个 segment
        if matched_segment.is_none() && self.current_segment_index < self.segments_list.len() {
            let seg = self.segments_list[self.current_segment_index].clone();
            self.current_segment_index += 1;
            info!("   ✅ 顺序匹配: 使用 segments_list[{}], speaker={:?}, gender={:?}", 
                  self.current_segment_index - 1, seg.speaker, seg.gender);
            matched_segment = Some(seg);
        }
        
        // 回退到同角色的下一个 segment
        if matched_segment.is_none() && !self.current_speaker.is_empty() {
            let speaker = self.current_speaker.clone();
            let mut found_marker = None;
            for (marker, seg) in &self.segments_map {
                if seg.speaker.as_ref().map(|s| s == &speaker).unwrap_or(false) {
                    matched_segment = Some(seg.clone());
                    found_marker = Some(marker.clone());
                    info!("   ✅ 回退匹配: 使用同角色({})的下一个 segment", speaker);
                    break;
                }
            }
            if let Some(marker) = found_marker {
                self.segments_map.remove(&marker);
            }
        }
        
        // 回退到同角色上一个使用过的 segment
        if matched_segment.is_none() && !self.current_speaker.is_empty() {
            if let Some(seg) = self.last_used_segment_by_speaker.get(&self.current_speaker) {
                matched_segment = Some(seg.clone());
                info!("   ✅ 回退匹配: 使用{}的上一个 segment（容错）", self.current_speaker);
            }
        }
        
        // 记录当前角色使用的 segment
        if let Some(seg) = &matched_segment {
            if !self.current_speaker.is_empty() {
                self.last_used_segment_by_speaker.insert(self.current_speaker.clone(), seg.clone());
            }
        }
        
        // 构建对话段落
        let segment = if let Some(matched) = matched_segment {
            // 详细记录匹配到的情感参数
            info!("   🎭 情感参数识别结果:");
            info!("      gender: {}", matched.gender.as_ref().map(|g| g.as_str()).unwrap_or("(未设置)"));
            info!("      emotion: {}", matched.emotion.as_ref().map(|e| e.as_str()).unwrap_or("(未设置)"));
            if let Some(ref vec) = matched.emotion_vector {
                let vec_parts: Vec<String> = vec.iter()
                    .filter(|(_, v)| **v > 0.0)
                    .map(|(k, v)| format!("{}={:.2}", k, v))
                    .collect();
                if !vec_parts.is_empty() {
                    info!("      emotion_vector: {}", vec_parts.join(", "));
                } else {
                    info!("      emotion_vector: (全零或空)");
                }
            } else {
                info!("      emotion_vector: (未设置)");
            }
            if let Some(ref desc) = matched.emotion_description {
                if !desc.is_empty() {
                    info!("      emotion_description: {}", desc);
                }
            }
            if let Some(speed) = matched.speed {
                if (speed - 1.0).abs() > 0.01 {
                    info!("      speed: {:.2}", speed);
                }
            }
            DialogueSegment::from_segment(dialogue_text.clone(), self.current_speaker.clone(), &matched, self.is_nsfw)
        } else {
            warn!("   ❌ 未匹配到任何 segment，使用默认空配置");
            warn!("      gender: (未设置)");
            warn!("      emotion_vector: (未设置)");
            DialogueSegment {
                text: dialogue_text.clone(),
                speaker: self.current_speaker.clone(),
                ..Default::default()
            }
        };
        
        self.detected_dialogues.push(segment.clone());
        
        // 跳过旁白
        if segment.skip_tts {
            info!("跳过旁白: {}...", &dialogue_text.chars().take(30).collect::<String>());
            return;
        }
        
        // 触发回调
        if let Some(callback) = &self.on_dialogue_detected {
            info!("检测到对话: [{}] {}...", segment.speaker, 
                  dialogue_text.chars().take(40).collect::<String>());
            if let Some(ref vec) = segment.emotion_vector {
                let vec_parts: Vec<String> = vec.iter()
                    .filter(|(_, v)| **v > 0.0)
                    .map(|(k, v)| format!("{}={:.2}", k, v))
                    .collect();
                if !vec_parts.is_empty() {
                    info!("   情感向量: {}", vec_parts.join(", "));
                }
            }
            if !segment.emotion_description.is_empty() {
                info!("   情感描述: {}", segment.emotion_description);
            }
            if (segment.speed - 1.0).abs() > 0.01 {
                info!("   语速: {:.2}", segment.speed);
            }
            callback(segment);
        }
    }
    
    /// 获取所有检测到的对话
    pub fn get_detected_dialogues(&self) -> &[DialogueSegment] {
        &self.detected_dialogues
    }
    
    /// 获取解析出的 tts_config
    pub fn get_tts_config(&self) -> Option<&TtsConfig> {
        self.tts_config.as_ref()
    }
}

impl Default for StreamingTTSParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    
    #[test]
    fn test_parse_simple_dialogue() {
        let detected = Arc::new(Mutex::new(Vec::new()));
        let detected_clone = detected.clone();
        
        let mut parser = StreamingTTSParser::new();
        parser.set_callback(Box::new(move |seg| {
            detected_clone.lock().unwrap().push(seg);
        }));
        
        let json = r#"{"tts_config": {"speaker": "Alice"}, "response": "**Alice**：「你好！」"}"#;
        parser.feed(json);
        
        let dialogues = detected.lock().unwrap();
        assert_eq!(dialogues.len(), 1);
        assert_eq!(dialogues[0].speaker, "Alice");
        assert_eq!(dialogues[0].text, "你好！");
    }
}
