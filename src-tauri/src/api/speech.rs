//! 语音识别 API 处理器
//! 
//! 提供语音识别相关的 API 端点，包括百度语音 API 代理、讯飞语音 API 代理和语音识别。

use axum::response::Json;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures::{SinkExt, StreamExt};
use url::form_urlencoded;
use once_cell::sync::Lazy;

use crate::services::http_client;

/// 模块级共享 HTTP 客户端（复用连接池，避免每次请求都创建新客户端）
static SPEECH_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| http_client::create_http_client());

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

    let client = &*SPEECH_CLIENT;
    
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

    let client = &*SPEECH_CLIENT;
    
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

// ========== 讯飞语音识别 API ==========

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
pub struct XunfeiRecognizeRequest {
    pub audio_data: String,
    pub app_id: String,
    pub api_key: String,
    pub api_secret: String,
}

#[derive(Debug, Deserialize)]
struct XunfeiWsResponse {
    code: i32,
    message: Option<String>,
    data: Option<XunfeiWsData>,
}

#[derive(Debug, Deserialize)]
struct XunfeiWsData {
    status: i32,
    result: Option<XunfeiWsResult>,
}

#[derive(Debug, Deserialize)]
struct XunfeiWsResult {
    ws: Option<Vec<XunfeiWsWord>>,
}

#[derive(Debug, Deserialize)]
struct XunfeiWsWord {
    cw: Option<Vec<XunfeiWsCw>>,
}

#[derive(Debug, Deserialize)]
struct XunfeiWsCw {
    w: Option<String>,
}

fn create_xunfei_auth_url(api_key: &str, api_secret: &str) -> Result<String, String> {
    let now = chrono::Utc::now();
    let date = now.format("%a, %d %b %Y %H:%M:%S GMT").to_string();

    let signature_origin = format!(
        "host: iat-api.xfyun.cn\ndate: {}\nGET /v2/iat HTTP/1.1",
        date
    );

    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes())
        .map_err(|e| format!("HMAC init failed: {}", e))?;
    mac.update(signature_origin.as_bytes());
    let signature = BASE64.encode(mac.finalize().into_bytes());

    let authorization_origin = format!(
        r#"api_key="{}", algorithm="hmac-sha256", headers="host date request-line", signature="{}""#,
        api_key, signature
    );
    let authorization = BASE64.encode(authorization_origin.as_bytes());

    let params = form_urlencoded::Serializer::new(String::new())
        .append_pair("authorization", &authorization)
        .append_pair("date", &date)
        .append_pair("host", "iat-api.xfyun.cn")
        .finish();

    Ok(format!("wss://iat-api.xfyun.cn/v2/iat?{}", params))
}

