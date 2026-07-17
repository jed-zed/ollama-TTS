//! Tracing Layer 用于将日志转发到 SSE Broadcaster
//!
//! 这个模块实现了一个自定义的 tracing Layer，将所有 tracing 日志事件
//! 转发到 SSE Broadcaster，使其能够在前端运行日志页面显示。
//!
//! 支持延迟激活：启动阶段不转发日志到 SSE，减少写锁竞争和内存开销。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

use super::sse_broadcaster::{LogEvent, SseBroadcaster};

/// 日志转发 Layer
/// 
/// 将 tracing 日志转发到 SSE Broadcaster。
/// 支持延迟激活：调用 `activate()` 后才开始转发日志。
pub struct BroadcasterLayer {
    broadcaster: Arc<SseBroadcaster>,
    /// 是否激活（启动完成后激活，减少启动期间的开销）
    active: Arc<AtomicBool>,
}

impl BroadcasterLayer {
    /// 创建新的 BroadcasterLayer（默认未激活）
    pub fn new(broadcaster: Arc<SseBroadcaster>) -> Self {
        Self {
            broadcaster,
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 获取激活控制句柄，用于稍后激活日志转发
    pub fn activation_handle(&self) -> Arc<AtomicBool> {
        self.active.clone()
    }
}

impl<S> Layer<S> for BroadcasterLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // 启动阶段不转发日志，减少写锁竞争
        if !self.active.load(Ordering::Relaxed) {
            return;
        }

        let level = match *event.metadata().level() {
            Level::ERROR => "ERROR",
            Level::WARN => "WARN",
            Level::INFO => "INFO",
            Level::DEBUG => "DEBUG",
            Level::TRACE => "DEBUG",
        };

        let mut message = String::new();
        let mut visitor = MessageVisitor(&mut message);
        event.record(&mut visitor);

        if message.is_empty() {
            return;
        }

        let module = event.metadata().module_path().map(|s| s.to_string());

        let log_event = LogEvent {
            id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: level.to_string(),
            message,
            module,
        };

        self.broadcaster.send_log(log_event);
    }
}

/// 消息访问器，用于提取日志消息
struct MessageVisitor<'a>(&'a mut String);

impl<'a> tracing::field::Visit for MessageVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            *self.0 = format!("{:?}", value).trim_matches('"').to_string();
        } else if self.0.is_empty() {
            *self.0 = format!("{:?}", value).trim_matches('"').to_string();
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" || self.0.is_empty() {
            *self.0 = value.to_string();
        }
    }
}
