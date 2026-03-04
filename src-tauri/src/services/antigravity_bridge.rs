use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::sync::{Mutex, Notify, Semaphore};
use tracing::{debug, info, warn};
use uuid::Uuid;

const MIN_TIMEOUT_MS: i32 = 1_000;
const MAX_TIMEOUT_MS: i32 = 300_000;
const MAX_PROMPT_LENGTH: usize = 2_000_000;
const MAX_CONTENT_LENGTH: usize = 1_000_000;
const DEFAULT_POLL_INTERVAL_MS: u64 = 1_000;
const DEFAULT_STABLE_ROUNDS: u8 = 2;
const DEFAULT_REQUEST_SOURCE: i32 = 0;
const DEFAULT_TASK_TTL_MS: u64 = 180_000;
const LS_CACHE_TTL_SECS: u64 = 60;
const DEFAULT_MAX_CONCURRENT: usize = 3;

/// 缓存的 LS 连接信息
struct CachedLsConnection {
    connection: LsConnection,
    discovered_at: Instant,
}

pub struct AntigravityBridge {
    inner: Arc<Mutex<BridgeInner>>,
    ls_cache: Arc<Mutex<Option<CachedLsConnection>>>,
    concurrency: Arc<Semaphore>,
}

struct BridgeInner {
    queue: VecDeque<BridgePendingTask>,
    waiters: HashMap<String, Arc<BridgeWaiter>>,
}

struct BridgeWaiter {
    result: Mutex<Option<String>>,
    notify: Notify,
    created_at: Instant,
}

#[derive(Clone)]
pub struct BridgePendingTask {
    pub id: String,
    pub prompt: String,
    pub created_at: String,
}

pub struct BridgeResponse {
    pub content: String,
}

struct LsConnection {
    port: u16,
    csrf_token: String,
    prefer_https: bool,
}

