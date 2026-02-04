//! 日志 API
//! 
//! 提供日志的 CRUD 和 SSE 推送接口。

use axum::{
    extract::{State, Query},
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    http::{header, StatusCode},
    Json,
};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::server::AppState;
use crate::services::sse_broadcaster::LogEvent;

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub level: Option<String>,
    pub module: Option<String>,
    pub search: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    100
}

#[derive(Serialize)]
pub struct LogsResponse {
    pub success: bool,
    pub data: Vec<LogEvent>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct LogStatsResponse {
    pub success: bool,
    pub total: usize,
    pub by_level: std::collections::HashMap<String, usize>,
    pub subscribers: usize,
}

#[derive(Serialize)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "json".to_string()
}

pub async fn get_logs(
    State(state): State<AppState>,
    Query(query): Query<LogsQuery>,
) -> Json<LogsResponse> {
    info!("获取日志列表: level={:?}, module={:?}, search={:?}", query.level, query.module, query.search);
    
    let logs = state.broadcaster.get_logs(
        query.level.as_deref(),
        query.module.as_deref(),
        query.search.as_deref(),
        query.limit,
        query.offset,
    );
    
    let total = state.broadcaster.get_log_count();
    
    Json(LogsResponse {
        success: true,
        data: logs,
        total,
    })
}

pub async fn get_log_stats(
    State(state): State<AppState>,
) -> Json<LogStatsResponse> {
    info!("获取日志统计");
    
    let stats = state.broadcaster.get_log_stats();
    
    Json(LogStatsResponse {
        success: true,
        total: stats.total,
        by_level: stats.by_level,
        subscribers: stats.subscribers,
    })
}

pub async fn clear_logs(
    State(state): State<AppState>,
) -> Json<SuccessResponse> {
    info!("清空日志");
    
    state.broadcaster.clear_logs();
    
    Json(SuccessResponse {
        success: true,
        message: "日志已清空".to_string(),
    })
}

pub async fn export_logs(
    State(state): State<AppState>,
    Query(query): Query<ExportQuery>,
) -> Response {
    info!("导出日志: format={}", query.format);
    
    let content = state.broadcaster.export_logs(&query.format);
    
    let (content_type, ext) = match query.format.as_str() {
        "json" => ("application/json", "json"),
        "csv" => ("text/csv", "csv"),
        _ => ("text/plain", "txt"),
    };
    
    let disposition = format!("attachment; filename=\"logs.{}\"", ext);
    
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, disposition.as_str()),
        ],
        content,
    ).into_response()
}

pub async fn test_logs(
    State(state): State<AppState>,
) -> Json<SuccessResponse> {
    info!("添加测试日志");
    
    state.broadcaster.add_test_logs();
    
    Json(SuccessResponse {
        success: true,
        message: "测试日志已添加".to_string(),
    })
}

pub async fn log_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("New SSE subscriber for logs");
    
    let rx = state.broadcaster.subscribe_logs();
    let stream = BroadcastStream::new(rx);
    
    let event_stream = stream.filter_map(|result| {
        match result {
            Ok(log_event) => {
                let data = serde_json::to_string(&log_event).ok()?;
                Some(Ok(Event::default().data(data)))
            }
            Err(_) => None,
        }
    });
    
    Sse::new(event_stream).keep_alive(KeepAlive::default())
}
