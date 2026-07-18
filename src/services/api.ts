/**
 * HTTP 客户端封装
 * 统一管理前后端通信
 */

import { invoke } from '@tauri-apps/api/core';
import type { ApiResponse } from './types';
import { HEALTH_CHECK_TIMEOUT_MS } from '@/lib/constants';
import { getBackendBaseUrl } from '@/lib/backendUrl';

class ApiClient {
  private baseUrl: string | null;

  constructor(baseUrl: string | null = null) {
    this.baseUrl = baseUrl;
  }

  /**
   * 设置基础 URL
   */
  setBaseUrl(url: string) {
    this.baseUrl = url;
  }

  /**
   * 获取基础 URL
   */
  getBaseUrl(): string {
    return this.baseUrl ?? getBackendBaseUrl();
  }

  /**
   * 启动后端服务 (Tauri)
   */
  async startBackend(): Promise<boolean> {
    try {
      await invoke('start_backend');
      return true;
    } catch (error) {
      console.warn('无法通过 Tauri 启动后端:', error);
      return false;
    }
  }

  /**
   * 停止后端服务 (Tauri)
   */
  async stopBackend(): Promise<boolean> {
    try {
      await invoke('stop_backend');
      return true;
    } catch (error) {
      console.warn('无法通过 Tauri 停止后端:', error);
      return false;
    }
  }

  /**
   * 通用请求方法
   */
  private async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<ApiResponse<T>> {
    const url = `${this.getBaseUrl()}${endpoint}`;

    const defaultHeaders: HeadersInit = {
      'Content-Type': 'application/json',
    };

    try {
      const response = await fetch(url, {
        ...options,
        headers: {
          ...defaultHeaders,
          ...options.headers,
        },
      });

      if (!response.ok) {
        const errorText = await response.text();
        let errorDetail = errorText || response.statusText;
        if (errorText) {
          try {
            const errorJson = JSON.parse(errorText) as {
              error?: string;
              message?: string;
              data?: { error?: string; message?: string };
            };
            errorDetail = errorJson.error
              ?? errorJson.message
              ?? errorJson.data?.error
              ?? errorJson.data?.message
              ?? errorDetail;
          } catch {
            // 非 JSON 错误体保留原文。
          }
        }
        return {
          success: false,
          error: `HTTP ${response.status}: ${errorDetail}`,
        };
      }

      const json = await response.json();
      
      // 后端返回 ApiResponse 格式: {success, data, error}
      // 需要正确解析嵌套结构
      if (typeof json === 'object' && json !== null && 'success' in json) {
        if (json.success && 'data' in json) {
          // 标准 ApiResponse 格式：{success: true, data: {...}}
          return {
            success: true,
            data: json.data as T,
          };
        } else if (json.success) {
          // 直接返回的响应格式：{success: true, available: true, ...}
          // 将整个对象作为 data 返回
          return {
            success: true,
            data: json as T,
          };
        } else {
          return {
            success: false,
            error: json.error || json.message || '请求失败',
          };
        }
      }
      
      // 兼容直接返回数据的情况
      return {
        success: true,
        data: json as T,
      };
    } catch (error) {
      const message = error instanceof Error ? error.message : '网络请求失败';
      return {
        success: false,
        error: message,
      };
    }
  }

  /**
   * GET 请求
   */
  async get<T>(endpoint: string): Promise<ApiResponse<T>> {
    return this.request<T>(endpoint, { method: 'GET' });
  }

  /**
   * POST 请求
   */
  async post<T>(endpoint: string, body?: unknown): Promise<ApiResponse<T>> {
    return this.request<T>(endpoint, {
      method: 'POST',
      body: body ? JSON.stringify(body) : undefined,
    });
  }

  /**
   * PUT 请求
   */
  async put<T>(endpoint: string, body?: unknown): Promise<ApiResponse<T>> {
    return this.request<T>(endpoint, {
      method: 'PUT',
      body: body ? JSON.stringify(body) : undefined,
    });
  }

  /**
   * DELETE 请求
   */
  async delete<T>(endpoint: string): Promise<ApiResponse<T>> {
    return this.request<T>(endpoint, { method: 'DELETE' });
  }

