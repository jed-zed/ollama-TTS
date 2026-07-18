//! 配置存储服务
//!
//! 提供配置的读取、原子保存和并发管理功能。

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use parking_lot::{Mutex, RwLock};
use serde_json::Value;
use tracing::{info, warn};

use crate::models::Config;
use crate::services::atomic_file;

const API_TARGET_CONFIRMATION_FIELD: &str = "api_target_confirmation_required";
const REMOVED_BRIDGE_ENABLED_FIELD: &str = "antigravity_bridge_enabled";
const REMOVED_BRIDGE_FIELDS: &[&str] = &[
    REMOVED_BRIDGE_ENABLED_FIELD,
    "antigravity_window_title",
    "antigravity_timeout_ms",
    "antigravity_max_retries",
    "antigravity_pending_submit_fallback_enabled",
    "antigravity_include_thinking",
    "antigravity_stream_enabled",
    "antigravity_bridge_model_id",
    "antigravity_max_concurrent",
    "antigravity_stream_char_delay",
];

pub const API_TARGET_CONFIRMATION_REQUIRED_MESSAGE: &str =
    "API 目标已变更或旧版桥接已移除。为避免把对话发送到未确认的 API，当前请求已阻止；请在“服务 → API 配置”中核对全部目标后重试。";
const API_TARGET_CHANGED_MESSAGE: &str =
    "API 配置已更新，当前请求使用了过期或未启用的 API URL；为避免误发，请重试。";

struct ApiTargetGuardState {
    required: bool,
    /// `None` 仅供不连接 ConfigStore 的默认/测试实例使用。
    allowed_urls: Option<HashSet<String>>,
}

/// 所有 LLM HTTP 出口共享的确认开关。
///
/// 它独立于配置快照保存，保证配置保存或重载并发发生时仍然 fail-closed。
#[derive(Clone)]
pub struct ApiTargetConfirmationGuard {
    state: Arc<RwLock<ApiTargetGuardState>>,
}

impl ApiTargetConfirmationGuard {
    pub(crate) fn new(required: bool) -> Self {
        Self {
            state: Arc::new(RwLock::new(ApiTargetGuardState {
                required,
                allowed_urls: if required { Some(HashSet::new()) } else { None },
            })),
        }
    }

    pub fn is_required(&self) -> bool {
        self.state.read().required
    }

    fn set_required(&self, required: bool) {
        self.state.write().required = required;
    }

    fn initialize_from_config(&self, config: &Config) {
        let mut state = self.state.write();
        state.required = config.api_target_confirmation_required;
        state.allowed_urls = Some(if state.required {
            HashSet::new()
        } else {
            configured_llm_targets(config)
        });
    }

    fn targets_match(&self, targets: &HashSet<String>) -> bool {
        self.state
            .read()
            .allowed_urls
            .as_ref()
            .is_some_and(|allowed| allowed == targets)
    }

    fn confirm_targets(&self, targets: HashSet<String>) {
        let mut state = self.state.write();
        state.allowed_urls = Some(targets);
        state.required = false;
    }

    pub fn ensure_confirmed_for(&self, api_url: &str) -> Result<(), String> {
        let state = self.state.read();
        if state.required {
            Err(API_TARGET_CONFIRMATION_REQUIRED_MESSAGE.to_string())
        } else if state
            .allowed_urls
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(api_url.trim()))
        {
            Err(API_TARGET_CHANGED_MESSAGE.to_string())
        } else {
            Ok(())
        }
    }
}

fn configured_llm_targets(config: &Config) -> HashSet<String> {
    let mut targets = HashSet::new();
    insert_non_empty_target(&mut targets, &config.api_url);

    if config.fallback_model_enabled && !config.fallback_preset_id.is_empty() {
        let preset_id = config.fallback_preset_id.as_str();
        if preset_id == "custom" {
            insert_non_empty_target(&mut targets, &config.custom_api_url);
        } else {
            let saved_url = config
                .preset_configs
                .get(preset_id)
                .map(|preset| preset.api_url.as_str())
                .filter(|url| !url.trim().is_empty());
            let target = saved_url.unwrap_or_else(|| builtin_preset_url(preset_id));
            insert_non_empty_target(&mut targets, target);
        }
    }

    targets
}

