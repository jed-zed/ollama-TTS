//! Ollama 代理 API 路由
//! 
//! 监听 localhost:11434，提供 Ollama 兼容的 API 接口。

use axum::{
    routing::{get, post},
    Router,
    Json,
    extract::{State, Request},
    http::StatusCode,
    body::Body,
    response::{Response, IntoResponse},
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, error, warn, debug};

use crate::services::config_store::ConfigStore;
use crate::services::sse_broadcaster::{SseBroadcaster, LogEvent};
use crate::services::llm_client::{ChatMessage, ChatRequest, GenerateRequest, LlmClient, extract_response_content, detect_fake_stream};
use crate::services::stream_parser::extract_response_only;
use crate::services::tts_parser::StreamingTTSParser;
use crate::services::tts_gsv::GPTSoVITSEngine;
use crate::services::audio_player::TTSManager;
use crate::services::antigravity_bridge::AntigravityBridge;
use crate::models::tts_types::TtsRequest;

/// 代理服务器专用状态（不包含 proxy_manager 避免循环依赖）
#[derive(Clone)]
pub struct ProxyAppState {
    pub config_store: Arc<ConfigStore>,
    pub broadcaster: Arc<SseBroadcaster>,
    pub gsv_engine: Option<Arc<Mutex<GPTSoVITSEngine>>>,
    pub tts_manager: Option<Arc<Mutex<TTSManager>>>,
    pub bridge: Arc<AntigravityBridge>,
}

/// 模型信息
#[derive(Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub model: String,
}

/// 模型列表响应
#[derive(Serialize)]
pub struct TagsResponse {
    pub models: Vec<ModelInfo>,
}

/// Ollama 聊天请求格式
#[derive(Debug, Deserialize)]
pub struct OllamaChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(flatten)]
    pub extra: Option<Value>,
}

