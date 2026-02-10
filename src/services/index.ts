/**
 * 服务层统一导出
 */

// API 客户端
export { apiClient, ApiClient } from './api';

// 配置服务
export {
  getConfig,
  updateConfig,
  updateConfigField,
  getApiPresets,
  switchApiPreset,
} from './config';

// TTS 服务
export {
  testTTSEngine,
  synthesize,
  stopPlayback,
  stopAll,
  setVolume,
  getPlaybackStatus,
  getCharacters,
  addCharacter,
  updateCharacter,
  deleteCharacter,
  switchCharacter,
} from './tts';

// 历史压缩服务
export {
  getMemoryStatus,
  compressHistory,
  compressAll,
  getCompressionConfig,
  updateCompressionConfig,
  savePromptTemplate,
  resetPromptTemplate,
} from './memory';

// 流式窗口服务
export {
  getStreamHistory,
  clearStreamHistory,
  streamChat,
  StreamManager,
  streamManager,
} from './stream';

// 类型导出
export type {
  ApiResponse,
  Config,
  GSVCharacter,
  HistoryCompressionConfig,
  TTSTestRequest,
  TTSTestResponse,
  TTSSynthesizeRequest,
  TTSSynthesizeResponse,
  StreamMessage,
  StreamEvent,
  StreamEventType,
  MemoryStatus,
  CompressRequest,
  CompressResponse,
  ChatMessage,
  ChatRequest,
  ChatResponse,
} from './types';

export type { ConnectionStatus } from './stream';