pub async fn recognize_xunfei(
    Json(req): Json<XunfeiRecognizeRequest>,
) -> Json<RecognizeResponse> {
    tracing::info!("Starting Xunfei speech recognition...");

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

    let ws_url = match create_xunfei_auth_url(&req.api_key, &req.api_secret) {
        Ok(url) => url,
        Err(e) => {
            tracing::error!("Failed to create Xunfei auth URL: {}", e);
            return Json(RecognizeResponse {
                success: false,
                text: None,
                error: Some(format!("Auth URL creation failed: {}", e)),
            });
        }
    };

    let (ws_stream, _) = match connect_async(&ws_url).await {
        Ok(conn) => conn,
        Err(e) => {
            tracing::error!("Failed to connect to Xunfei WebSocket: {}", e);
            return Json(RecognizeResponse {
                success: false,
                text: None,
                error: Some(format!("WebSocket connection failed: {}", e)),
            });
        }
    };

    let (mut write, mut read) = ws_stream.split();

    let frame_size: usize = 8000;
    let mut offset: usize = 0;
    let total_len = audio_bytes.len();

    while offset < total_len {
        let status = if offset == 0 {
            0
        } else if offset + frame_size >= total_len {
            2
        } else {
            1
        };

        let end = std::cmp::min(offset + frame_size, total_len);
        let chunk = &audio_bytes[offset..end];
        let chunk_b64 = BASE64.encode(chunk);

        let send_data = if status == 0 {
            serde_json::json!({
                "common": { "app_id": req.app_id },
                "business": {
                    "language": "zh_cn",
                    "domain": "iat",
                    "accent": "mandarin",
                    "vad_eos": 3000,
                    "dwa": "wpgs"
                },
                "data": {
                    "status": status,
                    "format": "audio/L16;rate=16000",
                    "encoding": "raw",
                    "audio": chunk_b64
                }
            })
        } else {
            serde_json::json!({
                "data": {
                    "status": status,
                    "format": "audio/L16;rate=16000",
                    "encoding": "raw",
                    "audio": chunk_b64
                }
            })
        };

        if let Err(e) = write.send(Message::Text(send_data.to_string())).await {
            tracing::error!("Failed to send audio frame to Xunfei: {}", e);
            return Json(RecognizeResponse {
                success: false,
                text: None,
                error: Some(format!("Failed to send audio data: {}", e)),
            });
        }

        offset = end;

        if status != 2 {
            tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;
        }
    }

    let mut result_text = Vec::new();
    let timeout = tokio::time::Duration::from_secs(30);
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            tracing::warn!("Xunfei recognition timed out");
            break;
        }

        match tokio::time::timeout(remaining, read.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                match serde_json::from_str::<XunfeiWsResponse>(&text) {
                    Ok(resp) => {
                        if resp.code != 0 {
                            let err_msg = resp.message.unwrap_or_else(|| "Unknown error".to_string());
                            tracing::error!("Xunfei returned error: {}", err_msg);
                            return Json(RecognizeResponse {
                                success: false,
                                text: None,
                                error: Some(format!("Xunfei error: {}", err_msg)),
                            });
                        }

                        if let Some(data) = resp.data {
                            if let Some(result) = data.result {
                                if let Some(ws_list) = result.ws {
                                    for ws_item in ws_list {
                                        if let Some(cw_list) = ws_item.cw {
                                            for cw in cw_list {
                                                if let Some(w) = cw.w {
                                                    if !w.is_empty() {
                                                        result_text.push(w);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if data.status == 2 {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse Xunfei response: {}", e);
                    }
                }
            }
            Ok(Some(Ok(Message::Close(_)))) => {
                break;
            }
            Ok(Some(Err(e))) => {
                tracing::error!("Xunfei WebSocket error: {}", e);
                return Json(RecognizeResponse {
                    success: false,
                    text: None,
                    error: Some(format!("WebSocket error: {}", e)),
                });
            }
            Ok(None) => {
                break;
            }
            Err(_) => {
                tracing::warn!("Xunfei recognition timed out");
                break;
            }
            _ => {}
        }
    }

    let _ = write.close().await;

    if result_text.is_empty() {
        tracing::warn!("Xunfei recognition returned empty result");
        Json(RecognizeResponse {
            success: false,
            text: None,
            error: Some("No speech recognized".to_string()),
        })
    } else {
        let text = result_text.join("");
        tracing::info!("Xunfei recognition successful: {}", text);
        Json(RecognizeResponse {
            success: true,
            text: Some(text),
            error: None,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct XunfeiTestRequest {
    pub app_id: String,
    pub api_key: String,
    pub api_secret: String,
}

#[derive(Debug, Serialize)]
pub struct XunfeiTestResponse {
    pub success: bool,
    pub message: Option<String>,
    pub error: Option<String>,
}

pub async fn test_xunfei_connection(
    Json(req): Json<XunfeiTestRequest>,
) -> Json<XunfeiTestResponse> {
    tracing::info!("Testing Xunfei Speech API connection...");

    let ws_url = match create_xunfei_auth_url(&req.api_key, &req.api_secret) {
        Ok(url) => url,
        Err(e) => {
            return Json(XunfeiTestResponse {
                success: false,
                message: None,
                error: Some(format!("Auth URL creation failed: {}", e)),
            });
        }
    };

    match tokio::time::timeout(
        tokio::time::Duration::from_secs(10),
        connect_async(&ws_url),
    ).await {
        Ok(Ok((ws_stream, _))) => {
            let (mut write, _) = ws_stream.split();

            let silent_audio = vec![0u8; 3200];
            let chunk_b64 = BASE64.encode(&silent_audio);

            let first_frame = serde_json::json!({
                "common": { "app_id": req.app_id },
                "business": {
                    "language": "zh_cn",
                    "domain": "iat",
                    "accent": "mandarin"
                },
                "data": {
                    "status": 2,
                    "format": "audio/L16;rate=16000",
                    "encoding": "raw",
                    "audio": chunk_b64
                }
            });

            match write.send(Message::Text(first_frame.to_string())).await {
                Ok(_) => {
                    let _ = write.close().await;
                    tracing::info!("Xunfei connection test successful");
                    Json(XunfeiTestResponse {
                        success: true,
                        message: Some("讯飞语音 API 连接成功".to_string()),
                        error: None,
                    })
                }
                Err(e) => {
                    Json(XunfeiTestResponse {
                        success: false,
                        message: None,
                        error: Some(format!("Failed to send test frame: {}", e)),
                    })
                }
            }
        }
        Ok(Err(e)) => {
            tracing::error!("Xunfei WebSocket connection failed: {}", e);
            Json(XunfeiTestResponse {
                success: false,
                message: None,
                error: Some(format!("WebSocket connection failed: {}", e)),
            })
        }
        Err(_) => {
            Json(XunfeiTestResponse {
                success: false,
                message: None,
                error: Some("Connection timed out (10s)".to_string()),
            })
        }
    }
}
