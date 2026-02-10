//! TTS API 端点
//!
//! 提供 TTS 相关的 HTTP API：
//! - POST /api/tts/synthesize - 合成语音
//! - POST /api/tts/stop - 停止当前播放
//! - POST /api/tts/stop-all - 停止所有播放
//! - POST /api/tts/volume - 设置音量
//! - GET /api/tts/status - 获取状态
//! - POST /api/tts/test - 测试引擎

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, error};

use crate::server::AppState;
use crate::models::tts_types::TtsRequest;

/// 合成请求
#[derive(Debug, Deserialize)]
pub struct SynthesizeRequest {
    pub text: String,
    #[serde(default)]
    pub speaker: String,
    #[serde(default)]
    pub gender: String,
    #[serde(default = "default_emotion")]
    pub emotion: String,
    #[serde(default)]
    pub emotion_description: String,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default)]
    pub emotion_vector: Option<HashMap<String, f32>>,
    #[serde(default)]
    pub is_nsfw: bool,
    #[serde(default)]
    pub auto_play: Option<bool>,
}

fn default_emotion() -> String {
    "neutral".to_string()
}

fn default_speed() -> f32 {
    1.0
}

/// 合成响应
#[derive(Debug, Serialize)]
pub struct SynthesizeResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion_used: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_count: Option<usize>,
}

/// 音量请求
#[derive(Debug, Deserialize)]
pub struct VolumeRequest {
    pub volume: f32,
}

/// TTS 状态响应
#[derive(Debug, Serialize)]
pub struct TtsStatusResponse {
    pub enabled: bool,
    pub auto_play: bool,
    pub playing: bool,
    pub is_playing: bool,
    pub is_synthesizing: bool,
    pub queue_size: usize,
    pub volume: f32,
    pub engine_available: bool,
}

/// 测试响应
#[derive(Debug, Serialize)]
pub struct TestResponse {
    pub success: bool,
    pub message: String,
    pub available: bool,
}

/// 测试引擎请求
#[derive(Debug, Deserialize, Default)]
pub struct TestEngineRequest {
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// POST /api/tts/synthesize - 合成语音
pub async fn synthesize(
    State(state): State<AppState>,
    Json(request): Json<SynthesizeRequest>,
) -> Response {
    info!("收到 TTS 合成请求: text={}, speaker={}, emotion={}", 
          request.text.chars().take(30).collect::<String>(),
          request.speaker,
          request.emotion);

    let auto_play_requested = request.auto_play;
    
    let tts_request = TtsRequest {
        text: request.text,
        speaker: request.speaker,
        gender: request.gender,
        emotion: request.emotion,
        emotion_description: request.emotion_description,
        speed: request.speed,
        emotion_vector: request.emotion_vector,
        is_nsfw: request.is_nsfw,
    };

    let result = {
        let mut gsv_engine = state.gsv_engine.lock().await;
        gsv_engine.synthesize(tts_request).await
    };

    if result.success {
        let mut tts_manager = state.tts_manager.lock().await;
        let should_auto_play = auto_play_requested.unwrap_or(tts_manager.auto_play);
        
        let audio_file = if let Some(ref data) = result.audio_data {
            if should_auto_play {
                tts_manager.play_audio(data.clone());
            }
            tts_manager.save_audio_to_file(data, None)
        } else if let Some(ref segments) = result.audio_segments {
            let mut saved_file = None;
            
            for (i, segment) in segments.iter().enumerate() {
                if should_auto_play {
                    tts_manager.enqueue_audio(segment.clone());
                }
                if i == 0 {
                    saved_file = tts_manager.save_audio_to_file(segment, None);
                }
            }
            saved_file
        } else {
            None
        };

        let segment_count = result.audio_segments.as_ref().map(|s: &Vec<Vec<u8>>| s.len());

        (StatusCode::OK, Json(SynthesizeResponse {
            success: true,
            error: None,
            audio_file,
            speaker_used: Some(result.speaker_used),
            emotion_used: Some(result.emotion_used),
            segment_count,
        })).into_response()
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(SynthesizeResponse {
            success: false,
            error: result.error,
            audio_file: None,
            speaker_used: None,
            emotion_used: None,
            segment_count: None,
        })).into_response()
    }
}

