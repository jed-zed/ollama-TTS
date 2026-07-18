//! 对话历史压缩模块
//!
//! 当 ConversationHistory 超过指定轮次数时，自动压缩旧消息为摘要。
//! 对应 Python 后端的 history_compressor.py。
//!
//! 核心业务规则：
//! 1. 轮次定义：1条玩家消息 + 后续所有NPC回复 = 1轮对话
//! 2. 压缩策略：保留最近N轮对话，压缩其余部分
//! 3. 熔断机制：待压缩区摘要比例超过阈值时终止压缩

use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

use crate::models::config::{
    HistoryCompressionConfig, DEFAULT_COMPRESSION_PROMPT, SUMMARY_MERGE_PROMPT,
};
use crate::services::{atomic_file, config_store::ApiTargetConfirmationGuard, http_client};

/// 对话历史分析结果（用于状态检测与阈值判定）
#[derive(Debug, Clone)]
pub struct HistoryAnalysis {
    /// 总计数（摘要数 + 对话轮次数）
    pub total_count: i32,
    /// 摘要数量
    pub summary_count: i32,
    /// 对话轮次数（不含摘要）
    pub round_count: i32,
}

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
    /// 消息数量（摘要数 + 对话轮次数）
    pub message_count: i32,
    /// 摘要数量
    pub summary_count: i32,
    /// 是否需要压缩
    pub needs_compression: bool,
    /// 最后压缩时间（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_compressed: Option<String>,
}

/// 压缩结果状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressionResultStatus {
    /// 成功压缩
    Success,
    /// 跳过（未达阈值/功能未启用/对话为空等）
    Skipped,
    /// 忽略（非角色文件，无需处理）
    Ignored,
    /// 失败（尝试压缩但出错，如 API 错误、保存失败等）
    Failed,
}

/// 压缩操作结果（内部使用）
#[derive(Debug)]
pub enum CompressOutcome {
    /// 成功压缩，返回结果消息
    Success(String),
    /// 跳过压缩，返回跳过原因
    Skip(String),
    /// 压缩失败，返回错误信息
    Error(String),
}

/// 压缩结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    /// 结果状态
    pub status: CompressionResultStatus,
    /// 是否成功（兼容旧代码，status == Success 时为 true）
    pub success: bool,
    /// 状态消息
    pub message: String,
    /// 压缩前消息数
    pub before_count: Option<i32>,
    /// 压缩后消息数
    pub after_count: Option<i32>,
}

impl CompressionResult {
    /// 创建成功结果
    pub fn success(message: String, before: i32, after: i32) -> Self {
        Self {
            status: CompressionResultStatus::Success,
            success: true,
            message,
            before_count: Some(before),
            after_count: Some(after),
        }
    }

    /// 创建跳过结果
    pub fn skipped(message: String, count: Option<i32>) -> Self {
        Self {
            status: CompressionResultStatus::Skipped,
            success: false,
            message,
            before_count: count,
            after_count: count,
        }
    }

    /// 创建失败结果
    pub fn failed(message: String, before: Option<i32>) -> Self {
        Self {
            status: CompressionResultStatus::Failed,
            success: false,
            message,
            before_count: before,
            after_count: None,
        }
    }

    /// 创建忽略结果（非角色文件）
    pub fn ignored(message: String) -> Self {
        Self {
            status: CompressionResultStatus::Ignored,
            success: false,
            message,
            before_count: None,
            after_count: None,
        }
    }
}

/// 历史压缩器
#[derive(Clone)]
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
    /// 共享 LLM 目标确认保护
    api_target_guard: ApiTargetConfirmationGuard,
}

