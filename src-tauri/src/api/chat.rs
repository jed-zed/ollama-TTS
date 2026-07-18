//! Chat API 处理器
//!
//! 提供直接的聊天接口，用于前端 AI 助手功能。

use axum::{body::Body, extract::State, http::StatusCode, response::Response, Json};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{error, info, warn};

use crate::server::AppState;
use crate::services::llm_client::{ChatMessage, ChatRequest, LlmClient};
use crate::services::wire_stream::{WireEvent, WireStreamDecoder};

#[derive(Debug, Deserialize)]
pub struct FrontendChatRequest {
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default = "default_stream")]
    pub stream: bool,
}

fn default_stream() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct StreamChunk {
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: StreamChunkData,
}

#[derive(Debug, Serialize)]
pub struct StreamChunkData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn chat(
    State(state): State<AppState>,
    Json(request): Json<FrontendChatRequest>,
) -> Response<Body> {
    let config = state.config_store.get();

    info!(
        "收到前端 chat 请求: {} 条消息, stream={}",
        request.messages.len(),
        request.stream
    );

    if config.api_url.is_empty() {
        error!("API URL 未配置");
        return error_response("API URL 未配置");
    }

    let model = request.model.unwrap_or_else(|| config.model_name.clone());

    let client = LlmClient::new(
        config.api_url.clone(),
        config.api_key.clone(),
        state.config_store.api_target_confirmation_guard(),
    );

    let chat_request = ChatRequest {
        model: model.clone(),
        messages: request.messages,
        stream: request.stream,
        extra: None,
    };

    let injection = if config.system_prompt_injection.is_empty() {
        None
    } else {
        Some(config.system_prompt_injection.as_str())
    };

    match client
        .chat(chat_request, injection, Some(&config.model_name))
        .await
    {
        Ok(result) => {
            info!(
                "Chat 请求成功: HTTP {}, streaming={}",
                result.response.status(),
                result.is_streaming
            );

            if result.is_streaming || result.is_fake_stream {
                process_stream_response(result.response).await
            } else {
                process_non_stream_response(result.response).await
            }
        }
        Err(e) => {
            error!("Chat 请求失败: {}", e);
            error_response(&e)
        }
    }
}

async fn process_stream_response(response: reqwest::Response) -> Response<Body> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(100);

    tokio::spawn(async move {
        let mut stream = response.bytes_stream();
        let mut decoder = WireStreamDecoder::new();
        let mut stream_error = None;

        'upstream: loop {
            let (events, at_eof) = match stream.next().await {
                Some(Ok(chunk)) => match decoder.push(&chunk) {
                    Ok(events) => (events, false),
                    Err(error) => {
                        stream_error = Some(error);
                        break;
                    }
                },
                Some(Err(error)) => {
                    stream_error = Some(format!("读取上游流失败: {}", error));
                    break;
                }
                None => match decoder.finish() {
                    Ok(events) => (events, true),
                    Err(error) => {
                        stream_error = Some(error);
                        break;
                    }
                },
            };

            for event in events {
                let payload = match event {
                    WireEvent::Done => break 'upstream,
                    WireEvent::Data(payload) => payload,
                };

                match serde_json::from_str::<Value>(&payload) {
                    Ok(data) => {
                        let content = extract_delta_content(&data);
                        if content.is_empty() {
                            continue;
                        }
                        let chunk = StreamChunk {
                            event_type: "content".to_string(),
                            data: StreamChunkData {
                                content: Some(content),
                                done: None,
                                error: None,
                            },
                        };
                        let sse_line = format!(
                            "data: {}\n\n",
                            serde_json::to_string(&chunk)
                                .expect("StreamChunk serialization cannot fail")
                        );
                        if tx.send(Ok(sse_line)).await.is_err() {
                            return;
                        }
                    }
                    Err(error) => warn!(?error, "忽略无法解析的上游流记录"),
                }
            }

            if at_eof {
                break;
            }
        }

        if let Some(error) = stream_error {
            error!("流式响应失败: {}", error);
            let error_chunk = StreamChunk {
                event_type: "error".to_string(),
                data: StreamChunkData {
                    content: None,
                    done: Some(true),
                    error: Some(error),
                },
            };
            let sse_line = format!(
                "data: {}\n\n",
                serde_json::to_string(&error_chunk).expect("StreamChunk serialization cannot fail")
            );
            let _ = tx.send(Ok(sse_line)).await;
        }

        let done_chunk = StreamChunk {
            event_type: "complete".to_string(),
            data: StreamChunkData {
                content: None,
                done: Some(true),
                error: None,
            },
        };
        let sse_line = format!(
            "data: {}\n\n",
            serde_json::to_string(&done_chunk).expect("StreamChunk serialization cannot fail")
        );
        let _ = tx.send(Ok(sse_line)).await;
        let _ = tx.send(Ok("data: [DONE]\n\n".to_string())).await;
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .body(body)
        .unwrap()
}

async fn process_non_stream_response(response: reqwest::Response) -> Response<Body> {
    match response.json::<Value>().await {
        Ok(data) => {
            let content = extract_response_content(&data);

            let result = json!({
                "type": "content",
                "data": {
                    "content": content,
                    "done": true
                }
            });

            let sse_content = format!("data: {}\n\ndata: [DONE]\n\n", result);

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .body(Body::from(sse_content))
                .unwrap()
        }
        Err(e) => {
            error!("解析响应失败: {}", e);
            error_response(&format!("解析响应失败: {}", e))
        }
    }
}

fn extract_delta_content(data: &Value) -> String {
    if let Some(choices) = data.get("choices").and_then(|v| v.as_array()) {
        if let Some(first) = choices.first() {
            if let Some(delta) = first.get("delta").and_then(|v| v.as_object()) {
                if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                    return content.to_string();
                }
            }
        }
    }

    if let Some(message) = data.get("message").and_then(|v| v.as_object()) {
        if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
            return content.to_string();
        }
    }

    if let Some(response) = data.get("response").and_then(|v| v.as_str()) {
        return response.to_string();
    }

    String::new()
}

fn extract_response_content(data: &Value) -> String {
    if let Some(choices) = data.get("choices").and_then(|v| v.as_array()) {
        if let Some(first) = choices.first() {
            if let Some(message) = first.get("message").and_then(|v| v.as_object()) {
                if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
                    return content.to_string();
                }
            }
        }
    }

    if let Some(message) = data.get("message").and_then(|v| v.as_object()) {
        if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
            return content.to_string();
        }
    }

    if let Some(response) = data.get("response").and_then(|v| v.as_str()) {
        return response.to_string();
    }

    String::new()
}

fn error_response(message: &str) -> Response<Body> {
    let body = json!({
        "type": "error",
        "data": {
            "error": message
        }
    });
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
