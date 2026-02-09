/**
 * 前后端通信类型定义
 */

// ========== 基础类型 ==========

export interface ApiResponse<T = unknown> {
  success: boolean;
  data?: T;
  error?: string;
}

// ========== 配置相关 ==========

export interface GSVCharacter {
  id: string;
  name: string;
  ref_audio_dir: string;
  emotion_audio_json?: string;
  gpt_model_path: string;
  sovits_model_path: string;
}

export interface HistoryCompressionConfig {
  save_data_path: string;
  max_messages: number;
  compress_count: number;
  check_interval: number;
  backup_before_compress: boolean;
  backup_dir: string;
  enabled: boolean;
  auto_compress?: boolean;
  custom_prompt: string;
}

export interface PresetApiConfig {
  api_url: string;
  api_key: string;
  model_name: string;
}

export interface UserPresetMeta {
  name: string;
  emoji?: string;
}

export interface Config {
  // API 配置
  api_url: string;
  api_key: string;
  model_name: string;
  current_api_preset: string;
  system_prompt_injection: string;

  // 自定义预设配置（向后兼容）
  custom_api_url?: string;
  custom_api_key?: string;
  custom_model_name?: string;

  // 按预设独立存储的 API 配置
  preset_configs?: Record<string, PresetApiConfig>;

  // 用户自定义预设元数据（名称和图标）
  user_presets?: Record<string, UserPresetMeta>;

  // TTS 配置
  tts_enabled: boolean;
  tts_engine: 'gpt_sovits' | 'xunfei';
  tts_auto_play: boolean;
  tts_save_audio: boolean;
  tts_volume: number;
  tts_play_mode: 'dialogue' | 'fulltext';

  // GPT-SoVITS 配置
  gsv_api_url: string;
  gsv_current_character: string;
  gsv_characters: Record<string, GSVCharacter>;
  gsv_use_server_path: boolean;
  gsv_server_path_prefix: string;
  gsv_top_k: number;
  gsv_top_p: number;
  gsv_temperature: number;
  gsv_default_male_character: string;
  gsv_default_female_character: string;

  // 百度语音配置
  baidu_app_id: string;
  baidu_api_key: string;
  baidu_secret_key: string;

  // 讯飞配置
  xunfei_app_id: string;
  xunfei_api_key: string;
  xunfei_api_secret: string;
  xunfei_default_voice: string;

  // 字幕配置
  char_delay: number;
  subtitle_font_color: string;
  subtitle_bg_color: string;
  subtitle_opacity: number;
  subtitle_font_size: number;

  // 用户自定义颜色预设
  custom_font_colors?: Record<string, string>;
  custom_bg_colors?: Record<string, string>;

  // 历史压缩配置
  history_compression: HistoryCompressionConfig;
}

// ========== TTS 相关 ==========

export interface TTSTestRequest {
  engine: string;
  url?: string;
}

export interface TTSTestResponse {
  available: boolean;
  message?: string;
}

export interface TTSSynthesizeRequest {
  text: string;
  emotion?: string;
  speaker?: string;
  speed?: number;
}

export interface TTSSynthesizeResponse {
  success: boolean;
  error?: string;
  audio_file?: string;
  speaker_used?: string;
  emotion_used?: string;
  segment_count?: number;
}

// ========== 流式窗口相关 ==========

export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

export interface StreamMessage {
  id: string;
  content: string;
  speaker?: string;
  emotion?: string;
  tts_engine?: string;
  tts_duration?: number;
  timestamp: number;
  is_playing?: boolean;
}

export type StreamEventType = 'stream_start' | 'content' | 'stream_end' | 'tts_status' | 'complete' | 'ping' | 'error';

export interface StreamEvent {
  type: StreamEventType;
  data: {
    content?: string;
    full_content?: string;
    speaker?: string;
    emotion?: string;
    is_nsfw?: boolean;
    is_streaming?: boolean;
    chunk_index?: number;
    char_count?: number;
    duration?: number;
    timestamp?: number;
    tts_status?: 'synthesizing' | 'playing' | 'done';
    error?: string;
  };
}

// ========== 历史压缩相关 ==========

export interface MemoryStatus {
  character_id: string;
  character_name: string;
  player_name: string;
  message_count: number;
  summary_count: number;
  needs_compression: boolean;
  last_compressed?: string;
  file_path: string;
}

export interface CompressRequest {
  character_id?: string;
}

export interface CompressResponse {
  success: boolean;
  compressed_count: number;
  summaries?: string[];
}

// ========== 聊天相关 ==========

export interface ChatMessage {
  role: 'user' | 'assistant' | 'system';
  content: string;
}

export interface ChatRequest {
  messages: ChatMessage[];
  model?: string;
  stream?: boolean;
}

export interface ChatResponse {
  message: ChatMessage;
  tts_config?: {
    speaker?: string;
    emotion?: string;
    skip_tts?: boolean;
    segments?: Array<{
      text_marker: string;
      speaker: string;
      emotion: string;
      emotion_description?: string;
      skip_tts?: boolean;
    }>;
  };
}
