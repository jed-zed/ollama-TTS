//! 语音识别 API 处理器
//! 
//! 提供语音识别相关的 API 端点，包括百度语音 API 代理和语音识别。

use axum::response::Json;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct BaiduTokenRequest {
    pub api_key: String,
    pub secret_key: String,
}

#[derive(Debug, Serialize)]
pub struct BaiduTokenResponse {
    pub success: bool,
    pub access_token: Option<String>,
    pub expires_in: Option<i64>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BaiduApiResponse {
    access_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// 测试百度语音 API 连接
/// 
/// POST /api/speech/baidu/test
pub async fn test_baidu_connection(
    Json(req): Json<BaiduTokenRequest>,
) -> Json<BaiduTokenResponse> {
    tracing::info!("Testing Baidu Speech API connection...");
    
    let url = format!(
        "https://aip.baidubce.com/oauth/2.0/token?grant_type=client_credentials&client_id={}&client_secret={}",
        req.api_key,
        req.secret_key
    );

    let client = reqwest::Client::new();
    
    match client.post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<BaiduApiResponse>().await {
                    Ok(data) => {
                        if let Some(token) = data.access_token {
                            tracing::info!("Baidu API connection successful, token obtained");
                            Json(BaiduTokenResponse {
                                success: true,
                                access_token: Some(token),
                                expires_in: data.expires_in,
                                error: None,
                                error_description: None,
                            })
                        } else {
                            tracing::warn!("Baidu API returned error: {:?}", data.error);
                            Json(BaiduTokenResponse {
                                success: false,
                                access_token: None,
                                expires_in: None,
                                error: data.error,
                                error_description: data.error_description,
                            })
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse Baidu API response: {}", e);
                        Json(BaiduTokenResponse {
                            success: false,
                            access_token: None,
                            expires_in: None,
                            error: Some("parse_error".to_string()),
                            error_description: Some(format!("Failed to parse response: {}", e)),
                        })
                    }
                }
            } else {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                tracing::error!("Baidu API returned HTTP {}: {}", status, body);
                Json(BaiduTokenResponse {
                    success: false,
                    access_token: None,
                    expires_in: None,
                    error: Some(format!("http_{}", status.as_u16())),
                    error_description: Some(format!("HTTP {}: {}", status, body)),
                })
            }
        }
        Err(e) => {
            tracing::error!("Failed to connect to Baidu API: {}", e);
            Json(BaiduTokenResponse {
                success: false,
                access_token: None,
                expires_in: None,
                error: Some("connection_error".to_string()),
                error_description: Some(format!("Failed to connect: {}", e)),
            })
        }
    }
}

// ========== 语音识别 API ==========

#[derive(Debug, Deserialize)]
pub struct RecognizeRequest {
    pub audio_data: String,
    pub format: Option<String>,
    pub sample_rate: Option<i32>,
    pub api_key: String,
    pub secret_key: String,
}

#[derive(Debug, Serialize)]
pub struct RecognizeResponse {
    pub success: bool,
    pub text: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BaiduAsrResponse {
    err_no: Option<i32>,
    err_msg: Option<String>,
    result: Option<Vec<String>>,
}

pub async fn recognize_speech(
    Json(req): Json<RecognizeRequest>,
) -> Json<RecognizeResponse> {
    tracing::info!("Starting speech recognition...");
    
    let token_url = format!(
        "https://aip.baidubce.com/oauth/2.0/token?grant_type=client_credentials&client_id={}&client_secret={}",
        req.api_key,
        req.secret_key
    );

    let client = reqwest::Client::new();
    
    let token_response = match client.post(&token_url)
        .header("Content-Type", "application/json")
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Failed to get access token: {}", e);
            return Json(RecognizeResponse {
                success: false,
                text: None,
                error: Some(format!("Failed to get access token: {}", e)),
            });
        }
    };

    let token_data: BaiduApiResponse = match token_response.json().await {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("Failed to parse token response: {}", e);
            return Json(RecognizeResponse {
                success: false,
                text: None,
                error: Some(format!("Failed to parse token response: {}", e)),
            });
        }
    };

    let access_token = match token_data.access_token {
        Some(token) => token,
        None => {
            let error_msg = token_data.error_description.unwrap_or_else(|| "Unknown error".to_string());
            tracing::error!("Failed to get access token: {}", error_msg);
            return Json(RecognizeResponse {
                success: false,
                text: None,
                error: Some(format!("Failed to get access token: {}", error_msg)),
            });
        }
    };

    let audio_bytes = match BASE64.decode(&req.audio_data) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("Failed to decode audio data: {}", e);
            return Json(RecognizeResponse {
                success: false,
                text: None,
                error: Some(format!("Failed to decode audio data: {}", e)),
            });
        }
    };

    let format = req.format.unwrap_or_else(|| "pcm".to_string());
    let sample_rate = req.sample_rate.unwrap_or(16000);
    
    let asr_url = format!(
        "https://vop.baidu.com/server_api?dev_pid=1537&cuid=tauri_app&token={}",
        access_token
    );

    let asr_response = match client.post(&asr_url)
        .header("Content-Type", format!("audio/{};rate={}", format, sample_rate))
        .body(audio_bytes)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Failed to call ASR API: {}", e);
            return Json(RecognizeResponse {
                success: false,
                text: None,
                error: Some(format!("Failed to call ASR API: {}", e)),
            });
        }
    };

    let asr_data: BaiduAsrResponse = match asr_response.json().await {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("Failed to parse ASR response: {}", e);
            return Json(RecognizeResponse {
                success: false,
                text: None,
                error: Some(format!("Failed to parse ASR response: {}", e)),
            });
        }
    };

    if asr_data.err_no == Some(0) {
        let text = asr_data.result
            .and_then(|r| r.into_iter().next())
            .unwrap_or_default();
        tracing::info!("Speech recognition successful: {}", text);
        Json(RecognizeResponse {
            success: true,
            text: Some(text),
            error: None,
        })
    } else {
        let error_msg = asr_data.err_msg.unwrap_or_else(|| "Unknown error".to_string());
        tracing::error!("ASR failed: {} (err_no: {:?})", error_msg, asr_data.err_no);
        Json(RecognizeResponse {
            success: false,
            text: None,
            error: Some(format!("ASR error: {}", error_msg)),
        })
    }
}
