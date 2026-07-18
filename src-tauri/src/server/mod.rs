//! HTTP 服务器模块
//!
//! 提供 Axum HTTP 服务器的启动和管理功能。
//! - 主 API 服务器监听配置的本机端口（默认 8765）
//! - 代理 API 服务器监听 localhost:11434 (Ollama 兼容)

pub mod main_api;
pub mod proxy_api;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::Request,
    http::{header, uri::Authority, Method, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{error, info, warn};

use crate::services::audio_player::TTSManager;
use crate::services::character_manager::CharacterManager;
use crate::services::config_store::ConfigStore;
use crate::services::history_compressor::HistoryCompressor;
use crate::services::proxy_manager::ProxyServerManager;
use crate::services::sse_broadcaster::SseBroadcaster;
use crate::services::tts_gsv::GPTSoVITSEngine;

fn is_loopback_hostname(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host.eq_ignore_ascii_case("127.0.0.1")
        || host.eq_ignore_ascii_case("[::1]")
        || host.eq_ignore_ascii_case("::1")
}

fn is_trusted_browser_origin(origin: &str) -> bool {
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    let Some(scheme) = uri.scheme_str() else {
        return false;
    };
    let Some(authority) = uri.authority() else {
        return false;
    };
    let host = authority.host();

    match scheme {
        "tauri" => host.eq_ignore_ascii_case("localhost"),
        "http" | "https" => {
            is_loopback_hostname(host) || host.eq_ignore_ascii_case("tauri.localhost")
        }
        _ => false,
    }
}

pub(crate) fn trusted_local_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            origin.to_str().ok().is_some_and(is_trusted_browser_origin)
        }))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT])
}

fn has_loopback_host(request: &Request) -> bool {
    request
        .headers()
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .and_then(|host| host.parse::<Authority>().ok())
        .is_some_and(|authority| is_loopback_hostname(authority.host()))
}

fn has_trusted_or_no_origin(request: &Request) -> bool {
    request
        .headers()
        .get(header::ORIGIN)
        .map(|origin| origin.to_str().ok().is_some_and(is_trusted_browser_origin))
        .unwrap_or(true)
}

async fn enforce_local_control_request(request: Request, next: Next) -> Response {
    if !has_loopback_host(&request) || !has_trusted_or_no_origin(&request) {
        warn!(
            host = ?request.headers().get(header::HOST),
            origin = ?request.headers().get(header::ORIGIN),
            "Rejected untrusted request to local control API"
        );
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }

    next.run(request).await
}

/// 应用状态，在所有路由之间共享
#[derive(Clone)]
pub struct AppState {
    /// 配置存储服务
    pub config_store: Arc<ConfigStore>,
    /// SSE 广播器
    pub broadcaster: Arc<SseBroadcaster>,
    /// 代理服务器管理器
    pub proxy_manager: Arc<ProxyServerManager>,
    /// GPT-SoVITS TTS 引擎
    pub gsv_engine: Arc<Mutex<GPTSoVITSEngine>>,
    /// TTS 管理器（播放器）
    pub tts_manager: Arc<Mutex<TTSManager>>,
    /// 角色管理器
    pub character_manager: Arc<RwLock<CharacterManager>>,
    /// 历史压缩器
    pub history_compressor: Arc<RwLock<HistoryCompressor>>,
}

impl AppState {
    /// 创建新的应用状态实例
    pub fn new(config_store: ConfigStore, broadcaster: SseBroadcaster) -> Self {
        Self::new_with_arc_broadcaster(config_store, Arc::new(broadcaster))
    }

    /// 使用预创建的 Arc<SseBroadcaster> 创建应用状态
    ///
    /// 注意：角色加载等重操作需要在创建后通过 `init_background_tasks()` 异步执行
    pub fn new_with_arc_broadcaster(
        config_store: ConfigStore,
        broadcaster: Arc<SseBroadcaster>,
    ) -> Self {
        let config = config_store.get();
        let config_store = Arc::new(config_store);

        // 轻量初始化：只创建空引擎对象，不做耗时操作
        let gsv_engine = Arc::new(Mutex::new(GPTSoVITSEngine::new()));
        let tts_manager = Arc::new(Mutex::new(TTSManager::new()));

        let mut proxy_manager = ProxyServerManager::new(config_store.clone(), broadcaster.clone());
        proxy_manager.set_tts_services(gsv_engine.clone(), tts_manager.clone());
        let proxy_manager = Arc::new(proxy_manager);

        // 创建空的角色管理器（延迟加载）
        let character_manager = CharacterManager::with_default_dir();

        let history_compressor = HistoryCompressor::new(
            config.history_compression.clone(),
            config.api_url.clone(),
            config.api_key.clone(),
            config.model_name.clone(),
            config_store.api_target_confirmation_guard(),
        );

        Self {
            config_store,
            broadcaster,
            proxy_manager,
            gsv_engine,
            tts_manager,
            character_manager: Arc::new(RwLock::new(character_manager)),
            history_compressor: Arc::new(RwLock::new(history_compressor)),
        }
    }

