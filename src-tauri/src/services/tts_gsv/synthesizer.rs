//! TTS 合成逻辑
//!
//! 统一 synthesize 和 synthesize_streaming 的公共逻辑，消除重复代码。

use std::time::Duration;
use tracing::{info, error};

use super::types::RefAudioInfo;
use crate::models::tts_types::TtsResult;

/// 构建 API 请求 payload
pub fn build_payload(
    segment_text: &str,
    primary_audio: &RefAudioInfo,
    is_v2_api: bool,
    top_k: i32,
    top_p: f32,
    temperature: f32,
    speed: f32,
) -> serde_json::Value {
    if is_v2_api {
        serde_json::json!({
            "text": segment_text,
            "text_lang": "zh",
            "ref_audio_path": primary_audio.audio_file,
            "prompt_text": primary_audio.text,
            "prompt_lang": "zh",
            "top_k": top_k,
            "top_p": top_p,
            "temperature": temperature,
            "speed": speed
        })
    } else {
        serde_json::json!({
            "text": segment_text,
            "text_language": "zh",
            "refer_wav_path": primary_audio.audio_file,
            "prompt_text": primary_audio.text,
            "prompt_language": "zh"
        })
    }
}

/// 发送合成请求并获取音频数据
pub async fn send_synthesis_request(
    client: &reqwest::Client,
    api_url: &str,
    payload: &serde_json::Value,
    timeout: Duration,
    segment_index: usize,
    total_segments: usize,
) -> Option<Vec<u8>> {
    match client
        .post(api_url)
        .json(payload)
        .timeout(timeout)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                let content_type = response.headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");

                if content_type.contains("audio") || content_type.contains("wav") || content_type.contains("octet-stream") {
                    match response.bytes().await {
                        Ok(bytes) => {
                            if total_segments > 1 {
                                info!("   ✅ 段落 {}/{} 合成成功: {} 字节", segment_index + 1, total_segments, bytes.len());
                            }
                            Some(bytes.to_vec())
                        }
                        Err(e) => {
                            error!("   ❌ 段落 {} 读取响应失败: {}", segment_index + 1, e);
                            None
                        }
                    }
                } else {
                    error!("   ❌ 段落 {} 未知响应格式: {}", segment_index + 1, content_type);
                    None
                }
            } else {
                error!("   ❌ 段落 {} HTTP {}", segment_index + 1, response.status());
                None
            }
        }
        Err(e) => {
            if e.is_timeout() {
                error!("   ❌ 段落 {} 请求超时", segment_index + 1);
            } else if e.is_connect() {
                error!("   ❌ 段落 {} 连接失败", segment_index + 1);
            } else {
                error!("   ❌ 段落 {} 异常: {}", segment_index + 1, e);
            }
            None
        }
    }
}

/// 构建失败的 TtsResult
pub fn fail_result(error: impl Into<String>) -> TtsResult {
    TtsResult {
        success: false,
        error: Some(error.into()),
        ..Default::default()
    }
}