/// POST /api/tts/stop - 停止当前播放
pub async fn stop(
    State(state): State<AppState>,
) -> Response {
    info!("停止当前 TTS 播放");
    
    let tts_manager = state.tts_manager.lock().await;
    tts_manager.stop();

    (StatusCode::OK, Json(serde_json::json!({
        "success": true,
        "message": "已停止当前播放"
    }))).into_response()
}

/// POST /api/tts/stop-all - 停止所有播放
pub async fn stop_all(
    State(state): State<AppState>,
) -> Response {
    info!("停止所有 TTS 播放");
    
    let tts_manager = state.tts_manager.lock().await;
    tts_manager.stop_all();

    (StatusCode::OK, Json(serde_json::json!({
        "success": true,
        "message": "已停止所有播放并清空队列"
    }))).into_response()
}

/// POST /api/tts/volume - 设置音量
pub async fn set_volume(
    State(state): State<AppState>,
    Json(request): Json<VolumeRequest>,
) -> Response {
    let volume = request.volume.clamp(0.0, 1.0);
    info!("设置 TTS 音量: {:.0}%", volume * 100.0);
    
    let mut tts_manager = state.tts_manager.lock().await;
    tts_manager.set_volume(volume);

    (StatusCode::OK, Json(serde_json::json!({
        "success": true,
        "volume": volume
    }))).into_response()
}

/// GET /api/tts/status - 获取 TTS 状态
pub async fn get_status(
    State(state): State<AppState>,
) -> Response {
    let tts_manager = state.tts_manager.lock().await;
    let player_status = tts_manager.get_status();
    let enabled = tts_manager.enabled;
    let auto_play = tts_manager.auto_play;
    drop(tts_manager);
    
    let gsv_engine = state.gsv_engine.lock().await;
    let engine_available = gsv_engine.test_connection().await;

    (StatusCode::OK, Json(TtsStatusResponse {
        enabled,
        auto_play,
        playing: player_status.playing,
        is_playing: player_status.playing,
        is_synthesizing: false,
        queue_size: player_status.queue_size,
        volume: player_status.volume,
        engine_available,
    })).into_response()
}

/// URL 验证函数
fn validate_tts_url(url: &str) -> Result<String, &'static str> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "URL 格式无效")?;
    
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("只支持 HTTP/HTTPS 协议");
    }
    
    let host = parsed.host_str().ok_or("缺少主机名")?;
    
    if host == "169.254.169.254" || host.ends_with(".internal") {
        return Err("不允许访问内部服务");
    }
    
    Ok(url.to_string())
}