fn extract_arg_value(cmdline: &str, arg_names: &[&str]) -> Option<String> {
    for arg in arg_names {
        let pattern = format!(r#"--{}\s+"?([^\s"]+)"?"#, regex::escape(arg));
        if let Ok(re) = Regex::new(&pattern) {
            if let Some(val) = re
                .captures(cmdline)
                .and_then(|caps| caps.get(1))
                .map(|m| m.as_str().to_string())
            {
                return Some(val);
            }
        }
    }
    None
}

impl AntigravityBridge {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BridgeInner {
                queue: VecDeque::new(),
                waiters: HashMap::new(),
            })),
            ls_cache: Arc::new(Mutex::new(None)),
            concurrency: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT)),
        }
    }

    /// 获取或发现 LS 连接（带缓存）
    async fn get_or_discover_connection(&self) -> Result<LsConnection, String> {
        // 检查缓存
        {
            let cache = self.ls_cache.lock().await;
            if let Some(ref cached) = *cache {
                if cached.discovered_at.elapsed().as_secs() < LS_CACHE_TTL_SECS {
                    info!("复用缓存的 LS 连接: port={}", cached.connection.port);
                    return Ok(LsConnection {
                        port: cached.connection.port,
                        csrf_token: cached.connection.csrf_token.clone(),
                        prefer_https: cached.connection.prefer_https,
                    });
                }
                info!("LS 连接缓存已过期，重新发现");
            }
        }

        // 发现新连接
        let connection = discover_ls_connection().await?;
        info!("LS 连接发现成功并缓存: port={}", connection.port);

        // 写入缓存
        {
            let mut cache = self.ls_cache.lock().await;
            *cache = Some(CachedLsConnection {
                connection: LsConnection {
                    port: connection.port,
                    csrf_token: connection.csrf_token.clone(),
                    prefer_https: connection.prefer_https,
                },
                discovered_at: Instant::now(),
            });
        }

        Ok(connection)
    }

    /// 清除 LS 连接缓存（连接失败时调用）
    async fn invalidate_ls_cache(&self) {
        let mut cache = self.ls_cache.lock().await;
        *cache = None;
        info!("LS 连接缓存已清除");
    }

    pub async fn enqueue(&self, prompt: String) -> Result<String, String> {
        if prompt.trim().is_empty() {
            return Err("bridge prompt is empty".to_string());
        }

        if prompt.len() > MAX_PROMPT_LENGTH {
            return Err("bridge prompt is too long".to_string());
        }

        let id = Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        let waiter = Arc::new(BridgeWaiter {
            result: Mutex::new(None),
            notify: Notify::new(),
            created_at: Instant::now(),
        });

        let task = BridgePendingTask {
            id: id.clone(),
            prompt,
            created_at,
        };

        let mut inner = self.inner.lock().await;
        inner.waiters.insert(id.clone(), waiter);
        inner.queue.push_back(task);

        Ok(id)
    }

    pub async fn next_pending(&self) -> Option<BridgePendingTask> {
        let mut inner = self.inner.lock().await;
        self.cleanup_expired_locked(&mut inner);
        inner.queue.pop_front()
    }

    pub async fn submit(&self, id: String, content: String) -> Result<(), String> {
        if id.trim().is_empty() {
            return Err("bridge request id is empty".to_string());
        }

        if content.trim().is_empty() {
            return Err("bridge submit content is empty".to_string());
        }

        if content.len() > MAX_CONTENT_LENGTH {
            return Err("bridge submit content is too long".to_string());
        }

        let waiter = {
            let inner = self.inner.lock().await;
            inner.waiters.get(&id).cloned()
        }
        .ok_or_else(|| "bridge request not found or expired".to_string())?;

        let mut result = waiter.result.lock().await;
        if result.is_some() {
            return Err("bridge request already submitted".to_string());
        }

        *result = Some(content);
        drop(result);

        waiter.notify.notify_waiters();
        Ok(())
    }

    pub async fn wait_for_result(&self, id: &str, timeout_ms: i32) -> Result<BridgeResponse, String> {
        if id.trim().is_empty() {
            return Err("bridge request id is empty".to_string());
        }

        let waiter = {
            let mut inner = self.inner.lock().await;
            self.cleanup_expired_locked(&mut inner);
            inner.waiters.get(id).cloned()
        }
        .ok_or_else(|| "bridge request not found".to_string())?;

        let timeout = timeout_ms.clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS) as u64;

        let wait_result = tokio::time::timeout(Duration::from_millis(timeout), async {
            loop {
                let notified = waiter.notify.notified();
                if let Some(content) = waiter.result.lock().await.take() {
                    return content;
                }
                notified.await;
            }
        })
        .await;

        self.cleanup_request(id).await;

        match wait_result {
            Ok(content) => Ok(BridgeResponse { content }),
            Err(_) => Err("bridge timeout: no submit result received".to_string()),
        }
    }

    pub async fn execute_builtin_ls_waitback(
        &self,
        prompt: String,
        timeout_ms: i32,
        include_thinking: bool,
    ) -> Result<String, String> {
        if prompt.trim().is_empty() {
            return Err("bridge input is empty".to_string());
        }

        if prompt.len() > MAX_PROMPT_LENGTH {
            return Err("bridge input is too long".to_string());
        }

        // 获取并发许可（超限请求在此排队等待）
        let _permit = self.concurrency.acquire().await
            .map_err(|_| "bridge concurrency semaphore closed".to_string())?;
        info!("内置 LS 桥接获取并发许可，开始执行");

        let timeout = timeout_ms.clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS) as u64;

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_millis(timeout))
            .build()
            .map_err(|err| format!("connect: build ls client failed: {}", err))?;

        // 尝试用缓存连接，失败则清除缓存重试
        let connection = match self.get_or_discover_connection().await {
            Ok(conn) => conn,
            Err(e) => {
                warn!("LS 连接发现失败: {}, 清除缓存重试", e);
                self.invalidate_ls_cache().await;
                return Err(e);
            }
        };

        // 每个请求创建独立的 cascade（请求隔离的关键）
        let cascade_id = match start_cascade(&client, &connection).await {
            Ok(id) => id,
            Err(e) => {
                // cascade 创建失败可能是连接已失效，清除缓存
                warn!("cascade 创建失败，清除 LS 缓存: {}", e);
                self.invalidate_ls_cache().await;
                return Err(e);
            }
        };
        info!("[{}] 创建独立 cascade 会话成功", cascade_id);

        info!(
            "[{}] 发送消息: prompt_len={}, preview={}",
            cascade_id,
            prompt.len(),
            prompt.chars().take(200).collect::<String>()
        );
        send_user_message(&client, &connection, &cascade_id, &prompt).await?;
        info!("[{}] 消息发送成功", cascade_id);

        // 轮询此 cascade 的 trajectory（每个 cascade_id 独立，不会混淆）
        let result = wait_for_stable_response(
            &client, &connection, &cascade_id, &prompt, timeout, include_thinking
        ).await;

        // _permit 在此作用域结束时自动释放，允许下一个排队请求进入
        result
    }

    async fn cleanup_request(&self, id: &str) {
        let mut inner = self.inner.lock().await;
        inner.waiters.remove(id);
        inner.queue.retain(|task| task.id != id);
    }

    fn cleanup_expired_locked(&self, inner: &mut BridgeInner) {
        let now = Instant::now();
        let expired_ids: Vec<String> = inner
            .waiters
            .iter()
            .filter_map(|(id, waiter)| {
                if now.duration_since(waiter.created_at).as_millis() as u64 > DEFAULT_TASK_TTL_MS {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        if expired_ids.is_empty() {
            return;
        }

        for id in &expired_ids {
            inner.waiters.remove(id);
        }
        inner.queue.retain(|task| !expired_ids.iter().any(|id| id == &task.id));
    }
}

async fn discover_ls_connection() -> Result<LsConnection, String> {
    let script = "Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine -match 'language_server' -and $_.CommandLine -match '--csrf_token' } | ForEach-Object { $_.ProcessId.ToString() + '|' + $_.CommandLine }";

    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(script)
        .output()
        .await
        .map_err(|err| format!("discover: failed to inspect process list: {}", err))?;

    if !output.status.success() {
        return Err(format!(
            "discover: process query failed with status {:?}",
            output.status.code()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut process_entries: Vec<(u32, String)> = Vec::new();
    for line in stdout.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut parts = line.splitn(2, '|');
        let pid_text = parts.next().unwrap_or_default().trim();
        let cmd = parts.next().unwrap_or_default().trim();
        if let Ok(pid) = pid_text.parse::<u32>() {
            if !cmd.is_empty() {
                process_entries.push((pid, cmd.to_string()));
            }
        }
    }

    if process_entries.is_empty() {
        warn!("内置 LS 桥接未发现包含 csrf_token 的进程参数");
        return Err("discover: no LS process with csrf token found".to_string());
    }

    // 像 SDK 一样构建临时 client 用于探测
    let probe_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| format!("discover: build probe client failed: {}", e))?;

    for (pid, cmdline) in process_entries {
        debug!("内置 LS 桥接发现候选进程: pid={}", pid);

        let csrf_token = extract_arg_value(&cmdline, &["csrf_token"]);
        let ext_port = extract_arg_value(&cmdline, &["extension_server_port"])
            .and_then(|v| v.parse::<u16>().ok());

        if csrf_token.is_none() {
            continue;
        }

        let csrf = csrf_token.unwrap_or_default();
        let ports = discover_all_listening_ports(pid, ext_port).await;

        // 像 SDK 的 _probePort 一样，对每个端口用 GetUserStatus 做 HTTPS/HTTP 探测
        for port in &ports {
            for proto in &["https", "http"] {
                let url = format!(
                    "{}://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/GetUserStatus",
                    proto, port
                );
                let mut req = probe_client
                    .post(&url)
                    .header("Content-Type", "application/json");
                if !csrf.is_empty() {
                    req = req.header("x-codeium-csrf-token", &csrf);
                }
                match req.body("{}").send().await {
                    Ok(resp) => {
                        let status = resp.status().as_u16();
                        // 200 或 401（缺 CSRF）都说明这是 ConnectRPC 端口
                        if status == 200 || status == 401 {
                            info!("端口探测成功: port={}, proto={}, status={}", port, proto, status);
                            return Ok(LsConnection {
                                port: *port,
                                csrf_token: csrf,
                                prefer_https: *proto == "https",
                            });
                        }
                        debug!("端口探测 {}:{} → status={}", proto, port, status);
                    }
                    Err(e) => {
                        debug!("端口探测 {}:{} → 连接失败: {}", proto, port, e);
                    }
                }
            }
        }

        // 如果所有探测都失败，退回到最小端口（兼容旧行为）
        if let Some(&port) = ports.first() {
            info!("端口探测均未通过 GetUserStatus，降级到最小端口: {}", port);
            return Ok(LsConnection {
                port,
                csrf_token: csrf,
                prefer_https: true,
            });
        }

        if let Some(port) = ext_port {
            return Ok(LsConnection {
                port,
                csrf_token: csrf,
                prefer_https: false,
            });
        }
    }

    warn!("内置 LS 桥接发现进程但缺少可用端口/csrf 参数");
    Err("discover: ls process found but missing usable port/csrf arguments".to_string())
}

async fn discover_all_listening_ports(pid: u32, ext_port: Option<u16>) -> Vec<u16> {
    let script = format!(
        "netstat -aon | findstr LISTENING | findstr {}",
        pid
    );

    let output = match Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(script)
        .output()
        .await
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let port_re = match Regex::new(r#"127\.0\.0\.1:(\d{2,5})"#) {
        Ok(re) => re,
        Err(_) => return Vec::new(),
    };

    let mut ports = Vec::new();
    for cap in port_re.captures_iter(&stdout) {
        if let Some(m) = cap.get(1) {
            if let Ok(port) = m.as_str().parse::<u16>() {
                if Some(port) != ext_port && !ports.contains(&port) {
                    ports.push(port);
                }
            }
        }
    }

    ports.sort_unstable();
    info!("LS 候选端口(排除 ext_port {:?}): {:?}", ext_port, ports);
    ports
}

async fn start_cascade(client: &reqwest::Client, connection: &LsConnection) -> Result<String, String> {
    let response = call_ls_method(
        client,
        connection,
        "StartCascade",
        json!({ "source": DEFAULT_REQUEST_SOURCE }),
    )
    .await?;

    response
        .get("cascadeId")
        .or_else(|| response.get("cascade_id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "send: StartCascade missing cascadeId".to_string())
}

async fn send_user_message(
    client: &reqwest::Client,
    connection: &LsConnection,
    cascade_id: &str,
    prompt: &str,
) -> Result<(), String> {
    // LS 强制要求指定 requestedModel
    let cascade_config = json!({
        "plannerConfig": {
            "plannerTypeConfig": {
                "conversational": {}
            },
            "requestedModel": {
                "model": 1018
            }
        }
    });

    let payloads: Vec<(&str, Value)> = vec![
        // proto3 canonical JSON: items: [{"text": "..."}]
        (
            "proto3_text_items",
            json!({
                "cascadeId": cascade_id,
                "items": [{ "text": prompt }],
                "cascadeConfig": cascade_config
            }),
        ),
        // 简化格式：userMessage 字段
        (
            "proto3_user_message",
            json!({
                "cascadeId": cascade_id,
                "userMessage": prompt,
                "cascadeConfig": cascade_config
            }),
        ),
        // ConnectRPC case/value 格式
        (
            "connectrpc_case_value",
            json!({
                "cascadeId": cascade_id,
                "items": [{
                    "chunk": {
                        "case": "text",
                        "value": prompt
                    }
                }],
                "cascadeConfig": cascade_config
            }),
        ),
    ];

    let mut last_err = String::new();
    for (variant, payload) in payloads {
        match call_ls_method(client, connection, "SendUserCascadeMessage", payload).await {
            Ok(resp) => {
                let raw_str = resp.to_string();
                let snapshot: String = raw_str.chars().take(500).collect();
                info!(
                    "SendUserCascadeMessage 成功: variant={}, len={}, data={}",
                    variant, raw_str.len(), snapshot
                );
                return Ok(());
            }
            Err(err) => {
                debug!(
                    "SendUserCascadeMessage 失败: variant={}, error={}",
                    variant, err
                );
                last_err = err;
            }
        }
    }

    Err(format!(
        "send: SendUserCascadeMessage failed with all payload variants: {}",
        last_err
    ))
}

/// 直接用 HTTPS 探测单个方法（不做 HTTP 降级），返回 (status_code, body)
async fn probe_https_method(
    client: &reqwest::Client,
    connection: &LsConnection,
    method: &str,
    payload: Value,
) -> Result<(u16, String), String> {
    let url = format!(
        "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/{}",
        connection.port, method
    );

    let mut request = client
        .post(&url)
        .header("Content-Type", "application/json");

    if !connection.csrf_token.is_empty() {
        request = request.header("x-codeium-csrf-token", &connection.csrf_token);
    }

    let response = request
        .body(payload.to_string())
        .send()
        .await
        .map_err(|e| format!("probe {}: {}", method, e))?;

    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok((status, body))
}

/// 对 LS 进行全面诊断，并找到可用的读取方法
async fn discover_read_method(
    client: &reqwest::Client,
    connection: &LsConnection,
    cascade_id: &str,
) -> Option<(&'static str, &'static str)> {
    info!("===== LS 方法诊断开始 =====");

    // 第一步：用已知可用的方法验证 HTTPS 连接
    let diagnostic_methods: Vec<(&str, Value)> = vec![
        ("GetUserStatus", json!({})),
        ("GetAllCascadeTrajectories", json!({})),
        ("GetUserTrajectoryDescriptions", json!({})),
    ];

    for (method, payload) in &diagnostic_methods {
        match probe_https_method(client, connection, method, payload.clone()).await {
            Ok((status, body)) => {
                let preview: String = body.chars().take(300).collect();
                info!("诊断 {} → HTTPS status={}, body_len={}, preview={}", method, status, body.len(), preview);
            }
            Err(err) => {
                info!("诊断 {} → HTTPS 请求失败: {}", method, err);
            }
        }
    }

    // 第二步：用 HTTPS 直接探测读取方法（不做 HTTP 降级，直接看 HTTPS 状态码）
    let read_candidates: Vec<(&'static str, &'static str)> = vec![
        ("GetConversation", "cascadeId"),
        ("GetConversation", "conversationId"),
        ("GetCascadeTrajectory", "cascadeId"),
        ("GetChat", "cascadeId"),
        ("GetCascade", "cascadeId"),
        ("GetCascadeState", "cascadeId"),
        ("GetCascadeMessages", "cascadeId"),
        ("GetCascadeConversation", "cascadeId"),
        ("GetConversationMessages", "cascadeId"),
    ];

    for (method, param) in &read_candidates {
        let payload = json!({ *param: cascade_id });
        match probe_https_method(client, connection, method, payload).await {
            Ok((status, body)) => {
                let preview: String = body.chars().take(300).collect();
                if status == 200 {
                    info!("探测成功: {}/{} → 200, body_len={}, preview={}", method, param, body.len(), preview);
                    return Some((method, param));
                }
                info!("探测 {}/{} → HTTPS status={}, body={}", method, param, status, preview);
            }
            Err(err) => {
                info!("探测 {}/{} → 请求失败: {}", method, param, err);
            }
        }
    }

    info!("===== LS 方法诊断结束：未找到可用的读取方法 =====");
    None
}

async fn wait_for_stable_response(
    client: &reqwest::Client,
    connection: &LsConnection,
    cascade_id: &str,
    prompt: &str,
    timeout_ms: u64,
    include_thinking: bool,
) -> Result<String, String> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);

    // 第一步：探测可用方法
    let discover_result = discover_read_method(client, connection, cascade_id).await;

    // 如果没有找到读取方法，降级到 trajectory summary 轮询
    if discover_result.is_none() {
        let elapsed = (tokio::time::Instant::now() - (deadline - Duration::from_millis(timeout_ms)))
            .as_millis() as u64;
        let remaining = timeout_ms.saturating_sub(elapsed);
        if remaining < 5000 {
            return Err("poll: 无法找到可用的读取方法且剩余时间不足".to_string());
        }
        return wait_for_trajectory_summary(client, connection, cascade_id, remaining).await;
    }

    let (read_method, read_param) = discover_result.unwrap();

    info!(
        "使用读取方法: {}/{}, cascade_id={}",
        read_method, read_param, cascade_id
    );

    // 第二步：轮询等待 cascade 完成
    let mut last_content = String::new();
    let mut stable_rounds = 0u8;
    let mut poll_count = 0u32;

    while tokio::time::Instant::now() < deadline {
        poll_count += 1;

        let payload = json!({ read_param: cascade_id });
        let response = match call_ls_method(client, connection, read_method, payload).await {
            Ok(r) => r,
            Err(err) => {
                if poll_count <= 5 {
                    info!("轮询[{}] {} 失败，跳过继续: {}", poll_count, read_method, err);
                }
                tokio::time::sleep(Duration::from_millis(DEFAULT_POLL_INTERVAL_MS)).await;
                continue;
            }
        };

        // 检查 cascade 运行状态
        let cascade_status = response
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("");
        let num_steps = response
            .get("numTotalSteps")
            .and_then(Value::as_u64)
            .unwrap_or(0);

        // 记录 step 类型（前几轮详细记录）
        if poll_count <= 5 {
            if let Some(steps) = response
                .get("trajectory")
                .and_then(|t| t.get("steps"))
                .and_then(Value::as_array)
            {
                let step_types: Vec<&str> = steps
                    .iter()
                    .map(|s| s.get("type").and_then(Value::as_str).unwrap_or("unknown"))
                    .collect();
                info!(
                    "轮询[{}] status={}, steps={}, types={:?}",
                    poll_count, cascade_status, num_steps, step_types
                );
            } else {
                info!(
                    "轮询[{}] status={}, steps={}",
                    poll_count, cascade_status, num_steps
                );
            }
        }

        // 如果 cascade 仍在运行，继续等待
        let is_finished = cascade_status.contains("IDLE")
            || cascade_status.contains("DONE")
            || cascade_status.contains("COMPLETED")
            || cascade_status.contains("FINISHED");

        if !is_finished {
            if poll_count <= 5 {
                info!("轮询[{}] cascade 仍在运行 ({}), 继续等待...", poll_count, cascade_status);
            }
            tokio::time::sleep(Duration::from_millis(DEFAULT_POLL_INTERVAL_MS)).await;
            continue;
        }

        // Cascade 已完成，提取回复
        let candidate = extract_assistant_text(&response, prompt, include_thinking);
        if poll_count <= 5 {
            info!(
                "轮询[{}] cascade 已完成, extract 结果: len={}, preview={}",
                poll_count,
                candidate.len(),
                candidate.chars().take(200).collect::<String>()
            );
        }

        if !candidate.is_empty() {
            if candidate == last_content {
                stable_rounds += 1;
            } else {
                stable_rounds = 1;
                last_content = candidate;
            }

            if stable_rounds >= DEFAULT_STABLE_ROUNDS {
                info!(
                    "内置 LS 桥接回复稳定: cascade_id={}, method={}, rounds={}, content_len={}",
                    cascade_id,
                    read_method,
                    stable_rounds,
                    last_content.len()
                );
                return Ok(last_content);
            }
        }

        tokio::time::sleep(Duration::from_millis(DEFAULT_POLL_INTERVAL_MS)).await;
    }

    // 超时但有内容时返回
    if !last_content.is_empty() {
        info!("内置 LS 桥接超时但有内容, 返回: len={}", last_content.len());
        return Ok(last_content);
    }

    warn!("内置 LS 桥接轮询超时: cascade_id={}", cascade_id);
    Err("poll: timeout waiting stable assistant response".to_string())
}

/// 降级路径：通过 GetAllCascadeTrajectories 轮询 summary 变化
async fn wait_for_trajectory_summary(
    client: &reqwest::Client,
    connection: &LsConnection,
    cascade_id: &str,
    timeout_ms: u64,
) -> Result<String, String> {
    info!("降级到 trajectory summary 轮询: cascade_id={}", cascade_id);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    let mut last_summary = String::new();
    let mut stable_rounds = 0u8;
    let mut poll_count = 0u32;

    while tokio::time::Instant::now() < deadline {
        poll_count += 1;
        match probe_https_method(client, connection, "GetAllCascadeTrajectories", json!({})).await {
            Ok((200, body)) => {
                // 在 trajectories JSON 中找到我们 cascade_id 对应的 summary
                if let Ok(parsed) = serde_json::from_str::<Value>(&body) {
                    if let Some(summary) = find_cascade_summary(&parsed, cascade_id) {
                        if poll_count <= 3 {
                            info!("trajectory 轮询[{}] summary={}", poll_count, summary);
                        }
                        if !summary.is_empty() {
                            if summary == last_summary {
                                stable_rounds += 1;
                            } else {
                                stable_rounds = 1;
                                last_summary = summary;
                            }
                            if stable_rounds >= DEFAULT_STABLE_ROUNDS {
                                info!("trajectory summary 稳定: cascade_id={}, summary={}", cascade_id, last_summary);
                                return Ok(last_summary);
                            }
                        }
                    }
                }
            }
            Ok((status, body)) => {
                if poll_count <= 3 {
                    let preview: String = body.chars().take(200).collect();
                    info!("trajectory 轮询[{}] status={}, body={}", poll_count, status, preview);
                }
            }
            Err(err) => {
                if poll_count <= 3 {
                    info!("trajectory 轮询[{}] 失败: {}", poll_count, err);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(DEFAULT_POLL_INTERVAL_MS)).await;
    }

    if !last_summary.is_empty() {
        info!("trajectory summary 超时但有内容，返回最后结果: len={}", last_summary.len());
        return Ok(last_summary);
    }
    Err("poll: trajectory summary timeout".to_string())
}

/// 在 GetAllCascadeTrajectories 返回的 JSON 中查找指定 cascade 的 summary
fn find_cascade_summary(root: &Value, cascade_id: &str) -> Option<String> {
    // 递归搜索包含 cascade_id 的对象，提取 summary 字段
    match root {
        Value::Object(map) => {
            // 检查当前对象是否包含目标 cascade_id
            let has_id = map.values().any(|v| v.as_str() == Some(cascade_id));
            if has_id {
                if let Some(summary) = map.get("summary").and_then(Value::as_str) {
                    return Some(summary.to_string());
                }
            }
            // 递归搜索子对象
            for (_, v) in map {
                if let Some(s) = find_cascade_summary(v, cascade_id) {
                    return Some(s);
                }
            }
            None
        }
        Value::Array(items) => {
            for item in items {
                if let Some(s) = find_cascade_summary(item, cascade_id) {
                    return Some(s);
                }
            }
            None
        }
        _ => None,
    }
}

async fn call_ls_method(
    client: &reqwest::Client,
    connection: &LsConnection,
    method: &str,
    payload: Value,
) -> Result<Value, String> {
    if method.trim().is_empty() {
        return Err("connect: ls method is empty".to_string());
    }

    let proto_order: [&str; 2] = if connection.prefer_https {
        ["https", "http"]
    } else {
        ["http", "https"]
    };

    let payload_text = payload.to_string();
    let mut last_error = String::new();

    for (idx, proto) in proto_order.iter().enumerate() {
        let url = format!(
            "{}://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/{}",
            proto, connection.port, method
        );

        let response = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("x-codeium-csrf-token", connection.csrf_token.as_str())
            .body(payload_text.clone())
            .send()
            .await;

        let response = match response {
            Ok(resp) => resp,
            Err(err) => {
                let err_text = err.to_string();
                last_error = format!("connect: request {} via {} failed: {}", method, proto, err_text);
                // 任意方法发生协议错误（SSL/TLS混用）时，允许切换协议重试
                let allow_fallback = idx == 0 && is_protocol_mismatch_message(&err_text);
                if allow_fallback {
                    debug!("协议不匹配，切换协议重试: method={}, proto={}", method, proto);
                    continue;
                }
                return Err(last_error.clone());
            }
        };

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| format!("parse: read {} body failed: {}", method, err))?;

        if !status.is_success() {
            let body_hint = truncate_for_error(&body);
            // 协议错误 OR 404（路由不存在，可能是协议选错了）时，允许切换协议重试
            let is_not_found = status.as_u16() == 404;
            let allow_fallback = idx == 0
                && (is_protocol_mismatch_message(&body_hint) || is_not_found);

            last_error = format!(
                "connect: {} via {} returned {}: {}",
                method,
                proto,
                status,
                body_hint
            );

            if allow_fallback {
                debug!("收到 {} {}，切换协议重试: method={}", status.as_u16(), if is_not_found { "Not Found" } else { "协议错误" }, method);
                continue;
            }

            return Err(last_error.clone());
        }

        return serde_json::from_str::<Value>(&body)
            .map_err(|err| format!("parse: decode {} response failed: {}", method, err));
    }

    if last_error.is_empty() {
        Err(format!("connect: request {} failed", method))
    } else {
        Err(last_error)
    }
}

fn extract_assistant_text(root: &Value, prompt: &str, include_thinking: bool) -> String {
    // 优先使用 trajectory steps 精准解析
    if let Some(text) = extract_from_trajectory_steps(root, prompt, include_thinking) {
        return text;
    }

    // 降级：通用最长文本搜索
    let mut candidates = Vec::new();
    collect_all_strings(root, &mut candidates);

    let prompt_trimmed = prompt.trim();
    info!("extract_assistant_text(降级): 收集到 {} 个候选文本", candidates.len());

    candidates
        .into_iter()
        .map(|text| text.trim().to_string())
        .filter(|text| text.len() > 2)
        .filter(|text| text != prompt_trimmed)
        .filter(|text| !looks_like_base64(text))
        .filter(|text| !looks_like_thinking_content(text))
        .max_by_key(|text| text.len())
        .unwrap_or_default()
}

/// 从 GetCascadeTrajectory 响应中精准提取 AI 助手的回复
/// 将步骤分类为"思考"和"正文"，正文必须返回，思考按 include_thinking 决定
fn extract_from_trajectory_steps(root: &Value, prompt: &str, include_thinking: bool) -> Option<String> {
    let steps = root
        .get("trajectory")
        .and_then(|t| t.get("steps"))
        .and_then(Value::as_array)
        .or_else(|| root.get("steps").and_then(Value::as_array))?;

    if steps.is_empty() {
        return None;
    }

    let status = root
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("");
    info!("trajectory 解析: steps={}, cascade_status={}", steps.len(), status);

    let prompt_trimmed = prompt.trim();

    // 分类收集：思考内容 vs 正文内容
    let mut thinking_texts: Vec<String> = Vec::new();
    let mut response_texts: Vec<(usize, String, String)> = Vec::new(); // (index, step_type, text)

    for (i, step) in steps.iter().enumerate() {
        let step_type = step.get("type").and_then(Value::as_str).unwrap_or("");
        let step_status = step
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("");

        // 跳过用户输入步骤
        if step_type.contains("USER_INPUT") {
            continue;
        }

        // 跳过标题/摘要生成步骤
        if step_type.contains("NOTIFY_USER")
            || step_type.contains("TITLE")
            || step_type.contains("SUMMARY")
            || step_type.contains("OBJECTIVE")
        {
            info!("step[{}] type={} → 跳过（标题/通知类型）", i, step_type);
            continue;
        }

        // 提取文本内容
        let mut step_texts = Vec::new();
        extract_step_text_content(step, &mut step_texts);

        // 过滤噪声
        let clean_texts: Vec<String> = step_texts
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| t.len() > 10)
            .filter(|t| t != prompt_trimmed)
            .filter(|t| !t.starts_with("CORTEX_"))
            .filter(|t| !t.starts_with("CASCADE_"))
            .filter(|t| !t.starts_with("file:///"))
            .filter(|t| !looks_like_uuid(t))
            .filter(|t| !looks_like_timestamp(t))
            .filter(|t| !is_title_generation_content(t))
            .filter(|t| !looks_like_base64(t))
            .filter(|t| !is_system_context_content(t))
            .collect();

        if clean_texts.is_empty() {
            continue;
        }

        // 判断是思考链还是正文
        let is_thinking_step = step_type.contains("THINKING")
            || step_type.contains("REASONING")
            || step_type.contains("PLANNING")
            || step_type.contains("THOUGHT");

        for text in &clean_texts {
            let is_thinking = is_thinking_step || looks_like_thinking_content(text);

            if is_thinking {
                info!("step[{}] type={} → 思考链, len={}", i, step_type, text.len());
                thinking_texts.push(text.clone());
            } else {
                info!(
                    "step[{}] type={}, status={}, 正文 len={}, preview={}",
                    i, step_type, step_status, text.len(),
                    text.chars().take(100).collect::<String>()
                );
                response_texts.push((i, step_type.to_string(), text.clone()));
            }
        }
    }

    // 必须有正文
    let response = response_texts
        .iter()
        .find(|(_, st, _)| st.contains("AGENT_MESSAGE") || st.contains("ASSISTANT"))
        .or_else(|| {
            response_texts.iter().max_by_key(|(_, _, text)| text.len())
        })
        .map(|(idx, st, text)| {
            info!("trajectory 正文: step[{}] type={}, len={}", idx, st, text.len());
            text.clone()
        });

    if response.is_none() {
        // 没有正文，但可能所有内容都被归类到了思考链
        // 如果没有正文但有思考链，把最长的思考链当作正文返回
        if !thinking_texts.is_empty() {
            let fallback = thinking_texts.iter().max_by_key(|t| t.len()).cloned();
            info!("trajectory: 无正文但有思考链，降级使用思考链作为正文");
            return fallback;
        }
        return None;
    }

    let response_text = response.unwrap();

    // 根据 include_thinking 决定是否拼接思考链
    if include_thinking && !thinking_texts.is_empty() {
        let thinking_combined = thinking_texts.join("\n\n");
        info!(
            "trajectory 结果: 思考链 len={} + 正文 len={}",
            thinking_combined.len(), response_text.len()
        );
        Some(format!(
            "<thinking>\n{}\n</thinking>\n\n{}",
            thinking_combined, response_text
        ))
    } else {
        info!("trajectory 结果: 正文 len={}", response_text.len());
        Some(response_text)
    }
}

/// 从 trajectory step 中提取文本内容
/// 步骤的 content 可能在不同字段：content, text, rawText, message 等
fn extract_step_text_content(step: &Value, collector: &mut Vec<String>) {
    match step {
        Value::String(text) => {
            if text.len() > 2 && text.len() <= MAX_CONTENT_LENGTH {
                collector.push(text.clone());
            }
        }
        Value::Array(items) => {
            for item in items {
                extract_step_text_content(item, collector);
            }
        }
        Value::Object(map) => {
            // 文本内容字段（直接提取值）
            const CONTENT_FIELDS: &[&str] = &[
                "content", "text", "rawText", "rawContent", "value",
                "message", "answer", "output", "result", "body", "markdown",
                "botMessage", "assistantMessage", "agentMessage", "response",
            ];

            // 跳过的元数据字段和思考链字段（不递归进入）
            const SKIP_FIELDS: &[&str] = &[
                "metadata", "internalMetadata", "statusTransitions",
                "executionId", "createdAt", "updatedAt", "timestamp",
                "source", "repository", "workspaces", "initializationStateId",
                "thinking", "reasoning", "thought", "thinkingContent",
                "reasoningContent", "plan", "analysis",
            ];

            for (key, nested) in map {
                let key_lower = key.to_ascii_lowercase();

                // 优先从内容字段提取
                if CONTENT_FIELDS.iter().any(|f| key_lower == f.to_ascii_lowercase()) {
                    if let Some(text) = nested.as_str() {
                        if text.len() > 2 {
                            collector.push(text.to_string());
                        }
                    }
                }

                // 跳过元数据字段
                if SKIP_FIELDS.iter().any(|f| key_lower == f.to_ascii_lowercase()) {
                    continue;
                }

                // 递归其他字段
                extract_step_text_content(nested, collector);
            }
        }
        _ => {}
    }
}

/// 通用文本收集（降级用）
fn collect_all_strings(value: &Value, collector: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if text.len() > 2 && text.len() <= MAX_CONTENT_LENGTH {
                collector.push(text.clone());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_all_strings(item, collector);
            }
        }
        Value::Object(map) => {
            for (_, nested) in map {
                collect_all_strings(nested, collector);
            }
        }
        _ => {}
    }
}

fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36
        && s.chars().filter(|c| *c == '-').count() == 4
        && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// 检测 Antigravity 系统注入的上下文元数据
fn is_system_context_content(text: &str) -> bool {
    let trimmed = text.trim();

    // Conversation History 摘要块
    if (trimmed.starts_with("Conversation History")
        || trimmed.starts_with("# Conversation History")
        || trimmed.starts_with("Here are the conversation IDs"))
        && trimmed.contains("conversation")
        && trimmed.contains("Created:")
    {
        return true;
    }

    // conversation_summaries 块
    if trimmed.contains("<conversation_summaries>")
        || trimmed.contains("</conversation_summaries>")
    {
        return true;
    }

    // USER_REQUEST / ADDITIONAL_METADATA 系统块
    if trimmed.contains("<USER_REQUEST>")
        || trimmed.contains("<ADDITIONAL_METADATA>")
        || trimmed.contains("<EPHEMERAL_MESSAGE>")
    {
        return true;
    }

    // Step Id / CHECKPOINT 元数据
    if trimmed.starts_with("Step Id:")
        || trimmed.contains("{{ CHECKPOINT")
    {
        return true;
    }

    // user_information 块
    if trimmed.contains("<user_information>")
        || trimmed.contains("</user_information>")
    {
        return true;
    }

    // 大量 "Conversation {uuid}" 条目 —— 是历史列表
    let conv_count = trimmed.matches("Conversation ").count();
    if conv_count >= 3 && trimmed.contains("USER Objective") {
        return true;
    }

    false
}
fn looks_like_timestamp(s: &str) -> bool {
    s.len() > 20 && s.contains('T') && s.contains(':') && (s.ends_with('Z') || s.contains('+'))
}

/// 检测内容是否是标题/摘要生成的系统输出
fn is_title_generation_content(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    (lower.contains("conversation title") && lower.contains("objective"))
        || (lower.contains("generating") && lower.contains("title"))
        || lower.starts_with("# conversation title")
        || lower.starts_with("generating conversation")
}

/// 检测内容是否看起来像 base64 编码的二进制数据
fn looks_like_base64(text: &str) -> bool {
    if text.len() < 50 {
        return false;
    }
    // base64 不包含空格和换行（除非是多行 base64，但 protobuf 通常是单行）
    if text.contains(' ') || text.contains('\n') {
        return false;
    }
    // 检查字符组成：base64 只含 A-Za-z0-9+/=
    let base64_chars = text.chars().filter(|c| {
        c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '='
    }).count();
    let ratio = base64_chars as f64 / text.len() as f64;
    // 如果 95%+ 字符是 base64 字符且长度超过 100，很可能是编码数据
    ratio > 0.95 && text.len() > 100
}

/// 检测内容是否是 AI 的思考链/推理过程
fn looks_like_thinking_content(text: &str) -> bool {
    // Gemini 思考链的典型模式：多个段落标题 + 以"I'm"/"I am"/"I'll"开头的分析性描述
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 3 {
        return false;
    }

    // 计算看起来像思考过程标题的行数（短行、标题式）
    let thinking_headers = lines.iter().filter(|line| {
        let trimmed = line.trim();
        let word_count = trimmed.split_whitespace().count();
        // 短标题（2-6个词），且包含动名词形式
        word_count >= 2 && word_count <= 8 && (
            trimmed.ends_with("Input")
            || trimmed.ends_with("Elements")
            || trimmed.ends_with("Response")
            || trimmed.ends_with("Scene")
            || trimmed.ends_with("Dialogue")
            || trimmed.ends_with("Approach")
            || trimmed.starts_with("Analyzing")
            || trimmed.starts_with("Extracting")
            || trimmed.starts_with("Refining")
            || trimmed.starts_with("Crafting")
            || trimmed.starts_with("Generating")
            || trimmed.starts_with("Processing")
            || trimmed.starts_with("Evaluating")
            || trimmed.starts_with("Planning")
            || trimmed.starts_with("Reviewing")
            || trimmed.starts_with("Considering")
        )
    }).count();

    // 如果有 3 个或更多思考标题，很可能是思考链
    if thinking_headers >= 3 {
        return true;
    }

    // 检查内容是否大量使用第一人称分析性语言
    let lower = text.to_ascii_lowercase();
    let thinking_phrases = [
        "i'm currently", "i'm now", "i'm focusing",
        "i've zeroed", "i'll need to", "i must",
        "i'm developing", "i'm generating", "i am now",
        "my focus is", "i'm setting the stage",
        "i'm dissecting", "i want to convey",
    ];
    let matches = thinking_phrases.iter().filter(|p| lower.contains(**p)).count();
    matches >= 3
}

fn is_protocol_mismatch_message(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    lower.contains("client sent an http request to an https server")
        || lower.contains("wrong version number")
        || lower.contains("ssl")
        || lower.contains("tls")
        || lower.contains("https")
        || lower.contains("http request")
}

fn truncate_for_error(content: &str) -> String {
    let max_chars = 200usize;
    let mut result = content.chars().take(max_chars).collect::<String>();
    if content.chars().count() > max_chars {
        result.push_str("...");
    }
    result
}
