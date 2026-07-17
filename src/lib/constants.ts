/**
 * 全局常量配置文件
 * 包含所有前端硬编码的 URL、超时时间以及默认配置项
 */

export const API_BASE_URL = 'http://127.0.0.1:8765';
export const OLLAMA_DEFAULT_URL = 'http://127.0.0.1:11434';
export const GSV_DEFAULT_TTS_URL = 'http://127.0.0.1:9880/tts';

export const MAX_HISTORY_SIZE = 20;
export const MIN_CONTENT_LENGTH = 10;
export const CONFIG_DEBOUNCE_MS = 300;
export const HEALTH_CHECK_INTERVAL_MS = 30000;
export const HEALTH_CHECK_TIMEOUT_MS = 2000;
export const SCROLL_THRESHOLD = 50;