impl HistoryCompressor {
    /// 创建新的压缩器
    pub fn new(
        config: HistoryCompressionConfig,
        api_url: String,
        api_key: String,
        model_name: String,
        api_target_guard: ApiTargetConfirmationGuard,
    ) -> Self {
        Self {
            config,
            api_url,
            api_key,
            model_name,
            client: http_client::create_http_client(),
            api_target_guard,
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

    /// 判断是否需要压缩（统一判定逻辑）
    ///
    /// 返回 (should_compress, skip_reason)
    /// - should_compress: 是否需要压缩
    /// - skip_reason: 如果不需要压缩，跳过的原因
    ///
    /// 注意：此方法用于实际执行压缩时的判定，会检查 enabled 配置
    /// 使用 total_count（摘要数 + 对话轮次数）作为判定基准
    pub fn should_compress(
        &self,
        analysis: &HistoryAnalysis,
        force: bool,
    ) -> (bool, Option<String>) {
        debug!(
            "📋 should_compress 检查: total={}, summaries={}, rounds={}, max={}, enabled={}, force={}",
            analysis.total_count, analysis.summary_count, analysis.round_count,
            self.config.max_messages, self.config.enabled, force
        );

        if !self.config.enabled && !force {
            return (false, Some("压缩功能未启用".to_string()));
        }

        if analysis.total_count <= self.config.max_messages && !force {
            return (
                false,
                Some(format!(
                    "总量 {}（{}摘要+{}轮对话）未超过阈值 {}",
                    analysis.total_count,
                    analysis.summary_count,
                    analysis.round_count,
                    self.config.max_messages
                )),
            );
        }

        (true, None)
    }

    /// 判断是否超过压缩阈值（基于总量，不检查 enabled）
    ///
    /// 用于状态检测，显示哪些文件"需要压缩"（即使功能未启用）
    /// 使用 total_count（摘要数 + 对话轮次数）作为判定基准
    pub fn exceeds_threshold(&self, analysis: &HistoryAnalysis) -> bool {
        let result = analysis.total_count > self.config.max_messages;
        debug!(
            "📊 阈值检查: Total({}) > Max({})? {} [summaries={}, rounds={}]",
            analysis.total_count,
            self.config.max_messages,
            if result {
                "YES -> 需要压缩"
            } else {
                "NO -> 无需压缩"
            },
            analysis.summary_count,
            analysis.round_count
        );
        result
    }

    /// 判断消息是否来自玩家
    fn is_player_message(message: &str, player_name: &str) -> bool {
        if message.starts_with("System:") || message.starts_with("[历史摘要") {
            return false;
        }

        let player_prefixes = ["Player:", "玩家:", "User:", "用户:", "player:", "user:"];

        if !player_name.is_empty() && message.starts_with(&format!("{}:", player_name)) {
            return true;
        }

        for prefix in player_prefixes {
            if message.starts_with(prefix) {
                return true;
            }
        }

        false
    }

    /// 判断消息是否是历史摘要
    fn is_summary_message(message: &str) -> bool {
        message.starts_with("System: [历史摘要") || message.starts_with("[历史摘要")
    }

    /// 将对话历史解析为轮次列表
    /// 定义：1轮 = 1条玩家消息 + 后续所有NPC回复（直到下一条玩家消息）
    fn parse_conversation_rounds(
        conversation_history: &[String],
        player_name: &str,
    ) -> (Vec<Vec<String>>, Vec<String>) {
        let mut rounds: Vec<Vec<String>> = Vec::new();
        let mut current_round: Vec<String> = Vec::new();
        let mut summaries: Vec<String> = Vec::new();

        for item in conversation_history {
            if Self::is_summary_message(item) {
                summaries.push(item.clone());
                continue;
            }

            if Self::is_player_message(item, player_name) {
                if !current_round.is_empty() {
                    rounds.push(current_round);
                }
                current_round = vec![item.clone()];
            } else {
                if current_round.is_empty() {
                    current_round = vec![item.clone()];
                } else {
                    current_round.push(item.clone());
                }
            }
        }

        if !current_round.is_empty() {
            rounds.push(current_round);
        }

        (rounds, summaries)
    }

    /// 计算对话轮次数（基于语义结构）
    fn count_rounds(conversation_history: &[String], player_name: &str) -> i32 {
        let (rounds, _) = Self::parse_conversation_rounds(conversation_history, player_name);
        rounds.len() as i32
    }

    /// 分析对话历史，返回完整统计信息
    /// 用于状态检测，确保 message_count 包含摘要和对话轮次
    fn analyze_history(conversation_history: &[String], player_name: &str) -> HistoryAnalysis {
        let (rounds, summaries) =
            Self::parse_conversation_rounds(conversation_history, player_name);
        let summary_count = summaries.len() as i32;
        let round_count = rounds.len() as i32;
        HistoryAnalysis {
            total_count: summary_count + round_count,
            summary_count,
            round_count,
        }
    }

    /// 计算消息列表中摘要的比例
    fn calculate_summary_ratio(messages: &[String]) -> f32 {
        if messages.is_empty() {
            return 0.0;
        }

        let summary_count = messages
            .iter()
            .filter(|m| Self::is_summary_message(m))
            .count();

        summary_count as f32 / messages.len() as f32
    }

    /// 检查待压缩区域的健康度（防止重复压缩熔断机制）
    fn check_compression_health(
        messages_to_compress: &[String],
        threshold: f32,
    ) -> (bool, f32, String) {
        if messages_to_compress.is_empty() {
            return (false, 0.0, "待压缩区域为空".to_string());
        }

        let summary_ratio = Self::calculate_summary_ratio(messages_to_compress);

        if summary_ratio > threshold {
            return (
                false,
                summary_ratio,
                format!(
                    "⚠️ 过度压缩保护生效：待压缩区摘要比例 {:.1}% 超过阈值 {:.0}%，终止压缩",
                    summary_ratio * 100.0,
                    threshold * 100.0
                ),
            );
        }

        (
            true,
            summary_ratio,
            format!("健康检查通过：摘要比例 {:.1}%", summary_ratio * 100.0),
        )
    }

    /// 计算消息数量（兼容旧接口，现在基于轮次）
    fn count_messages(conversation_history: &[String]) -> i32 {
        (conversation_history.len() / 2) as i32
    }

    /// 扫描指定目录下的 JSON 文件（只扫描根目录，不进入子目录）
    /// 子目录通常是备份或弃用的文件，不应该被识别为角色档案
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

        let mut json_files = Vec::new();

        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let file_path = entry.path();
                if file_path.is_file() {
                    if file_path.extension().and_then(|e| e.to_str()) == Some("json") {
                        json_files.push(file_path);
                    }
                }
            }
        }