  /**
   * 流式请求 (SSE)
   */
  async stream(
    endpoint: string,
    body: unknown,
    onChunk: (data: unknown) => void,
    onError?: (error: string) => void,
    onComplete?: () => void
  ): Promise<void> {
    const url = `${this.getBaseUrl()}${endpoint}`;

    try {
      const response = await fetch(url, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Accept': 'text/event-stream',
        },
        body: JSON.stringify(body),
      });

      if (!response.ok) {
        const errorText = await response.text();
        let errorDetail = errorText || response.statusText;
        if (errorText) {
          try {
            const errorJson = JSON.parse(errorText) as {
              error?: string;
              message?: string;
              data?: { error?: string; message?: string };
            };
            errorDetail = errorJson.error
              ?? errorJson.message
              ?? errorJson.data?.error
              ?? errorJson.data?.message
              ?? errorDetail;
          } catch {
            // 非 JSON 错误体保留原文。
          }
        }
        onError?.(`HTTP ${response.status}: ${errorDetail}`);
        return;
      }

      const reader = response.body?.getReader();
      if (!reader) {
        onError?.('无法获取响应流');
        return;
      }

      const decoder = new TextDecoder();
      let buffer = '';
      let dataLines: string[] = [];
      let completed = false;

      const complete = () => {
        if (!completed) {
          completed = true;
          onComplete?.();
        }
      };

      const dispatchEvent = (): boolean => {
        if (dataLines.length === 0) return false;

        const payload = dataLines.join('\n');
        dataLines = [];

        if (payload.trim() === '[DONE]') {
          complete();
          return true;
        }

        try {
          onChunk(JSON.parse(payload));
        } catch {
          onError?.('流事件解析失败');
        }

        return false;
      };

      const processLine = (rawLine: string): boolean => {
        const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine;

        // 空行结束一个 SSE 事件；注释行只用于传输层保活。
        if (line === '') return dispatchEvent();
        if (line.startsWith(':')) return false;

        const colonIndex = line.indexOf(':');
        const field = colonIndex >= 0 ? line.slice(0, colonIndex) : line;
        let value = colonIndex >= 0 ? line.slice(colonIndex + 1) : '';
        if (value.startsWith(' ')) value = value.slice(1);

        if (field === 'data') dataLines.push(value);
        return false;
      };

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (processLine(line)) {
            await reader.cancel();
            return;
          }
        }
      }

      buffer += decoder.decode();
      if (buffer && processLine(buffer)) {
        complete();
        return;
      }
      dispatchEvent();
      complete();
    } catch (error) {
      const message = error instanceof Error ? error.message : '流式请求失败';
      onError?.(message);
    }
  }
}

// 导出单例
export const apiClient = new ApiClient();

// 导出类用于测试
export { ApiClient };

/**
 * 健康检查响应类型
 */
export interface HealthCheckResponse {
  status: string;
  version: string;
  timestamp: string;
  service: string;
}

/**
 * 检查后端健康状态
 */
export async function checkBackendHealth(): Promise<{
  connected: boolean;
  data?: HealthCheckResponse;
  error?: string;
}> {
  try {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), HEALTH_CHECK_TIMEOUT_MS);

    const response = await fetch(`${apiClient.getBaseUrl()}/health`, {
      method: 'GET',
      headers: { 'Content-Type': 'application/json' },
      signal: controller.signal,
    });

    clearTimeout(timeoutId);

    if (!response.ok) {
      return { connected: false, error: `HTTP ${response.status}` };
    }

    const data = await response.json();
    return { connected: true, data };
  } catch (error) {
    const message = error instanceof Error ? error.message : '连接失败';
    return { connected: false, error: message };
  }
}

/**
 * 轮询检测后端健康状态
 * 使用渐进式轮询策略：前 3 秒使用短间隔（100ms），之后使用长间隔（500ms）
 * @param timeout 最大等待时间 (毫秒)
 * @param fastInterval 快速轮询间隔 (毫秒)，用于前 3 秒
 * @param slowInterval 慢速轮询间隔 (毫秒)，用于 3 秒后
 * @param onProgress 进度回调 (可选)
 * @returns 是否成功连接
 */
