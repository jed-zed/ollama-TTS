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
use tracing::{error, info};

use crate::models::tts_types::TtsRequest;
use crate::server::AppState;
use crate::services::tts_gsv::GPTSoVITSEngine;

const MAX_SYNTHESIS_TEXT_CHARS: usize = 20_000;
const STATUS_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

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

fn validate_synthesize_request(request: &SynthesizeRequest) -> Result<(), String> {
    let text_len = request.text.chars().count();
    if request.text.trim().is_empty() {
        return Err("合成文本不能为空".to_string());
    }
    if text_len > MAX_SYNTHESIS_TEXT_CHARS {
        return Err(format!(
            "合成文本不能超过 {} 个字符",
            MAX_SYNTHESIS_TEXT_CHARS
        ));
    }
    if !request.speed.is_finite() || !(0.1..=4.0).contains(&request.speed) {
        return Err("语速必须是 0.1 到 4.0 之间的有限数值".to_string());
    }
    if request
        .emotion_vector
        .as_ref()
        .is_some_and(|values| values.values().any(|value| !value.is_finite()))
    {
        return Err("情感向量必须包含有限数值".to_string());
    }
    Ok(())
}

fn synthesize_error(status: StatusCode, error: impl Into<String>) -> Response {
    (
        status,
        Json(SynthesizeResponse {
            success: false,
            error: Some(error.into()),
            audio_file: None,
            speaker_used: None,
            emotion_used: None,
            segment_count: None,
        }),
    )
        .into_response()
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
    if let Err(error) = validate_synthesize_request(&request) {
        return synthesize_error(StatusCode::BAD_REQUEST, error);
    }

    info!(
        "收到 TTS 合成请求: text={}, speaker={}, emotion={}",
        request.text.chars().take(30).collect::<String>(),
        request.speaker,
        request.emotion
    );

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
        let segment_count = result
            .audio_segments
            .as_ref()
            .map(Vec::len)
            .or_else(|| result.audio_data.as_ref().map(|_| 1))
            .unwrap_or(0);
        if segment_count == 0 {
            return synthesize_error(StatusCode::BAD_GATEWAY, "TTS 引擎返回成功但没有音频数据");
        }

        let mut tts_manager = state.tts_manager.lock().await;
        let should_auto_play = auto_play_requested.unwrap_or(tts_manager.auto_play);

        let audio_file = if let Some(ref data) = result.audio_data {
            if should_auto_play {
                // 请求级 auto_play 应覆盖全局默认，因此使用已决策的直接入队接口。
                tts_manager.enqueue_audio(data.clone());
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

        (
            StatusCode::OK,
            Json(SynthesizeResponse {
                success: true,
                error: None,
                audio_file,
                speaker_used: Some(result.speaker_used),
                emotion_used: Some(result.emotion_used),
                segment_count: Some(segment_count),
            }),
        )
            .into_response()
    } else {
        synthesize_error(
            StatusCode::BAD_GATEWAY,
            result.error.unwrap_or_else(|| "TTS 合成失败".to_string()),
        )
    }
}

/// POST /api/tts/stop - 停止当前播放
pub async fn stop(State(state): State<AppState>) -> Response {
    info!("停止当前 TTS 播放");

    let tts_manager = state.tts_manager.lock().await;
    tts_manager.stop();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "message": "已停止当前播放"
        })),
    )
        .into_response()
}

/// POST /api/tts/stop-all - 停止所有播放
pub async fn stop_all(State(state): State<AppState>) -> Response {
    info!("停止所有 TTS 播放");

    let tts_manager = state.tts_manager.lock().await;
    tts_manager.stop_all();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "message": "已停止所有播放并清空队列"
        })),
    )
        .into_response()
}

/// POST /api/tts/volume - 设置音量
pub async fn set_volume(
    State(state): State<AppState>,
    Json(request): Json<VolumeRequest>,
) -> Response {
    if !request.volume.is_finite() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "音量必须是有限数值"
            })),
        )
            .into_response();
    }

    let volume = request.volume.clamp(0.0, 1.0);
    info!("设置 TTS 音量: {:.0}%", volume * 100.0);

    let mut tts_manager = state.tts_manager.lock().await;
    tts_manager.set_volume(volume);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "volume": volume
        })),
    )
        .into_response()
}

