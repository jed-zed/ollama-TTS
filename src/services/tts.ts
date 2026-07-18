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
  TTSStatusResponse,
  TTSCommandResponse,
  CharacterInfo,
  CharacterListResponse,
  AddCharacterRequest,
  UpdateCharacterRequest,
} from './types';

/**
 * 测试 TTS 引擎连接
 * @param engine 引擎名称
 * @param url 可选的 API URL（用于测试未保存的配置）
 */
export async function testTTSEngine(
  engine: TTSTestRequest['engine'],
  url?: string
): Promise<ApiResponse<TTSTestResponse>> {
  const request: TTSTestRequest = { engine, url };
  return apiClient.post<TTSTestResponse>('/api/tts/test', request);
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
export async function stopPlayback(): Promise<ApiResponse<TTSCommandResponse>> {
  return apiClient.post<TTSCommandResponse>('/api/tts/stop');
}

/**
 * 停止所有播放并清空队列
 */
export async function stopAll(): Promise<ApiResponse<TTSCommandResponse>> {
  return apiClient.post<TTSCommandResponse>('/api/tts/stop-all');
}

/**
 * 设置音量
 */
export async function setVolume(
  volume: number
): Promise<ApiResponse<TTSCommandResponse>> {
  return apiClient.post<TTSCommandResponse>('/api/tts/volume', { volume });
}

/**
 * 获取播放状态
 */
export async function getPlaybackStatus(): Promise<ApiResponse<TTSStatusResponse>> {
  return apiClient.get<TTSStatusResponse>('/api/tts/status');
}

// ========== 角色管理 ==========

/**
 * 获取所有角色
 */
export async function getCharacters(): Promise<ApiResponse<CharacterListResponse>> {
  return apiClient.get<CharacterListResponse>('/api/characters');
}

/**
 * 添加角色
 */
export async function addCharacter(
  character: AddCharacterRequest
): Promise<ApiResponse<CharacterInfo>> {
  return apiClient.post<CharacterInfo>('/api/characters', character);
}

/**
 * 更新角色
 */
export async function updateCharacter(
  id: string,
  character: UpdateCharacterRequest
): Promise<ApiResponse<string>> {
  return apiClient.put<string>(`/api/characters/${encodeURIComponent(id)}`, character);
}

/**
 * 删除角色
 */
export async function deleteCharacter(
  id: string
): Promise<ApiResponse<string>> {
  return apiClient.delete<string>(`/api/characters/${encodeURIComponent(id)}`);
}

/**
 * 切换当前角色
 */
export async function switchCharacter(
  characterId: string
): Promise<ApiResponse<string>> {
  return apiClient.post<string>('/api/characters/switch', { character_id: characterId });
}
