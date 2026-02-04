//! LLM API 客户端
//! 
//! 提供与远程 AI API 的通信功能，支持流式和非流式响应。

use std::time::Duration;

use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, error};

/// 假流式模型后缀列表
const FAKE_STREAM_SUFFIXES: &[&str] = &[
    "-preview", "-stream", "-streaming",
    "-fast", "-turbo", "-instant"
];

/// LLM 请求结果
/// 
/// 包含响应和流式模式信息
pub struct LlmRequestResult {
    pub response: Response,
    pub is_streaming: bool,
    pub is_fake_stream: bool,
}

/// LLM 客户端
/// 
/// 负责与远程 AI API 通信，支持流式和非流式请求。
pub struct LlmClient {
    /// HTTP 客户端
    client: Client,
    /// API 地址
    api_url: String,
    /// API 密钥
    api_key: String,
}

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// 聊天请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(flatten)]
    pub extra: Option<Value>,
}

/// 生成请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub model: String,
    pub prompt: String,
    #[serde(default)]
    pub stream: bool,
    #[serde(flatten)]
    pub extra: Option<Value>,
}

/// LLM 响应（非流式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub model: String,
    pub created_at: String,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
}

impl LlmClient {
    /// 创建新的 LLM 客户端
    pub fn new(api_url: String, api_key: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        
        Self {
            client,
            api_url,
            api_key,
        }
    }
    
    /// 更新配置
    pub fn update_config(&mut self, api_url: String, api_key: String) {
        self.api_url = api_url;
        self.api_key = api_key;
    }
    
    /// 注入系统提示词
    /// 
    /// 如果消息中已有 system 角色，则追加到其内容末尾；
    /// 否则在消息列表开头插入新的 system 消息。
    pub fn inject_prompt(messages: &mut Vec<ChatMessage>, injection_text: &str) {
        if injection_text.is_empty() {
            info!("无提示词注入");
            return;
        }
        
        // 查找现有的 system 消息
        for msg in messages.iter_mut() {
            if msg.role == "system" {
                msg.content = format!("{}\n\n{}", msg.content, injection_text);
                info!("提示词已追加到现有 system 消息 (长度: {} 字符)", injection_text.len());
                return;
            }
        }
        
        // 插入新的 system 消息
        messages.insert(0, ChatMessage {
            role: "system".to_string(),
            content: injection_text.to_string(),
        });
        info!("提示词已作为新 system 消息插入 (长度: {} 字符)", injection_text.len());
    }
    
    /// 发送聊天请求
    /// 
    /// # Arguments
    /// * `request` - 聊天请求
    /// * `system_prompt_injection` - 系统提示词注入
    /// 
    /// # Returns
    /// 返回 HTTP 响应，可用于流式或非流式处理
    pub async fn chat(
        &self,
        mut request: ChatRequest,
        system_prompt_injection: Option<&str>,
    ) -> Result<LlmRequestResult, String> {
        let original_stream = request.stream;
        let (real_model, force_stream) = detect_fake_stream(&request.model);
        let should_stream = original_stream || force_stream;
        let is_fake_stream = !original_stream && force_stream;
        
        if let Some(injection) = system_prompt_injection {
            Self::inject_prompt(&mut request.messages, injection);
        }
        
        let payload = json!({
            "model": real_model,
            "messages": request.messages,
            "stream": should_stream
        });
        
        info!("发送 chat 请求: 模型={}, 流式={}, 假流式={}", real_model, should_stream, is_fake_stream);
        
        let (response, is_streaming) = self.send_request(payload, should_stream).await?;
        Ok(LlmRequestResult {
            response,
            is_streaming,
            is_fake_stream,
        })
    }
    
