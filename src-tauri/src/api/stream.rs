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
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;
use tracing::{info, warn};

use crate::api::config::ApiResponse;
use crate::server::AppState;
use crate::services::sse_broadcaster::StreamEvent;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

fn as_sse_event(event: StreamEvent) -> Result<Event, Infallible> {
    // StreamEvent 只包含可序列化的字符串、数值和布尔值。
    let data = serde_json::to_string(&event).expect("StreamEvent serialization cannot fail");
    Ok(Event::default().data(data))
}

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

    let (rx, initial_state) = state.broadcaster.subscribe_stream_with_state();
    let broadcaster = state.broadcaster.clone();

    let initial = tokio_stream::once(as_sse_event(StreamEvent::snapshot(
        initial_state.content,
        initial_state.streaming,
    )));

    let broadcast_events = futures::stream::unfold(
        (rx, broadcaster),
        |(mut receiver, broadcaster)| async move {
            let event = match receiver.recv().await {
                Ok(event) => event,
                Err(RecvError::Lagged(skipped)) => {
                    // 当前 receiver 里仍有快照已包含的旧事件；换成新订阅，避免重复追加。
                    let (new_receiver, snapshot) = broadcaster.subscribe_stream_with_state();
                    receiver = new_receiver;
                    warn!(
                        skipped,
                        "SSE subscriber lagged; resubscribed with a full snapshot"
                    );
                    StreamEvent::snapshot(snapshot.content, snapshot.streaming)
                }
                Err(RecvError::Closed) => return None,
            };

            Some((as_sse_event(event), (receiver, broadcaster)))
        },
    );

    // Axum 的 KeepAlive 是 SSE 注释，浏览器不会为它触发 onmessage。
    // 额外发送 JSON ping，让前端的应用层心跳能够真实观察到连接存活。
    let first_heartbeat = tokio::time::Instant::now() + HEARTBEAT_INTERVAL;
    let heartbeat = IntervalStream::new(tokio::time::interval_at(
        first_heartbeat,
        HEARTBEAT_INTERVAL,
    ))
    .map(|_| as_sse_event(StreamEvent::ping()));

    let event_stream = initial.chain(broadcast_events.merge(heartbeat));

    Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(HEARTBEAT_INTERVAL)
            .text("keep-alive"),
    )
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
pub async fn clear_stream(State(state): State<AppState>) -> Json<ApiResponse<()>> {
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
    info!(
        "Testing stream with content: {}...",
        request.content.chars().take(30).collect::<String>()
    );

    state.broadcaster.start_stream();

    // 模拟逐字发送
    for char in request.content.chars() {
        state.broadcaster.add_chunk(&char.to_string());
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    state.broadcaster.end_stream();

    Json(ApiResponse::success(()))
}
