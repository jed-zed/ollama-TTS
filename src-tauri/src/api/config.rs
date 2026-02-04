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
pub async fn update_config(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<Config>>, (StatusCode, Json<ApiResponse<()>>)> {
    info!("PUT /api/config");
    
    // 始终使用部分更新（merge），只更新请求中包含的字段
    // 不允许用默认值覆盖现有配置
    match state.config_store.merge_update(body.clone()) {
        Ok(updated_config) => {
            info!("Config updated ({} fields)", body.as_object().map(|o| o.len()).unwrap_or(0));
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
