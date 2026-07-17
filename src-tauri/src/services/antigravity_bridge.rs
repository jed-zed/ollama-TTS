use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::sync::{Mutex, Notify, Semaphore};
use tracing::{debug, info, warn};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows CREATE_NO_WINDOW flag — 防止 powershell 弹出终端窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
use uuid::Uuid;

const MIN_TIMEOUT_MS: i32 = 1_000;
const MAX_TIMEOUT_MS: i32 = 300_000;
const MAX_PROMPT_LENGTH: usize = 2_000_000;
const MAX_CONTENT_LENGTH: usize = 1_000_000;
const DEFAULT_POLL_INTERVAL_MS: u64 = 1_000;
const STREAM_POLL_INTERVAL_MS: u64 = 300;
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
    csrf_header: String,
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
        Self::with_concurrency(DEFAULT_MAX_CONCURRENT)
    }

    pub fn with_concurrency(max_concurrent: usize) -> Self {
        let max = if max_concurrent == 0 { DEFAULT_MAX_CONCURRENT } else { max_concurrent };
        Self {
            inner: Arc::new(Mutex::new(BridgeInner {
                queue: VecDeque::new(),
                waiters: HashMap::new(),
            })),
            ls_cache: Arc::new(Mutex::new(None)),
            concurrency: Arc::new(Semaphore::new(max)),
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
                        csrf_header: cached.connection.csrf_header.clone(),
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
                    csrf_header: connection.csrf_header.clone(),
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
        model_id: &str,
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
            .http1_only()
            .no_gzip()
            .no_brotli()
            .no_deflate()
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
        let cascade_title: String = format!("O2O: {}", prompt.chars().take(30).collect::<String>());
        let cascade_id = match start_cascade(&client, &connection, Some(&cascade_title)).await {
            Ok(id) => id,
            Err(e) => {
                // cascade 创建失败可能是连接已失效，清除缓存
                warn!("cascade 创建失败，清除 LS 缓存: {}", e);
                self.invalidate_ls_cache().await;
                return Err(e);
            }
        };
        info!("[{}] 创建独立 cascade 会话成功", cascade_id);

        // 首次请求时查询 GetUserStatus 输出可用模型信息（仅一次）
        {
            use std::sync::atomic::{AtomicBool, Ordering};
            static LOGGED_MODELS: AtomicBool = AtomicBool::new(false);
            if !LOGGED_MODELS.swap(true, Ordering::Relaxed) {
                if let Ok(status) = call_ls_method(&client, &connection, "GetUserStatus", serde_json::json!({})).await {
                    let s = status.to_string();
                    info!("========= 可用模型信息 (GetUserStatus) =========\n{}", &s.chars().take(3000).collect::<String>());
                }
            }
        }

        info!(
            "[{}] 发送消息: prompt_len={}, preview={}",
            cascade_id,
            prompt.len(),
            prompt.chars().take(200).collect::<String>()
        );
        send_user_message(&client, &connection, &cascade_id, &prompt, model_id).await?;
        info!("[{}] 消息发送成功", cascade_id);

        // 轮询此 cascade 的 trajectory（每个 cascade_id 独立，不会混淆）
        let result = wait_for_stable_response(
            &client, &connection, &cascade_id, &prompt, timeout, include_thinking
        ).await;

        // _permit 在此作用域结束时自动释放，允许下一个排队请求进入
        result
    }

    /// 流式版本：返回一个 Receiver，轮询过程中每次检测到新增内容就发送 delta
    pub async fn execute_builtin_ls_stream(
        &self,
        prompt: String,
        timeout_ms: i32,
        include_thinking: bool,
        model_id: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<String>, String> {
        if prompt.trim().is_empty() {
            return Err("bridge input is empty".to_string());
        }
        if prompt.len() > MAX_PROMPT_LENGTH {
            return Err("bridge input is too long".to_string());
        }

        let _permit = Arc::clone(&self.concurrency).acquire_owned().await
            .map_err(|_| "bridge concurrency semaphore closed".to_string())?;
        info!("内置 LS 流式桥接获取并发许可");

        let timeout = timeout_ms.clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS) as u64;

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_millis(timeout))
            .build()
            .map_err(|err| format!("connect: build ls client failed: {}", err))?;

        let connection = match self.get_or_discover_connection().await {
            Ok(conn) => conn,
            Err(e) => {
                warn!("LS 连接发现失败: {}, 清除缓存重试", e);
                self.invalidate_ls_cache().await;
                return Err(e);
            }
        };

        let cascade_title: String = format!("O2O: {}", prompt.chars().take(30).collect::<String>());
        let cascade_id = match start_cascade(&client, &connection, Some(&cascade_title)).await {
            Ok(id) => id,
            Err(e) => {
                warn!("cascade 创建失败，清除 LS 缓存: {}", e);
                self.invalidate_ls_cache().await;
                return Err(e);
            }
        };
        info!("[{}] 流式 cascade 创建成功", cascade_id);

        send_user_message(&client, &connection, &cascade_id, &prompt, model_id).await?;
        info!("[{}] 流式消息发送成功", cascade_id);

        let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);
        let prompt_clone = prompt.clone();

        // 在后台 spawn 轮询任务，通过 tx 增量发送内容
        tokio::spawn(async move {
            // 持有 _permit 直到 spawn 的任务结束
            let _permit_guard = _permit;

            let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout);
            let mut last_len: usize = 0;
            let mut poll_count = 0u32;

            // 先探测读取方法
            let discover_result = discover_read_method(&client, &connection, &cascade_id).await;
            let read_info = match discover_result {
                Some(info) => info,
                None => {
                    let _ = tx.send("[ERROR] 无法找到 LS 读取方法".to_string()).await;
                    return;
                }
            };
            let (read_method, read_param) = read_info;

            while tokio::time::Instant::now() < deadline {
                poll_count += 1;
                let payload = json!({ read_param: &cascade_id });

                let response = match call_ls_method(&client, &connection, read_method, payload).await {
                    Ok(r) => r,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_millis(STREAM_POLL_INTERVAL_MS)).await;
                        continue;
                    }
                };

                let cascade_status = response.get("status").and_then(Value::as_str).unwrap_or("");
                let is_finished = cascade_status.contains("IDLE")
                    || cascade_status.contains("DONE")
                    || cascade_status.contains("COMPLETED")
                    || cascade_status.contains("FINISHED");

                // 提取当前累计内容
                let full_text = extract_assistant_text(&response, &prompt_clone, include_thinking, true);
                if full_text.len() > last_len {
                    // 发送增量 delta
                    let delta = full_text[last_len..].to_string();
                    last_len = full_text.len();
                    if tx.send(delta).await.is_err() {
                        info!("[{}] 流式接收端已关闭，停止轮询", cascade_id);
                        return;
                    }
                }

                if is_finished && !full_text.is_empty() {
                    info!(
                        "[{}] 流式 cascade 完成: polls={}, total_len={}",
                        cascade_id, poll_count, full_text.len()
                    );
                    return;
                }

                tokio::time::sleep(Duration::from_millis(STREAM_POLL_INTERVAL_MS)).await;
            }

            warn!("[{}] 流式桥接轮询超时", cascade_id);
        });

        Ok(rx)
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
    // 一步到位：找进程 → 取端口 → curl 探测，和手动发现方式完全一致
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$procs = Get-CimInstance Win32_Process | Where-Object {
    $_.Name -match 'language_server' -and $_.CommandLine -match '--csrf_token'
}
foreach ($proc in $procs) {
    $token = ''
    if ($proc.CommandLine -match '--csrf_token\s+(\S+)') { $token = $Matches[1] }
    if (-not $token) { continue }
    $ports = @(Get-NetTCPConnection -OwningProcess $proc.ProcessId -State Listen -EA SilentlyContinue | Select-Object -ExpandProperty LocalPort)
    if ($ports.Count -eq 0) {
        # 备选：netstat
        netstat -aon | Select-String ('LISTENING\s+' + $proc.ProcessId + '\s*$') | ForEach-Object {
            if ($_ -match ':(\d+)\s') { $ports += [int]$Matches[1] }
        }
    }
    foreach ($p in $ports) {
        foreach ($proto in @('http','https')) {
            $url = "${proto}://127.0.0.1:${p}/exa.language_server_pb.LanguageServerService/GetUserStatus"
            $code = ''
            try {
                $code = & curl.exe -s -o NUL -w '%{http_code}' --connect-timeout 1 -X POST $url -H 'Content-Type: application/json' -H 'connect-protocol-version: 1' -H "x-codeium-csrf-token: $token" -d '{}' 2>$null
            } catch {}
            if ($code -eq '200') {
                Write-Output "${p}|${token}|x-codeium-csrf-token|${proto}"
                exit 0
            }
        }
    }
}
Write-Output 'NOTFOUND'
"#;

    let mut cmd = Command::new("powershell");
    cmd.arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(script);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output()
        .await
        .map_err(|err| format!("discover: failed to run discovery script: {}", err))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    info!("LS 发现脚本输出: {}", stdout);

    if stdout.is_empty() || stdout == "NOTFOUND" {
        return Err("discover: no reachable LS found via curl probe".to_string());
    }

    let parts: Vec<&str> = stdout.lines().last().unwrap_or("").split('|').collect();
    if parts.len() < 4 {
        return Err(format!("discover: unexpected script output: {}", stdout));
    }

    let port = parts[0].parse::<u16>()
        .map_err(|_| format!("discover: invalid port: {}", parts[0]))?;
    let csrf_token = parts[1].to_string();
    let csrf_header = parts[2].to_string();
    let prefer_https = parts[3] == "https";

    info!("LS 发现成功: port={}, token={}..., header={}, proto={}",
          port, &csrf_token[..csrf_token.len().min(8)], csrf_header, parts[3]);

    Ok(LsConnection {
        port,
        csrf_token,
        csrf_header,
        prefer_https,
    })
}