/// Ollama 生成请求格式
#[derive(Debug, Deserialize)]
pub struct OllamaGenerateRequest {
    pub model: String,
    pub prompt: String,
    #[serde(default)]
    pub stream: bool,
    #[serde(flatten)]
    pub extra: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct BridgeSubmitRequest {
    pub id: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct BridgePendingItem {
    pub id: String,
    pub prompt: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct BridgePendingResponse {
    pub ok: bool,
    pub pending: Option<BridgePendingItem>,
}

#[derive(Serialize)]
pub struct BridgeSubmitResponse {
    pub ok: bool,
}

static DIALOGUE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_dialogue_id() -> u64 {
    DIALOGUE_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn broadcast_log(broadcaster: &SseBroadcaster, level: &str, message: &str) {
    let event = LogEvent {
        id: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: level.to_string(),
        message: message.to_string(),
        module: Some("Proxy".to_string()),
    };
    broadcaster.send_log(event);
}

const FALLBACK_TRIGGER_PREFIX: &str = "SYSTEM:\n";

fn collect_user_prompt(messages: &[ChatMessage]) -> String {
    // 直接拼接所有消息的原始内容，不做任何处理
    messages
        .iter()
        .filter(|m| !m.content.is_empty())
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn bridge_error_response(message: &str) -> Response<Body> {
    let body = json!({
        "model": "antigravity-ui",
        "created_at": chrono::Utc::now().to_rfc3339(),
        "done": true,
        "error": message
    });
    Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn bridge_chat_response(model: &str, content: &str) -> Response<Body> {
    let body = json!({
        "model": model,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "message": {
            "role": "assistant",
            "content": content,
        },
        "done": true
    });
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn bridge_generate_response(model: &str, content: &str) -> Response<Body> {
    let body = json!({
        "model": model,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "response": content,
        "done": true
    });
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn bridge_chat_stream_response(model: &str, content: &str) -> Response<Body> {
    let chunk = json!({
        "model": model,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "message": {
            "role": "assistant",
            "content": content,
        },
        "done": false
    });
    let done = json!({
        "model": model,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "message": {
            "role": "assistant",
            "content": "",
        },
        "done": true
    });
    let body = format!("{}\n{}\n", chunk, done);
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/x-ndjson")
        .body(Body::from(body))
        .unwrap()
}

fn bridge_generate_stream_response(model: &str, content: &str) -> Response<Body> {
    let chunk = json!({
        "model": model,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "response": content,
        "done": false
    });
    let done = json!({
        "model": model,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "response": "",
        "done": true
    });
    let body = format!("{}\n{}\n", chunk, done);
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/x-ndjson")
        .body(Body::from(body))
        .unwrap()
}

async fn execute_bridge_waitback(
    state: &ProxyAppState,
    prompt: String,
    timeout_ms: i32,
    max_retries: i32,
    fallback_enabled: bool,
    include_thinking: bool,
) -> Result<String, String> {
    let retries = max_retries.clamp(0, 3) as usize + 1;
    let mut last_error: Option<String> = None;
    let fallback_wait_timeout_ms = timeout_ms.clamp(3_000, 10_000);

    for attempt in 1..=retries {
        let stage_msg = format!(
            "Antigravity 内置桥接尝试: attempt={}/{}, stage=discover/connect/send/poll/parse",
            attempt, retries
        );
        info!("{}", stage_msg);
        broadcast_log(&state.broadcaster, "INFO", &stage_msg);

        match state.bridge.execute_builtin_ls_waitback(prompt.clone(), timeout_ms, include_thinking).await {
            Ok(content) => {
                let ok_msg = format!("Antigravity 内置桥接成功: attempt={}/{}", attempt, retries);
                info!("{}", ok_msg);
                broadcast_log(&state.broadcaster, "SUCCESS", &ok_msg);
                return Ok(content);
            }
            Err(err) => {
                let err_msg = format!("Antigravity 内置桥接失败: attempt={}/{}, error={}", attempt, retries, err);
                warn!("{}", err_msg);
                broadcast_log(&state.broadcaster, "WARN", &err_msg);
                last_error = Some(err);
            }
        }
    }

    if !fallback_enabled {
        return Err(last_error.unwrap_or_else(|| "bridge builtin execution failed".to_string()));
    }

    let fallback_msg = "Antigravity 内置桥接失败，进入 pending/submit 显式降级通道";
    warn!("{}", fallback_msg);
    broadcast_log(&state.broadcaster, "WARN", fallback_msg);

    for attempt in 1..=retries {
        match state.bridge.enqueue(prompt.clone()).await {
            Ok(request_id) => {
                let queue_msg = format!(
                    "Antigravity 降级桥接排队成功: attempt={}/{}, request_id={}",
                    attempt, retries, request_id
                );
                info!("{}", queue_msg);
                broadcast_log(&state.broadcaster, "INFO", &queue_msg);

                match state
                    .bridge
                    .wait_for_result(&request_id, fallback_wait_timeout_ms)
                    .await
                {
                    Ok(result) => return Ok(result.content),
                    Err(err) => {
                        last_error = Some(err.clone());
                        let wait_err_msg = format!(
                            "Antigravity 降级桥接等待失败: attempt={}/{}, request_id={}, timeout_ms={}, error={}",
                            attempt, retries, request_id, fallback_wait_timeout_ms, err
                        );
                        warn!("{}", wait_err_msg);
                        broadcast_log(&state.broadcaster, "WARN", &wait_err_msg);
                    }
                }
            }
            Err(err) => {
                last_error = Some(err.clone());
                let enqueue_err_msg = format!(
                    "Antigravity 降级桥接排队失败: attempt={}/{}, error={}",
                    attempt, retries, err
                );
                warn!("{}", enqueue_err_msg);
                broadcast_log(&state.broadcaster, "WARN", &enqueue_err_msg);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "bridge execution failed".to_string()))
}

fn get_builtin_preset_url(preset_id: &str) -> &'static str {
    match preset_id {
        "local" => "http://127.0.0.1:11434/v1/chat/completions",
        "siliconflow" => "https://api.siliconflow.cn/v1/chat/completions",
        "deepseek" => "https://api.deepseek.com/v1/chat/completions",
        "volcengine" => "https://ark.cn-beijing.volces.com/api/v3/chat/completions",
        "ggchan" => "https://gcli.ggchan.dev/v1/chat/completions",
        "openrouter" => "https://openrouter.ai/api/v1/chat/completions",
        "openai" => "https://api.openai.com/v1/chat/completions",
        _ => "",
    }
}

fn get_builtin_preset_model(preset_id: &str) -> &'static str {
    match preset_id {
        "deepseek" => "deepseek-chat",
        "ggchan" => "gemini-3-pro-preview",
        _ => "",
    }
}

struct FallbackOverride {
    api_url: String,
    api_key: String,
    model_name: String,
}

fn check_fallback_trigger(messages: &[ChatMessage], config: &crate::models::config::Config) -> Option<FallbackOverride> {
    if !config.fallback_model_enabled || config.fallback_preset_id.is_empty() {
        return None;
    }

    let user_content = messages.iter().rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    if !user_content.starts_with(FALLBACK_TRIGGER_PREFIX) {
        return None;
    }

    let preset_id = &config.fallback_preset_id;

    let (api_url, api_key, model_name) = if preset_id == "custom" {
        (
            config.custom_api_url.clone(),
            config.custom_api_key.clone(),
            config.custom_model_name.clone(),
        )
    } else {
        let saved = config.preset_configs.get(preset_id.as_str());
        let builtin_url = get_builtin_preset_url(preset_id);
        let builtin_model = get_builtin_preset_model(preset_id);

        let url = saved.and_then(|s| if s.api_url.is_empty() { None } else { Some(s.api_url.clone()) })
            .unwrap_or_else(|| builtin_url.to_string());
        let key = saved.map(|s| s.api_key.clone()).unwrap_or_default();
        let model = saved.and_then(|s| if s.model_name.is_empty() { None } else { Some(s.model_name.clone()) })
            .unwrap_or_else(|| builtin_model.to_string());

        (url, key, model)
    };

    if api_url.is_empty() && model_name.is_empty() {
        return None;
    }

    Some(FallbackOverride { api_url, api_key, model_name })
}

/// 获取模型列表
/// 
/// GET /api/tags
async fn get_tags(State(state): State<ProxyAppState>) -> Json<TagsResponse> {
    let config = state.config_store.get();
    let configured_model = config.model_name.trim();
    let base_model = if configured_model.is_empty() {
        "default:latest".to_string()
    } else {
        configured_model.to_string()
    };
    
    let mut models = vec![
        ModelInfo { name: base_model.clone(), model: base_model.clone() },
    ];
    
    for suffix in &["-preview", "-stream", "-fast"] {
        models.push(ModelInfo {
            name: format!("{}{}", base_model, suffix),
            model: format!("{}{}", base_model, suffix),
        });
    }
    
    let msg = format!("返回模型列表: {} 个模型", models.len());
    info!("{}", msg);
    broadcast_log(&state.broadcaster, "INFO", &msg);
    Json(TagsResponse { models })
}

async fn bridge_pending(State(state): State<ProxyAppState>) -> Json<BridgePendingResponse> {
    let pending = state.bridge.next_pending().await.map(|task| BridgePendingItem {
        id: task.id,
        prompt: task.prompt,
        created_at: task.created_at,
    });

    Json(BridgePendingResponse {
        ok: true,
        pending,
    })
}

async fn bridge_submit(
    State(state): State<ProxyAppState>,
    Json(request): Json<BridgeSubmitRequest>,
) -> Response<Body> {
    let request_id = request.id.trim().to_string();
    let content = request.content.trim().to_string();

    if request_id.is_empty() || content.is_empty() {
        let body = json!({ "ok": false, "error": "invalid bridge submit payload" });
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
    }

    match state.bridge.submit(request_id, content).await {
        Ok(_) => Json(BridgeSubmitResponse { ok: true }).into_response(),
        Err(err) => {
            let status = if err.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            let body = json!({ "ok": false, "error": err });
            Response::builder()
                .status(status)
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        }
    }
}

/// 处理聊天请求
/// 
/// POST /api/chat
async fn chat(
    State(state): State<ProxyAppState>,
    Json(request): Json<OllamaChatRequest>,
) -> Response<Body> {
    let config = state.config_store.get();
    let base_model = &config.model_name;
    let (real_model, is_fake_stream) = detect_fake_stream(&request.model, base_model);
    let should_stream = request.stream || is_fake_stream;
    
    let msg = format!(
        "收到 chat 请求: 原始model={}, 实际model={}, 原始stream={}, 实际stream={}, 假流式={}, 配置原模型={}",
        request.model, real_model, request.stream, should_stream, is_fake_stream, base_model
    );
    info!("{}", msg);
    broadcast_log(&state.broadcaster, "INFO", &msg);

    if config.antigravity_bridge_enabled {
        let bridge_prompt = collect_user_prompt(&request.messages);
        if bridge_prompt.trim().is_empty() {
            return bridge_error_response("bridge input is empty");
        }

        match execute_bridge_waitback(
            &state,
            bridge_prompt,
            config.antigravity_timeout_ms,
            config.antigravity_max_retries,
            config.antigravity_pending_submit_fallback_enabled,
            config.antigravity_include_thinking,
        )
        .await
        {
            Ok(content) => {
                broadcast_log(&state.broadcaster, "SUCCESS", "Antigravity UI 自动化桥接执行成功");
                if request.stream {
                    return bridge_chat_stream_response(&request.model, &content);
                }
                return bridge_chat_response(&request.model, &content);
            }
            Err(err) => {
                let err_msg = format!("Antigravity UI 自动化桥接执行失败: {}", err);
                error!("{}", err_msg);
                broadcast_log(&state.broadcaster, "ERROR", &err_msg);
                return bridge_error_response(&err_msg);
            }
        }
    }
    
    if config.api_url.is_empty() {
        let err_msg = "API URL 未配置";
        error!("{}", err_msg);
        broadcast_log(&state.broadcaster, "ERROR", err_msg);
        return error_response(err_msg);
    }
    
    let fallback = check_fallback_trigger(&request.messages, &config);
    let (use_api_url, use_api_key, use_model) = if let Some(ref fb) = fallback {
        let fb_msg = format!(
            "[辅助模型] 触发切换 → 模型: {}, API: {}",
            fb.model_name, fb.api_url
        );
        info!("{}", fb_msg);
        broadcast_log(&state.broadcaster, "INFO", &fb_msg);
        (fb.api_url.clone(), fb.api_key.clone(), fb.model_name.clone())
    } else {
        (config.api_url.clone(), config.api_key.clone(), request.model.clone())
    };

    let client = LlmClient::new(use_api_url, use_api_key);
    
    let chat_request = ChatRequest {
        model: use_model,
        messages: request.messages,
        stream: request.stream,
        extra: request.extra,
    };
    
    let injection = if fallback.is_some() || config.system_prompt_injection.is_empty() {
        None
    } else {
        Some(config.system_prompt_injection.as_str())
    };
    
    let broadcaster = state.broadcaster.clone();
    let gsv_engine = state.gsv_engine.clone();
    let tts_manager = state.tts_manager.clone();
    match client.chat(chat_request, injection, Some(base_model)).await {
        Ok(result) => {
            let success_msg = format!("请求成功: HTTP {}", result.response.status());
            info!("{}", success_msg);
            broadcast_log(&broadcaster, "SUCCESS", &success_msg);
            
            if result.is_fake_stream {
                broadcast_log(&broadcaster, "INFO", "使用假流式模式：内部流式接收，返回完整响应");
                process_fake_stream_response(result.response, request.model, true, broadcaster, gsv_engine, tts_manager).await
            } else if result.is_streaming {
                process_stream_response(result.response, request.model, true, broadcaster, gsv_engine, tts_manager).await
            } else {
                process_non_stream_response(result.response, request.model, true, broadcaster, gsv_engine, tts_manager).await
            }
        }
        Err(e) => {
            let err_msg = format!("请求失败: {}", e);
            error!("{}", err_msg);
            broadcast_log(&broadcaster, "ERROR", &err_msg);
            error_response(&e)
        }
    }
}

/// 处理生成请求
/// 
/// POST /api/generate
async fn generate(
    State(state): State<ProxyAppState>,
    Json(request): Json<OllamaGenerateRequest>,
) -> Response<Body> {
    let config = state.config_store.get();
    let base_model = &config.model_name;
    let (real_model, is_fake_stream) = detect_fake_stream(&request.model, base_model);
    let should_stream = request.stream || is_fake_stream;
    
    let msg = format!(
        "收到 generate 请求: 原始model={}, 实际model={}, 原始stream={}, 实际stream={}, 假流式={}, 配置原模型={}",
        request.model, real_model, request.stream, should_stream, is_fake_stream, base_model
    );
    info!("{}", msg);
    broadcast_log(&state.broadcaster, "INFO", &msg);

    if config.antigravity_bridge_enabled {
        if request.prompt.trim().is_empty() {
            return bridge_error_response("bridge input is empty");
        }

        match execute_bridge_waitback(
            &state,
            request.prompt.clone(),
            config.antigravity_timeout_ms,
            config.antigravity_max_retries,
            config.antigravity_pending_submit_fallback_enabled,
            config.antigravity_include_thinking,
        )
        .await
        {
            Ok(content) => {
                broadcast_log(&state.broadcaster, "SUCCESS", "Antigravity UI 自动化桥接执行成功");
                if request.stream {
                    return bridge_generate_stream_response(&request.model, &content);
                }
                return bridge_generate_response(&request.model, &content);
            }
            Err(err) => {
                let err_msg = format!("Antigravity UI 自动化桥接执行失败: {}", err);
                error!("{}", err_msg);
                broadcast_log(&state.broadcaster, "ERROR", &err_msg);
                return bridge_error_response(&err_msg);
            }
        }
    }
    
    if config.api_url.is_empty() {
        let err_msg = "API URL 未配置";
        error!("{}", err_msg);
        broadcast_log(&state.broadcaster, "ERROR", err_msg);
        return error_response(err_msg);
    }
    
    let prompt_messages = vec![ChatMessage { role: "user".to_string(), content: request.prompt.clone() }];
    let fallback = check_fallback_trigger(&prompt_messages, &config);
    let (use_api_url, use_api_key, use_model) = if let Some(ref fb) = fallback {
        let fb_msg = format!(
            "[辅助模型] 触发切换 → 模型: {}, API: {}",
            fb.model_name, fb.api_url
        );
        info!("{}", fb_msg);
        broadcast_log(&state.broadcaster, "INFO", &fb_msg);
        (fb.api_url.clone(), fb.api_key.clone(), fb.model_name.clone())
    } else {
        (config.api_url.clone(), config.api_key.clone(), request.model.clone())
    };

    let client = LlmClient::new(use_api_url, use_api_key);
    
    let generate_request = GenerateRequest {
        model: use_model,
        prompt: request.prompt,
        stream: request.stream,
        extra: request.extra,
    };
    
    let injection = if fallback.is_some() || config.system_prompt_injection.is_empty() {
        None
    } else {
        Some(config.system_prompt_injection.as_str())
    };
    
    let broadcaster = state.broadcaster.clone();
    let gsv_engine = state.gsv_engine.clone();
    let tts_manager = state.tts_manager.clone();
    match client.generate(generate_request, injection, Some(base_model)).await {
        Ok(result) => {
            let success_msg = format!("请求成功: HTTP {}", result.response.status());
            info!("{}", success_msg);
            broadcast_log(&broadcaster, "SUCCESS", &success_msg);
            
            if result.is_fake_stream {
                broadcast_log(&broadcaster, "INFO", "使用假流式模式：内部流式接收，返回完整响应");
                process_fake_stream_response(result.response, request.model, false, broadcaster, gsv_engine, tts_manager).await
            } else if result.is_streaming {
                process_stream_response(result.response, request.model, false, broadcaster, gsv_engine, tts_manager).await
            } else {
                process_non_stream_response(result.response, request.model, false, broadcaster, gsv_engine, tts_manager).await
            }
        }
        Err(e) => {
            let err_msg = format!("请求失败: {}", e);
            error!("{}", err_msg);
            broadcast_log(&broadcaster, "ERROR", &err_msg);
            error_response(&e)
        }
    }
}

/// 处理流式响应（真流式：实时发送内容到字幕窗口，同时转发给游戏）
async fn process_stream_response(
    response: reqwest::Response,
    model_name: String,
    is_chat: bool,
    broadcaster: Arc<SseBroadcaster>,
    gsv_engine: Option<Arc<Mutex<GPTSoVITSEngine>>>,
    tts_manager: Option<Arc<Mutex<TTSManager>>>,
) -> Response<Body> {
    let msg = "开始处理流式响应（真流式：实时显示字幕）...";
    info!("{}", msg);
    broadcast_log(&broadcaster, "INFO", msg);
    
    let tts_enabled = gsv_engine.is_some() && tts_manager.is_some();
    if tts_enabled {
        info!("TTS 已启用，将在检测到对话时触发语音合成");
        broadcast_log(&broadcaster, "INFO", "TTS 已启用");
    }
    
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(100);
    
    let model_for_task = model_name.clone();
    let broadcaster_clone = broadcaster.clone();
    tokio::spawn(async move {
        let mut total_content = String::new();
        let mut chunk_count = 0;
        let mut tts_parser = StreamingTTSParser::new();
        let mut stream_started = false;
        
        if tts_enabled {
            let gsv_engine_clone = gsv_engine.clone().unwrap();
            let tts_manager_clone = tts_manager.clone().unwrap();
            let broadcaster_for_tts = broadcaster_clone.clone();
            
            tts_parser.set_callback(Box::new(move |segment| {
                let text = segment.text.clone();
                let speaker = segment.speaker.clone();
                let emotion = segment.emotion.clone();
                let emotion_desc = segment.emotion_description.clone();
                let speed = segment.speed;
                let gender = segment.gender.clone();
                let emotion_vector = segment.emotion_vector.clone();
                let is_nsfw = segment.is_nsfw;
                
                info!("TTS 触发: [{}] {}...", speaker, text.chars().take(30).collect::<String>());
                
                let gsv = gsv_engine_clone.clone();
                let mgr = tts_manager_clone.clone();
                let bc = broadcaster_for_tts.clone();
                
                tauri::async_runtime::spawn(async move {
                    let tts_request = TtsRequest {
                        text: text.clone(),
                        speaker,
                        gender,
                        emotion,
                        emotion_description: emotion_desc,
                        speed,
                        emotion_vector,
                        is_nsfw,
                    };
                    
                    let dialogue_id = next_dialogue_id();
                    let (tx, mut rx) = tokio::sync::mpsc::channel::<crate::models::tts_types::TtsSegmentAudio>(16);
                    
                    let mgr_for_receiver = mgr.clone();
                    let bc_for_receiver = bc.clone();
                    
                    let receiver_handle = tokio::spawn(async move {
                        let mut received_count = 0;
                        let mut total_bytes = 0;
                        while let Some(segment_audio) = rx.recv().await {
                            received_count += 1;
                            total_bytes += segment_audio.audio_data.len();
                            
                            let mut manager = mgr_for_receiver.lock().await;
                            manager.enqueue_audio_buffered(
                                dialogue_id,
                                segment_audio.seq,
                                segment_audio.total,
                                segment_audio.audio_data,
                            );
                            
                            if segment_audio.is_last {
                                broadcast_log(&bc_for_receiver, "SUCCESS", 
                                    &format!("TTS 流式合成完成: {} 段, {} 字节", received_count, total_bytes));
                            }
                        }
                    });
                    
                    let audio_result = {
                        let mut engine = gsv.lock().await;
                        engine.synthesize_streaming(tts_request, dialogue_id, tx).await
                    };
                    
                    let _ = receiver_handle.await;
                    
                    if !audio_result.success {
                        let err_msg = audio_result.error.unwrap_or_else(|| "未知错误".to_string());
                        error!("TTS 合成失败: {}", err_msg);
                        broadcast_log(&bc, "ERROR", &format!("TTS 合成失败: {}", err_msg));
                    }
                });
            }));
        }
        
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    let chunk_str = String::from_utf8_lossy(&chunk);
                    buffer.push_str(&chunk_str);
                    
                    while let Some(newline_pos) = buffer.find('\n') {
                        let line = buffer[..newline_pos].trim().to_string();
                        buffer = buffer[newline_pos + 1..].to_string();
                        
                        if line.is_empty() {
                            continue;
                        }
                        
                        let json_str = if line.starts_with("data: ") {
                            &line[6..]
                        } else {
                            &line
                        };
                        
                        if json_str == "[DONE]" {
                            let done_msg = "流式响应接收完成";
                            info!("{}", done_msg);
                            broadcast_log(&broadcaster_clone, "SUCCESS", done_msg);
                            break;
                        }
                        
                        if let Ok(data) = serde_json::from_str::<Value>(json_str) {
                            let raw_content = extract_delta_content(&data);
                            if !raw_content.is_empty() {
                                total_content.push_str(&raw_content);
                                chunk_count += 1;
                                
                                let filtered = tts_parser.feed(&raw_content);
                                if !filtered.is_empty() {
                                    if !stream_started {
                                        broadcaster_clone.start_stream();
                                        stream_started = true;
                                        debug!("检测到 response 字段，开始字幕流");
                                    }
                                    debug!("字幕内容: {} 字符", filtered.len());
                                    broadcaster_clone.add_chunk(&filtered);
                                }
                            }
                            
                            let ollama_response = if is_chat {
                                json!({
                                    "model": model_for_task,
                                    "created_at": chrono::Utc::now().to_rfc3339(),
                                    "message": {
                                        "role": "assistant",
                                        "content": raw_content
                                    },
                                    "done": false
                                })
                            } else {
                                json!({
                                    "model": model_for_task,
                                    "created_at": chrono::Utc::now().to_rfc3339(),
                                    "response": raw_content,
                                    "done": false
                                })
                            };
                            
                            let line = format!("{}\n", ollama_response);
                            if tx.send(Ok(line)).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    let err_msg = format!("读取流错误: {}", e);
                    error!("{}", err_msg);
                    broadcast_log(&broadcaster_clone, "ERROR", &err_msg);
                    break;
                }
            }
        }
        
        if stream_started {
            broadcaster_clone.end_stream();
        }
        
        let final_response = if is_chat {
            json!({
                "model": model_for_task,
                "created_at": chrono::Utc::now().to_rfc3339(),
                "message": {
                    "role": "assistant",
                    "content": ""
                },
                "done": true
            })
        } else {
            json!({
                "model": model_for_task,
                "created_at": chrono::Utc::now().to_rfc3339(),
                "response": "",
                "done": true
            })
        };
        
        let _ = tx.send(Ok(format!("{}\n", final_response))).await;
        
        let stats_msg = format!("流式响应统计: {} 个数据块, {} 字符", chunk_count, total_content.len());
        info!("{}", stats_msg);
        broadcast_log(&broadcaster_clone, "INFO", &stats_msg);
    });
    
    let stream = ReceiverStream::new(rx);
    let body = Body::from_stream(stream);
    
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/x-ndjson")
        .header("Transfer-Encoding", "chunked")
        .body(body)
        .unwrap()
}

/// 处理假流式响应（带 -preview 后缀：内部流式接收，实时显示字幕，返回完整响应）
/// 
/// 这种模式用于：
/// - 游戏请求 stream=false，但模型名带 -preview 后缀
/// - 内部使用流式接收内容，实时发送到字幕窗口显示
/// - 等待全部完成后返回完整响应给游戏
async fn process_fake_stream_response(
    response: reqwest::Response,
    model_name: String,
    is_chat: bool,
    broadcaster: Arc<SseBroadcaster>,
    gsv_engine: Option<Arc<Mutex<GPTSoVITSEngine>>>,
    tts_manager: Option<Arc<Mutex<TTSManager>>>,
) -> Response<Body> {
    let msg = "开始处理假流式响应（内部流式接收，实时显示字幕，返回完整响应）...";
    info!("{}", msg);
    broadcast_log(&broadcaster, "INFO", msg);
    
    let tts_enabled = gsv_engine.is_some() && tts_manager.is_some();
    if tts_enabled {
        info!("TTS 已启用（假流式模式）");
    }
    
    let mut total_content = String::new();
    let mut chunk_count = 0;
    let mut tts_parser = StreamingTTSParser::new();
    let mut stream_started = false;
    
    if tts_enabled {
        let gsv_engine_clone = gsv_engine.clone().unwrap();
        let tts_manager_clone = tts_manager.clone().unwrap();
        let broadcaster_for_tts = broadcaster.clone();
        
        tts_parser.set_callback(Box::new(move |segment| {
            let text = segment.text.clone();
            let speaker = segment.speaker.clone();
            let emotion = segment.emotion.clone();
            let emotion_desc = segment.emotion_description.clone();
            let speed = segment.speed;
            let gender = segment.gender.clone();
            let emotion_vector = segment.emotion_vector.clone();
            let is_nsfw = segment.is_nsfw;
            
            info!("TTS 触发（假流式）: [{}] {}...", speaker, text.chars().take(30).collect::<String>());
            
            let gsv = gsv_engine_clone.clone();
            let mgr = tts_manager_clone.clone();
            let bc = broadcaster_for_tts.clone();
            
            tauri::async_runtime::spawn(async move {
                let tts_request = TtsRequest {
                    text: text.clone(),
                    speaker,
                    gender,
                    emotion,
                    emotion_description: emotion_desc,
                    speed,
                    emotion_vector,
                    is_nsfw,
                };
                
                let dialogue_id = next_dialogue_id();
                let (tx, mut rx) = tokio::sync::mpsc::channel::<crate::models::tts_types::TtsSegmentAudio>(16);
                
                let mgr_for_receiver = mgr.clone();
                let bc_for_receiver = bc.clone();
                
                let receiver_handle = tokio::spawn(async move {
                    let mut received_count = 0;
                    let mut total_bytes = 0;
                    while let Some(segment_audio) = rx.recv().await {
                        received_count += 1;
                        total_bytes += segment_audio.audio_data.len();
                        
                        let mut manager = mgr_for_receiver.lock().await;
                        manager.enqueue_audio_buffered(
                            dialogue_id,
                            segment_audio.seq,
                            segment_audio.total,
                            segment_audio.audio_data,
                        );
                        
                        if segment_audio.is_last {
                            broadcast_log(&bc_for_receiver, "SUCCESS", 
                                &format!("TTS 流式合成完成: {} 段, {} 字节", received_count, total_bytes));
                        }
                    }
                });
                
                let audio_result = {
                    let mut engine = gsv.lock().await;
                    engine.synthesize_streaming(tts_request, dialogue_id, tx).await
                };
                
                let _ = receiver_handle.await;
                
                if !audio_result.success {
                    let err_msg = audio_result.error.unwrap_or_else(|| "未知错误".to_string());
                    error!("TTS 合成失败: {}", err_msg);
                    broadcast_log(&bc, "ERROR", &format!("TTS 合成失败: {}", err_msg));
                }
            });
        }));
    }
    
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                let chunk_str = String::from_utf8_lossy(&chunk);
                buffer.push_str(&chunk_str);
                
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();
                    
                    if line.is_empty() {
                        continue;
                    }
                    
                    let json_str = if line.starts_with("data: ") {
                        &line[6..]
                    } else {
                        &line
                    };
                    
                    if json_str == "[DONE]" {
                        break;
                    }
                    
                    if let Ok(data) = serde_json::from_str::<Value>(json_str) {
                        let raw_content = extract_delta_content(&data);
                        if !raw_content.is_empty() {
                            total_content.push_str(&raw_content);
                            chunk_count += 1;
                            
                            let filtered = tts_parser.feed(&raw_content);
                            if !filtered.is_empty() {
                                if !stream_started {
                                    broadcaster.start_stream();
                                    stream_started = true;
                                    debug!("假流式：检测到 response 字段，开始字幕流");
                                }
                                debug!("假流式字幕内容: {} 字符", filtered.len());
                                broadcaster.add_chunk(&filtered);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("读取流错误: {}", e);
                error!("{}", err_msg);
                broadcast_log(&broadcaster, "ERROR", &err_msg);
                if stream_started {
                    broadcaster.end_stream();
                }
                return error_response(&err_msg);
            }
        }
    }
    
    if stream_started {
        broadcaster.end_stream();
    }
    
    let stats_msg = format!("假流式响应完成: {} 个数据块, {} 字符", chunk_count, total_content.len());
    info!("{}", stats_msg);
    broadcast_log(&broadcaster, "SUCCESS", &stats_msg);
    
    let result = if is_chat {
        json!({
            "model": model_name,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "message": {
                "role": "assistant",
                "content": total_content
            },
            "done": true
        })
    } else {
        json!({
            "model": model_name,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "response": total_content,
            "done": true
        })
    };
    
    Json(result).into_response()
}

/// 处理非流式响应（假流式：收到完整响应后一次性发送内容到字幕窗口）
/// 
/// 不带后缀时：收到完整响应，然后模拟流式发送到字幕窗口
async fn process_non_stream_response(
    response: reqwest::Response,
    model_name: String,
    is_chat: bool,
    broadcaster: Arc<SseBroadcaster>,
    gsv_engine: Option<Arc<Mutex<GPTSoVITSEngine>>>,
    tts_manager: Option<Arc<Mutex<TTSManager>>>,
) -> Response<Body> {
    let msg = "处理非流式响应（假流式：收到完整响应后显示字幕）...";
    info!("{}", msg);
    broadcast_log(&broadcaster, "INFO", msg);
    
    let tts_enabled = gsv_engine.is_some() && tts_manager.is_some();
    
    match response.json::<Value>().await {
        Ok(data) => {
            let raw_content = extract_response_content(&data);
            let len_msg = format!("响应长度: {} 字符", raw_content.len());
            info!("{}", len_msg);
            broadcast_log(&broadcaster, "INFO", &len_msg);
            
            let filtered_content = extract_response_only(&raw_content);
            debug!("过滤后字幕内容: {} 字符", filtered_content.len());
            
            if !filtered_content.is_empty() {
                broadcaster.start_stream();
                broadcaster.add_chunk(&filtered_content);
                broadcaster.end_stream();
            }
            
            if tts_enabled {
                let gsv = gsv_engine.clone().unwrap();
                let mgr = tts_manager.clone().unwrap();
                let bc = broadcaster.clone();
                
                let mut tts_parser = StreamingTTSParser::new();
                let gsv_for_cb = gsv.clone();
                let mgr_for_cb = mgr.clone();
                let bc_for_cb = bc.clone();
                
                tts_parser.set_callback(Box::new(move |segment| {
                    let text = segment.text.clone();
                    let speaker = segment.speaker.clone();
                    let emotion = segment.emotion.clone();
                    let emotion_desc = segment.emotion_description.clone();
                    let speed = segment.speed;
                    let gender = segment.gender.clone();
                    let emotion_vector = segment.emotion_vector.clone();
                    let is_nsfw = segment.is_nsfw;
                    
                    info!("TTS 触发（非流式）: [{}] {}...", speaker, text.chars().take(30).collect::<String>());
                    
                    let gsv = gsv_for_cb.clone();
                    let mgr = mgr_for_cb.clone();
                    let bc = bc_for_cb.clone();
                    
                    tauri::async_runtime::spawn(async move {
                        let tts_request = TtsRequest {
                            text: text.clone(),
                            speaker,
                            gender,
                            emotion,
                            emotion_description: emotion_desc,
                            speed,
                            emotion_vector,
                            is_nsfw,
                        };
                        
                        let dialogue_id = next_dialogue_id();
                        let (tx, mut rx) = tokio::sync::mpsc::channel::<crate::models::tts_types::TtsSegmentAudio>(16);
                        
                        let mgr_for_receiver = mgr.clone();
                        let bc_for_receiver = bc.clone();
                        
                        let receiver_handle = tokio::spawn(async move {
                            let mut received_count = 0;
                            let mut total_bytes = 0;
                            while let Some(segment_audio) = rx.recv().await {
                                received_count += 1;
                                total_bytes += segment_audio.audio_data.len();
                                
                                let mut manager = mgr_for_receiver.lock().await;
                                manager.enqueue_audio_buffered(
                                    dialogue_id,
                                    segment_audio.seq,
                                    segment_audio.total,
                                    segment_audio.audio_data,
                                );
                                
                                if segment_audio.is_last {
                                    broadcast_log(&bc_for_receiver, "SUCCESS", 
                                        &format!("TTS 流式合成完成: {} 段, {} 字节", received_count, total_bytes));
                                }
                            }
                        });
                        
                        let audio_result = {
                            let mut engine = gsv.lock().await;
                            engine.synthesize_streaming(tts_request, dialogue_id, tx).await
                        };
                        
                        let _ = receiver_handle.await;
                        
                        if !audio_result.success {
                            let err_msg = audio_result.error.unwrap_or_else(|| "未知错误".to_string());
                            error!("TTS 合成失败: {}", err_msg);
                            broadcast_log(&bc, "ERROR", &format!("TTS 合成失败: {}", err_msg));
                        }
                    });
                }));
                
                tts_parser.feed(&raw_content);
            }
            
            let result = if is_chat {
                json!({
                    "model": model_name,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                    "message": {
                        "role": "assistant",
                        "content": raw_content
                    },
                    "done": true
                })
            } else {
                json!({
                    "model": model_name,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                    "response": raw_content,
                    "done": true
                })
            };
            
            Json(result).into_response()
        }
        Err(e) => {
            let err_msg = format!("解析响应失败: {}", e);
            error!("{}", err_msg);
            broadcast_log(&broadcaster, "ERROR", &err_msg);
            error_response(&format!("解析响应失败: {}", e))
        }
    }
}

/// 从流式响应中提取 delta 内容
fn extract_delta_content(data: &Value) -> String {
    // OpenAI 格式: choices[0].delta.content
    if let Some(choices) = data.get("choices").and_then(|v| v.as_array()) {
        if let Some(first) = choices.first() {
            if let Some(delta) = first.get("delta").and_then(|v| v.as_object()) {
                if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                    return content.to_string();
                }
            }
        }
    }
    
    // Ollama 格式: message.content 或 response
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

/// 创建错误响应
fn error_response(message: &str) -> Response<Body> {
    let body = json!({ "error": message });
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn fallback_handler(
    State(state): State<ProxyAppState>,
    request: Request,
) -> Response<Body> {
    let method = request.method().to_string();
    let uri = request.uri().to_string();
    let msg = format!("未知请求: {} {} (404)", method, uri);
    warn!("{}", msg);
    broadcast_log(&state.broadcaster, "WARN", &msg);
    
    let body = json!({ "error": format!("Not Found: {} {}", method, uri) });
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Ollama 健康检查
///
/// GET /
async fn health_check() -> &'static str {
    "Ollama is running"
}

/// Ollama 版本信息
///
/// GET /api/version
async fn get_version() -> Json<Value> {
    Json(json!({ "version": "0.6.2" }))
}

/// 创建代理路由器
// ==================== OpenAI 兼容接口 ====================

/// OpenAI 格式的 chat completions 响应（非流式）
fn openai_chat_response(model: &str, content: &str) -> Response<Body> {
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..24].to_string());
    let body = json!({
        "id": id,
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": content,
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0
        }
    });
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// OpenAI 格式的 chat completions SSE 流式响应
fn openai_chat_stream_response(model: &str, content: &str) -> Response<Body> {
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..24].to_string());
    let chunk = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "delta": {
                "role": "assistant",
                "content": content,
            },
            "finish_reason": Value::Null
        }]
    });
    let done_chunk = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "delta": {},
            "finish_reason": "stop"
        }]
    });
    let body = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        chunk, done_chunk
    );
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .body(Body::from(body))
        .unwrap()
}

