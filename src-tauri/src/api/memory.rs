//! 记忆压缩 API 端点
//!
//! 提供对话历史压缩的状态查询和操作功能。

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::models::config::{HistoryCompressionConfig, DEFAULT_COMPRESSION_PROMPT};
use crate::server::AppState;
use crate::services::history_compressor::{
    CompressionResult, CompressionResultStatus, CompressionStatus,
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

/// 压缩状态列表响应（保留用于内部使用）
#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct MemoryStatusResponse {
    pub files: Vec<CompressionStatus>,
    pub total: usize,
    pub needs_compression_count: usize,
}

/// 压缩响应（匹配前端 CompressResponse）
#[derive(Debug, Serialize)]
pub struct CompressResponse {
    pub success: bool,
    pub compressed_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summaries: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// GET /api/memory/status - 获取所有文件的压缩状态
pub async fn get_memory_status(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<CompressionStatus>>> {
    let compressor = state.history_compressor.read().await;
    let files = compressor.get_compression_status();
    let total = files.len();
    let needs_compression_count = files.iter().filter(|f| f.needs_compression).count();

    info!(
        "获取压缩状态: {} 个文件, {} 个需要压缩",
        total, needs_compression_count
    );

    Json(ApiResponse::success(files))
}

/// 压缩请求（支持 file_path 或 character_id）
#[derive(Debug, Deserialize)]
pub struct CompressRequest {
    /// 文件路径（优先使用）
    #[serde(default)]
    pub file_path: Option<String>,
    /// 角色 ID（如果未提供 file_path，则根据此查找）
    #[serde(default)]
    pub character_id: Option<String>,
    /// 是否强制压缩（忽略阈值）
    #[serde(default)]
    pub force: bool,
}

/// POST /api/memory/compress - 压缩指定文件
pub async fn compress_file(
    State(state): State<AppState>,
    Json(request): Json<CompressRequest>,
) -> Result<Json<ApiResponse<CompressResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let compressor = state.history_compressor.read().await.clone();

    let file_path_str = if let Some(fp) = &request.file_path {
        fp.clone()
    } else if let Some(char_id) = &request.character_id {
        let statuses = compressor.get_compression_status();
        if let Some(status) = statuses.iter().find(|s| s.character_id == *char_id) {
            status.file_path.clone()
        } else {
            error!("找不到角色: {}", char_id);
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error(format!("找不到角色: {}", char_id))),
            ));
        }
    } else {
        error!("必须提供 file_path 或 character_id");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("必须提供 file_path 或 character_id")),
        ));
    };

    let file_path = std::path::Path::new(&file_path_str);

    if !file_path.exists() {
        error!("文件不存在: {}", file_path_str);
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error(format!("文件不存在: {}", file_path_str))),
        ));
    }

    info!("开始压缩文件: {}", file_path_str);
    let result = compressor
        .check_and_compress_file(file_path, request.force)
        .await;

    let response = CompressResponse {
        success: result.success,
        compressed_count: if result.success { 1 } else { 0 },
        summaries: if result.success {
            Some(vec![result.message.clone()])
        } else {
            None
        },
        message: Some(result.message.clone()),
    };

    if result.success {
        info!("压缩成功: {}", result.message);
        Ok(Json(ApiResponse::success(response)))
    } else {
        error!("压缩失败: {}", result.message);
        Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(result.message)),
        ))
    }
}

/// 批量压缩请求
#[derive(Debug, Deserialize)]
pub struct CompressAllRequest {
    /// 是否强制压缩（忽略阈值）
    #[serde(default)]
    pub force: bool,
}

/// 批量压缩响应
#[derive(Debug, Serialize)]
pub struct CompressAllResponse {
    pub results: Vec<CompressionResult>,
    pub total: usize,
    pub success_count: usize,
    pub failed_count: usize,
}