/// GET /api/tts/status - 获取 TTS 状态
pub async fn get_status(State(state): State<AppState>) -> Response {
    let tts_manager = state.tts_manager.lock().await;
    let player_status = tts_manager.get_status();
    let enabled = tts_manager.enabled;
    let auto_play = tts_manager.auto_play;
    drop(tts_manager);

    let is_synthesizing = state.gsv_engine.try_lock().is_err();
    let api_url = state.config_store.get().gsv_api_url;
    let engine_available = tokio::time::timeout(
        STATUS_PROBE_TIMEOUT,
        GPTSoVITSEngine::test_connection_url(&api_url),
    )
    .await
    .unwrap_or(false);

    (
        StatusCode::OK,
        Json(TtsStatusResponse {
            enabled,
            auto_play,
            playing: player_status.playing,
            is_playing: player_status.playing,
            is_synthesizing,
            queue_size: player_status.queue_size,
            volume: player_status.volume,
            engine_available,
        }),
    )
        .into_response()
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
    info!(
        "测试 TTS 引擎连接 - 收到请求: engine={:?}, url={:?}",
        request.engine, request.url
    );

    if request
        .engine
        .as_deref()
        .is_some_and(|engine| engine != "gpt_sovits")
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(TestResponse {
                success: false,
                message: "当前仅支持 gpt_sovits TTS 引擎".to_string(),
                available: false,
            }),
        )
            .into_response();
    }

    let configured_url = state.config_store.get().gsv_api_url;
    info!("当前引擎 URL: {}", configured_url);

    let is_temp_test = request.url.as_ref().map_or(false, |u| !u.is_empty());

    let test_url = if let Some(ref url) = request.url {
        if !url.is_empty() {
            info!("使用请求中的 URL: {}", url);
            match validate_tts_url(url) {
                Ok(valid_url) => valid_url,
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(TestResponse {
                            success: false,
                            message: format!("URL 验证失败: {}", e),
                            available: false,
                        }),
                    )
                        .into_response();
                }
            }
        } else {
            info!("URL 为空，使用配置中的 URL");
            configured_url.clone()
        }
    } else {
        info!("未提供 URL，使用配置中的 URL");
        configured_url
    };

    info!("最终使用 API 地址: {}", test_url);
    let available = GPTSoVITSEngine::test_connection_url(&test_url).await;
    info!("连接测试结果: {}", available);

    if available && is_temp_test {
        match state.config_store.modify(|config| {
            config.gsv_api_url = test_url.clone();
        }) {
            Ok(config) => {
                let mut gsv_engine = state.gsv_engine.lock().await;
                gsv_engine.initialize(&config);
                info!("测试成功，已保存 GSV API URL: {}", test_url);
            }
            Err(e) => {
                error!("保存 GSV API URL 失败: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(TestResponse {
                        success: false,
                        message: format!("连接成功，但保存 GSV API URL 失败: {}", e),
                        available: true,
                    }),
                )
                    .into_response();
            }
        }
    }

    if available {
        let message = if is_temp_test {
            format!("GPT-SoVITS API 连接成功并已保存: {}", test_url)
        } else {
            format!("GPT-SoVITS API 连接成功: {}", test_url)
        };
        (
            StatusCode::OK,
            Json(TestResponse {
                success: true,
                message,
                available: true,
            }),
        )
            .into_response()
    } else {
        (
            StatusCode::OK,
            Json(TestResponse {
                success: true,
                message: format!("GPT-SoVITS API 无法连接: {}", test_url),
                available: false,
            }),
        )
            .into_response()
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
    use reqwest::Url;
    use std::time::Instant;

    info!("诊断 TTS 连接");

    let url = request.url.unwrap_or_else(|| {
        let config = state.config_store.get();
        config.gsv_api_url.clone()
    });

    let parsed = match Url::parse(&url) {
        Ok(u) => u,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(DiagnoseResponse {
                    success: false,
                    url: url.clone(),
                    host: String::new(),
                    port: 0,
                    tcp_reachable: false,
                    http_reachable: false,
                    http_status: None,
                    latency_ms: None,
                    error: Some(format!("URL 解析失败: {}", e)),
                }),
            )
                .into_response();
        }
    };

    let host = parsed.host_str().unwrap_or("").to_string();
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addr = if host.contains(':') {
        format!("[{}]:{}", host, port)
    } else {
        format!("{}:{}", host, port)
    };

    let start = Instant::now();

    let tcp_result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::TcpStream::connect(&addr),
    )
    .await;

    let tcp_reachable = matches!(tcp_result, Ok(Ok(_)));
    let tcp_latency = start.elapsed().as_millis() as u64;

    let (http_reachable, http_status, http_error) = if tcp_reachable {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let authority_host = if host.contains(':') {
            format!("[{}]", host)
        } else {
            host.clone()
        };
        let base_url = format!("{}://{}:{}", parsed.scheme(), authority_host, port);

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

    (
        StatusCode::OK,
        Json(DiagnoseResponse {
            success: tcp_reachable,
            url,
            host,
            port,
            tcp_reachable,
            http_reachable,
            http_status,
            latency_ms: Some(tcp_latency),
            error: http_error,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_synthesis_request() -> SynthesizeRequest {
        SynthesizeRequest {
            text: "测试文本".to_string(),
            speaker: String::new(),
            gender: String::new(),
            emotion: "neutral".to_string(),
            emotion_description: String::new(),
            speed: 1.0,
            emotion_vector: None,
            is_nsfw: false,
            auto_play: None,
        }
    }

    #[test]
    fn synthesis_request_rejects_empty_text_and_invalid_speed() {
        let mut request = valid_synthesis_request();
        request.text = "  \n".to_string();
        assert!(validate_synthesize_request(&request).is_err());

        request.text = "有效文本".to_string();
        request.speed = 0.0;
        assert!(validate_synthesize_request(&request).is_err());

        request.speed = 1.0;
        assert!(validate_synthesize_request(&request).is_ok());
    }

    #[test]
    fn tts_url_requires_http_and_blocks_metadata_endpoint() {
        assert!(validate_tts_url("http://127.0.0.1:9880/tts").is_ok());
        assert!(validate_tts_url("ftp://127.0.0.1/tts").is_err());
        assert!(validate_tts_url("http://169.254.169.254/latest/meta-data").is_err());
    }
}