fn insert_non_empty_target(targets: &mut HashSet<String>, api_url: &str) {
    let normalized = api_url.trim();
    if !normalized.is_empty() {
        targets.insert(normalized.to_string());
    }
}

pub(crate) fn builtin_preset_url(preset_id: &str) -> &'static str {
    match preset_id {
        "local" => "http://127.0.0.1:11434/v1/chat/completions",
        "siliconflow" => "https://api.siliconflow.cn/v1/chat/completions",
        "deepseek" => "https://api.deepseek.com/v1/chat/completions",
        "volcengine" => "https://ark.cn-beijing.volces.com/api/v3/chat/completions",
        "ggchan" => "https://gcli.ggchan.dev/v1/chat/completions",
        "openrouter" => "https://openrouter.ai/api/v1/chat/completions",
        "openai" => "https://api.openai.com/v1/chat/completions",
        _ => "",
    }
}

struct StoredConfig {
    config: Config,
    raw: Value,
}

struct LoadedConfig {
    stored: StoredConfig,
    needs_rewrite: bool,
    removed_bridge_was_enabled: bool,
    load_failed: bool,
}

impl LoadedConfig {
    fn defaults(load_failed: bool) -> Self {
        let config = Config::default();
        let raw =
            serde_json::to_value(&config).unwrap_or_else(|_| Value::Object(Default::default()));
        Self {
            stored: StoredConfig { config, raw },
            needs_rewrite: false,
            removed_bridge_was_enabled: false,
            load_failed,
        }
    }
}

/// 配置存储服务。
///
/// `operation_lock` 覆盖读取当前值、原子持久化和内存提交的完整事务；
/// `state` 同时保存类型化视图和原始 JSON，以保留未知顶层字段。
pub struct ConfigStore {
    config_path: PathBuf,
    state: RwLock<StoredConfig>,
    operation_lock: Mutex<()>,
    api_target_guard: ApiTargetConfirmationGuard,
    writes_blocked: AtomicBool,
}

impl ConfigStore {
    pub fn new(config_path: PathBuf) -> Self {
        let LoadedConfig {
            stored,
            needs_rewrite,
            removed_bridge_was_enabled,
            load_failed,
        } = Self::load_from_file(&config_path);

        if removed_bridge_was_enabled {
            warn!("已移除的桥接模式在旧配置中处于开启状态；确认 API 目标前将阻止代理请求");
        }

        let confirmation_required = stored.config.api_target_confirmation_required;
        let api_target_guard = ApiTargetConfirmationGuard::new(confirmation_required);
        api_target_guard.initialize_from_config(&stored.config);
        let store = Self {
            config_path,
            state: RwLock::new(stored),
            operation_lock: Mutex::new(()),
            api_target_guard,
            writes_blocked: AtomicBool::new(load_failed),
        };

        if needs_rewrite {
            let raw = store.state.read().raw.clone();
            if let Err(error) = store.save_raw_value(&raw) {
                warn!("清理已移除的桥接配置字段失败: {}", error);
            } else {
                info!("已清理旧版桥接配置字段");
            }
        }

        store
    }

