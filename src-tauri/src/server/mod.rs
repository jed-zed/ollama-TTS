//! HTTP 服务器模块
//! 
//! 提供 Axum HTTP 服务器的启动和管理功能。
//! - 主 API 服务器监听 localhost:8765
//! - 代理 API 服务器监听 localhost:11434 (Ollama 兼容)

pub mod main_api;
pub mod proxy_api;

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, error};

use crate::services::config_store::ConfigStore;
use crate::services::sse_broadcaster::SseBroadcaster;
use crate::services::proxy_manager::ProxyServerManager;
use crate::services::tts_gsv::GPTSoVITSEngine;
use crate::services::audio_player::TTSManager;
use crate::services::character_manager::CharacterManager;
use crate::services::history_compressor::HistoryCompressor;

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
    pub fn new_with_arc_broadcaster(config_store: ConfigStore, broadcaster: Arc<SseBroadcaster>) -> Self {
        let config = config_store.get();
        let config_store = Arc::new(config_store);
        
        let mut gsv_engine = GPTSoVITSEngine::new();
        gsv_engine.initialize(&config);
        let gsv_engine = Arc::new(Mutex::new(gsv_engine));
        
        let mut tts_manager = TTSManager::new();
        tts_manager.configure(&config);
        let tts_manager = Arc::new(Mutex::new(tts_manager));
        
        let mut proxy_manager = ProxyServerManager::new(
            config_store.clone(),
            broadcaster.clone(),
        );
        proxy_manager.set_tts_services(gsv_engine.clone(), tts_manager.clone());
        let proxy_manager = Arc::new(proxy_manager);
        
        let character_manager = CharacterManager::with_default_dir();
        character_manager.load_all_characters();
        
        let history_compressor = HistoryCompressor::new(
            config.history_compression.clone(),
            config.api_url.clone(),
            config.api_key.clone(),
            config.model_name.clone(),
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
}

/// 启动主 API 服务器 (localhost:8765)
async fn start_main_server(state: AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 配置 CORS 中间件 - 允许所有来源（开发环境）
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    
    // 构建主 API 路由
    let app = main_api::create_router(state)
        .layer(cors);
    
    // 绑定地址
    let addr = SocketAddr::from(([127, 0, 0, 1], 8765));
    info!("Starting main API server on http://{}", addr);
    
    // 启动服务器
    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    axum::serve(listener, app)
        .await
        .map_err(|e| {
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
pub fn spawn_server_with_broadcaster(config_store: ConfigStore, broadcaster: Arc<SseBroadcaster>) -> AppState {
    let state = AppState::new_with_arc_broadcaster(config_store, broadcaster);
    
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
