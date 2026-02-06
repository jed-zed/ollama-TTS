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
use crate::models::tts_types::TtsRequest;

/// 代理服务器专用状态（不包含 proxy_manager 避免循环依赖）
#[derive(Clone)]
pub struct ProxyAppState {
    pub config_store: Arc<ConfigStore>,
    pub broadcaster: Arc<SseBroadcaster>,
    pub gsv_engine: Option<Arc<Mutex<GPTSoVITSEngine>>>,
    pub tts_manager: Option<Arc<Mutex<TTSManager>>>,
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

/// 获取模型列表
/// 
/// GET /api/tags
async fn get_tags(State(state): State<ProxyAppState>) -> Json<TagsResponse> {
    let config = state.config_store.get();
    let base_model = if config.model_name.is_empty() {
        "default:latest".to_string()
    } else {
        config.model_name.clone()
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
    
    if config.api_url.is_empty() {
        let err_msg = "API URL 未配置";
        error!("{}", err_msg);
        broadcast_log(&state.broadcaster, "ERROR", err_msg);
        return error_response(err_msg);
    }
    
    let client = LlmClient::new(config.api_url.clone(), config.api_key.clone());
    
    let chat_request = ChatRequest {
        model: request.model.clone(),
        messages: request.messages,
        stream: request.stream,
        extra: request.extra,
    };
    
    let injection = if config.system_prompt_injection.is_empty() {
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
    
    if config.api_url.is_empty() {
        let err_msg = "API URL 未配置";
        error!("{}", err_msg);
        broadcast_log(&state.broadcaster, "ERROR", err_msg);
        return error_response(err_msg);
    }
    
    let client = LlmClient::new(config.api_url.clone(), config.api_key.clone());
    
    let generate_request = GenerateRequest {
        model: request.model.clone(),
        prompt: request.prompt,
        stream: request.stream,
        extra: request.extra,
    };
    
    let injection = if config.system_prompt_injection.is_empty() {
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
                    
                    let audio_result = {
                        let mut engine = gsv.lock().await;
                        engine.synthesize(tts_request).await
                    };
                    
                    if audio_result.success {
                        let mut manager = mgr.lock().await;
                        
                        if let Some(segments) = audio_result.audio_segments {
                            let total_bytes: usize = segments.iter().map(|s| s.len()).sum();
                            info!("TTS 合成成功: {} 段, 共 {} 字节", segments.len(), total_bytes);
                            broadcast_log(&bc, "SUCCESS", &format!("TTS 合成完成: {} 段, {} 字节", segments.len(), total_bytes));
                            
                            for (i, segment_data) in segments.into_iter().enumerate() {
                                info!("播放段落 {}: {} 字节", i + 1, segment_data.len());
                                manager.play_audio(segment_data);
                            }
                        } else if let Some(audio_data) = audio_result.audio_data {
                            info!("TTS 合成成功: {} 字节", audio_data.len());
                            broadcast_log(&bc, "SUCCESS", &format!("TTS 合成完成: {} 字节", audio_data.len()));
                            manager.play_audio(audio_data);
                        }
                    } else {
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
                                    "message": {
                                        "role": "assistant",
                                        "content": raw_content
                                    },
                                    "done": false
                                })
                            } else {
                                json!({
                                    "model": model_for_task,
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
                "message": {
                    "role": "assistant",
                    "content": ""
                },
                "done": true
            })
        } else {
            json!({
                "model": model_for_task,
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
                
                let audio_result = {
                    let mut engine = gsv.lock().await;
                    engine.synthesize(tts_request).await
                };
                
                if audio_result.success {
                    let mut manager = mgr.lock().await;
                    
                    if let Some(segments) = audio_result.audio_segments {
                        let total_bytes: usize = segments.iter().map(|s| s.len()).sum();
                        info!("TTS 合成成功（假流式）: {} 段, 共 {} 字节", segments.len(), total_bytes);
                        broadcast_log(&bc, "SUCCESS", &format!("TTS 合成完成: {} 段, {} 字节", segments.len(), total_bytes));
                        
                        for (i, segment_data) in segments.into_iter().enumerate() {
                            info!("播放段落 {}: {} 字节", i + 1, segment_data.len());
                            manager.play_audio(segment_data);
                        }
                    } else if let Some(audio_data) = audio_result.audio_data {
                        info!("TTS 合成成功: {} 字节", audio_data.len());
                        broadcast_log(&bc, "SUCCESS", &format!("TTS 合成完成: {} 字节", audio_data.len()));
                        manager.play_audio(audio_data);
                    }
                } else {
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
                        
                        let audio_result = {
                            let mut engine = gsv.lock().await;
                            engine.synthesize(tts_request).await
                        };
                        
                        if audio_result.success {
                            let mut manager = mgr.lock().await;
                            
                            if let Some(segments) = audio_result.audio_segments {
                                let total_bytes: usize = segments.iter().map(|s| s.len()).sum();
                                info!("TTS 合成成功（非流式）: {} 段, 共 {} 字节", segments.len(), total_bytes);
                                broadcast_log(&bc, "SUCCESS", &format!("TTS 合成完成: {} 段, {} 字节", segments.len(), total_bytes));
                                
                                for (i, segment_data) in segments.into_iter().enumerate() {
                                    info!("播放段落 {}: {} 字节", i + 1, segment_data.len());
                                    manager.play_audio(segment_data);
                                }
                            } else if let Some(audio_data) = audio_result.audio_data {
                                info!("TTS 合成成功: {} 字节", audio_data.len());
                                broadcast_log(&bc, "SUCCESS", &format!("TTS 合成完成: {} 字节", audio_data.len()));
                                manager.play_audio(audio_data);
                            }
                        } else {
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

/// 创建代理路由器
pub fn create_router(state: ProxyAppState) -> Router {
    Router::new()
        .route("/api/tags", get(get_tags))
        .route("/api/chat", post(chat))
        .route("/api/generate", post(generate))
        .fallback(fallback_handler)
        .with_state(state)
}