    fn load_from_file(path: &Path) -> LoadedConfig {
        if !path.exists() {
            info!("Config file not found at {:?}, using defaults", path);
            return LoadedConfig::defaults(false);
        }

        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                warn!("Failed to read config file: {}, using defaults", error);
                return LoadedConfig::defaults(true);
            }
        };
        let mut raw = match serde_json::from_str::<Value>(&content) {
            Ok(raw) => raw,
            Err(error) => {
                warn!("Failed to parse config file: {}, using defaults", error);
                return LoadedConfig::defaults(true);
            }
        };
        let original_raw = raw.clone();

        let mut needs_rewrite = false;
        let mut removed_bridge_was_enabled = false;
        if let Some(object) = raw.as_object_mut() {
            removed_bridge_was_enabled = !matches!(
                object.get(REMOVED_BRIDGE_ENABLED_FIELD),
                None | Some(Value::Bool(false))
            );

            // 仅删除旧版本明确写出的字段，保留插件或未来版本的同前缀扩展。
            for field in REMOVED_BRIDGE_FIELDS {
                needs_rewrite |= object.remove(*field).is_some();
            }

            if removed_bridge_was_enabled {
                object.insert(API_TARGET_CONFIRMATION_FIELD.to_string(), Value::Bool(true));
                // 旧桥接原先优先于 fallback；迁移时先停用 fallback，避免语义漂移。
                object.insert("fallback_model_enabled".to_string(), Value::Bool(false));
            }
        }

        match serde_json::from_value::<Config>(raw.clone()) {
            Ok(config) => {
                info!("Config loaded from {:?}", path);
                LoadedConfig {
                    stored: StoredConfig { config, raw },
                    needs_rewrite,
                    removed_bridge_was_enabled,
                    load_failed: false,
                }
            }
            Err(error) => {
                warn!("Failed to parse config file: {}, using defaults", error);
                let mut config = Config::default();
                if removed_bridge_was_enabled {
                    config.api_target_confirmation_required = true;
                    config.fallback_model_enabled = false;
                }
                LoadedConfig {
                    stored: StoredConfig {
                        config,
                        raw: original_raw,
                    },
                    needs_rewrite: false,
                    removed_bridge_was_enabled,
                    load_failed: true,
                }
            }
        }
    }

    pub fn get(&self) -> Config {
        self.state.read().config.clone()
    }

    pub fn api_target_confirmation_guard(&self) -> ApiTargetConfirmationGuard {
        self.api_target_guard.clone()
    }

    /// 完整替换配置。调用方若只需修改部分字段，应优先使用 `modify` 或 `merge_update`。
    pub fn update(&self, new_config: Config) -> Result<(), String> {
        let _operation = self.operation_lock.lock();
        self.ensure_writable()?;
        self.commit_locked(new_config, false)?;
        info!("Config updated and saved");
        Ok(())
    }

    /// 在同一个配置事务内修改当前快照，避免陈旧的完整配置覆盖并发更新。
    pub fn modify<F>(&self, update: F) -> Result<Config, String>
    where
        F: FnOnce(&mut Config),
    {
        let _operation = self.operation_lock.lock();
        self.ensure_writable()?;
        let mut config = self.state.read().config.clone();
        update(&mut config);
        self.commit_locked(config, false)
    }

    /// 顶层部分更新。普通 HTTP 配置更新不能清除迁移保护。
    pub fn merge_update(&self, partial_config: Value) -> Result<Config, String> {
        let _operation = self.operation_lock.lock();
        self.ensure_writable()?;
        let mut config = self.state.read().config.clone();

        let mut config_json = serde_json::to_value(&config)
            .map_err(|error| format!("Failed to serialize config: {}", error))?;
        if let (Some(base), Some(update)) =
            (config_json.as_object_mut(), partial_config.as_object())
        {
            for (key, value) in update {
                if key == API_TARGET_CONFIRMATION_FIELD {
                    continue;
                }
                base.insert(key.clone(), value.clone());
            }
        }

        config = serde_json::from_value(config_json)
            .map_err(|error| format!("Failed to deserialize merged config: {}", error))?;
        self.commit_locked(config, false)
    }

    /// 仅供受信的 Tauri IPC 调用，原子确认用户核对过的完整 API 目标集合。
    pub fn confirm_api_target(
        &self,
        api_url: String,
        expected_targets: Vec<String>,
    ) -> Result<Config, String> {
        let normalized = api_url.trim();
        if normalized.is_empty() {
            return Err("API URL 不能为空".to_string());
        }

        let _operation = self.operation_lock.lock();
        self.ensure_writable()?;
        let mut config = self.state.read().config.clone();
        if !self.api_target_guard.is_required() {
            if config.api_url.trim() == normalized {
                return Ok(config);
            }
            return Err("当前配置不需要目标确认，拒绝通过确认命令修改 API URL".to_string());
        }

        config.api_url = normalized.to_string();
        let actual_targets = configured_llm_targets(&config);
        let expected_targets = expected_targets
            .into_iter()
            .map(|target| target.trim().to_string())
            .filter(|target| !target.is_empty())
            .collect::<HashSet<_>>();
        if expected_targets != actual_targets {
            return Err(
                "API 目标在确认期间发生变化，请重新核对页面显示的全部目标后再确认。".to_string(),
            );
        }
        config.api_target_confirmation_required = false;
        self.commit_locked(config, true)
    }

    fn ensure_writable(&self) -> Result<(), String> {
        if self.writes_blocked.load(Ordering::Acquire) {
            Err("配置文件读取或校验失败；为避免覆盖原文件，当前配置为只读。请修复 config.json 后重启应用。".to_string())
        } else {
            Ok(())
        }
    }

    /// `operation_lock` 必须由调用方持有。
    fn commit_locked(
        &self,
        mut new_config: Config,
        allow_confirmation_clear: bool,
    ) -> Result<Config, String> {
        let guard_was_required = self.api_target_guard.is_required();
        let configured_targets = configured_llm_targets(&new_config);
        let targets_changed = !self.api_target_guard.targets_match(&configured_targets);
        if !allow_confirmation_clear && (guard_was_required || targets_changed) {
            new_config.api_target_confirmation_required = true;
        }

        // 开启保护必须先于磁盘操作；关闭保护则只在持久化成功后生效。
        if new_config.api_target_confirmation_required {
            self.api_target_guard.set_required(true);
        }

        let current_raw = self.state.read().raw.clone();
        let next_raw = Self::merge_config_into_raw(current_raw, &new_config)?;
        self.save_raw_value(&next_raw)?;

        {
            let mut state = self.state.write();
            *state = StoredConfig {
                config: new_config.clone(),
                raw: next_raw,
            };
        }
        if allow_confirmation_clear {
            self.api_target_guard.confirm_targets(configured_targets);
        } else {
            self.api_target_guard
                .set_required(new_config.api_target_confirmation_required);
        }

        Ok(new_config)
    }

    fn merge_config_into_raw(mut raw: Value, config: &Config) -> Result<Value, String> {
        let serialized = serde_json::to_value(config)
            .map_err(|error| format!("Failed to serialize config: {}", error))?;
        let serialized = serialized
            .as_object()
            .ok_or_else(|| "Serialized config is not a JSON object".to_string())?;

        if !raw.is_object() {
            raw = Value::Object(Default::default());
        }
        let object = raw
            .as_object_mut()
            .expect("raw was normalized to a JSON object");
        for field in REMOVED_BRIDGE_FIELDS {
            object.remove(*field);
        }
        for (key, value) in serialized {
            match key.as_str() {
                "history_compression" => {
                    if let Some(existing) = object.get_mut(key) {
                        merge_object_preserving_unknown(existing, value);
                    } else {
                        object.insert(key.clone(), value.clone());
                    }
                }
                "preset_configs" | "user_presets" | "gsv_characters" => {
                    if let Some(existing) = object.get_mut(key) {
                        merge_dynamic_object_map(existing, value);
                    } else {
                        object.insert(key.clone(), value.clone());
                    }
                }
                _ => {
                    object.insert(key.clone(), value.clone());
                }
            }
        }
        Ok(raw)
    }

    fn save_raw_value(&self, raw: &Value) -> Result<(), String> {
        let json = serde_json::to_string_pretty(raw)
            .map_err(|error| format!("Failed to serialize config: {}", error))?;
        atomic_file::write(&self.config_path, json.as_bytes())
            .map_err(|error| format!("Failed to save config atomically: {}", error))?;
        info!("Config saved to {:?}", self.config_path);
        Ok(())
    }

    /// 重新从文件加载。reload 不是用户确认动作，不能清除已生效的保护。
    pub fn reload(&self) {
        let _operation = self.operation_lock.lock();
        let mut loaded = Self::load_from_file(&self.config_path);
        if loaded.load_failed {
            self.writes_blocked.store(true, Ordering::Release);
            warn!("配置重载失败，继续使用当前内存配置");
            return;
        }
        if loaded.removed_bridge_was_enabled {
            warn!("已移除的桥接模式在旧配置中处于开启状态；确认 API 目标前将阻止代理请求");
        }

        let loaded_targets = configured_llm_targets(&loaded.stored.config);
        let target_changed = !self.api_target_guard.targets_match(&loaded_targets);
        if (self.api_target_guard.is_required() || target_changed)
            && !loaded.stored.config.api_target_confirmation_required
        {
            loaded.stored.config.api_target_confirmation_required = true;
            if let Some(object) = loaded.stored.raw.as_object_mut() {
                object.insert(API_TARGET_CONFIRMATION_FIELD.to_string(), Value::Bool(true));
            }
            loaded.needs_rewrite = true;
        }

        if loaded.stored.config.api_target_confirmation_required {
            self.api_target_guard.set_required(true);
        }

        if loaded.needs_rewrite {
            if let Err(error) = self.save_raw_value(&loaded.stored.raw) {
                warn!("清理已移除的桥接配置字段失败: {}", error);
                return;
            }
        }

        let loaded_config = loaded.stored.config.clone();
        *self.state.write() = loaded.stored;
        self.api_target_guard
            .set_required(loaded_config.api_target_confirmation_required);
        self.writes_blocked.store(false, Ordering::Release);
        info!("Config reloaded from file");
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }
}

