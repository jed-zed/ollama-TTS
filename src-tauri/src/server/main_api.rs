//! 主 API 路由定义
//! 
//! 定义 localhost:8765 上的所有 API 端点。

use axum::{
    routing::{get, post},
    Router,
    Json,
};
use serde::Serialize;
use tracing::info;

use super::AppState;
use crate::api::config::{get_config, update_config};
use crate::api::proxy::{get_proxy_status, start_proxy, stop_proxy, test_proxy, test_stream};
use crate::api::stream::{stream_events, get_stream_state, clear_stream, get_stream_history, test_stream as test_stream_content};
use crate::api::logs::{log_stream, get_logs, get_log_stats, clear_logs, export_logs, test_logs};
use crate::api::chat::chat;
use crate::api::tts::{synthesize, stop, stop_all, set_volume, get_status, test_engine, diagnose_connection};
use crate::api::characters::{
    get_characters, get_character, add_character, update_character, 
    delete_character, switch_character, reload_characters, get_current_character
};
use crate::api::memory::{
    get_memory_status, compress_file, compress_all, 
    get_memory_config, update_memory_config, trigger_auto_check, scan_files,
    get_prompt_template, save_prompt_template, reset_prompt_template
};
use crate::api::speech::{test_baidu_connection, recognize_speech, test_xunfei_connection, recognize_xunfei};

/// 健康检查响应
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
}

/// 健康检查端点
async fn health_check() -> Json<HealthResponse> {
    info!("Health check requested");
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: 0, // TODO: 实现实际的运行时间计算
    })
}

/// 创建主 API 路由器
/// 
/// # Arguments
/// * `state` - 应用状态
/// 
/// # Returns
/// 配置好的 Axum Router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // 健康检查
        .route("/health", get(health_check))
        // 配置 API
        .route("/api/config", get(get_config).put(update_config))
        // Chat API (前端 AI 助手)
        .route("/api/chat", post(chat))
        // 代理状态 API
        .route("/api/proxy/status", get(get_proxy_status))
        .route("/api/proxy/start", post(start_proxy))
        .route("/api/proxy/stop", post(stop_proxy))
        .route("/api/proxy/test", post(test_proxy))
        .route("/api/proxy/test-stream", post(test_stream))
        // 流式 API (SSE)
        .route("/api/stream/events", get(stream_events))
        .route("/api/stream/state", get(get_stream_state))
        .route("/api/stream/clear", post(clear_stream))
        .route("/api/stream/history", get(get_stream_history))
        .route("/api/stream/test", post(test_stream_content))
        // 日志 API
        .route("/api/logs", get(get_logs).delete(clear_logs))
        .route("/api/logs/stats", get(get_log_stats))
        .route("/api/logs/export", get(export_logs))
        .route("/api/logs/stream", get(log_stream))
        .route("/api/logs/test", post(test_logs))
        // TTS API
        .route("/api/tts/synthesize", post(synthesize))
        .route("/api/tts/stop", post(stop))
        .route("/api/tts/stop-all", post(stop_all))
        .route("/api/tts/volume", post(set_volume))
        .route("/api/tts/status", get(get_status))
        .route("/api/tts/test", post(test_engine))
        .route("/api/tts/diagnose", post(diagnose_connection))
        // 角色管理 API
        .route("/api/characters", get(get_characters).post(add_character))
        .route("/api/characters/current", get(get_current_character))
        .route("/api/characters/switch", post(switch_character))
        .route("/api/characters/reload", post(reload_characters))
        .route("/api/characters/:id", get(get_character).put(update_character).delete(delete_character))
        // 记忆压缩 API
        .route("/api/memory/status", get(get_memory_status))
        .route("/api/memory/compress", post(compress_file))
        .route("/api/memory/compress-all", post(compress_all))
        .route("/api/memory/config", get(get_memory_config).put(update_memory_config))
        .route("/api/memory/auto-check", post(trigger_auto_check))
        .route("/api/memory/scan", post(scan_files))
        .route("/api/memory/prompt-template", get(get_prompt_template).put(save_prompt_template))
        .route("/api/memory/prompt-template/reset", post(reset_prompt_template))
        // 语音识别 API
        .route("/api/speech/baidu/test", post(test_baidu_connection))
        .route("/api/speech/recognize", post(recognize_speech))
        .route("/api/speech/xunfei/test", post(test_xunfei_connection))
        .route("/api/speech/xunfei/recognize", post(recognize_xunfei))
        .with_state(state)
}
