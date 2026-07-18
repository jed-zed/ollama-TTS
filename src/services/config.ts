/**
 * 配置服务
 * 管理应用配置的读写，包含超时、重试和本地缓存机制
 */

import { apiClient } from './api';
import type { Config, ApiResponse } from './types';

// 本地存储键
const CONFIG_CACHE_KEY = 'app_config_cache_v2';
const CONFIG_CACHE_TIMESTAMP_KEY = 'app_config_cache_timestamp_v2';
const CACHE_MAX_AGE = 5 * 60 * 1000; // 5分钟缓存有效期

try {
  localStorage.removeItem('app_config_cache');
  localStorage.removeItem('app_config_cache_timestamp');
} catch (e) {
  console.warn('[Config] 清理旧版缓存失败:', e);
}

// 配置请求超时时间（缓存优先，超时可以短一些）
const CONFIG_TIMEOUT = 2000; // 2秒

// 最大重试次数（有缓存兜底，减少重试）
const MAX_RETRIES = 1;

/**
 * 从本地缓存获取配置
 */
function getCachedConfig(): Config | null {
  try {
    const cached = localStorage.getItem(CONFIG_CACHE_KEY);
    const timestamp = localStorage.getItem(CONFIG_CACHE_TIMESTAMP_KEY);
    
    if (cached && timestamp) {
      const age = Date.now() - parseInt(timestamp, 10);
      if (age < CACHE_MAX_AGE) {
        return JSON.parse(cached);
      }
    }
  } catch (e) {
    console.warn('[Config] 读取缓存失败:', e);
  }
  return null;
}

/**
 * 保存配置到本地缓存
 */
function setCachedConfig(config: Config): void {
  try {
    localStorage.setItem(CONFIG_CACHE_KEY, JSON.stringify(config));
    localStorage.setItem(CONFIG_CACHE_TIMESTAMP_KEY, Date.now().toString());
  } catch (e) {
    console.warn('[Config] 保存缓存失败:', e);
  }
}

/**
 * 带超时的请求包装器
 */
async function withTimeout<T>(
  promise: Promise<T>,
  timeout: number
): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error(`请求超时 (${timeout}ms)`));
    }, timeout);

    promise
      .then((result) => {
        clearTimeout(timer);
        resolve(result);
      })
      .catch((error) => {
        clearTimeout(timer);
        reject(error);
      });
  });
}

/**
 * 带重试的请求包装器
 */
async function withRetry<T>(
  fn: () => Promise<T>,
  maxRetries: number
): Promise<T> {
  let lastError: Error | null = null;
  
  for (let i = 0; i <= maxRetries; i++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error as Error;
      if (i < maxRetries) {
        console.warn(`[Config] 请求失败，重试 ${i + 1}/${maxRetries}...`);
        // 指数退避
        await new Promise(r => setTimeout(r, Math.pow(2, i) * 500));
      }
    }
  }
  
  throw lastError;
}

/**
 * 获取完整配置
 * 缓存优先策略：先返回缓存快速启动，后台静默刷新
 */
export async function getConfig(
  options: { forceFresh?: boolean } = {},
): Promise<ApiResponse<Config>> {
  const cached = getCachedConfig();
  
  // 后台获取最新配置的函数
  const fetchLatest = async (): Promise<ApiResponse<Config>> => {
    try {
      const response = await withRetry(
        async () => {
          const result = await withTimeout(
            apiClient.get<Config>('/api/config'),
            CONFIG_TIMEOUT,
          );
          if (!result.success) {
            throw new Error(result.error || '配置加载失败');
          }
          return result;
        },
        MAX_RETRIES
      );
      if (response.success && response.data) {
        setCachedConfig(response.data);
      }
      return response;
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : '配置加载失败',
      };
    }
  };

  // 如果有缓存，立即返回缓存，后台静默刷新
  if (cached && !options.forceFresh) {
    fetchLatest().catch(() => {});
    return { success: true, data: cached };
  }

  // 无缓存时，同步获取配置
  const response = await fetchLatest();
  if (response.success) {
    return response;
  }

  return {
    success: false,
    error: response.error || '配置加载失败，请检查后端服务是否启动',
  };
}

/**
 * 更新配置
 */
export async function updateConfig(
  config: Partial<Config>
): Promise<ApiResponse<Config>> {
  const response = await apiClient.put<Config>('/api/config', config);
  
  // 更新成功后清除缓存，让下次读取从服务器获取最新配置
  if (response.success && response.data) {
    setCachedConfig(response.data);
  }
  
  return response;
}

/**
 * 更新单个配置项
 */
export async function updateConfigField<K extends keyof Config>(
  key: K,
  value: Config[K]
): Promise<ApiResponse<Config>> {
  return updateConfig({ [key]: value } as Partial<Config>);
}

/**
 * 获取 API 预设列表
 */
export async function getApiPresets(): Promise<
  ApiResponse<Array<{ id: string; name: string; api_url: string; model: string }>>
> {
  return apiClient.get('/api/presets');
}

/**
 * 切换 API 预设
 */
export async function switchApiPreset(
  presetId: string
): Promise<ApiResponse<{ success: boolean }>> {
  return apiClient.post('/api/presets/switch', { preset_id: presetId });
}

/**
 * 清除配置缓存
 */
export function clearConfigCache(): void {
  localStorage.removeItem(CONFIG_CACHE_KEY);
  localStorage.removeItem(CONFIG_CACHE_TIMESTAMP_KEY);
}
