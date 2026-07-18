//! 代理控制 API
//!
//! 提供代理服务的状态查询和控制接口。

use axum::{extract::State, Json};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::api::config::ApiResponse;
use crate::server::AppState;
use crate::services::llm_client::{extract_stream_content, LlmClient};
use crate::services::sse_broadcaster::{LogEvent, SseBroadcaster};
use crate::services::wire_stream::{WireEvent, WireStreamDecoder};

fn broadcast_log(broadcaster: &SseBroadcaster, level: &str, message: &str) {
    let event = LogEvent {
        id: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: level.to_string(),
        message: message.to_string(),
        module: Some("ProxyControl".to_string()),
    };
    broadcaster.send_log(event);
}

/// 代理状态
#[derive(Debug, Clone, Serialize)]
pub struct ProxyStatus {
    /// 代理是否正在运行
    pub running: bool,
    /// 主 API 端口
    pub main_port: u16,
    /// 代理端口 (Ollama 兼容)
    pub proxy_port: u16,
    /// API URL 是否已配置
    pub api_configured: bool,
    /// 上次测试结果
    pub last_test_result: Option<String>,
}

/// 测试结果
#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub success: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
}

/// 启动代理响应
#[derive(Debug, Clone, Serialize)]
pub struct StartProxyResponse {
    pub success: bool,
    pub port: Option<u16>,
    pub error: Option<String>,
}

/// 停止代理响应
#[derive(Debug, Clone, Serialize)]
pub struct StopProxyResponse {
    pub success: bool,
    pub error: Option<String>,
}

/// 获取代理状态
///
/// GET /api/proxy/status
pub async fn get_proxy_status(State(state): State<AppState>) -> Json<ApiResponse<ProxyStatus>> {
    let config = state.config_store.get();
    let is_running = state.proxy_manager.is_running();

    let status = ProxyStatus {
        running: is_running,
        main_port: config.main_api_port,
        proxy_port: 11434,
        api_configured: !config.api_url.is_empty(),
        last_test_result: None,
    };

    Json(ApiResponse::success(status))
}

/// 启动代理服务器
///
/// POST /api/proxy/start
pub async fn start_proxy(State(state): State<AppState>) -> Json<StartProxyResponse> {
    info!("Starting proxy server via API...");
    broadcast_log(&state.broadcaster, "INFO", "正在启动代理服务器...");

    match state.proxy_manager.start().await {
        Ok(()) => {
            info!("Proxy server started successfully");
            broadcast_log(
                &state.broadcaster,
                "SUCCESS",
                "代理服务器启动成功 (端口 11434)",
            );
            Json(StartProxyResponse {
                success: true,
                port: Some(11434),
                error: None,
            })
        }
        Err(e) => {
            error!("Failed to start proxy server: {}", e);
            broadcast_log(
                &state.broadcaster,
                "ERROR",
                &format!("代理服务器启动失败: {}", e),
            );
            Json(StartProxyResponse {
                success: false,
                port: None,
                error: Some(e),
            })
        }
    }
}

/// 停止代理服务器
///
/// POST /api/proxy/stop
pub async fn stop_proxy(State(state): State<AppState>) -> Json<StopProxyResponse> {
    info!("Stopping proxy server via API...");
    broadcast_log(&state.broadcaster, "INFO", "正在停止代理服务器...");

    match state.proxy_manager.stop().await {
        Ok(()) => {
            info!("Proxy server stopped successfully");
            broadcast_log(&state.broadcaster, "SUCCESS", "代理服务器已停止");
            Json(StopProxyResponse {
                success: true,
                error: None,
            })
        }
        Err(e) => {
            error!("Failed to stop proxy server: {}", e);
            broadcast_log(
                &state.broadcaster,
                "ERROR",
                &format!("代理服务器停止失败: {}", e),
            );
            Json(StopProxyResponse {
                success: false,
                error: Some(e),
            })
        }
    }
}

/// 测试代理连接
///
/// POST /api/proxy/test
pub async fn test_proxy(State(state): State<AppState>) -> Json<ApiResponse<TestResult>> {
    info!("测试代理连接...");
    broadcast_log(&state.broadcaster, "INFO", "正在测试API连接...");

    let config = state.config_store.get();

    if config.api_url.is_empty() {
        broadcast_log(&state.broadcaster, "WARN", "API URL 未配置");
        return Json(ApiResponse::success(TestResult {
            success: false,
            message: "API URL 未配置".to_string(),
            latency_ms: None,
        }));
    }

    broadcast_log(
        &state.broadcaster,
        "DEBUG",
        &format!("API URL: {}", config.api_url),
    );

    let client = LlmClient::new(
        config.api_url.clone(),
        config.api_key.clone(),
        state.config_store.api_target_confirmation_guard(),
    );

    let start = std::time::Instant::now();

    match client.test_connection().await {
        Ok(()) => {
            let latency = start.elapsed().as_millis() as u64;
            info!("代理测试成功, 延迟: {}ms", latency);
            broadcast_log(
                &state.broadcaster,
                "SUCCESS",
                &format!("API连接测试成功, 延迟: {}ms", latency),
            );
            Json(ApiResponse::success(TestResult {
                success: true,
                message: "连接成功".to_string(),
                latency_ms: Some(latency),
            }))
        }
        Err(e) => {
            error!("代理测试失败: {}", e);
            broadcast_log(
                &state.broadcaster,
                "ERROR",
                &format!("API连接测试失败: {}", e),
            );
            Json(ApiResponse::success(TestResult {
                success: false,
                message: e,
                latency_ms: None,
            }))
        }
    }
}