/// OpenAI 格式的错误响应
fn openai_error_response(message: &str) -> Response<Body> {
    let body = json!({
        "error": {
            "message": message,
            "type": "server_error",
            "code": "internal_error"
        }
    });
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// 处理 OpenAI 格式的 chat completions 请求
///
/// POST /v1/chat/completions
/// POST /chat/completions
async fn openai_chat_completions(
    State(state): State<ProxyAppState>,
    body: axum::body::Bytes,
) -> Response<Body> {
    // 用 Value 解析，保留所有原始字段不丢失
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            let err = format!("JSON 解析失败: {} (body_len={})", e, body.len());
            error!("{}", err);
            return openai_error_response(&err);
        }
    };

    let model = raw.get("model").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let stream = raw.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);
    let config = state.config_store.get();
    let base_model = &config.model_name;
    let (real_model, is_fake_stream) = detect_fake_stream(&model, base_model);
    let should_stream = stream || is_fake_stream;

    let msg = format!(
        "收到 OpenAI chat/completions 请求: model={}, stream={}, 假流式={}, body_len={}",
        model, should_stream, is_fake_stream, body.len()
    );
    info!("{}", msg);
    broadcast_log(&state.broadcaster, "INFO", &msg);

    // Antigravity 桥接
    if config.antigravity_bridge_enabled {
        // 按照 antigravity 示范格式构建 prompt：
        // 1. 只包含 messages 数组（去掉 frequency_penalty, max_tokens 等）
        // 2. 每个 message 对象中 role 在 content 前面
        // 3. Pretty-printed JSON 格式
        let bridge_prompt = {
            let messages = raw.get("messages").and_then(|v| v.as_array());
            match messages {
                Some(msgs) => {
                    // 手动构建 JSON 字符串，控制 key 顺序：role 在 content 前面
                    let mut result = String::from("{\n  \"messages\": [\n");
                    for (i, msg) in msgs.iter().enumerate() {
                        let obj = msg.as_object();
                        result.push_str("    {\n");
                        if let Some(map) = obj {
                            // 先输出 role，再输出 content，最后输出其他字段
                            let role = map.get("role");
                            let content = map.get("content");
                            let mut fields: Vec<String> = Vec::new();
                            if let Some(r) = role {
                                fields.push(format!("      \"role\": {}", serde_json::to_string(r).unwrap_or_default()));
                            }
                            if let Some(c) = content {
                                fields.push(format!("      \"content\": {}", serde_json::to_string(c).unwrap_or_default()));
                            }
                            // 其他字段（如 name 等）
                            for (k, v) in map {
                                if k != "role" && k != "content" {
                                    fields.push(format!("      \"{}\": {}", k, serde_json::to_string(v).unwrap_or_default()));
                                }
                            }
                            result.push_str(&fields.join(",\n"));
                            result.push('\n');
                        }
                        result.push_str("    }");
                        if i < msgs.len() - 1 {
                            result.push(',');
                        }
                        result.push('\n');
                    }
                    result.push_str("  ]\n}");
                    result
                }
                None => {
                    // 如果没有 messages，回退到 pretty-print 整个 JSON
                    serde_json::to_string_pretty(&raw).unwrap_or_default()
                }
            }
        };
        if bridge_prompt.trim().is_empty() {
            return openai_error_response("bridge input is empty");
        }
        info!("OpenAI 桥接 prompt 长度: {} bytes, 前500字符: {}", bridge_prompt.len(), bridge_prompt.chars().take(500).collect::<String>());

        match execute_bridge_waitback(
            &state,
            bridge_prompt,
            config.antigravity_timeout_ms,
            config.antigravity_max_retries,
            config.antigravity_pending_submit_fallback_enabled,
            config.antigravity_include_thinking,
        )
        .await
        {
            Ok(content) => {
                broadcast_log(&state.broadcaster, "SUCCESS", "Antigravity 桥接成功 (OpenAI 格式)");
                if should_stream {
                    return openai_chat_stream_response(&model, &content);
                }
                return openai_chat_response(&model, &content);
            }
            Err(err) => {
                let err_msg = format!("Antigravity 桥接失败: {}", err);
                error!("{}", err_msg);
                broadcast_log(&state.broadcaster, "ERROR", &err_msg);
                return openai_error_response(&err_msg);
            }
        }
    }

    // 走外部 API — 需要反序列化为 ChatRequest
    if config.api_url.is_empty() {
        return openai_error_response("API URL 未配置");
    }

    let llm = LlmClient::new(config.api_url.clone(), config.api_key.clone());
    let messages: Vec<ChatMessage> = raw.get("messages")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let extra: Option<Value> = {
        let mut obj = if let Value::Object(m) = raw { m } else { serde_json::Map::new() };
        obj.remove("model");
        obj.remove("messages");
        obj.remove("stream");
        if obj.is_empty() { None } else { Some(Value::Object(obj)) }
    };
    let chat_request = ChatRequest {
        model: real_model.clone(),
        messages,
        stream: should_stream,
        extra,
    };

    match llm.chat(chat_request, Some(&config.system_prompt_injection), Some(base_model)).await {
        Ok(result) => {
            if result.is_streaming {
                // 转发外部流式响应（原始格式可能已是 OpenAI 格式）
                let stream = result.response.bytes_stream();
                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/event-stream")
                    .header("Cache-Control", "no-cache")
                    .body(Body::from_stream(stream))
                    .unwrap()
            } else {
                // 非流式：解析响应并转换为 OpenAI 格式
                match result.response.text().await {
                    Ok(text) => {
                        let content = if let Ok(data) = serde_json::from_str::<Value>(&text) {
                            extract_response_content(&data)
                        } else {
                            text
                        };
                        openai_chat_response(&real_model, &content)
                    }
                    Err(e) => openai_error_response(&format!("读取响应失败: {}", e)),
                }
            }
        }
        Err(err) => openai_error_response(&err),
    }
}

/// OpenAI models 列表
///
/// GET /v1/models
async fn openai_models(
    State(state): State<ProxyAppState>,
) -> Response<Body> {
    let config = state.config_store.get();
    let model_name = if config.model_name.is_empty() {
        "antigravity".to_string()
    } else {
        config.model_name.clone()
    };
    let body = json!({
        "object": "list",
        "data": [{
            "id": model_name,
            "object": "model",
            "created": chrono::Utc::now().timestamp(),
            "owned_by": "local"
        }]
    });
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

pub fn create_router(state: ProxyAppState) -> Router {
    Router::new()
        .route("/", get(health_check))
        .route("/api/version", get(get_version))
        .route("/api/tags", get(get_tags))
        .route("/api/bridge/pending", get(bridge_pending))
        .route("/api/bridge/submit", post(bridge_submit))
        .route("/api/chat", post(chat))
        .route("/api/generate", post(generate))
        // OpenAI 兼容路由
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/chat/completions", post(openai_chat_completions))
        .route("/v1/models", get(openai_models))
        .fallback(fallback_handler)
        .with_state(state)
}
