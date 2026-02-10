//! 配置 API 处理器
//! 
//! 提供配置的获取和更新 API。

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{info, error};

use crate::server::AppState;
use crate::models::Config;

/// API 响应包装器
#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    /// 创建成功响应
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
}

impl ApiResponse<()> {
    /// 创建错误响应
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// 获取配置
/// 
/// GET /api/config
/// 
/// 返回当前配置的完整内容。
pub async fn get_config(
    State(state): State<AppState>,
) -> Json<ApiResponse<Config>> {
    info!("GET /api/config");
    let config = state.config_store.get();
    Json(ApiResponse::success(config))
}

/// 更新配置请求体
#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    /// 完整配置（可选，如果提供则完全替换）
    #[serde(flatten)]
    pub config: Option<Config>,
}

/// 更新配置
/// 
/// PUT /api/config
/// 
/// 支持两种更新模式：
/// 1. 完整替换：提供完整的 Config 对象
/// 2. 部分更新：只提供需要更新的字段
/// 
/// 配置更新后会自动重新初始化相关服务。
pub async fn update_config(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<Config>>, (StatusCode, Json<ApiResponse<()>>)> {
    info!("PUT /api/config");
    
    // 检查配置更新类型
    let config_keys: Vec<&str> = body.as_object()
        .map(|obj| obj.keys().map(|k| k.as_str()).collect())
        .unwrap_or_default();
    
    let has_tts_config = config_keys.iter().any(|k| k.starts_with("gsv_") || k.starts_with("tts_"));
    let has_memory_config = config_keys.iter().any(|k| k.starts_with("history_compression") || *k == "history_compression");
    let has_api_config = config_keys.iter().any(|k| *k == "api_url" || *k == "api_key" || *k == "model_name");
    
    // 始终使用部分更新（merge），只更新请求中包含的字段
    // 不允许用默认值覆盖现有配置
    match state.config_store.merge_update(body.clone()) {
        Ok(updated_config) => {
            info!("Config updated ({} fields): {:?}", config_keys.len(), config_keys);
            
            // 如果更新了 TTS 相关配置，重新初始化引擎
            if has_tts_config {
                info!("检测到 TTS 配置更新，重新初始化引擎");
                
                // 重新初始化 GSV 引擎
                {
                    let mut gsv_engine = state.gsv_engine.lock().await;
                    gsv_engine.initialize(&updated_config);
                    info!("GPT-SoVITS 引擎已重新初始化");
                }
                
                // 重新配置 TTS 管理器
                {
                    let mut tts_manager = state.tts_manager.lock().await;
                    tts_manager.configure(&updated_config);
                    info!("TTS 管理器已重新配置");
                }
            }
            
            // 如果更新了记忆压缩相关配置，重新配置压缩器
            if has_memory_config {
                info!("检测到记忆压缩配置更新，重新配置压缩器");
                let mut compressor = state.history_compressor.write().await;
                compressor.update_config(updated_config.history_compression.clone());
                info!("历史压缩器配置已更新: save_data_path={}", updated_config.history_compression.save_data_path);
            }
            
            // 如果更新了 API 配置，同步到压缩器
            if has_api_config {
                info!("检测到 API 配置更新，同步到压缩器");
                let mut compressor = state.history_compressor.write().await;
                compressor.update_api_config(
                    updated_config.api_url.clone(),
                    updated_config.api_key.clone(),
                    updated_config.model_name.clone(),
                );
                info!("历史压缩器 API 配置已同步");
            }
            
            Ok(Json(ApiResponse::success(updated_config)))
        }
        Err(e) => {
            error!("Failed to update config: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e)),
            ))
        }
    }
}

/// 重载配置
/// 
/// POST /api/config/reload
/// 
/// 从文件重新加载配置，丢弃内存中的更改。
pub async fn reload_config(
    State(state): State<AppState>,
) -> Json<ApiResponse<Config>> {
    info!("POST /api/config/reload");
    state.config_store.reload();
    let config = state.config_store.get();
    Json(ApiResponse::success(config))
}
