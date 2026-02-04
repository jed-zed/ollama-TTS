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
        // TODO: 阶段 5 添加 TTS API
        // .route("/api/tts/synthesize", post(tts_synthesize))
        // .route("/api/tts/stop", post(tts_stop))
        // TODO: 阶段 6 添加角色管理 API
        // .route("/api/characters", get(get_characters).post(add_character))
        // TODO: 阶段 7 添加记忆压缩 API
        // .route("/api/memory/status", get(memory_status))
        .with_state(state)
}