/// 流式测试请求体
#[derive(Debug, Deserialize)]
pub struct StreamTestRequest {
    #[serde(default = "default_test_prompt")]
    pub prompt: String,
}

fn default_test_prompt() -> String {
    "你好".to_string()
}

/// 流式测试结果（包含完整响应）
#[derive(Debug, Clone, Serialize)]
pub struct StreamTestResult {
    pub success: bool,
    pub message: String,
    pub response: String,
    pub latency_ms: Option<u64>,
}

/// 流式测试
///
/// POST /api/proxy/test-stream
pub async fn test_stream(
    State(state): State<AppState>,
    Json(request): Json<StreamTestRequest>,
) -> Json<ApiResponse<StreamTestResult>> {
    info!("测试流式连接...");
    broadcast_log(&state.broadcaster, "INFO", "正在测试流式连接...");

    let config = state.config_store.get();

    if config.api_url.is_empty() {
        broadcast_log(&state.broadcaster, "WARN", "API URL 未配置");
        return Json(ApiResponse::success(StreamTestResult {
            success: false,
            message: "API URL 未配置".to_string(),
            response: String::new(),
            latency_ms: None,
        }));
    }

    broadcast_log(
        &state.broadcaster,
        "DEBUG",
        &format!("测试模型: {}", config.model_name),
    );

    let client = LlmClient::new(
        config.api_url.clone(),
        config.api_key.clone(),
        state.config_store.api_target_confirmation_guard(),
    );

    let chat_request = crate::services::llm_client::ChatRequest {
        model: config.model_name.clone(),
        messages: vec![crate::services::llm_client::ChatMessage {
            role: "user".to_string(),
            content: request.prompt,
        }],
        stream: true,
        extra: None,
    };

    let start = std::time::Instant::now();

    match client
        .chat(chat_request, None, Some(&config.model_name))
        .await
    {
        Ok(result) => {
            if result.response.status().is_success() {
                let mut full_response = String::new();
                let mut bytes = result.response.bytes_stream();
                let mut decoder = WireStreamDecoder::new();

                'upstream: loop {
                    let (events, at_eof) = match bytes.next().await {
                        Some(Ok(chunk)) => match decoder.push(&chunk) {
                            Ok(events) => (events, false),
                            Err(error) => {
                                error!("解析流式响应失败: {}", error);
                                break;
                            }
                        },
                        Some(Err(error)) => {
                            error!("读取流式响应失败: {}", error);
                            break;
                        }
                        None => match decoder.finish() {
                            Ok(events) => (events, true),
                            Err(error) => {
                                error!("解析流式响应尾部失败: {}", error);
                                break;
                            }
                        },
                    };

                    for event in events {
                        let payload = match event {
                            WireEvent::Done => break 'upstream,
                            WireEvent::Data(payload) => payload,
                        };
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&payload) {
                            full_response.push_str(&extract_stream_content(&data));
                        }
                    }

                    if at_eof {
                        break;
                    }
                }

                let latency = start.elapsed().as_millis() as u64;
                info!(
                    "流式测试成功, 总耗时: {}ms, 响应长度: {} 字符",
                    latency,
                    full_response.len()
                );
                broadcast_log(
                    &state.broadcaster,
                    "SUCCESS",
                    &format!("流式连接测试成功, 总耗时: {}ms", latency),
                );
                Json(ApiResponse::success(StreamTestResult {
                    success: true,
                    message: format!("流式连接成功, 总耗时: {}ms", latency),
                    response: full_response,
                    latency_ms: Some(latency),
                }))
            } else {
                let err_msg = format!("HTTP 错误: {}", result.response.status());
                broadcast_log(&state.broadcaster, "ERROR", &err_msg);
                Json(ApiResponse::success(StreamTestResult {
                    success: false,
                    message: err_msg,
                    response: String::new(),
                    latency_ms: None,
                }))
            }
        }
        Err(e) => {
            error!("流式测试失败: {}", e);
            broadcast_log(&state.broadcaster, "ERROR", &format!("流式测试失败: {}", e));
            Json(ApiResponse::success(StreamTestResult {
                success: false,
                message: e,
                response: String::new(),
                latency_ms: None,
            }))
        }
    }
}
