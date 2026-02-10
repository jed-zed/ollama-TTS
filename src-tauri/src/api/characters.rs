//! 角色管理 API 端点
//!
//! 提供角色的 CRUD 操作和切换功能。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use tracing::{info, error};

use crate::server::AppState;
use crate::models::character::{
    CharacterConfig, CharacterInfo, AddCharacterRequest,
    UpdateCharacterRequest, SwitchCharacterRequest,
};

/// 通用 API 响应
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// 角色列表响应
#[derive(Debug, Serialize)]
pub struct CharacterListResponse {
    pub characters: Vec<CharacterInfo>,
    pub total: usize,
    pub current_character_id: Option<String>,
}

/// GET /api/characters - 获取角色列表
pub async fn get_characters(
    State(state): State<AppState>,
) -> Json<ApiResponse<CharacterListResponse>> {
    let character_manager = state.character_manager.read().await;
    let characters = character_manager.list_available_characters();
    let current_id = character_manager.get_current_character_id();
    let total = characters.len();

    info!("获取角色列表: {} 个角色", total);

    Json(ApiResponse::success(CharacterListResponse {
        characters,
        total,
        current_character_id: current_id,
    }))
}

/// GET /api/characters/:id - 获取单个角色
pub async fn get_character(
    State(state): State<AppState>,
    Path(character_id): Path<String>,
) -> Result<Json<ApiResponse<CharacterConfig>>, (StatusCode, Json<ApiResponse<()>>)> {
    let character_manager = state.character_manager.read().await;
    
    match character_manager.get_character(&character_id) {
        Some(character) => {
            info!("获取角色: {}", character_id);
            Ok(Json(ApiResponse::success(character)))
        }
        None => {
            error!("角色不存在: {}", character_id);
            Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error(format!("角色不存在: {}", character_id))),
            ))
        }
    }
}

/// POST /api/characters - 添加角色
pub async fn add_character(
    State(state): State<AppState>,
    Json(request): Json<AddCharacterRequest>,
) -> Result<Json<ApiResponse<CharacterInfo>>, (StatusCode, Json<ApiResponse<()>>)> {
    let character_manager = state.character_manager.read().await;

    let config = CharacterConfig {
        character_id: request.character_id.clone(),
        name: request.name.clone(),
        name_aliases: if request.name_aliases.is_empty() {
            vec![request.name.clone()]
        } else {
            request.name_aliases
        },
        gender: request.gender,
        ref_audio_dir: request.ref_audio_dir,
        description: request.description,
        available_emotions: request.available_emotions,
    };

    match character_manager.add_character(config.clone()) {
        Ok(()) => {
            info!("添加角色成功: {}", request.character_id);
            Ok(Json(ApiResponse::success(CharacterInfo::from(&config))))
        }
        Err(e) => {
            error!("添加角色失败: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(e)),
            ))
        }
    }
}

/// PUT /api/characters/:id - 更新角色
pub async fn update_character(
    State(state): State<AppState>,
    Path(character_id): Path<String>,
    Json(request): Json<UpdateCharacterRequest>,
) -> Result<Json<ApiResponse<String>>, (StatusCode, Json<ApiResponse<()>>)> {
    let character_manager = state.character_manager.read().await;

    match character_manager.update_character(&character_id, request) {
        Ok(()) => {
            info!("更新角色成功: {}", character_id);
            Ok(Json(ApiResponse::success("更新成功".to_string())))
        }
        Err(e) => {
            error!("更新角色失败: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(e)),
            ))
        }
    }
}

/// DELETE /api/characters/:id - 删除角色
pub async fn delete_character(
    State(state): State<AppState>,
    Path(character_id): Path<String>,
) -> Result<Json<ApiResponse<String>>, (StatusCode, Json<ApiResponse<()>>)> {
    let character_manager = state.character_manager.read().await;

    match character_manager.delete_character(&character_id) {
        Ok(()) => {
            info!("删除角色成功: {}", character_id);
            Ok(Json(ApiResponse::success("删除成功".to_string())))
        }
        Err(e) => {
            error!("删除角色失败: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(e)),
            ))
        }
    }
}

/// POST /api/characters/switch - 切换当前角色
pub async fn switch_character(
    State(state): State<AppState>,
    Json(request): Json<SwitchCharacterRequest>,
) -> Result<Json<ApiResponse<String>>, (StatusCode, Json<ApiResponse<()>>)> {
    let character_manager = state.character_manager.read().await;

    if character_manager.switch_character(&request.character_id) {
        info!("切换角色成功: {}", request.character_id);
        Ok(Json(ApiResponse::success(format!(
            "已切换到角色: {}",
            request.character_id
        ))))
    } else {
        error!("切换角色失败: 角色不存在 {}", request.character_id);
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error(format!(
                "角色不存在: {}",
                request.character_id
            ))),
        ))
    }
}

/// POST /api/characters/reload - 重新加载角色配置
pub async fn reload_characters(
    State(state): State<AppState>,
) -> Json<ApiResponse<String>> {
    let character_manager = state.character_manager.read().await;

    if character_manager.reload_characters() {
        let count = character_manager.character_count();
        info!("重新加载角色配置成功: {} 个角色", count);
        Json(ApiResponse::success(format!(
            "重新加载成功: {} 个角色",
            count
        )))
    } else {
        error!("重新加载角色配置失败");
        Json(ApiResponse::error("重新加载失败"))
    }
}

/// 获取当前角色
#[derive(Debug, Serialize)]
pub struct CurrentCharacterResponse {
    pub character: Option<CharacterConfig>,
    pub character_id: Option<String>,
}

/// GET /api/characters/current - 获取当前选中的角色
pub async fn get_current_character(
    State(state): State<AppState>,
) -> Json<ApiResponse<CurrentCharacterResponse>> {
    let character_manager = state.character_manager.read().await;
    let character = character_manager.get_current_character();
    let character_id = character_manager.get_current_character_id();

    Json(ApiResponse::success(CurrentCharacterResponse {
        character,
        character_id,
    }))
}
