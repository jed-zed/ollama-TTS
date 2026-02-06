//! 对话历史压缩模块
//!
//! 当 ConversationHistory 超过指定消息数时，自动压缩旧消息为摘要。
//! 对应 Python 后端的 history_compressor.py。

use std::fs;
use std::path::{Path, PathBuf};
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn, error, debug};

use crate::models::config::{HistoryCompressionConfig, DEFAULT_COMPRESSION_PROMPT};

/// 压缩状态信息（与前端 MemoryStatus 类型匹配）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionStatus {
    /// 角色 ID（从文件名提取）
    pub character_id: String,
    /// 角色名称
    pub character_name: String,
    /// 玩家名称
    pub player_name: String,
    /// 文件路径
    pub file_path: String,
    /// 消息数量
    pub message_count: i32,
    /// 是否需要压缩
    pub needs_compression: bool,
    /// 最后压缩时间（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_compressed: Option<String>,
}

/// 压缩结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    /// 是否成功
    pub success: bool,
    /// 状态消息
    pub message: String,
    /// 压缩前消息数
    pub before_count: Option<i32>,
    /// 压缩后消息数
    pub after_count: Option<i32>,
}

/// 历史压缩器
pub struct HistoryCompressor {
    /// 压缩配置
    config: HistoryCompressionConfig,
    /// API URL
    api_url: String,
    /// API Key
    api_key: String,
    /// 模型名称
    model_name: String,
    /// HTTP 客户端
    client: reqwest::Client,
}

impl HistoryCompressor {
    /// 创建新的压缩器
    pub fn new(
        config: HistoryCompressionConfig,
        api_url: String,
        api_key: String,
        model_name: String,
    ) -> Self {
        Self {
            config,
            api_url,
            api_key,
            model_name,
            client: reqwest::Client::new(),
        }
    }

    /// 更新配置
    pub fn update_config(&mut self, config: HistoryCompressionConfig) {
        self.config = config;
    }

    /// 更新 API 配置
    pub fn update_api_config(&mut self, api_url: String, api_key: String, model_name: String) {
        self.api_url = api_url;
        self.api_key = api_key;
        self.model_name = model_name;
    }

    /// 计算消息数量（每2条记录算1个message）
    fn count_messages(conversation_history: &[String]) -> i32 {
        (conversation_history.len() / 2) as i32
    }

    /// 扫描指定目录下的所有 JSON 文件
    pub fn scan_json_files(&self) -> Vec<PathBuf> {
        let save_data_path = &self.config.save_data_path;
        if save_data_path.is_empty() {
            warn!("未设置 save_data_path");
            return Vec::new();
        }

        let path = Path::new(save_data_path);
        if !path.exists() {
            warn!("路径不存在: {}", save_data_path);
            return Vec::new();
        }

        let backup_dir = &self.config.backup_dir;
        let mut json_files = Vec::new();

        Self::scan_directory_recursive(path, backup_dir, &mut json_files);
        json_files
    }