async fn discover_all_listening_ports(pid: u32, ext_port: Option<u16>) -> Vec<u16> {
    // 策略1: Get-NetTCPConnection（精准按 PID，Windows 8+）
    // 策略2: netstat + 严格正则匹配 PID（兼容所有 Windows）
    let script = format!(
        r#"try {{
  $p = Get-NetTCPConnection -OwningProcess {pid} -State Listen -EA Stop
  $p | ForEach-Object {{ $_.LocalPort }}
}} catch {{
  netstat -aon | Select-String 'LISTENING\s+{pid}\s*$' | ForEach-Object {{
    if ($_ -match ':(\d+)\s') {{ $Matches[1] }}
  }}
}}"#,
        pid = pid
    );

    let mut cmd = Command::new("powershell");
    cmd.arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(script);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = match cmd.output().await {
        Ok(o) => o,
        Err(e) => {
            warn!("端口发现命令执行失败: {}", e);
            return Vec::new();
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ports = Vec::new();
    for line in stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Ok(port) = line.parse::<u16>() {
            if Some(port) != ext_port && !ports.contains(&port) {
                ports.push(port);
            }
        }
    }

    ports.sort_unstable();
    info!("LS 候选端口(排除 ext_port {:?}): {:?} (pid={})", ext_port, ports, pid);
    ports
}

