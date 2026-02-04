//! 代理服务器管理器
//! 
//! 提供 Ollama 代理服务器的启动、停止和状态管理功能。

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use parking_lot::RwLock;
use tokio::sync::watch;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, error, warn};

use crate::server::proxy_api::{self, ProxyAppState};
use crate::services::config_store::ConfigStore;
use crate::services::sse_broadcaster::{SseBroadcaster, LogEvent};

/// 代理服务器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyState {
    /// 已停止
    Stopped,
    /// 正在启动
    Starting,
    /// 运行中
    Running,
    /// 正在停止
    Stopping,
}

/// 代理服务器管理器内部状态
struct ProxyManagerInner {
    /// 当前状态
    state: ProxyState,
    /// 关闭信号发送器
    shutdown_tx: Option<watch::Sender<bool>>,
}

/// 代理服务器管理器
/// 
/// 管理 Ollama 兼容代理服务器（11434端口）的生命周期
#[derive(Clone)]
pub struct ProxyServerManager {
    inner: Arc<RwLock<ProxyManagerInner>>,
    config_store: Arc<ConfigStore>,
    broadcaster: Arc<SseBroadcaster>,
}

impl ProxyServerManager {
    /// 创建新的代理服务器管理器
    pub fn new(config_store: Arc<ConfigStore>, broadcaster: Arc<SseBroadcaster>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ProxyManagerInner {
                state: ProxyState::Stopped,
                shutdown_tx: None,
            })),
            config_store,
            broadcaster,
        }
    }
    
    fn broadcast_log(&self, level: &str, message: &str) {
        let event = LogEvent {
            id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: level.to_string(),
            message: message.to_string(),
            module: Some("ProxyManager".to_string()),
        };
        self.broadcaster.send_log(event);
    }

    /// 获取当前状态
    pub fn state(&self) -> ProxyState {
        self.inner.read().state
    }

    /// 检查是否正在运行
    pub fn is_running(&self) -> bool {
        self.inner.read().state == ProxyState::Running
    }

    /// 启动代理服务器
    pub async fn start(&self) -> Result<(), String> {
        {
            let inner = self.inner.read();
            match inner.state {
                ProxyState::Running => {
                    info!("Proxy server is already running");
                    self.broadcast_log("INFO", "代理服务器已在运行中");
                    return Ok(());
                }
                ProxyState::Starting => {
                    return Err("Proxy server is starting".to_string());
                }
                ProxyState::Stopping => {
                    return Err("Proxy server is stopping".to_string());
                }
                ProxyState::Stopped => {}
            }
        }

        {
            let mut inner = self.inner.write();
            inner.state = ProxyState::Starting;
        }

        info!("Starting Ollama proxy server...");
        self.broadcast_log("INFO", "正在启动 Ollama 代理服务器...");

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        {
            let mut inner = self.inner.write();
            inner.shutdown_tx = Some(shutdown_tx);
        }

        let state = ProxyAppState {
            config_store: self.config_store.clone(),
            broadcaster: self.broadcaster.clone(),
        };

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        let app: Router = proxy_api::create_router(state).layer(cors);

        let addr = SocketAddr::from(([127, 0, 0, 1], 11434));

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind proxy server to {}: {}", addr, e);
                self.broadcast_log("ERROR", &format!("端口绑定失败 ({}): {}", addr, e));
                let mut inner = self.inner.write();
                inner.state = ProxyState::Stopped;
                inner.shutdown_tx = None;
                return Err(format!("Failed to bind: {}", e));
            }
        };

        info!("Ollama proxy server listening on http://{}", addr);
        self.broadcast_log("SUCCESS", &format!("Ollama 代理服务器已启动: http://{}", addr));

        {
            let mut inner = self.inner.write();
            inner.state = ProxyState::Running;
        }

        let inner_clone = self.inner.clone();
        let broadcaster_clone = self.broadcaster.clone();

        tauri::async_runtime::spawn(async move {
            let shutdown_signal = async move {
                let mut rx = shutdown_rx;
                while !*rx.borrow() {
                    if rx.changed().await.is_err() {
                        break;
                    }
                }
                info!("Proxy server received shutdown signal");
            };

            let server = axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal);

            if let Err(e) = server.await {
                error!("Proxy server error: {}", e);
                let event = LogEvent {
                    id: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    level: "ERROR".to_string(),
                    message: format!("代理服务器错误: {}", e),
                    module: Some("ProxyManager".to_string()),
                };
                broadcaster_clone.send_log(event);
            }

            let mut inner = inner_clone.write();
            inner.state = ProxyState::Stopped;
            inner.shutdown_tx = None;
            info!("Proxy server stopped");
        });

        Ok(())
    }

    /// 停止代理服务器
    pub async fn stop(&self) -> Result<(), String> {
        let shutdown_tx = {
            let mut inner = self.inner.write();
            match inner.state {
                ProxyState::Stopped => {
                    info!("Proxy server is already stopped");
                    self.broadcast_log("INFO", "代理服务器已停止");
                    return Ok(());
                }
                ProxyState::Stopping => {
                    return Err("Proxy server is already stopping".to_string());
                }
                ProxyState::Starting => {
                    warn!("Proxy server is starting, waiting...");
                    self.broadcast_log("WARN", "代理服务器正在启动中，请稍候...");
                }
                ProxyState::Running => {}
            }

            inner.state = ProxyState::Stopping;
            inner.shutdown_tx.take()
        };

        info!("Stopping Ollama proxy server...");
        self.broadcast_log("INFO", "正在停止 Ollama 代理服务器...");

        if let Some(tx) = shutdown_tx {
            let _ = tx.send(true);
        }

        for _ in 0..50 {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            if self.inner.read().state == ProxyState::Stopped {
                info!("Proxy server stopped successfully");
                self.broadcast_log("SUCCESS", "代理服务器已成功停止");
                return Ok(());
            }
        }

        {
            let mut inner = self.inner.write();
            inner.state = ProxyState::Stopped;
            inner.shutdown_tx = None;
        }

        warn!("Proxy server stop timeout, forced to stopped state");
        self.broadcast_log("WARN", "代理服务器停止超时，已强制停止");
        Ok(())
    }

    /// 重启代理服务器
    pub async fn restart(&self) -> Result<(), String> {
        self.stop().await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        self.start().await
    }
}