export async function pollBackendHealth(
  timeout: number = 10000,
  fastInterval: number = 100,
  slowInterval: number = 500,
  onProgress?: (elapsed: number, total: number) => void
): Promise<boolean> {
  const startTime = Date.now();
  const fastPhaseDuration = 3000;

  while (Date.now() - startTime < timeout) {
    const elapsed = Date.now() - startTime;
    onProgress?.(elapsed, timeout);

    const result = await checkBackendHealth();
    if (result.connected) {
      return true;
    }

    // 前 3 秒使用快速轮询，之后使用慢速轮询
    const interval = elapsed < fastPhaseDuration ? fastInterval : slowInterval;
    await new Promise(r => setTimeout(r, interval));
  }

  return false;
}

/**
 * 轮询检测后端是否已停止
 * @param timeout 最大等待时间 (毫秒)
 * @param interval 轮询间隔 (毫秒)
 * @returns 是否已停止
 */
export async function pollBackendStopped(
  timeout: number = 5000,
  interval: number = 300
): Promise<boolean> {
  const startTime = Date.now();

  while (Date.now() - startTime < timeout) {
    const result = await checkBackendHealth();
    if (!result.connected) {
      return true;
    }

    await new Promise(r => setTimeout(r, interval));
  }

  return false;
}

/**
 * 代理服务状态响应类型
 */
export interface ProxyStatusResponse {
  running: boolean;
  port: number | null;
}

/**
 * 获取 Ollama 代理服务状态
 */
export async function getProxyStatus(): Promise<{
  success: boolean;
  data?: ProxyStatusResponse;
  error?: string;
}> {
  return apiClient.get<ProxyStatusResponse>('/api/proxy/status');
}

/**
 * 启动 Ollama 代理服务
 */
export async function startProxy(): Promise<{
  success: boolean;
  port?: number;
  error?: string;
}> {
  try {
    const response = await fetch(`${apiClient.getBaseUrl()}/api/proxy/start`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    });

    const data = await response.json();
    return { success: data.success, port: data.port, error: data.error };
  } catch (error) {
    const message = error instanceof Error ? error.message : '启动失败';
    return { success: false, error: message };
  }
}

/**
 * 停止 Ollama 代理服务
 */
export async function stopProxy(): Promise<{
  success: boolean;
  error?: string;
}> {
  try {
    const response = await fetch(`${apiClient.getBaseUrl()}/api/proxy/stop`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    });

    const data = await response.json();
    return { success: data.success, error: data.error };
  } catch (error) {
    const message = error instanceof Error ? error.message : '停止失败';
    return { success: false, error: message };
  }
}

/**
 * 测试 Ollama 代理连接
 */
export async function testProxy(): Promise<{
  success: boolean;
  response?: string;
  error?: string;
}> {
  try {
    const response = await fetch(`${apiClient.getBaseUrl()}/api/proxy/test`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    });

    const json = await response.json();
    // 后端返回 ApiResponse<TestResult> 格式: {success, data: {success, message, latency_ms}}
    if (json.success && json.data) {
      return { 
        success: json.data.success, 
        response: json.data.message, 
        error: json.data.success ? undefined : json.data.message 
      };
    }
    return { success: false, error: json.error || '测试失败' };
  } catch (error) {
    const message = error instanceof Error ? error.message : '测试失败';
    return { success: false, error: message };
  }
}

export interface TestStreamMetrics {
  first_token_ms: number;
  total_ms: number;
  token_count: number;
}

export interface TestStreamCallbacks {
  onChunk: (chunk: string) => void;
  onComplete: (metrics: TestStreamMetrics) => void;
  onError: (error: string) => void;
}

export async function testProxyStream(callbacks: TestStreamCallbacks): Promise<void> {
  try {
    const response = await fetch(`${apiClient.getBaseUrl()}/api/proxy/test-stream`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ prompt: '你好' }),
    });

    if (!response.ok) {
      try {
        const json = await response.json();
        if (json.data && json.data.message) {
          callbacks.onError(json.data.message);
        } else {
          callbacks.onError(`HTTP ${response.status}`);
        }
      } catch {
        callbacks.onError(`HTTP ${response.status}`);
      }
      return;
    }

    const json = await response.json();
    if (json.success && json.data) {
      if (json.data.success) {
        callbacks.onChunk(json.data.response || json.data.message);
        callbacks.onComplete({
          first_token_ms: json.data.latency_ms || 0,
          total_ms: json.data.latency_ms || 0,
          token_count: (json.data.response || '').length,
        });
      } else {
        callbacks.onError(json.data.message);
      }
    } else {
      callbacks.onError(json.error || '测试失败');
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : '流式测试失败';
    callbacks.onError(message);
  }
}
