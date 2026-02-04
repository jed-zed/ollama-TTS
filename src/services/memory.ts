/**
 * 历史压缩服务
 * 管理对话历史的压缩操作
 */

import { apiClient } from './api';
import type {
  ApiResponse,
  MemoryStatus,
  CompressRequest,
  CompressResponse,
  HistoryCompressionConfig,
} from './types';

/**
 * 获取所有角色的压缩状态
 */
export async function getMemoryStatus(): Promise<ApiResponse<MemoryStatus[]>> {
  return apiClient.get<MemoryStatus[]>('/api/memory/status');
}

/**
 * 执行压缩
 * @param characterId 可选，指定角色ID。不传则压缩所有需要压缩的角色
 */
export async function compressHistory(
  characterId?: string
): Promise<ApiResponse<CompressResponse>> {
  const request: CompressRequest = characterId ? { character_id: characterId } : {};
  return apiClient.post<CompressResponse>('/api/memory/compress', request);
}

/**
 * 压缩所有超过阈值的角色
 */
export async function compressAll(): Promise<ApiResponse<CompressResponse>> {
  return apiClient.post<CompressResponse>('/api/memory/compress-all');
}

/**
 * 获取压缩配置
 */
export async function getCompressionConfig(): Promise<
  ApiResponse<HistoryCompressionConfig>
> {
  return apiClient.get<HistoryCompressionConfig>('/api/memory/config');
}

/**
 * 更新压缩配置
 */
export async function updateCompressionConfig(
  config: Partial<HistoryCompressionConfig>
): Promise<ApiResponse<{ success: boolean }>> {
  return apiClient.put('/api/memory/config', config);
}

/**
 * 保存压缩提示词模板
 */
export async function savePromptTemplate(
  template: string
): Promise<ApiResponse<{ success: boolean }>> {
  return apiClient.put('/api/memory/prompt-template', { template });
}

/**
 * 重置提示词模板为默认值
 */
export async function resetPromptTemplate(): Promise<
  ApiResponse<{ template: string }>
> {
  return apiClient.post('/api/memory/prompt-template/reset');
}