/// POST /api/memory/compress-all - 压缩所有需要压缩的文件
pub async fn compress_all(
    State(state): State<AppState>,
    Json(request): Json<CompressAllRequest>,
) -> Json<ApiResponse<CompressResponse>> {
    let compressor = state.history_compressor.read().await.clone();

    info!("开始批量压缩, force={}", request.force);
    let results = compressor.scan_and_compress_all(request.force).await;

    let total = results.len();
    let success_count = results
        .iter()
        .filter(|r| r.status == CompressionResultStatus::Success)
        .count();
    let skipped_count = results
        .iter()
        .filter(|r| r.status == CompressionResultStatus::Skipped)
        .count();
    let ignored_count = results
        .iter()
        .filter(|r| r.status == CompressionResultStatus::Ignored)
        .count();
    let failed_count = results
        .iter()
        .filter(|r| r.status == CompressionResultStatus::Failed)
        .count();

    let summaries: Vec<String> = results
        .iter()
        .filter(|r| r.status == CompressionResultStatus::Success)
        .map(|r| r.message.clone())
        .collect();

    info!(
        "批量压缩完成: {} 成功, {} 跳过, {} 忽略, {} 失败 (共 {} 个文件)",
        success_count, skipped_count, ignored_count, failed_count, total
    );

    Json(ApiResponse::success(CompressResponse {
        success: success_count > 0 || failed_count == 0,
        compressed_count: success_count as i32,
        summaries: if summaries.is_empty() {
            None
        } else {
            Some(summaries)
        },
        message: Some(format!(
            "压缩完成: {} 成功, {} 跳过, {} 忽略, {} 失败",
            success_count, skipped_count, ignored_count, failed_count
        )),
    }))
}

/// GET /api/memory/config - 获取压缩配置
pub async fn get_memory_config(
    State(state): State<AppState>,
) -> Json<ApiResponse<HistoryCompressionConfig>> {
    let config = state.config_store.get();
    info!("获取压缩配置");
    Json(ApiResponse::success(config.history_compression))
}

/// 更新压缩配置请求
#[derive(Debug, Deserialize, Default)]
pub struct UpdateMemoryConfigRequest {
    pub enabled: Option<bool>,
    #[serde(alias = "auto_compress")]
    pub auto_compress: Option<bool>,
    pub save_data_path: Option<String>,
    pub max_messages: Option<i32>,
    pub compress_count: Option<i32>,
    pub check_interval: Option<i32>,
    pub backup_before_compress: Option<bool>,
    pub backup_dir: Option<String>,
    pub custom_prompt: Option<String>,
}

/// PUT /api/memory/config - 更新压缩配置
pub async fn update_memory_config(
    State(state): State<AppState>,
    Json(request): Json<UpdateMemoryConfigRequest>,
) -> Result<Json<ApiResponse<HistoryCompressionConfig>>, (StatusCode, Json<ApiResponse<()>>)> {
    if request.max_messages.is_some_and(|value| value <= 0) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("max_messages 必须大于 0")),
        ));
    }
    if request.compress_count.is_some_and(|value| value <= 0) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("compress_count 必须大于 0")),
        ));
    }
    if request.check_interval.is_some_and(|value| value <= 0) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("check_interval 必须大于 0")),
        ));
    }

    let update_result = state.config_store.modify(move |config| {
        if let Some(value) = request.enabled.or(request.auto_compress) {
            config.history_compression.enabled = value;
        }
        if let Some(save_data_path) = request.save_data_path {
            config.history_compression.save_data_path = save_data_path;
        }
        if let Some(max_messages) = request.max_messages {
            config.history_compression.max_messages = max_messages;
        }
        if let Some(compress_count) = request.compress_count {
            config.history_compression.compress_count = compress_count;
        }
        if let Some(check_interval) = request.check_interval {
            config.history_compression.check_interval = check_interval;
        }
        if let Some(backup_before_compress) = request.backup_before_compress {
            config.history_compression.backup_before_compress = backup_before_compress;
        }
        if let Some(backup_dir) = request.backup_dir {
            config.history_compression.backup_dir = backup_dir;
        }
        if let Some(custom_prompt) = request.custom_prompt {
            config.history_compression.custom_prompt = custom_prompt;
        }
    });

    match update_result {
        Ok(_) => {
            let mut compressor = state.history_compressor.write().await;
            let latest_config = state.config_store.get();
            compressor.update_config(latest_config.history_compression.clone());

            info!("更新压缩配置成功");
            Ok(Json(ApiResponse::success(
                latest_config.history_compression,
            )))
        }
        Err(e) => {
            error!("更新压缩配置失败: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("保存配置失败: {}", e))),
            ))
        }
    }
}

