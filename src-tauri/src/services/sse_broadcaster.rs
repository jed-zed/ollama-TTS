//! SSE 广播器
//!
//! 使用 tokio::sync::broadcast 实现多客户端订阅的 SSE 推送。

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, info};

/// SSE 事件数据（与前端 StreamEvent.data 匹配）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreamEventData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<f64>,
}

/// SSE 事件类型（与前端 StreamEvent 匹配）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: StreamEventData,
}

impl StreamEvent {
    /// 创建流开始事件
    pub fn stream_start() -> Self {
        Self {
            event_type: "stream_start".to_string(),
            data: StreamEventData {
                is_streaming: Some(true),
                timestamp: Some(chrono::Utc::now().timestamp_millis() as f64 / 1000.0),
                ..Default::default()
            },
        }
    }

    /// 创建内容事件
    pub fn content(content: String) -> Self {
        Self {
            event_type: "content".to_string(),
            data: StreamEventData {
                content: Some(content),
                is_streaming: Some(true),
                ..Default::default()
            },
        }
    }

    /// 创建流结束事件
    pub fn stream_end() -> Self {
        Self {
            event_type: "stream_end".to_string(),
            data: StreamEventData {
                is_streaming: Some(false),
                timestamp: Some(chrono::Utc::now().timestamp_millis() as f64 / 1000.0),
                ..Default::default()
            },
        }
    }

    /// 创建清除事件
    pub fn clear() -> Self {
        Self {
            event_type: "clear".to_string(),
            data: StreamEventData::default(),
        }
    }

    /// 创建错误事件
    pub fn error(message: String) -> Self {
        Self {
            event_type: "error".to_string(),
            data: StreamEventData {
                error: Some(message),
                ..Default::default()
            },
        }
    }

    /// 创建 ping 事件
    pub fn ping() -> Self {
        Self {
            event_type: "ping".to_string(),
            data: StreamEventData::default(),
        }
    }

    /// 创建当前流状态快照，用于首次订阅和广播滞后后的重同步。
    pub fn snapshot(content: String, is_streaming: bool) -> Self {
        Self {
            event_type: "content".to_string(),
            data: StreamEventData {
                full_content: Some(content),
                is_streaming: Some(is_streaming),
                timestamp: Some(chrono::Utc::now().timestamp_millis() as f64 / 1000.0),
                ..Default::default()
            },
        }
    }
}

/// 日志事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// 日志ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 时间戳
    pub timestamp: String,
    /// 日志级别
    pub level: String,
    /// 日志消息
    pub message: String,
    /// 模块名（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
}

/// 日志统计
#[derive(Debug, Clone, Serialize, Default)]
pub struct LogStats {
    pub total: usize,
    pub by_level: std::collections::HashMap<String, usize>,
    pub subscribers: usize,
}

/// 日志状态
#[derive(Debug, Clone, Default)]
pub struct LogState {
    pub logs: Vec<LogEvent>,
    pub id_counter: u64,
}

/// SSE 广播器
///
/// 支持多客户端订阅，使用 broadcast channel 实现。
#[derive(Clone)]
pub struct SseBroadcaster {
    /// 字幕/流式内容广播通道
    stream_tx: broadcast::Sender<StreamEvent>,
    /// 日志广播通道
    log_tx: broadcast::Sender<LogEvent>,
    /// 当前流状态
    state: Arc<parking_lot::RwLock<StreamState>>,
    /// 日志状态
    log_state: Arc<parking_lot::RwLock<LogState>>,
    /// 日志 ID 计数器
    log_id_counter: Arc<AtomicU64>,
}

/// 流状态
#[derive(Debug, Clone, Default)]
pub struct StreamState {
    /// 是否正在流式传输
    pub streaming: bool,
    /// 累积的内容
    pub content: String,
    /// 历史记录
    pub history: Vec<String>,
}