    /// 发送生成请求
    pub async fn generate(
        &self,
        request: GenerateRequest,
        system_prompt_injection: Option<&str>,
    ) -> Result<LlmRequestResult, String> {
        let original_stream = request.stream;
        let (real_model, force_stream) = detect_fake_stream(&request.model);
        let should_stream = original_stream || force_stream;
        let is_fake_stream = !original_stream && force_stream;
        
        let mut messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: request.prompt,
            }
        ];
        
        if let Some(injection) = system_prompt_injection {
            Self::inject_prompt(&mut messages, injection);
        }
        
        let payload = json!({
            "model": real_model,
            "messages": messages,
            "stream": should_stream
        });
        
        info!("发送 generate 请求: 模型={}, 流式={}, 假流式={}", real_model, should_stream, is_fake_stream);
        
        let (response, is_streaming) = self.send_request(payload, should_stream).await?;
        Ok(LlmRequestResult {
            response,
            is_streaming,
            is_fake_stream,
        })
    }
    
    /// 发送 HTTP 请求
    async fn send_request(&self, payload: Value, stream: bool) -> Result<(Response, bool), String> {
        let mut request_builder = self.client
            .post(&self.api_url)
            .header("Content-Type", "application/json; charset=utf-8");
        
        if !self.api_key.is_empty() {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", self.api_key));
        }
        
        let response = request_builder
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("API 返回错误: {} - {}", status, body);
            return Err(format!("API 错误: {} - {}", status, body));
        }
        
        info!("请求成功: HTTP {}", response.status());
        Ok((response, stream))
    }
    
    /// 测试 API 连接
    pub async fn test_connection(&self) -> Result<(), String> {
        let payload = json!({
            "model": "test",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 1
        });
        
        let response = self.client
            .post(&self.api_url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("连接失败: {}", e))?;
        
        if response.status().is_success() || response.status().as_u16() == 400 {
            // 400 可能是模型不存在，但说明 API 可达
            Ok(())
        } else {
            Err(format!("API 返回错误: {}", response.status()))
        }
    }
}

/// 检测假流式模型
/// 
/// 如果模型名以特定后缀结尾，移除后缀并返回 true 表示强制流式。
/// 
/// # Arguments
/// * `model_name` - 模型名称
/// 
/// # Returns
/// (真实模型名, 是否强制流式)
pub fn detect_fake_stream(model_name: &str) -> (String, bool) {
    for suffix in FAKE_STREAM_SUFFIXES {
        if model_name.ends_with(suffix) {
            let real_model = model_name[..model_name.len() - suffix.len()].to_string();
            info!("检测到假流式后缀: {} -> {}", model_name, real_model);
            return (real_model, true);
        }
    }
    
    info!("使用原始模型名: {} (无后缀)", model_name);
    (model_name.to_string(), false)
}

/// 从响应数据中提取 response 内容
pub fn extract_response_content(data: &Value) -> String {
    if let Some(response) = data.get("response").and_then(|v| v.as_str()) {
        return response.to_string();
    }
    
    if let Some(message) = data.get("message").and_then(|v| v.as_object()) {
        if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
            return content.to_string();
        }
    }
    
    if let Some(choices) = data.get("choices").and_then(|v| v.as_array()) {
        if let Some(first) = choices.first() {
            if let Some(message) = first.get("message").and_then(|v| v.as_object()) {
                if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
                    return content.to_string();
                }
            }
            if let Some(delta) = first.get("delta").and_then(|v| v.as_object()) {
                if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                    return content.to_string();
                }
            }
        }
    }
    
    data.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_fake_stream() {
        assert_eq!(detect_fake_stream("gpt-4"), ("gpt-4".to_string(), false));
        assert_eq!(detect_fake_stream("gpt-4-preview"), ("gpt-4".to_string(), true));
        assert_eq!(detect_fake_stream("gpt-4-stream"), ("gpt-4".to_string(), true));
        assert_eq!(detect_fake_stream("gpt-4-fast"), ("gpt-4".to_string(), true));
    }
    
    #[test]
    fn test_inject_prompt() {
        let mut messages = vec![
            ChatMessage { role: "user".to_string(), content: "hello".to_string() }
        ];
        
        LlmClient::inject_prompt(&mut messages, "Be helpful.");
        
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, "Be helpful.");
    }
    
    #[test]
    fn test_inject_prompt_existing_system() {
        let mut messages = vec![
            ChatMessage { role: "system".to_string(), content: "You are an AI.".to_string() },
            ChatMessage { role: "user".to_string(), content: "hello".to_string() }
        ];
        
        LlmClient::inject_prompt(&mut messages, "Be helpful.");
        
        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("You are an AI."));
        assert!(messages[0].content.contains("Be helpful."));
    }
}