        json_files
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
                if let Err(e) = atomic_file::write(file_path, content.as_bytes()) {
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

    /// 备份文件到存档根目录的统一备份文件夹
    pub fn backup_file(&self, file_path: &Path) -> Option<PathBuf> {
        let save_data_path = &self.config.save_data_path;
        if save_data_path.is_empty() {
            error!("未设置 save_data_path，无法备份");
            return None;
        }

        let backup_path = Path::new(save_data_path).join(&self.config.backup_dir);

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

        match atomic_file::copy(file_path, &backup_file_path) {
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
        round_count: i32,
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
            .replace("{round_count}", &round_count.to_string())
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
            "stream": false
        });

        info!(
            "正在调用 AI 压缩，内容长度: {} 字符, 模型: {}",
            conversation_content.len(),
            self.model_name
        );
        info!("API URL: {}", self.api_url);

        self.api_target_guard.ensure_confirmed_for(&self.api_url)?;

        let response = self
            .client
            .post(&self.api_url)
            .headers(headers)
            .json(&payload)
            .timeout(std::time::Duration::from_secs(300))
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "无法读取错误响应".to_string());
            error!("API 返回错误状态: {}, 响应内容: {}", status, error_body);
            return Err(format!("API 返回错误状态: {} - {}", status, error_body));
        }

        let data: Value = response
            .json()
            .await
            .map_err(|e| format!("解析响应失败: {}", e))?;

        if let Some(choices) = data.get("choices").and_then(|v| v.as_array()) {
            if let Some(first_choice) = choices.first() {
                let finish_reason = first_choice
                    .get("finish_reason")
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
                let finish_reason = first_candidate
                    .get("finishReason")
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

    /// 调用 AI 合并多个摘要
    /// 当摘要数量过多时使用，精简亲密互动细节但保留关键信息
    pub async fn call_ai_for_summary_merge(
        &self,
        summaries: &[String],
        character_name: &str,
        player_name: &str,
    ) -> Result<String, String> {
        let summaries_content = summaries.join("\n\n---\n\n");
        let summary_count = summaries.len();

        let prompt = SUMMARY_MERGE_PROMPT
            .replace("{character_name}", character_name)
            .replace("{player_name}", player_name)
            .replace("{summary_count}", &summary_count.to_string())
            .replace("{summaries_content}", &summaries_content);

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
            "stream": false
        });

        info!(
            "正在调用 AI 合并摘要，摘要数量: {}, 内容长度: {} 字符",
            summary_count,
            summaries_content.len()
        );

        self.api_target_guard.ensure_confirmed_for(&self.api_url)?;

        let response = self
            .client
            .post(&self.api_url)
            .headers(headers)
            .json(&payload)
            .timeout(std::time::Duration::from_secs(300))
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "无法读取错误响应".to_string());
            error!("API 返回错误状态: {}, 响应内容: {}", status, error_body);
            return Err(format!("API 返回错误状态: {} - {}", status, error_body));
        }

        let data: Value = response
            .json()
            .await
            .map_err(|e| format!("解析响应失败: {}", e))?;

        // OpenAI 格式
        if let Some(choices) = data.get("choices").and_then(|v| v.as_array()) {
            if let Some(first_choice) = choices.first() {
                if let Some(content) = first_choice
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                {
                    return Ok(content.to_string());
                }
            }
        }

        // Gemini 格式
        if let Some(candidates) = data.get("candidates").and_then(|v| v.as_array()) {
            if let Some(first_candidate) = candidates.first() {
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

    /// 压缩对话历史（新版：保留最近N轮策略）
    /// 返回 CompressOutcome 区分成功、跳过、失败三种情况
    pub async fn compress_conversation_history(&self, data: &mut Value) -> CompressOutcome {
        let conversation_history = match data.get("ConversationHistory").and_then(|v| v.as_array())
        {
            Some(arr) => arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>(),
            None => {
                return CompressOutcome::Skip("对话历史字段不存在".to_string());
            }
        };

        if conversation_history.is_empty() {
            return CompressOutcome::Skip("对话历史为空".to_string());
        }

        let character_name = data
            .get("Name")
            .and_then(|v| v.as_str())
            .unwrap_or("角色")
            .to_string();

        let player_name = data
            .get("PlayerInfo")
            .and_then(|pi| pi.get("RealName").or_else(|| pi.get("ClaimedName")))
            .and_then(|v| v.as_str())
            .unwrap_or("玩家")
            .to_string();

        let (rounds, existing_summaries) =
            Self::parse_conversation_rounds(&conversation_history, &player_name);
        let total_rounds = rounds.len() as i32;
        let compress_count = self.config.compress_count;
        let summary_count = existing_summaries.len();

        info!(
            "[{}] 解析对话：共 {} 轮对话，{} 个历史摘要",
            character_name, total_rounds, summary_count
        );

        // 计算摘要合并阈值：触发阈值的 50%，向上取整
        let summary_merge_threshold = ((self.config.max_messages as f32) * 0.5).ceil() as usize;

        // 检查是否需要触发摘要合并
        if summary_count >= summary_merge_threshold {
            info!(
                "[{}] 摘要数量 {} >= 阈值 {}，触发摘要合并",
                character_name, summary_count, summary_merge_threshold
            );

            // 调用 AI 合并摘要
            let merged_summary = match self
                .call_ai_for_summary_merge(&existing_summaries, &character_name, &player_name)
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    error!("❌ [{}] 摘要合并失败: {}", character_name, e);
                    return CompressOutcome::Error(e);
                }
            };

            // 创建合并后的摘要条目
            let merged_summary_entry = format!(
                "System: [历史摘要 - 合并了{}个摘要]\n\n{}",
                summary_count, merged_summary
            );

            // 重建对话历史：1个合并摘要 + 所有对话轮次
            let mut new_history: Vec<Value> = vec![Value::String(merged_summary_entry)];
            for round in &rounds {
                for msg in round {
                    new_history.push(Value::String(msg.clone()));
                }
            }

            data["ConversationHistory"] = Value::Array(new_history);

            let result_msg = format!(
                "✅ 摘要合并完成: {}个摘要 -> 1个精简摘要, 保留{}轮对话",
                summary_count, total_rounds
            );
            info!("[{}] {}", character_name, result_msg);

            return CompressOutcome::Success(result_msg);
        }

        let total_count = (summary_count as i32) + total_rounds;

        if total_rounds == 0 {
            return CompressOutcome::Skip("没有可压缩的对话轮次".to_string());
        }

        let actual_compress_count = if total_rounds <= compress_count {
            info!(
                "[{}] 对话轮次 {} <= 压缩数量 {}，但总量 {} 超标，压缩全部对话轮次",
                character_name, total_rounds, compress_count, total_count
            );
            total_rounds as usize
        } else {
            compress_count as usize
        };

        let split_index = actual_compress_count.min(total_rounds as usize);

        let rounds_to_compress: Vec<_> = rounds[..split_index].to_vec();
        let rounds_to_keep: Vec<_> = rounds[split_index..].to_vec();

        let messages_to_compress: Vec<String> = rounds_to_compress
            .iter()
            .flat_map(|r| r.iter().cloned())
            .collect();

        let messages_to_keep: Vec<String> = rounds_to_keep
            .iter()
            .flat_map(|r| r.iter().cloned())
            .collect();

        let (is_healthy, _ratio, health_msg) = Self::check_compression_health(
            &messages_to_compress,
            self.config.summary_ratio_threshold,
        );

        if !is_healthy {
            warn!("[{}] {}", character_name, health_msg);
            return CompressOutcome::Skip(health_msg);
        }

        info!("[{}] {}", character_name, health_msg);

        let conversation_text = messages_to_compress.join("\n");
        let message_count = messages_to_compress.len() as i32;
        let round_count = rounds_to_compress.len() as i32;

        info!(
            "[{}] 开始压缩：压缩最旧 {} 轮（{} 条消息），保留最新 {} 轮",
            character_name,
            round_count,
            message_count,
            rounds_to_keep.len()
        );

        let summary = match self
            .call_ai_for_compression(
                &conversation_text,
                &character_name,
                &player_name,
                message_count,
                round_count,
            )
            .await
        {
            Ok(s) => s,
            Err(e) => {
                error!("❌ [{}] 压缩失败: {}", character_name, e);
                return CompressOutcome::Error(e);
            }
        };

        let summary_entry = format!(
            "System: [历史摘要 - 压缩了{}条消息，共{}轮对话]\n\n{}",
            message_count, round_count, summary
        );

        let mut new_summaries = existing_summaries;
        new_summaries.push(summary_entry);

        let mut new_history: Vec<Value> = new_summaries
            .into_iter()
            .map(|s| Value::String(s))
            .collect();
        new_history.extend(messages_to_keep.into_iter().map(|s| Value::String(s)));

        data["ConversationHistory"] = Value::Array(new_history);

        let result_msg = format!(
            "✅ 压缩完成: 压缩{}轮({}条消息) -> 1条摘要, 保留{}轮对话",
            round_count,
            message_count,
            rounds_to_keep.len()
        );
        info!("[{}] {}", character_name, result_msg);

        CompressOutcome::Success(result_msg)
    }

    /// 检查并压缩单个文件
    pub async fn check_and_compress_file(
        &self,
        file_path: &Path,
        force: bool,
    ) -> CompressionResult {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let mut data = match self.load_character_data(file_path) {
            Some(d) => d,
            None => {
                debug!("[忽略] {} - 非角色文件或无法读取", file_name);
                return CompressionResult::ignored(format!("非角色文件: {}", file_name));
            }
        };

        let conversation_history: Vec<String> = data
            .get("ConversationHistory")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let character_name = data
            .get("Name")
            .and_then(|v| v.as_str())
            .unwrap_or("未知")
            .to_string();

        let player_name = data
            .get("PlayerInfo")
            .and_then(|pi| pi.get("RealName").or_else(|| pi.get("ClaimedName")))
            .and_then(|v| v.as_str())
            .unwrap_or("玩家")
            .to_string();

        let analysis = Self::analyze_history(&conversation_history, &player_name);

        info!(
            "📋 [{}] 压缩判定: Total({}) > Max({})? {} [summaries={}, rounds={}]",
            character_name,
            analysis.total_count,
            self.config.max_messages,
            if self.exceeds_threshold(&analysis) {
                "YES"
            } else {
                "NO"
            },
            analysis.summary_count,
            analysis.round_count
        );

        if !self.exceeds_threshold(&analysis) {
            debug!(
                "[跳过] {} - 总量 {} 未超过阈值 {}，无需压缩",
                character_name, analysis.total_count, self.config.max_messages
            );
            return CompressionResult::skipped(
                format!(
                    "[{}] 总量 {}（{}摘要+{}轮对话）未超过阈值 {}，无需压缩",
                    character_name,
                    analysis.total_count,
                    analysis.summary_count,
                    analysis.round_count,
                    self.config.max_messages
                ),
                Some(analysis.total_count),
            );
        }

        if !self.config.enabled && !force {
            debug!("[跳过] {} - 压缩功能未启用", character_name);
            return CompressionResult::skipped(
                format!("[{}] 压缩功能未启用", character_name),
                Some(analysis.total_count),
            );
        }

        info!(
            "[压缩] {} - 开始压缩: 总量 {}（{}摘要+{}轮对话）",
            character_name, analysis.total_count, analysis.summary_count, analysis.round_count
        );

        if self.config.backup_before_compress {
            if self.backup_file(file_path).is_none() {
                error!("[失败] {} - 备份失败，取消压缩", character_name);
                return CompressionResult::failed(
                    format!("[{}] 备份失败，取消压缩", character_name),
                    Some(analysis.total_count),
                );
            }
        }

        match self.compress_conversation_history(&mut data).await {
            CompressOutcome::Success(msg) => {
                if self.save_character_data(file_path, &data) {
                    let new_history: Vec<String> = data
                        .get("ConversationHistory")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let new_analysis = Self::analyze_history(&new_history, &player_name);

                    info!("[成功] {} - {}", character_name, msg);
                    CompressionResult::success(
                        format!("[{}] {}", character_name, msg),
                        analysis.total_count,
                        new_analysis.total_count,
                    )
                } else {
                    error!("[失败] {} - 保存失败", character_name);
                    CompressionResult::failed(
                        format!("[{}] 保存失败", character_name),
                        Some(analysis.total_count),
                    )
                }
            }
            CompressOutcome::Skip(reason) => {
                debug!("[跳过] {} - {}", character_name, reason);
                CompressionResult::skipped(
                    format!("[{}] {}", character_name, reason),
                    Some(analysis.total_count),
                )
            }
            CompressOutcome::Error(e) => {
                error!("[失败] {} - 压缩失败: {}", character_name, e);
                CompressionResult::failed(
                    format!("[{}] 压缩失败: {}", character_name, e),
                    Some(analysis.total_count),
                )
            }
        }
    }

    /// 扫描并压缩所有需要压缩的文件
    pub async fn scan_and_compress_all(&self, force: bool) -> Vec<CompressionResult> {
        let json_files = self.scan_json_files();
        info!(
            "📂 扫描到 {} 个 JSON 文件, force={}",
            json_files.len(),
            force
        );

        let mut results = Vec::new();
        let mut success_count = 0;
        let mut skipped_count = 0;
        let mut ignored_count = 0;
        let mut failed_count = 0;

        for file_path in json_files {
            let result = self.check_and_compress_file(&file_path, force).await;

            match result.status {
                CompressionResultStatus::Success => {
                    success_count += 1;
                    info!("✅ {}", result.message);
                }
                CompressionResultStatus::Skipped => {
                    skipped_count += 1;
                }
                CompressionResultStatus::Ignored => {
                    ignored_count += 1;
                }
                CompressionResultStatus::Failed => {
                    failed_count += 1;
                    error!("❌ {}", result.message);
                }
            }

            results.push(result);
        }

        info!(
            "📊 批量压缩统计: {} 成功, {} 跳过, {} 忽略, {} 失败",
            success_count, skipped_count, ignored_count, failed_count
        );

        results
    }

    /// 获取所有文件的压缩状态
    pub fn get_compression_status(&self) -> Vec<CompressionStatus> {
        info!(
            "📊 get_compression_status: enabled={}, max_messages={}, save_data_path={}",
            self.config.enabled, self.config.max_messages, self.config.save_data_path
        );

        let json_files = self.scan_json_files();
        let mut results = Vec::new();

        for file_path in json_files {
            if let Some(data) = self.load_character_data(&file_path) {
                let conversation_history: Vec<String> = data
                    .get("ConversationHistory")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let character_id = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let character_name = data
                    .get("Name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知")
                    .to_string();

                let player_name = data
                    .get("PlayerInfo")
                    .and_then(|pi| pi.get("RealName").or_else(|| pi.get("ClaimedName")))
                    .and_then(|v| v.as_str())
                    .unwrap_or("玩家")
                    .to_string();

                let analysis = Self::analyze_history(&conversation_history, &player_name);
                let needs_compression = self.exceeds_threshold(&analysis);

                results.push(CompressionStatus {
                    character_id,
                    character_name,
                    player_name,
                    file_path: file_path.to_string_lossy().to_string(),
                    message_count: analysis.total_count,
                    summary_count: analysis.summary_count,
                    needs_compression,
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
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let player_name = data
                    .get("PlayerInfo")
                    .and_then(|pi| pi.get("RealName").or_else(|| pi.get("ClaimedName")))
                    .and_then(|v| v.as_str())
                    .unwrap_or("玩家");

                let analysis = Self::analyze_history(&conversation_history, player_name);

                if self.exceeds_threshold(&analysis) {
                    let character_name =
                        data.get("Name").and_then(|v| v.as_str()).unwrap_or("未知");

                    info!(
                        "🔄 [自动压缩] {}: 总量 {}（{}摘要+{}轮对话）超过阈值 {}，开始压缩...",
                        character_name,
                        analysis.total_count,
                        analysis.summary_count,
                        analysis.round_count,
                        self.config.max_messages
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
            ApiTargetConfirmationGuard::new(false),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn confirmation_guard_blocks_history_before_network() {
        let compressor = HistoryCompressor::new(
            HistoryCompressionConfig::default(),
            "http://127.0.0.1:1/v1/chat/completions".to_string(),
            String::new(),
            "test".to_string(),
            ApiTargetConfirmationGuard::new(true),
        );

        let error = compressor
            .call_ai_for_compression("private history", "character", "player", 1, 1)
            .await
            .unwrap_err();
        assert!(error.contains("旧版桥接已移除"));

        let error = compressor
            .call_ai_for_summary_merge(&["private summary".to_string()], "character", "player")
            .await
            .unwrap_err();
        assert!(error.contains("旧版桥接已移除"));
    }
}