/// POST /api/tts/test - 测试 TTS 引擎
pub async fn test_engine(
    State(state): State<AppState>,
    Json(request): Json<TestEngineRequest>,
) -> Response {
    info!("测试 TTS 引擎连接 - 收到请求: engine={:?}, url={:?}", request.engine, request.url);
    
    let mut gsv_engine = state.gsv_engine.lock().await;
    
    let original_url = gsv_engine.get_api_url().to_string();
    info!("当前引擎 URL: {}", original_url);
    
    let is_temp_test = request.url.as_ref().map_or(false, |u| !u.is_empty());
    
    let test_url = if let Some(ref url) = request.url {
        if !url.is_empty() {
            info!("使用请求中的 URL: {}", url);
            match validate_tts_url(url) {
                Ok(valid_url) => {
                    gsv_engine.set_api_url(&valid_url);
                    valid_url
                }
                Err(e) => {
                    return (StatusCode::BAD_REQUEST, Json(TestResponse {
                        success: false,
                        message: format!("URL 验证失败: {}", e),
                        available: false,
                    })).into_response();
                }
            }
        } else {
            info!("URL 为空，使用配置中的 URL");
            let config = state.config_store.get();
            gsv_engine.initialize(&config);
            config.gsv_api_url.clone()
        }
    } else {
        info!("未提供 URL，使用配置中的 URL");
        let config = state.config_store.get();
        gsv_engine.initialize(&config);
        config.gsv_api_url.clone()
    };
    
    info!("最终使用 API 地址: {}", test_url);
    let available = gsv_engine.test_connection().await;
    info!("连接测试结果: {}", available);

    if available && is_temp_test {
        let mut config = state.config_store.get();
        config.gsv_api_url = test_url.clone();
        if let Err(e) = state.config_store.update(config.clone()) {
            error!("保存 GSV API URL 失败: {}", e);
        } else {
            gsv_engine.initialize(&config);
            info!("测试成功，已保存 GSV API URL: {}", test_url);
        }
    } else if !available && is_temp_test {
        gsv_engine.set_api_url(&original_url);
        info!("测试失败，恢复原始 API 地址: {}", original_url);
    }

    if available {
        (StatusCode::OK, Json(TestResponse {
            success: true,
            message: format!("GPT-SoVITS API 连接成功并已保存: {}", test_url),
            available: true,
        })).into_response()
    } else {
        (StatusCode::OK, Json(TestResponse {
            success: true,
            message: format!("GPT-SoVITS API 无法连接: {}", test_url),
            available: false,
        })).into_response()
    }
}

/// 诊断响应
#[derive(Debug, Serialize)]
pub struct DiagnoseResponse {
    pub success: bool,
    pub url: String,
    pub host: String,
    pub port: u16,
    pub tcp_reachable: bool,
    pub http_reachable: bool,
    pub http_status: Option<u16>,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// 诊断请求
#[derive(Debug, Deserialize, Default)]
pub struct DiagnoseRequest {
    #[serde(default)]
    pub url: Option<String>,
}

/// POST /api/tts/diagnose - 诊断连接问题
pub async fn diagnose_connection(
    State(state): State<AppState>,
    Json(request): Json<DiagnoseRequest>,
) -> Response {
    use std::time::Instant;
    use reqwest::Url;
    
    info!("诊断 TTS 连接");
    
    let url = request.url.unwrap_or_else(|| {
        let config = state.config_store.get();
        config.gsv_api_url.clone()
    });
    
    let parsed = match Url::parse(&url) {
        Ok(u) => u,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(DiagnoseResponse {
                success: false,
                url: url.clone(),
                host: String::new(),
                port: 0,
                tcp_reachable: false,
                http_reachable: false,
                http_status: None,
                latency_ms: None,
                error: Some(format!("URL 解析失败: {}", e)),
            })).into_response();
        }
    };
    
    let host = parsed.host_str().unwrap_or("").to_string();
    let port = parsed.port().unwrap_or(80);
    let addr = format!("{}:{}", host, port);
    
    let start = Instant::now();
    
    let tcp_result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::TcpStream::connect(&addr)
    ).await;
    
    let tcp_reachable = matches!(tcp_result, Ok(Ok(_)));
    let tcp_latency = start.elapsed().as_millis() as u64;
    
    let (http_reachable, http_status, http_error) = if tcp_reachable {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        
        let base_url = format!("{}://{}:{}", parsed.scheme(), host, port);
        
        match client.get(&base_url).send().await {
            Ok(response) => (true, Some(response.status().as_u16()), None),
            Err(e) => (false, None, Some(format!("{}", e))),
        }
    } else {
        let err = match tcp_result {
            Ok(Err(e)) => format!("TCP 连接失败: {}", e),
            Err(_) => "TCP 连接超时".to_string(),
            _ => "未知错误".to_string(),
        };
        (false, None, Some(err))
    };
    
    (StatusCode::OK, Json(DiagnoseResponse {
        success: tcp_reachable,
        url,
        host,
        port,
        tcp_reachable,
        http_reachable,
        http_status,
        latency_ms: Some(tcp_latency),
        error: http_error,
    })).into_response()
}
