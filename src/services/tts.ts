/**
 * TTS 服务
 * 管理语音合成相关操作
 */

import { apiClient } from './api';
import type {
  ApiResponse,
  TTSTestRequest,
  TTSTestResponse,
  TTSSynthesizeRequest,
  TTSSynthesizeResponse,
  GSVCharacter,
} from './types';

/**
 * 测试 TTS 引擎连接
 * @param engine 引擎名称
 * @param url 可选的 API URL（用于测试未保存的配置）
 */
export async function testTTSEngine(
  engine: string,
  url?: string
): Promise<ApiResponse<TTSTestResponse>> {
  return apiClient.post<TTSTestResponse>('/api/tts/test', { engine, url });
}

/**
 * 合成语音
 */
export async function synthesize(
  request: TTSSynthesizeRequest
): Promise<ApiResponse<TTSSynthesizeResponse>> {
  return apiClient.post<TTSSynthesizeResponse>('/api/tts/synthesize', request);
}

/**
 * 停止当前播放
 */
export async function stopPlayback(): Promise<ApiResponse<{ success: boolean }>> {
  return apiClient.post('/api/tts/stop');
}

/**
 * 停止所有播放并清空队列
 */
export async function stopAll(): Promise<ApiResponse<{ success: boolean }>> {
  return apiClient.post('/api/tts/stop-all');
}

/**
 * 设置音量
 */
export async function setVolume(
  volume: number
): Promise<ApiResponse<{ success: boolean }>> {
  return apiClient.post('/api/tts/volume', { volume });
}

/**
 * 获取播放状态
 */
export async function getPlaybackStatus(): Promise<
  ApiResponse<{
    is_playing: boolean;
    is_synthesizing: boolean;
    queue_size: number;
  }>
> {
  return apiClient.get('/api/tts/status');
}

// ========== 角色管理 ==========

/**
 * 获取所有角色
 */
export async function getCharacters(): Promise<ApiResponse<GSVCharacter[]>> {
  return apiClient.get<GSVCharacter[]>('/api/characters');
}

/**
 * 添加角色
 */
export async function addCharacter(
  character: GSVCharacter
): Promise<ApiResponse<{ success: boolean }>> {
  return apiClient.post('/api/characters', character);
}

/**
 * 更新角色
 */
export async function updateCharacter(
  id: string,
  character: Partial<GSVCharacter>
): Promise<ApiResponse<{ success: boolean }>> {
  return apiClient.put(`/api/characters/${id}`, character);
}

/**
 * 删除角色
 */
export async function deleteCharacter(
  id: string
): Promise<ApiResponse<{ success: boolean }>> {
  return apiClient.delete(`/api/characters/${id}`);
}

/**
 * 切换当前角色
 */
export async function switchCharacter(
  characterId: string
): Promise<ApiResponse<{ success: boolean }>> {
  return apiClient.post('/api/characters/switch', { character_id: characterId });
}
