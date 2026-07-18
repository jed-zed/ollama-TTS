//! HTTP 客户端工厂
//!
//! 仅负责统一超时和浏览器 User-Agent。
//! 代理策略交给 reqwest 默认行为（系统环境/系统代理）处理，避免手动代理配置导致回归。

use once_cell::sync::Lazy;
use std::time::Duration;

/// Chrome 浏览器 User-Agent
const CHROME_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// 默认 HTTP 客户端在整个进程内共享。
///
/// `reqwest::Client::clone` 只克隆轻量句柄，底层连接池、DNS 缓存等状态保持共享。
/// 认证信息仍由每个请求单独设置，避免不同上游之间泄漏请求头。
static SHARED_HTTP_CLIENT: Lazy<reqwest::Client> =
    Lazy::new(|| build_http_client(Duration::from_secs(300), Duration::from_secs(30)));

/// 创建统一配置的 HTTP 客户端
///
/// - 代理行为：使用 reqwest 默认策略（环境变量/系统设置）
/// - 设置 Chrome User-Agent（防止被反代/CDN 拦截）
/// - 默认超时 300 秒，连接超时 30 秒
pub fn create_http_client() -> reqwest::Client {
    SHARED_HTTP_CLIENT.clone()
}

/// 创建带自定义超时的 HTTP 客户端
///
/// 同样会应用统一的超时和浏览器 UA。
pub fn create_http_client_with_timeout(
    timeout: Duration,
    connect_timeout: Duration,
) -> reqwest::Client {
    build_http_client(timeout, connect_timeout)
}

fn build_http_client(timeout: Duration, connect_timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(connect_timeout)
        .user_agent(CHROME_USER_AGENT)
        .build()
        .expect("创建 HTTP 客户端失败")
}
