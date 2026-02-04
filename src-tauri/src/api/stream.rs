//! 流式 API
//! 
//! 提供字幕/流式内容的 SSE 推送接口。

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures::stream::Stream;
use serde::Serialize;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::info;

use crate::api::config::ApiResponse;
use crate::server::AppState;

/// 流状态响应
#[derive(Debug, Serialize)]
pub struct StreamStateResponse {
    /// 是否正在流式传输
    pub streaming: bool,
    /// 当前内容
    pub content: String,
    /// 订阅者数量
    pub subscribers: usize,
}

/// SSE 事件流
/// 
/// GET /api/stream/events
/// 
/// 返回 SSE 事件流，客户端可以订阅流式内容更新。
pub async fn stream_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("New SSE subscriber for stream events");
    
    let rx = state.broadcaster.subscribe_stream();
    let stream = BroadcastStream::new(rx);
    
    let event_stream = stream.filter_map(|result| {
        match result {
            Ok(event) => {
                let data = serde_json::to_string(&event).ok()?;
                Some(Ok(Event::default().data(data)))
            }
            Err(_) => None, // 忽略滞后错误
        }
    });
    
    Sse::new(event_stream).keep_alive(KeepAlive::default())
}

/// 获取流状态
/// 
/// GET /api/stream/state
pub async fn get_stream_state(
    State(state): State<AppState>,
) -> Json<ApiResponse<StreamStateResponse>> {
    let stream_state = state.broadcaster.get_state();
    
    Json(ApiResponse::success(StreamStateResponse {
        streaming: stream_state.streaming,
        content: stream_state.content,
        subscribers: state.broadcaster.stream_subscriber_count(),
    }))
}

/// 清除流内容
/// 
/// POST /api/stream/clear
pub async fn clear_stream(
    State(state): State<AppState>,
) -> Json<ApiResponse<()>> {
    info!("Stream content cleared");
    state.broadcaster.clear();
    Json(ApiResponse::success(()))
}

/// 流消息（与前端 StreamMessage 类型对应）
#[derive(Debug, Clone, Serialize)]
pub struct StreamMessageResponse {
    pub id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    pub timestamp: u64,
}

/// 获取流历史
/// 
/// GET /api/stream/history
pub async fn get_stream_history(
    State(_state): State<AppState>,
) -> Json<ApiResponse<Vec<StreamMessageResponse>>> {
    // TODO: 实现真正的历史记录存储
    // 目前返回空数组
    Json(ApiResponse::success(vec![]))
}

/// 测试发送流内容（开发用）
/// 
/// POST /api/stream/test
#[derive(Debug, serde::Deserialize)]
pub struct TestStreamRequest {
    #[serde(default = "default_test_content")]
    pub content: String,
}

fn default_test_content() -> String {
    "这是一条测试消息。\n第二行内容。".to_string()
}

pub async fn test_stream(
    State(state): State<AppState>,
    Json(request): Json<TestStreamRequest>,
) -> Json<ApiResponse<()>> {
    info!("Testing stream with content: {}...", 
          request.content.chars().take(30).collect::<String>());
    
    state.broadcaster.start_stream();
    
    // 模拟逐字发送
    for char in request.content.chars() {
        state.broadcaster.add_chunk(&char.to_string());
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
    
    state.broadcaster.end_stream();
    
    Json(ApiResponse::success(()))
}