    /// 递归扫描目录
    fn scan_directory_recursive(dir: &Path, exclude_dir: &str, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                
                if path.is_dir() {
                    if path.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n != exclude_dir)
                        .unwrap_or(true)
                    {
                        Self::scan_directory_recursive(&path, exclude_dir, files);
                    }
                } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    files.push(path);
                }
            }
        }
    }

    /// 加载角色 JSON 数据
    pub fn load_character_data(&self, file_path: &Path) -> Option<Value> {
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                error!("读取文件失败 {:?}: {}", file_path, e);
                return None;
            }
        };

        let data: Value = match serde_json::from_str(&content) {
            Ok(d) => d,
            Err(e) => {
                error!("JSON 解析失败 {:?}: {}", file_path, e);
                return None;
            }
        };

        if !data.is_object() {
            debug!("跳过非角色文件 {:?}: 根元素不是对象", file_path);
            return None;
        }

        if data.get("ConversationHistory").is_none() {
            debug!("跳过文件 {:?}: 没有 ConversationHistory 字段", file_path);
            return None;
        }

        Some(data)
    }

    /// 保存角色 JSON 数据
    pub fn save_character_data(&self, file_path: &Path, data: &Value) -> bool {
        match serde_json::to_string_pretty(data) {
            Ok(content) => {
                if let Err(e) = fs::write(file_path, content) {
                    error!("保存文件失败 {:?}: {}", file_path, e);
                    return false;
                }
                true
            }
            Err(e) => {
                error!("序列化失败: {}", e);
                false
            }
        }
    }

    /// 备份文件
    pub fn backup_file(&self, file_path: &Path) -> Option<PathBuf> {
        let file_dir = file_path.parent()?;
        let backup_path = file_dir.join(&self.config.backup_dir);

        if !backup_path.exists() {
            if let Err(e) = fs::create_dir_all(&backup_path) {
                error!("创建备份目录失败: {}", e);
                return None;
            }
        }

        let file_name = file_path.file_name()?.to_str()?;
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let backup_file_name = format!("{}_{}", timestamp, file_name);
        let backup_file_path = backup_path.join(backup_file_name);

        match fs::copy(file_path, &backup_file_path) {
            Ok(_) => {
                info!("已备份: {:?}", backup_file_path);
                Some(backup_file_path)
            }
            Err(e) => {
                error!("备份失败 {:?}: {}", file_path, e);
                None
            }
        }
    }

    /// 调用 AI 进行压缩
    pub async fn call_ai_for_compression(
        &self,
        conversation_content: &str,
        character_name: &str,
        player_name: &str,
        message_count: i32,
    ) -> Result<String, String> {
        let prompt_template = if self.config.custom_prompt.trim().is_empty() {
            DEFAULT_COMPRESSION_PROMPT
        } else {
            &self.config.custom_prompt
        };

        let prompt = prompt_template
            .replace("{character_name}", character_name)
            .replace("{player_name}", player_name)
            .replace("{message_count}", &message_count.to_string())
            .replace("{conversation_content}", conversation_content);

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json; charset=utf-8".parse().unwrap(),
        );
        if !self.api_key.is_empty() {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.api_key).parse().unwrap(),
            );
        }

        let payload = serde_json::json!({
            "model": self.model_name,
            "messages": [
                {"role": "user", "content": prompt}
            ],
            "stream": false,
            "generationConfig": {
                "maxOutputTokens": 16384,
                "temperature": 0.3,
                "topP": 0.9
            },
            "safetySettings": [
                {"category": "HARM_CATEGORY_HARASSMENT", "threshold": "BLOCK_NONE"},
                {"category": "HARM_CATEGORY_HATE_SPEECH", "threshold": "BLOCK_NONE"},
                {"category": "HARM_CATEGORY_SEXUALLY_EXPLICIT", "threshold": "BLOCK_NONE"},
                {"category": "HARM_CATEGORY_DANGEROUS_CONTENT", "threshold": "BLOCK_NONE"}
            ]
        });

        info!("正在调用 AI 压缩，内容长度: {} 字符", conversation_content.len());

        let response = self.client
            .post(&self.api_url)
            .headers(headers)
            .json(&payload)
            .timeout(std::time::Duration::from_secs(300))
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("API 返回错误状态: {}", response.status()));
        }

        let data: Value = response.json().await
            .map_err(|e| format!("解析响应失败: {}", e))?;

        if let Some(choices) = data.get("choices").and_then(|v| v.as_array()) {
            if let Some(first_choice) = choices.first() {
                let finish_reason = first_choice.get("finish_reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if finish_reason == "length" || finish_reason == "MAX_TOKENS" {
                    return Err("AI 响应被截断（达到最大 token 限制）".to_string());
                }

                if finish_reason == "content_filter" || finish_reason == "SAFETY" {
                    return Err("AI 响应被内容安全过滤器拦截".to_string());
                }

                if let Some(content) = first_choice
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                {
                    return Ok(content.to_string());
                }
            }
        }

        if let Some(candidates) = data.get("candidates").and_then(|v| v.as_array()) {
            if let Some(first_candidate) = candidates.first() {
                let finish_reason = first_candidate.get("finishReason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if finish_reason == "MAX_TOKENS" {
                    return Err("AI 响应被截断（达到最大 token 限制）".to_string());
                }

                if finish_reason == "SAFETY" {
                    return Err("AI 响应被内容安全过滤器拦截".to_string());
                }

                if let Some(content) = first_candidate
                    .get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|part| part.get("text"))
                    .and_then(|t| t.as_str())
                {
                    return Ok(content.to_string());
                }
            }
        }

        Err("无法解析 AI 响应格式".to_string())
    }

    /// 压缩对话历史
    pub async fn compress_conversation_history(
        &self,
        data: &mut Value,
    ) -> Result<String, String> {
        let conversation_history = data
            .get("ConversationHistory")
            .and_then(|v| v.as_array())
            .ok_or("对话历史为空")?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();

        if conversation_history.is_empty() {
            return Err("对话历史为空".to_string());
        }

        let mut existing_summaries: Vec<String> = Vec::new();
        let mut uncompressed_messages: Vec<String> = Vec::new();

        for item in &conversation_history {
            if item.starts_with("System: [历史摘要") {
                existing_summaries.push(item.clone());
            } else {
                uncompressed_messages.push(item.clone());
            }
        }

        if uncompressed_messages.len() <= 20 {
            return Err("未压缩的对话历史太短，无需压缩".to_string());
        }

        let mut lines_to_compress = (self.config.compress_count * 2) as usize;
        if uncompressed_messages.len() < lines_to_compress {
            lines_to_compress = uncompressed_messages.len().saturating_sub(10);
            if lines_to_compress == 0 {
                return Err("未压缩的对话历史太短，无需压缩".to_string());
            }
        }

        let to_compress: Vec<_> = uncompressed_messages[..lines_to_compress].to_vec();
        let to_keep: Vec<_> = uncompressed_messages[lines_to_compress..].to_vec();

        let character_name = data.get("Name")
            .and_then(|v| v.as_str())
            .unwrap_or("角色");

        let player_name = data.get("PlayerInfo")
            .and_then(|pi| pi.get("RealName").or_else(|| pi.get("ClaimedName")))
            .and_then(|v| v.as_str())
            .unwrap_or("玩家");

        let conversation_text = to_compress.join("\n");
        let message_count = to_compress.len() as i32;

        let summary = self.call_ai_for_compression(
            &conversation_text,
            character_name,
            player_name,
            message_count,
        ).await?;

        let summary_entry = format!(
            "System: [历史摘要 - 压缩了{}条消息，共{}轮对话]\n\n{}",
            message_count,
            message_count / 2,
            summary
        );

        existing_summaries.push(summary_entry);

        let to_keep_len = to_keep.len();
        let mut new_history: Vec<Value> = existing_summaries
            .into_iter()
            .map(|s| Value::String(s))
            .collect();
        new_history.extend(to_keep.into_iter().map(|s| Value::String(s)));

        data["ConversationHistory"] = Value::Array(new_history);

        let result_msg = format!(
            "压缩完成: {}条 -> 1条摘要, 保留{}条对话",
            message_count,
            to_keep_len
        );
        info!("{}", result_msg);

        Ok(result_msg)
    }

    /// 检查并压缩单个文件
    pub async fn check_and_compress_file(
        &self,
        file_path: &Path,
        force: bool,
    ) -> CompressionResult {
        if !self.config.enabled && !force {
            return CompressionResult {
                success: false,
                message: "压缩功能未启用".to_string(),
                before_count: None,
                after_count: None,
            };
        }

        let mut data = match self.load_character_data(file_path) {
            Some(d) => d,
            None => {
                return CompressionResult {
                    success: false,
                    message: format!("无法读取文件: {:?}", file_path),
                    before_count: None,
                    after_count: None,
                };
            }
        };

        let conversation_history: Vec<String> = data
            .get("ConversationHistory")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect())
            .unwrap_or_default();

        let current_messages = Self::count_messages(&conversation_history);
        let character_name = data.get("Name")
            .and_then(|v| v.as_str())
            .unwrap_or("未知")
            .to_string();

        if !force && current_messages <= self.config.max_messages {
            return CompressionResult {
                success: false,
                message: format!(
                    "[{}] 消息数 {} 未超过阈值 {}",
                    character_name, current_messages, self.config.max_messages
                ),
                before_count: Some(current_messages),
                after_count: Some(current_messages),
            };
        }

        info!("[{}] 开始压缩: 当前 {} 条消息", character_name, current_messages);

        if self.config.backup_before_compress {
            if self.backup_file(file_path).is_none() {
                return CompressionResult {
                    success: false,
                    message: format!("[{}] 备份失败，取消压缩", character_name),
                    before_count: Some(current_messages),
                    after_count: None,
                };
            }
        }

        match self.compress_conversation_history(&mut data).await {
            Ok(msg) => {
                if self.save_character_data(file_path, &data) {
                    let new_history: Vec<String> = data
                        .get("ConversationHistory")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect())
                        .unwrap_or_default();
                    let new_messages = Self::count_messages(&new_history);

                    CompressionResult {
                        success: true,
                        message: format!("[{}] {}", character_name, msg),
                        before_count: Some(current_messages),
                        after_count: Some(new_messages),
                    }
                } else {
                    CompressionResult {
                        success: false,
                        message: format!("[{}] 保存失败", character_name),
                        before_count: Some(current_messages),
                        after_count: None,
                    }
                }
            }
            Err(e) => {
                CompressionResult {
                    success: false,
                    message: format!("[{}] 压缩失败: {}", character_name, e),
                    before_count: Some(current_messages),
                    after_count: None,
                }
            }
        }
    }

    /// 扫描并压缩所有需要压缩的文件
    pub async fn scan_and_compress_all(&self, force: bool) -> Vec<CompressionResult> {
        let json_files = self.scan_json_files();
        info!("扫描到 {} 个 JSON 文件", json_files.len());

        let mut results = Vec::new();
        for file_path in json_files {
            let result = self.check_and_compress_file(&file_path, force).await;
            if result.success {
                info!("{}", result.message);
            } else {
                debug!("{}", result.message);
            }
            results.push(result);
        }

        results
    }

    /// 获取所有文件的压缩状态
    pub fn get_compression_status(&self) -> Vec<CompressionStatus> {
        let json_files = self.scan_json_files();
        let mut results = Vec::new();

        for file_path in json_files {
            if let Some(data) = self.load_character_data(&file_path) {
                let conversation_history: Vec<String> = data
                    .get("ConversationHistory")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect())
                    .unwrap_or_default();

                let message_count = Self::count_messages(&conversation_history);
                
                let character_id = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let character_name = data.get("Name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知")
                    .to_string();

                let player_name = data.get("PlayerInfo")
                    .and_then(|pi| pi.get("RealName").or_else(|| pi.get("ClaimedName")))
                    .and_then(|v| v.as_str())
                    .unwrap_or("玩家")
                    .to_string();

                results.push(CompressionStatus {
                    character_id,
                    character_name,
                    player_name,
                    file_path: file_path.to_string_lossy().to_string(),
                    message_count,
                    needs_compression: message_count > self.config.max_messages,
                    last_compressed: None,
                });
            }
        }

        results.sort_by(|a, b| b.message_count.cmp(&a.message_count));
        results
    }

    /// 自动压缩检查（后台调用）
    pub async fn auto_compress_check(&self) {
        if !self.config.enabled {
            return;
        }

        if self.config.save_data_path.is_empty() {
            return;
        }

        let json_files = self.scan_json_files();

        for file_path in json_files {
            if let Some(data) = self.load_character_data(&file_path) {
                let conversation_history: Vec<String> = data
                    .get("ConversationHistory")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect())
                    .unwrap_or_default();

                let current_messages = Self::count_messages(&conversation_history);

                if current_messages > self.config.max_messages {
                    let character_name = data.get("Name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("未知");

                    info!(
                        "🔄 [自动压缩] {}: {} 条消息超过阈值 {}，开始压缩...",
                        character_name, current_messages, self.config.max_messages
                    );

                    let result = self.check_and_compress_file(&file_path, false).await;
                    if result.success {
                        info!("[自动压缩] {}", result.message);
                    } else {
                        warn!("[自动压缩] {}", result.message);
                    }

                    break;
                }
            }
        }
    }

    /// 获取配置
    pub fn get_config(&self) -> &HistoryCompressionConfig {
        &self.config
    }
}

impl Default for HistoryCompressor {
    fn default() -> Self {
        Self::new(
            HistoryCompressionConfig::default(),
            String::new(),
            String::new(),
            String::new(),
        )
    }
}