    /// 在后台执行重量级初始化任务
    ///
    /// 将角色配置加载、TTS 引擎初始化等耗时操作移到后台线程，
    /// 避免阻塞 Tauri 窗口创建。
    pub fn init_background_tasks(&self) {
        let config_store = self.config_store.clone();
        let gsv_engine = self.gsv_engine.clone();
        let tts_manager = self.tts_manager.clone();
        let character_manager = self.character_manager.clone();

        tauri::async_runtime::spawn(async move {
            // 使用 spawn_blocking 在独立线程池中执行同步 I/O 操作
            let _ = tokio::task::spawn_blocking(move || {
                let config = config_store.get();

                // 初始化 TTS 引擎
                {
                    if let Ok(mut engine) = gsv_engine.try_lock() {
                        engine.initialize(&config);
                        tracing::info!("GPT-SoVITS 引擎初始化完成");
                    }
                }

                // 配置 TTS 管理器
                {
                    if let Ok(mut manager) = tts_manager.try_lock() {
                        manager.configure(&config);
                        tracing::info!("TTS 管理器配置完成");
                    }
                }

                // 加载角色配置（磁盘 I/O）
                {
                    if let Ok(mgr) = character_manager.try_write() {
                        mgr.load_all_characters();
                        tracing::info!("角色配置加载完成");
                    }
                }
            })
            .await;
        });
    }
}

/// 启动主 API 服务器（端口由配置决定，默认 8765）
async fn start_main_server(
    state: AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 主控制 API 包含配置和文件变更能力，始终只监听 loopback。
    // lan_access_enabled 仅控制 Ollama 兼容代理服务器。
    let config = state.config_store.get();
    let port = config.main_api_port;

    // 构建主 API 路由
    let app = main_api::create_router(state)
        .layer(trusted_local_cors_layer())
        .layer(middleware::from_fn(enforce_local_control_request));

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("Starting main API server on http://{}", addr);

    // 启动服务器
    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app).await.map_err(|e| {
        error!("Main API server error: {}", e);
        Box::new(e) as Box<dyn std::error::Error + Send + Sync>
    })?;

    Ok(())
}

/// 在后台任务中启动 HTTP 服务器（非阻塞）
///
/// 只启动主 API 服务器，代理服务器由 ProxyServerManager 控制
///
/// # Arguments
/// * `config_store` - 配置存储服务实例
/// * `broadcaster` - SSE 广播器实例
///
/// # Returns
/// 返回 AppState 供后续使用（如 Tauri 命令中访问 proxy_manager）
pub fn spawn_server(config_store: ConfigStore, broadcaster: SseBroadcaster) -> AppState {
    let state = AppState::new(config_store, broadcaster);

    // 启动主 API 服务器
    let main_state = state.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = start_main_server(main_state).await {
            error!("Failed to start main API server: {}", e);
        }
    });

    // 代理服务器不自动启动，由用户通过 API 控制
    info!("Main API server spawned in background (proxy server not started)");

    state
}

/// 使用预创建的 Arc<SseBroadcaster> 启动 HTTP 服务器
///
/// 这个函数用于需要在启动服务器之前共享 broadcaster 引用的场景（如 tracing layer）
///
/// # Arguments
/// * `config_store` - 配置存储服务实例
/// * `broadcaster` - 预创建的 Arc<SseBroadcaster>
///
/// # Returns
/// 返回 AppState 供后续使用
pub fn spawn_server_with_broadcaster(
    config_store: ConfigStore,
    broadcaster: Arc<SseBroadcaster>,
) -> AppState {
    let state = AppState::new_with_arc_broadcaster(config_store, broadcaster);

    // 在后台执行重量级初始化（角色加载、TTS 引擎配置等）
    state.init_background_tasks();

    // 启动主 API 服务器
    let main_state = state.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = start_main_server(main_state).await {
            error!("Failed to start main API server: {}", e);
        }
    });

    // 代理服务器不自动启动，由用户通过 API 控制
    info!("Main API server spawned in background (proxy server not started)");

    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_origins_are_limited_to_tauri_and_loopback() {
        assert!(is_trusted_browser_origin("tauri://localhost"));
        assert!(is_trusted_browser_origin("http://tauri.localhost"));
        assert!(is_trusted_browser_origin("http://localhost:5173"));
        assert!(is_trusted_browser_origin("https://127.0.0.1:8443"));

        assert!(!is_trusted_browser_origin("null"));
        assert!(!is_trusted_browser_origin("file://localhost"));
        assert!(!is_trusted_browser_origin("https://example.com"));
        assert!(!is_trusted_browser_origin("http://127.0.0.1.example.com"));
    }

    #[test]
    fn loopback_host_parser_rejects_dns_rebinding_hosts() {
        for host in [
            "localhost",
            "localhost:8765",
            "127.0.0.1:8765",
            "[::1]:8765",
        ] {
            let authority: Authority = host.parse().unwrap();
            assert!(is_loopback_hostname(authority.host()), "{host}");
        }

        for host in [
            "example.com",
            "localhost.example.com",
            "127.0.0.1.example.com",
        ] {
            let authority: Authority = host.parse().unwrap();
            assert!(!is_loopback_hostname(authority.host()), "{host}");
        }
    }
}