fn merge_object_preserving_unknown(raw: &mut Value, typed: &Value) {
    if let (Some(raw_object), Some(typed_object)) = (raw.as_object_mut(), typed.as_object()) {
        for (key, value) in typed_object {
            if let Some(existing) = raw_object.get_mut(key) {
                merge_object_preserving_unknown(existing, value);
            } else {
                raw_object.insert(key.clone(), value.clone());
            }
        }
    } else {
        *raw = typed.clone();
    }
}

fn merge_dynamic_object_map(raw: &mut Value, typed: &Value) {
    if let (Some(raw_object), Some(typed_object)) = (raw.as_object_mut(), typed.as_object()) {
        raw_object.retain(|key, _| typed_object.contains_key(key));
        for (key, value) in typed_object {
            if let Some(existing) = raw_object.get_mut(key) {
                merge_object_preserving_unknown(existing, value);
            } else {
                raw_object.insert(key.clone(), value.clone());
            }
        }
    } else {
        *raw = typed.clone();
    }
}

/// 获取默认配置文件路径（可执行文件同目录下的 config.json）。
pub fn get_default_config_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    exe_dir.join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.main_api_port, 8765);
        assert!(config.tts_enabled);
        assert_eq!(config.gsv_api_url, "http://localhost:9880/tts");
    }

    #[test]
    fn test_config_store_new() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let store = ConfigStore::new(path);
        assert!(store.get().tts_enabled);
    }

    #[test]
    fn test_config_store_save_and_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let store = ConfigStore::new(path.clone());

        let mut config = store.get();
        config.api_url = "http://test.api".to_string();
        config.tts_enabled = false;
        store.update(config).unwrap();

        let store2 = ConfigStore::new(path);
        let loaded = store2.get();
        assert_eq!(loaded.api_url, "http://test.api");
        assert!(!loaded.tts_enabled);
    }

    #[test]
    fn test_removed_bridge_config_requires_explicit_api_confirmation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut legacy = serde_json::to_value(Config::default()).unwrap();
        let legacy_object = legacy.as_object_mut().unwrap();
        legacy_object.insert(
            "api_url".to_string(),
            Value::String("https://previous.example/v1".to_string()),
        );
        legacy_object.insert(REMOVED_BRIDGE_ENABLED_FIELD.to_string(), Value::Bool(true));
        legacy_object.insert(
            "antigravity_timeout_ms".to_string(),
            Value::Number(60_000.into()),
        );
        legacy_object.insert("fallback_model_enabled".to_string(), Value::Bool(true));
        legacy_object.insert(
            "plugin_custom_setting".to_string(),
            Value::String("keep-me".to_string()),
        );
        legacy_object.insert(
            "antigravity_plugin_custom".to_string(),
            Value::String("also-keep-me".to_string()),
        );
        legacy_object
            .get_mut("history_compression")
            .and_then(Value::as_object_mut)
            .unwrap()
            .insert("plugin_nested_setting".to_string(), Value::Bool(true));
        legacy_object.insert(
            "preset_configs".to_string(),
            serde_json::json!({
                "plugin-preset": {
                    "api_url": "https://preset.example/v1",
                    "api_key": "",
                    "model_name": "preset-model",
                    "plugin_nested_setting": 42
                }
            }),
        );
        fs::write(&path, serde_json::to_string_pretty(&legacy).unwrap()).unwrap();

        let store = ConfigStore::new(path.clone());
        let loaded = store.get();
        assert_eq!(loaded.api_url, "https://previous.example/v1");
        assert!(loaded.api_target_confirmation_required);
        assert!(!loaded.fallback_model_enabled);
        assert!(store.api_target_confirmation_guard().is_required());

        let normalized: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(normalized.get(REMOVED_BRIDGE_ENABLED_FIELD).is_none());
        assert!(normalized.get("antigravity_timeout_ms").is_none());
        assert_eq!(normalized["plugin_custom_setting"], "keep-me");
        assert_eq!(normalized["antigravity_plugin_custom"], "also-keep-me");
        assert_eq!(normalized["fallback_model_enabled"], false);
        assert_eq!(
            normalized["history_compression"]["plugin_nested_setting"],
            true
        );
        assert_eq!(
            normalized["preset_configs"]["plugin-preset"]["plugin_nested_setting"],
            42
        );

        let mut stale_full_update = store.get();
        stale_full_update.api_target_confirmation_required = false;
        stale_full_update.tts_volume = 0.6;
        store.update(stale_full_update).unwrap();
        assert!(store.get().api_target_confirmation_required);

        let unrelated = store
            .merge_update(serde_json::json!({ "tts_volume": 0.5 }))
            .unwrap();
        assert!(unrelated.api_target_confirmation_required);

        let automatic_api_save = store
            .merge_update(serde_json::json!({
                "api_url": "https://previous.example/v1"
            }))
            .unwrap();
        assert!(automatic_api_save.api_target_confirmation_required);

        let confirmation_without_target = store
            .merge_update(serde_json::json!({
                "api_target_confirmation_required": false
            }))
            .unwrap();
        assert!(confirmation_without_target.api_target_confirmation_required);

        let http_confirmation_attempt = store
            .merge_update(serde_json::json!({
                "api_url": "https://untrusted.example/v1",
                "api_target_confirmation_required": false
            }))
            .unwrap();
        assert!(http_confirmation_attempt.api_target_confirmation_required);

        let confirmed = store
            .confirm_api_target(
                "https://confirmed.example/v1".to_string(),
                vec!["https://confirmed.example/v1".to_string()],
            )
            .unwrap();
        assert_eq!(confirmed.api_url, "https://confirmed.example/v1");
        assert!(!confirmed.api_target_confirmation_required);
        assert!(!store.api_target_confirmation_guard().is_required());
        assert!(store
            .api_target_confirmation_guard()
            .ensure_confirmed_for("https://previous.example/v1")
            .unwrap_err()
            .contains("过期"));
        store
            .api_target_confirmation_guard()
            .ensure_confirmed_for("https://confirmed.example/v1")
            .unwrap();

        let persisted: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(persisted["plugin_custom_setting"], "keep-me");
        assert_eq!(persisted["antigravity_plugin_custom"], "also-keep-me");
        assert_eq!(
            persisted["history_compression"]["plugin_nested_setting"],
            true
        );
        assert_eq!(
            persisted["preset_configs"]["plugin-preset"]["plugin_nested_setting"],
            42
        );
    }

    #[test]
    fn malformed_removed_bridge_enabled_value_is_fail_closed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut legacy = serde_json::to_value(Config::default()).unwrap();
        legacy.as_object_mut().unwrap().insert(
            REMOVED_BRIDGE_ENABLED_FIELD.to_string(),
            Value::String("true".to_string()),
        );
        fs::write(&path, serde_json::to_string_pretty(&legacy).unwrap()).unwrap();

        let store = ConfigStore::new(path);
        assert!(store.get().api_target_confirmation_required);
        assert!(store.api_target_confirmation_guard().is_required());
    }

    #[test]
    fn ordinary_updates_cannot_authorize_new_llm_targets() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let store = ConfigStore::new(path);
        let main_target = "https://main.example/v1";
        let fallback_target = "https://fallback.example/v1";

        let changed = store
            .merge_update(serde_json::json!({ "api_url": main_target }))
            .unwrap();
        assert!(changed.api_target_confirmation_required);
        assert!(store.api_target_confirmation_guard().is_required());

        store
            .merge_update(serde_json::json!({
                "fallback_model_enabled": true,
                "fallback_preset_id": "custom",
                "custom_api_url": fallback_target
            }))
            .unwrap();
        let mismatch = store
            .confirm_api_target(main_target.to_string(), vec![main_target.to_string()])
            .unwrap_err();
        assert!(mismatch.contains("发生变化"));
        assert!(store.api_target_confirmation_guard().is_required());

        let confirmed = store
            .confirm_api_target(
                main_target.to_string(),
                vec![main_target.to_string(), fallback_target.to_string()],
            )
            .unwrap();
        assert!(!confirmed.api_target_confirmation_required);
        store
            .api_target_confirmation_guard()
            .ensure_confirmed_for(fallback_target)
            .unwrap();

        let redirected = store
            .merge_update(serde_json::json!({
                "api_url": "https://redirected.example/v1"
            }))
            .unwrap();
        assert!(redirected.api_target_confirmation_required);
        assert!(store.api_target_confirmation_guard().is_required());
        assert!(store
            .api_target_confirmation_guard()
            .ensure_confirmed_for("https://redirected.example/v1")
            .is_err());
    }

    #[test]
    fn malformed_legacy_config_stays_fail_closed_and_read_only() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let malformed = serde_json::json!({
            "main_api_port": "not-a-port",
            "api_url": "https://unconfirmed.example/v1",
            "antigravity_bridge_enabled": true
        });
        fs::write(&path, serde_json::to_string_pretty(&malformed).unwrap()).unwrap();

        let store = ConfigStore::new(path.clone());
        assert!(store.api_target_confirmation_guard().is_required());
        assert!(store
            .merge_update(serde_json::json!({ "tts_volume": 0.5 }))
            .unwrap_err()
            .contains("只读"));

        let unchanged: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(unchanged["main_api_port"], "not-a-port");
        assert_eq!(unchanged[REMOVED_BRIDGE_ENABLED_FIELD], true);
    }

    #[test]
    fn concurrent_partial_updates_keep_both_changes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let store = Arc::new(ConfigStore::new(path.clone()));
        let first = Arc::clone(&store);
        let second = Arc::clone(&store);

        let first = std::thread::spawn(move || {
            first
                .merge_update(serde_json::json!({ "tts_volume": 0.25 }))
                .unwrap();
        });
        let second = std::thread::spawn(move || {
            second
                .merge_update(serde_json::json!({ "subtitle_font_size": 31 }))
                .unwrap();
        });
        first.join().unwrap();
        second.join().unwrap();

        let loaded = store.get();
        assert_eq!(loaded.tts_volume, 0.25);
        assert_eq!(loaded.subtitle_font_size, 31);
        let persisted: Config = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(persisted.tts_volume, 0.25);
        assert_eq!(persisted.subtitle_font_size, 31);
    }
}