/// POST /api/memory/auto-check - 触发自动压缩检查
pub async fn trigger_auto_check(State(state): State<AppState>) -> Json<ApiResponse<String>> {
    let compressor = state.history_compressor.read().await.clone();

    info!("触发自动压缩检查");
    compressor.auto_compress_check().await;

    Json(ApiResponse::success("自动压缩检查已触发".to_string()))
}

/// 扫描文件请求
#[derive(Debug, Deserialize, Default)]
pub struct ScanRequest {
    /// 自定义扫描路径（可选，默认使用配置中的路径）
    pub path: Option<String>,
}

/// 扫描文件响应
#[derive(Debug, Serialize)]
pub struct ScanResponse {
    pub files: Vec<String>,
    pub count: usize,
}

/// POST /api/memory/scan - 扫描可压缩的文件
pub async fn scan_files(State(state): State<AppState>) -> Json<ApiResponse<ScanResponse>> {
    let compressor = state.history_compressor.read().await;
    let files: Vec<String> = compressor
        .scan_json_files()
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let count = files.len();

    info!("扫描到 {} 个 JSON 文件", count);

    Json(ApiResponse::success(ScanResponse { files, count }))
}

/// 提示词模板请求
#[derive(Debug, Deserialize)]
pub struct PromptTemplateRequest {
    /// 模板内容
    pub template: String,
}

/// 提示词模板响应
#[derive(Debug, Serialize)]
pub struct PromptTemplateResponse {
    /// 模板内容
    pub template: String,
}

/// GET /api/memory/prompt-template - 获取当前提示词模板
pub async fn get_prompt_template(
    State(state): State<AppState>,
) -> Json<ApiResponse<PromptTemplateResponse>> {
    let config = state.config_store.get();
    let template = if config.history_compression.custom_prompt.trim().is_empty() {
        DEFAULT_COMPRESSION_PROMPT.to_string()
    } else {
        config.history_compression.custom_prompt.clone()
    };

    info!("获取压缩提示词模板");
    Json(ApiResponse::success(PromptTemplateResponse { template }))
}

/// PUT /api/memory/prompt-template - 保存提示词模板
pub async fn save_prompt_template(
    State(state): State<AppState>,
    Json(request): Json<PromptTemplateRequest>,
) -> Result<Json<ApiResponse<PromptTemplateResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    let template = request.template;
    match state.config_store.modify(move |config| {
        config.history_compression.custom_prompt = template;
    }) {
        Ok(_) => {
            let mut compressor = state.history_compressor.write().await;
            let latest_config = state.config_store.get();
            let response_template = latest_config.history_compression.custom_prompt.clone();
            compressor.update_config(latest_config.history_compression);

            info!("保存压缩提示词模板成功");
            Ok(Json(ApiResponse::success(PromptTemplateResponse {
                template: response_template,
            })))
        }
        Err(e) => {
            error!("保存压缩提示词模板失败: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("保存失败: {}", e))),
            ))
        }
    }
}

/// POST /api/memory/prompt-template/reset - 重置提示词模板为默认值
pub async fn reset_prompt_template(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<PromptTemplateResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    match state.config_store.modify(|config| {
        config.history_compression.custom_prompt = String::new();
    }) {
        Ok(_) => {
            let mut compressor = state.history_compressor.write().await;
            let latest_config = state.config_store.get();
            compressor.update_config(latest_config.history_compression);

            info!("重置压缩提示词模板为默认值");
            Ok(Json(ApiResponse::success(PromptTemplateResponse {
                template: DEFAULT_COMPRESSION_PROMPT.to_string(),
            })))
        }
        Err(e) => {
            error!("重置压缩提示词模板失败: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!("重置失败: {}", e))),
            ))
        }
    }
}