async fn start_cascade(client: &reqwest::Client, connection: &LsConnection, _title: Option<&str>) -> Result<String, String> {
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
    model_id: &str,
) -> Result<(), String> {
    // LS 强制要求指定 requestedModel — 尝试多种 proto3 JSON 格式
    // 格式1: 直接 "model": "MODEL_xxx"（proto3 枚举字符串）
    let config_v1 = json!({
        "plannerConfig": {
            "plannerTypeConfig": {
                "conversational": {}
            },
            "requestedModel": {
                "model": model_id
            }
        }
    });
    // 格式2: ConnectRPC oneof 格式 choice.case/value
    let config_v2 = json!({
        "plannerConfig": {
            "plannerTypeConfig": {
                "conversational": {}
            },
            "requestedModel": {
                "choice": {
                    "case": "model",
                    "value": model_id
                }
            }
        }
    });
    // 格式3: modelOrAlias 嵌套
    let config_v3 = json!({
        "plannerConfig": {
            "plannerTypeConfig": {
                "conversational": {}
            },
            "requestedModel": {
                "modelOrAlias": {
                    "model": model_id
                }
            }
        }
    });

    let payloads: Vec<(&str, Value)> = vec![
        // config_v1: "model": "MODEL_xxx" + items text
        (
            "v1_model_enum",
            json!({
                "cascadeId": cascade_id,
                "items": [{ "text": prompt }],
                "cascadeConfig": config_v1
            }),
        ),
        // config_v2: ConnectRPC choice + items text
        (
            "v2_choice",
            json!({
                "cascadeId": cascade_id,
                "items": [{ "text": prompt }],
                "cascadeConfig": config_v2
            }),
        ),
        // config_v3: modelOrAlias + items text
        (
            "v3_modelOrAlias",
            json!({
                "cascadeId": cascade_id,
                "items": [{ "text": prompt }],
                "cascadeConfig": config_v3
            }),
        ),
        // config_v1 + ConnectRPC items 格式
        (
            "v1_connectrpc_items",
            json!({
                "cascadeId": cascade_id,
                "items": [{
                    "chunk": {
                        "case": "text",
                        "value": prompt
                    }
                }],
                "cascadeConfig": config_v1
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

    // 所有 payload variant 全部失败 — 查询 GetUserStatus 输出可用模型信息
    if let Ok(status) = call_ls_method(client, connection, "GetUserStatus", serde_json::json!({})).await {
        let status_str = status.to_string();
        info!("GetUserStatus (调试可用模型): {}", &status_str.chars().take(2000).collect::<String>());
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

    // 用 call_ls_method (curl) 探测读取方法
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
        match call_ls_method(client, connection, method, payload).await {
            Ok(body) => {
                let preview = body.to_string();
                let preview: String = preview.chars().take(300).collect();
                info!("探测成功: {}/{} → body_len={}, preview={}", method, param, preview.len(), preview);
                return Some((method, param));
            }
            Err(err) => {
                info!("探测 {}/{} → 失败: {}", method, param, err);
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
            // Cascade 仍在运行，但也尝试提取已部分生成的内容
            let candidate = extract_assistant_text(&response, prompt, include_thinking, true);
            if !candidate.is_empty() && candidate.len() >= last_content.len() {
                last_content = candidate;
                stable_rounds = 0; // 还在运行中，不算稳定
            }
            if poll_count <= 5 {
                info!("轮询[{}] cascade 仍在运行 ({}), 当前最长内容 len={}", poll_count, cascade_status, last_content.len());
            }
            tokio::time::sleep(Duration::from_millis(DEFAULT_POLL_INTERVAL_MS)).await;
            continue;
        }

        // Cascade 已完成，提取回复
        let candidate = extract_assistant_text(&response, prompt, include_thinking, false);
        if poll_count <= 5 {
            info!(
                "轮询[{}] cascade 已完成, extract 结果: len={}, preview={}",
                poll_count,
                candidate.len(),
                candidate.chars().take(200).collect::<String>()
            );
        }

        if !candidate.is_empty() {
            // 关键：只有新内容 >= 已有最长内容时才更新
            // 防止 LS 临时用 bot-ID 等短字符串替换已生成的真实内容
            if candidate.len() >= last_content.len() {
                if candidate == last_content {
                    stable_rounds += 1;
                } else {
                    stable_rounds = 1;
                    last_content = candidate;
                }
            } else {
                // 新内容比已有内容短 —— 忽略，保留已有的长内容
                info!(
                    "轮询[{}] 新内容更短 ({}< {})，保留旧内容",
                    poll_count, candidate.len(), last_content.len()
                );
                stable_rounds += 1; // 但也算一轮稳定
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
        match call_ls_method(client, connection, "GetAllCascadeTrajectories", json!({})).await {
            Ok(parsed) => {
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
    _client: &reqwest::Client,
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

    for (_idx, proto) in proto_order.iter().enumerate() {
        let url = format!(
            "{}://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/{}",
            proto, connection.port, method
        );

        let csrf_header_val = format!("{}: {}", connection.csrf_header, connection.csrf_token);

        let mut cmd = Command::new("curl.exe");
        // 写入临时文件避免 Windows 命令行长度限制 (32KB)
        let tmp_path = std::env::temp_dir().join(format!("ls_payload_{}.json", std::process::id()));
        if let Err(e) = tokio::fs::write(&tmp_path, &payload_text).await {
            last_error = format!("connect: write payload temp file failed: {}", e);
            continue;
        }
        let at_file = format!("@{}", tmp_path.display());

        cmd.arg("-s")
            .arg("--connect-timeout").arg("5")
            .arg("-m").arg("120")
            .arg("-X").arg("POST")
            .arg(&url)
            .arg("-H").arg("Content-Type: application/json")
            .arg("-H").arg("connect-protocol-version: 1")
            .arg("-H").arg(&csrf_header_val)
            .arg("-d").arg(&at_file)
            .arg("-w").arg("\n%{http_code}");
        if *proto == "https" {
            cmd.arg("-k");
        }
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let output = match cmd.output().await {
            Ok(o) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                o
            }
            Err(err) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                last_error = format!("connect: curl {} via {} failed: {}", method, proto, err);
                continue;
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        // curl -w "\n%{http_code}" 在最后一行输出 HTTP 状态码
        let (body, status_str) = match stdout.rsplit_once('\n') {
            Some((b, s)) => (b.to_string(), s.trim().to_string()),
            None => (stdout.clone(), String::new()),
        };

        let status_code: u16 = status_str.parse().unwrap_or(0);

        if status_code == 0 {
            last_error = format!("connect: curl {} via {} returned no status (connection failed)", method, proto);
            continue;
        }

        if status_code < 200 || status_code >= 300 {
            let body_hint = truncate_for_error(&body);
            last_error = format!(
                "connect: {} via {} returned {} {}: {}",
                method, proto, status_code,
                if status_code == 502 { "Bad Gateway" }
                else if status_code == 401 { "Unauthorized" }
                else if status_code == 404 { "Not Found" }
                else { "" },
                body_hint
            );

            // 404 允许切换协议重试
            if status_code == 404 && _idx == 0 {
                continue;
            }
            return Err(last_error);
        }

        return serde_json::from_str::<Value>(&body)
            .map_err(|err| format!("parse: decode {} response failed: {} body={}", method, err, &body[..body.len().min(200)]));
    }

    if last_error.is_empty() {
        Err(format!("connect: request {} failed", method))
    } else {
        Err(last_error)
    }
}

fn extract_assistant_text(root: &Value, prompt: &str, include_thinking: bool, skip_degraded: bool) -> String {
    // 优先使用 trajectory steps 精准解析
    if let Some(text) = extract_from_trajectory_steps(root, prompt, include_thinking) {
        return text;
    }

    // 如果调用者要求跳过降级路径（如 cascade 仍在 RUNNING），直接返回空
    if skip_degraded {
        return String::new();
    }

    // 降级：通用最长文本搜索
    let mut candidates = Vec::new();
    collect_all_strings(root, &mut candidates);

    let prompt_trimmed = prompt.trim();
    let total = candidates.len();

    // 第一层：基本清洗（去掉明确的噪声）
    let cleaned: Vec<String> = candidates
        .into_iter()
        .map(|text| text.trim().to_string())
        .filter(|text| text.len() > 10)
        .filter(|text| text != prompt_trimmed)
        .filter(|text| !text.starts_with("CORTEX_"))
        .filter(|text| !text.starts_with("CASCADE_STATUS") && !text.starts_with("CASCADE_STEP") && !text.starts_with("CASCADE_STATE"))
        .filter(|text| !text.starts_with("file:///"))
        .filter(|text| !text.starts_with("bot-"))
        .filter(|text| !text.starts_with("STOP_REASON_"))
        .filter(|text| !text.starts_with("MODEL_PLACEHOLDER"))
        .filter(|text| !looks_like_uuid(text))
        .filter(|text| !looks_like_timestamp(text))
        .filter(|text| !looks_like_duration_str(text))
        .filter(|text| !looks_like_base64(text))
        .collect();

    info!(
        "extract_assistant_text(降级): 总候选={}, 基本清洗后={}",
        total, cleaned.len()
    );

    // 第二层：尝试排除系统上下文噪声
    let strict_filtered: Vec<&String> = cleaned.iter()
        .filter(|text| !is_system_context_content(text))
        .filter(|text| !is_title_generation_content(text))
        .filter(|text| !looks_like_thinking_content(text))
        .collect();

    // 如果严格过滤后还有候选，选最长的
    if !strict_filtered.is_empty() {
        let selected = strict_filtered.into_iter().max_by_key(|t| t.len()).unwrap();
        info!(
            "extract_assistant_text(降级): 严格过滤选中, len={}, preview={}",
            selected.len(),
            selected.chars().take(100).collect::<String>()
        );
        selected.clone()
    } else {
        // 严格过滤移除了所有候选（通常是系统元数据），返回空让轮询继续等待
        info!("extract_assistant_text(降级): 严格过滤为空，返回空（等待真正的 AI 回复）");
        String::new()
    }
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
    debug!("trajectory 解析: steps={}, cascade_status={}", steps.len(), status);

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
        // 只跳过纯粹的标题/摘要/目标生成步骤
        // NOTIFY_USER 不能跳过！CORTEX_STEP_TYPE_NOTIFY_USER 包含 AI 的正式回复
        if step_type.contains("TITLE_GENERATION")
            || step_type.contains("SUMMARY_GENERATION")
            || step_type.contains("OBJECTIVE_GENERATION")
            || step_type.contains("GENERATE_TITLE")
            || step_type.contains("GENERATE_SUMMARY")
        {
            info!("step[{}] type={} → 跳过（标题/摘要生成类型）", i, step_type);
            continue;
        }

        // 提取文本内容
        let mut step_texts = Vec::new();
        extract_step_text_content(step, &mut step_texts);

        // 诊断日志：dump step 的 JSON 结构（前3个step）
        if i < 3 {
            if let Value::Object(map) = step {
                let keys_info: Vec<String> = map.iter().map(|(k, v)| {
                    match v {
                        Value::String(s) => format!("{}:str({})", k, s.len()),
                        Value::Array(a) => format!("{}:arr({})", k, a.len()),
                        Value::Object(m) => format!("{}:obj({}keys)", k, m.len()),
                        Value::Number(n) => format!("{}:num({})", k, n),
                        Value::Bool(b) => format!("{}:bool({})", k, b),
                        Value::Null => format!("{}:null", k),
                    }
                }).collect();
                info!("step[{}] type={} JSON结构: {}", i, step_type, keys_info.join(", "));
            }
        }

        // 诊断日志：显示提取到的原始文本（前5个step详细记录）
        if i < 5 {
            for (ti, t) in step_texts.iter().enumerate() {
                let preview: String = t.chars().take(150).collect();
                info!("step[{}] 原始文本[{}]: len={}, preview={}", i, ti, t.len(), preview);
            }
        }

        // 过滤噪声
        let clean_texts: Vec<String> = step_texts
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| t.len() > 10)
            .filter(|t| t != prompt_trimmed)
            .filter(|t| !t.starts_with("CORTEX_"))
            .filter(|t| !t.starts_with("CASCADE_STATUS") && !t.starts_with("CASCADE_STEP") && !t.starts_with("CASCADE_STATE"))
            .filter(|t| !t.starts_with("file:///"))
            .filter(|t| !t.starts_with("bot-"))
            .filter(|t| !t.starts_with("STOP_REASON_"))
            .filter(|t| !t.starts_with("MODEL_PLACEHOLDER"))
            .filter(|t| !looks_like_uuid(t))
            .filter(|t| !looks_like_timestamp(t))
            .filter(|t| !looks_like_duration_str(t))
            .filter(|t| !is_title_generation_content(t))
            .filter(|t| !looks_like_base64(t))
            .filter(|t| !is_system_context_content(t))
            .collect();

        // 诊断日志：显示过滤后的文本
        if i < 5 {
            for (ti, t) in clean_texts.iter().enumerate() {
                let preview: String = t.chars().take(150).collect();
                info!("step[{}] 过滤后[{}]: len={}, preview={}", i, ti, t.len(), preview);
            }
        }

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

    // 拼接所有正文步骤（AI 回复可能被 LS 拆分到多个 step 里）
    if response_texts.is_empty() {
        // 没有正文，但可能所有内容都被归类到了思考链
        if !thinking_texts.is_empty() {
            let fallback = thinking_texts.iter().max_by_key(|t| t.len()).cloned();
            info!("trajectory: 无正文但有思考链，降级使用思考链作为正文");
            return fallback;
        }
        return None;
    }

    // 合并正文步骤（去重：流式轮询时同一内容可能出现在多个 step）
    let response_text: String = if response_texts.len() == 1 {
        let (idx, st, text) = &response_texts[0];
        info!("trajectory 正文: step[{}] type={}, len={}", idx, st, text.len());
        text.clone()
    } else {
        // 按长度降序，优先保留长文本；跳过被其他文本包含的内容
        let mut sorted: Vec<&(usize, String, String)> = response_texts.iter().collect();
        sorted.sort_by(|a, b| b.2.len().cmp(&a.2.len()));

        let mut unique_parts: Vec<&str> = Vec::new();
        for (idx, st, text) in &sorted {
            let is_subset = unique_parts.iter().any(|existing| existing.contains(text.as_str()));
            if is_subset {
                debug!("  step[{}] type={}, len={} → 跳过（已被包含）", idx, st, text.len());
                continue;
            }
            info!("  step[{}] type={}, len={}", idx, st, text.len());
            unique_parts.push(text.as_str());
        }

        info!("trajectory 正文: {} 个步骤去重后 {} 个独立部分", response_texts.len(), unique_parts.len());

        if unique_parts.len() == 1 {
            unique_parts[0].to_string()
        } else {
            // 按原始 step 顺序排列
            let mut ordered: Vec<&str> = Vec::new();
            for (_, _, text) in &response_texts {
                if unique_parts.contains(&text.as_str()) && !ordered.contains(&text.as_str()) {
                    ordered.push(text.as_str());
                }
            }
            ordered.join("\n\n")
        }
    };

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
        || trimmed.starts_with("Here are the conversation IDs")
        || trimmed.starts_with("# Conversation Logs"))
        && (trimmed.contains("conversation") || trimmed.contains("Conversation"))
        && (trimmed.contains("Created:") || trimmed.contains("Last modified:"))
    {
        return true;
    }

    // conversation_summaries 块
    if trimmed.contains("<conversation_summaries>")
        || trimmed.contains("</conversation_summaries>")
        || trimmed.contains("conversation_summaries")
    {
        return true;
    }

    // USER_REQUEST / ADDITIONAL_METADATA 系统块
    if trimmed.contains("<USER_REQUEST>")
        || trimmed.contains("<ADDITIONAL_METADATA>")
        || trimmed.contains("<EPHEMERAL_MESSAGE>")
        || trimmed.contains("<USER_SETTINGS_CHANGE>")
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

    // 大量 "Conversation" 条目 —— 是历史列表
    let conv_count = trimmed.matches("Conversation ").count();
    if conv_count >= 3 && (trimmed.contains("USER Objective") || trimmed.contains("Last modified:")) {
        return true;
    }

    // 新增：检测 "Most recent N conversations" 格式
    if trimmed.contains("most recent") && trimmed.contains("conversations") && trimmed.contains("reverse chronological") {
        return true;
    }

    // 新增：Antigravity 内部的 MCP / artifact 元数据
    if trimmed.contains("<mcp_servers>")
        || trimmed.contains("<artifacts>")
        || trimmed.contains("<identity>")
        || trimmed.contains("<agentic_mode_overview>")
        || trimmed.contains("<web_application_development>")
        || trimmed.contains("<communication_style>")
    {
        return true;
    }

    // 新增：前一段对话的截断总结
    if (trimmed.starts_with("The earlier parts of this conversation have been truncated")
        || trimmed.contains("CHECKPOINT MESSAGE"))
        && trimmed.len() > 500
    {
        return true;
    }

    false
}
fn looks_like_timestamp(s: &str) -> bool {
    s.len() > 20 && s.contains('T') && s.contains(':') && (s.ends_with('Z') || s.contains('+'))
}

/// 检测 protobuf Duration 格式字符串，如 "0.658818200s"、"12.345s"
fn looks_like_duration_str(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.len() < 2 || !trimmed.ends_with('s') {
        return false;
    }
    let num_part = &trimmed[..trimmed.len() - 1];
    num_part.parse::<f64>().is_ok()
}

/// 检测内容是否是标题/摘要生成的系统输出
fn is_title_generation_content(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    // 已知的标题/摘要生成模式
    if (lower.contains("conversation title") && lower.contains("objective"))
        || (lower.contains("generating") && lower.contains("title") && lower.len() < 200)
        || lower.starts_with("# conversation title")
        || lower.starts_with("generating conversation")
    {
        return true;
    }
    // 用户目标摘要（trajectory description metadata）
    // 必须同时包含 "user's" + "main/primary goal" 且总长度较短（<500），避免匹配到正文
    if text.len() < 500 {
        if (lower.contains("the user's main goal") || lower.contains("the user's primary goal"))
            && !lower.contains("```")  // 排除包含代码块的内容
        {
            return true;
        }
    }
    false
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