impl SseBroadcaster {
    /// 创建新的 SSE 广播器
    ///
    /// # Arguments
    /// * `capacity` - 广播通道容量
    pub fn new(capacity: usize) -> Self {
        let (stream_tx, _) = broadcast::channel(capacity);
        let (log_tx, _) = broadcast::channel(capacity);

        Self {
            stream_tx,
            log_tx,
            state: Arc::new(parking_lot::RwLock::new(StreamState::default())),
            log_state: Arc::new(parking_lot::RwLock::new(LogState::default())),
            log_id_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    /// 订阅流式事件
    pub fn subscribe_stream(&self) -> broadcast::Receiver<StreamEvent> {
        self.stream_tx.subscribe()
    }

    /// 原子地取得订阅和对应状态快照。
    ///
    /// `send_stream` 会先更新 `state` 再广播。这里在读取状态期间完成订阅，
    /// 可确保快照之后发生的更新一定进入接收队列，避免重连窗口丢事件。
    pub fn subscribe_stream_with_state(&self) -> (broadcast::Receiver<StreamEvent>, StreamState) {
        let state = self.state.read();
        let receiver = self.stream_tx.subscribe();
        (receiver, state.clone())
    }

    /// 订阅日志事件
    pub fn subscribe_logs(&self) -> broadcast::Receiver<LogEvent> {
        self.log_tx.subscribe()
    }

    /// 发送流式事件
    pub fn send_stream(&self, event: StreamEvent) {
        // 更新状态
        {
            let mut state = self.state.write();
            match event.event_type.as_str() {
                "stream_start" => {
                    state.streaming = true;
                    state.content.clear();
                }
                "content" => {
                    if let Some(ref content) = event.data.content {
                        state.content.push_str(content);
                    }
                }
                "stream_end" => {
                    state.streaming = false;
                    if !state.content.is_empty() {
                        let content_clone = state.content.clone();
                        state.history.push(content_clone);
                        // 只保留最近 10 条历史
                        if state.history.len() > 10 {
                            state.history.remove(0);
                        }
                    }
                }
                "clear" => {
                    state.content.clear();
                }
                _ => {}
            }
        }

        // 广播事件（忽略没有接收者的情况）
        let _ = self.stream_tx.send(event);
    }

    /// 发送日志事件
    pub fn send_log(&self, mut event: LogEvent) {
        let id = self.log_id_counter.fetch_add(1, Ordering::SeqCst);
        event.id = Some(format!("log_{:08}", id));

        {
            let mut log_state = self.log_state.write();
            log_state.logs.push(event.clone());
            if log_state.logs.len() > 10000 {
                log_state.logs.remove(0);
            }
        }

        let _ = self.log_tx.send(event);
    }

    /// 发送日志消息（便捷方法）
    pub fn log(&self, level: &str, message: &str) {
        let event = LogEvent {
            id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: level.to_string(),
            message: message.to_string(),
            module: None,
        };
        self.send_log(event);
    }

    /// 获取日志列表
    pub fn get_logs(
        &self,
        level: Option<&str>,
        module: Option<&str>,
        search: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Vec<LogEvent> {
        let log_state = self.log_state.read();
        let mut logs: Vec<LogEvent> = log_state.logs.iter().rev().cloned().collect();

        if let Some(lvl) = level {
            logs.retain(|l| l.level == lvl);
        }
        if let Some(mod_name) = module {
            let mod_lower = mod_name.to_lowercase();
            logs.retain(|l| {
                l.module
                    .as_ref()
                    .map_or(false, |m| m.to_lowercase().contains(&mod_lower))
            });
        }
        if let Some(s) = search {
            let s_lower = s.to_lowercase();
            logs.retain(|l| l.message.to_lowercase().contains(&s_lower));
        }

        logs.into_iter().skip(offset).take(limit).collect()
    }

    /// 获取日志总数
    pub fn get_log_count(&self) -> usize {
        self.log_state.read().logs.len()
    }

    pub fn get_filtered_log_count(
        &self,
        level: Option<&str>,
        module: Option<&str>,
        search: Option<&str>,
    ) -> usize {
        let log_state = self.log_state.read();
        let mut count = 0;

        for log in log_state.logs.iter() {
            if let Some(lvl) = level {
                if log.level != lvl {
                    continue;
                }
            }
            if let Some(mod_name) = module {
                let mod_lower = mod_name.to_lowercase();
                if !log
                    .module
                    .as_ref()
                    .map_or(false, |m| m.to_lowercase().contains(&mod_lower))
                {
                    continue;
                }
            }
            if let Some(s) = search {
                let s_lower = s.to_lowercase();
                if !log.message.to_lowercase().contains(&s_lower) {
                    continue;
                }
            }
            count += 1;
        }

        count
    }

    /// 获取日志统计
    pub fn get_log_stats(&self) -> LogStats {
        let log_state = self.log_state.read();
        let mut by_level = std::collections::HashMap::new();

        for log in &log_state.logs {
            *by_level.entry(log.level.clone()).or_insert(0) += 1;
        }

        LogStats {
            total: log_state.logs.len(),
            by_level,
            subscribers: self.log_tx.receiver_count(),
        }
    }

    /// 清空日志
    pub fn clear_logs(&self) {
        let mut log_state = self.log_state.write();
        log_state.logs.clear();
    }

    /// 导出日志
    pub fn export_logs(&self, format: &str) -> String {
        let logs = self.get_logs(None, None, None, 10000, 0);

        match format {
            "json" => serde_json::to_string_pretty(&logs).unwrap_or_default(),
            "csv" => {
                let mut lines = vec!["id,timestamp,level,module,message".to_string()];
                for log in logs {
                    let msg = log.message.replace('"', "\"\"");
                    let module = log.module.unwrap_or_default();
                    let id = log.id.unwrap_or_default();
                    lines.push(format!(
                        "{},{},{},{},\"{}\"",
                        id, log.timestamp, log.level, module, msg
                    ));
                }
                lines.join("\n")
            }
            _ => {
                let mut lines = vec![];
                for log in logs {
                    let module = log.module.unwrap_or_else(|| "System".to_string());
                    lines.push(format!(
                        "[{}] [{}] [{}] {}",
                        log.timestamp, log.level, module, log.message
                    ));
                }
                lines.join("\n")
            }
        }
    }

    /// 添加测试日志
    pub fn add_test_logs(&self) {
        let levels = vec![
            ("INFO", "Test", "这是一条测试日志"),
            ("WARN", "Test", "这是一条警告日志"),
            ("ERROR", "Test", "这是一条错误日志"),
            ("SUCCESS", "Test", "这是一条成功日志"),
            ("DEBUG", "Test", "这是一条调试日志"),
        ];

        for (level, module, message) in levels {
            let event = LogEvent {
                id: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
                level: level.to_string(),
                message: message.to_string(),
                module: Some(module.to_string()),
            };
            self.send_log(event);
        }
    }

    /// 开始流式传输
    pub fn start_stream(&self) {
        info!("Stream started");
        self.send_stream(StreamEvent::stream_start());
    }

    /// 添加内容块
    pub fn add_chunk(&self, content: &str) {
        if !content.is_empty() {
            debug!("Stream chunk: {} chars", content.len());
            self.send_stream(StreamEvent::content(content.to_string()));
        }
    }

    /// 结束流式传输
    pub fn end_stream(&self) {
        info!("Stream ended");
        self.send_stream(StreamEvent::stream_end());
    }

    /// 清除内容
    pub fn clear(&self) {
        info!("Stream cleared");
        self.send_stream(StreamEvent::clear());
    }

    /// 发送错误
    pub fn send_error(&self, message: &str) {
        self.send_stream(StreamEvent::error(message.to_string()));
    }

    /// 获取当前状态
    pub fn get_state(&self) -> StreamState {
        self.state.read().clone()
    }

    /// 获取当前内容
    pub fn get_content(&self) -> String {
        self.state.read().content.clone()
    }

    /// 获取历史记录
    pub fn get_history(&self) -> Vec<String> {
        self.state.read().history.clone()
    }

    /// 获取订阅者数量（流式）
    pub fn stream_subscriber_count(&self) -> usize {
        self.stream_tx.receiver_count()
    }

    /// 获取订阅者数量（日志）
    pub fn log_subscriber_count(&self) -> usize {
        self.log_tx.receiver_count()
    }
}

impl Default for SseBroadcaster {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_broadcaster() {
        let broadcaster = SseBroadcaster::new(10);
        let mut rx = broadcaster.subscribe_stream();

        broadcaster.start_stream();
        broadcaster.add_chunk("Hello, ");
        broadcaster.add_chunk("world!");
        broadcaster.end_stream();

        // 接收事件
        let event1 = rx.recv().await.unwrap();
        assert_eq!(event1.event_type, "stream_start");

        let event2 = rx.recv().await.unwrap();
        assert_eq!(event2.event_type, "content");
        assert_eq!(event2.data.content, Some("Hello, ".to_string()));
    }

    #[test]
    fn test_state() {
        let broadcaster = SseBroadcaster::new(10);

        broadcaster.start_stream();
        assert!(broadcaster.get_state().streaming);

        broadcaster.add_chunk("test");
        assert_eq!(broadcaster.get_content(), "test");

        broadcaster.end_stream();
        assert!(!broadcaster.get_state().streaming);
        assert_eq!(broadcaster.get_history().len(), 1);
    }

    #[test]
    fn snapshot_contains_full_content_and_streaming_state() {
        let event = StreamEvent::snapshot("完整内容".to_string(), true);

        assert_eq!(event.event_type, "content");
        assert_eq!(event.data.full_content.as_deref(), Some("完整内容"));
        assert_eq!(event.data.is_streaming, Some(true));
    }
}
