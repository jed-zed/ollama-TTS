/**
 * 配置状态管理
 * 管理应用配置的加载、更新和持久化
 * 
 * 三级存储：内存缓存 → localStorage → 后端API
 * 多页面同步：BroadcastChannel
 */

import { create } from 'zustand';
import { shallow } from 'zustand/shallow';
import type { Config, GSVCharacter } from '../services/types';
import * as configService from '../services/config';
import { subtitleWindow, isTauri } from '../services/tauri';
import { OLLAMA_DEFAULT_URL, GSV_DEFAULT_TTS_URL, CONFIG_DEBOUNCE_MS } from '@/lib/constants';

let debounceTimer: ReturnType<typeof setTimeout> | null = null;
const pendingUpdates: Map<string, unknown> = new Map();
const CONFIG_STORAGE_KEY = 'ollama-tts-config';
const CONFIG_VERSION_KEY = 'ollama-tts-config-version';
const BROADCAST_CHANNEL_NAME = 'ollama-tts-config-sync';

let configChannel: BroadcastChannel | null = null;

try {
  configChannel = new BroadcastChannel(BROADCAST_CHANNEL_NAME);
} catch (e) {
  console.warn('[Config] BroadcastChannel not supported');
}

const emitConfigToOverlay = (config: Partial<Config>) => {
  if (isTauri()) {
    subtitleWindow.emitConfigUpdate(config as Record<string, unknown>);
  }
};

const broadcastConfigUpdate = (config: Config) => {
  if (configChannel) {
    configChannel.postMessage({ type: 'config-update', config, timestamp: Date.now() });
  }
};

const getStoredConfig = (): Config | null => {
  try {
    const stored = localStorage.getItem(CONFIG_STORAGE_KEY);
    if (stored) {
      return JSON.parse(stored);
    }
  } catch (e) {
    console.warn('[Config] Failed to parse stored config:', e);
  }
  return null;
};

const saveConfigToStorage = (config: Config) => {
  try {
    localStorage.setItem(CONFIG_STORAGE_KEY, JSON.stringify(config));
    localStorage.setItem(CONFIG_VERSION_KEY, Date.now().toString());
  } catch (e) {
    console.warn('[Config] Failed to save config to storage:', e);
  }
};

interface ConfigState {
  // 配置数据
  config: Config | null;
  isLoading: boolean;
  error: string | null;

  // 操作
  loadConfig: () => Promise<void>;
  updateConfig: <K extends keyof Config>(key: K, value: Config[K]) => Promise<void>;
  updateConfigBatch: (updates: Partial<Config>) => Promise<void>;

  // 便捷方法
  setApiUrl: (url: string) => Promise<void>;
  setApiKey: (key: string) => Promise<void>;
  setModelName: (model: string) => Promise<void>;
  setTTSEnabled: (enabled: boolean) => Promise<void>;
  setTTSEngine: (engine: 'gpt_sovits' | 'xunfei') => Promise<void>;
  setCurrentCharacter: (characterId: string) => Promise<void>;
  setVolume: (volume: number) => Promise<void>;
  setCharDelay: (delay: number) => Promise<void>;
}

// 默认配置
const defaultConfig: Config = {
  api_url: OLLAMA_DEFAULT_URL,
  api_key: '',
  model_name: 'qwen2.5:7b',
  current_api_preset: 'local',
  system_prompt_injection: '',

  // 自定义预设配置
  custom_api_url: '',
  custom_api_key: '',
  custom_model_name: '',

  // 按预设独立存储的 API 配置
  preset_configs: {},

  // 用户自定义预设元数据
  user_presets: {},

  tts_enabled: true,
  tts_engine: 'gpt_sovits',
  tts_auto_play: true,
  tts_save_audio: false,
  tts_volume: 0.8,
  tts_play_mode: 'dialogue',

  gsv_api_url: GSV_DEFAULT_TTS_URL,
  gsv_current_character: '',
  gsv_characters: {},
  gsv_use_server_path: false,
  gsv_server_path_prefix: '',
  gsv_top_k: 50,
  gsv_top_p: 0.9,
  gsv_temperature: 1.0,
  gsv_default_male_character: '',
  gsv_default_female_character: '',

  xunfei_app_id: '',
  xunfei_api_key: '',
  xunfei_api_secret: '',
  xunfei_default_voice: 'x4_yezi',

  char_delay: 30,
  subtitle_font_color: '#ffffff',
  subtitle_bg_color: 'rgba(255,255,255,0.1)',
  subtitle_opacity: 0.8,
  subtitle_font_size: 24,

  subtitle_text_only: false,

  custom_font_colors: {},
  custom_bg_colors: {},

  history_compression: {
    save_data_path: '',
    max_messages: 100,
    compress_count: 70,
    check_interval: 10,
    backup_before_compress: true,
    auto_compress: false,
    custom_prompt: '',
  },
};

export const useConfigStore = create<ConfigState>((set, get) => ({
  config: getStoredConfig() || defaultConfig,
  isLoading: false,
  error: null,

  loadConfig: async () => {
    const storedConfig = getStoredConfig();
    
    if (storedConfig) {
      set({ config: storedConfig, isLoading: false });
    } else {
      set({ isLoading: true, error: null });
    }
    
    const response = await configService.getConfig();

    if (response.success && response.data) {
      set({ config: response.data, isLoading: false });
      saveConfigToStorage(response.data);
    } else if (!storedConfig) {
      set({
        config: defaultConfig,
        isLoading: false,
        error: response.error || '加载配置失败，使用默认配置',
      });
    } else {
      set({ isLoading: false });
    }
  },

  updateConfig: async (key, value) => {
    const currentConfig = get().config;
    if (!currentConfig) return;

    const newConfig = { ...currentConfig, [key]: value };
    set({ config: newConfig });
    saveConfigToStorage(newConfig);
    broadcastConfigUpdate(newConfig);

    const subtitleKeys = ['subtitle_opacity', 'subtitle_font_color', 'subtitle_bg_color', 'subtitle_font_size', 'char_delay', 'subtitle_text_only'];
    if (subtitleKeys.includes(key as string)) {
      emitConfigToOverlay({ [key]: value });
    }

    pendingUpdates.set(key as string, value);
    
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
    
    debounceTimer = setTimeout(async () => {
      const updates: Partial<Config> = {};
      pendingUpdates.forEach((v, k) => {
        (updates as Record<string, unknown>)[k] = v;
      });
      pendingUpdates.clear();
      
      const response = await configService.updateConfig(updates);
      if (!response.success) {
        set({ error: response.error });
      }
    }, CONFIG_DEBOUNCE_MS);
  },

  updateConfigBatch: async (updates) => {
    const currentConfig = get().config;
    if (!currentConfig) return;

    const newConfig = { ...currentConfig, ...updates };
    set({ config: newConfig });
    saveConfigToStorage(newConfig);
    broadcastConfigUpdate(newConfig);

    const response = await configService.updateConfig(updates);
    if (!response.success) {
      set({ config: currentConfig, error: response.error });
    }
  },

  // 便捷方法
  setApiUrl: (url) => get().updateConfig('api_url', url),
  setApiKey: (key) => get().updateConfig('api_key', key),
  setModelName: (model) => get().updateConfig('model_name', model),
  setTTSEnabled: (enabled) => get().updateConfig('tts_enabled', enabled),
  setTTSEngine: (engine) => get().updateConfig('tts_engine', engine),
  setCurrentCharacter: (id) => get().updateConfig('gsv_current_character', id),
  setVolume: (volume) => get().updateConfig('tts_volume', volume),
  setCharDelay: (delay) => get().updateConfig('char_delay', delay),
}));

// 选择器 hooks
export const useConfig = () => useConfigStore((state) => state.config);
export const useConfigLoading = () => useConfigStore((state) => state.isLoading);
export const useConfigError = () => useConfigStore((state) => state.error);

// TTS 相关选择器 - 使用 shallow 比较防止无限循环
export const useTTSConfig = () =>
  useConfigStore(
    (state) => ({
      enabled: state.config?.tts_enabled ?? true,
      engine: state.config?.tts_engine ?? 'gpt_sovits',
      autoPlay: state.config?.tts_auto_play ?? true,
      volume: state.config?.tts_volume ?? 0.8,
      playMode: state.config?.tts_play_mode ?? 'dialogue',
    }),
    shallow
  );

// GSV 相关选择器 - 使用 shallow 比较防止无限循环
export const useGSVConfig = () =>
  useConfigStore(
    (state) => ({
      apiUrl: state.config?.gsv_api_url ?? '',
      currentCharacter: state.config?.gsv_current_character ?? '',
      characters: state.config?.gsv_characters ?? {},
      topK: state.config?.gsv_top_k ?? 50,
      topP: state.config?.gsv_top_p ?? 0.9,
      temperature: state.config?.gsv_temperature ?? 1.0,
    }),
    shallow
  );

// 字幕相关选择器 - 使用 shallow 比较防止无限循环
export const useSubtitleConfig = () =>
  useConfigStore(
    (state) => ({
      charDelay: state.config?.char_delay ?? 30,
      fontColor: state.config?.subtitle_font_color ?? '#ffffff',
      bgColor: state.config?.subtitle_bg_color ?? '#000000',
      opacity: state.config?.subtitle_opacity ?? 0.8,
      fontSize: state.config?.subtitle_font_size ?? 24,
    }),
    shallow
  );

// BroadcastChannel 监听 - 多页面配置同步
if (configChannel) {
  configChannel.onmessage = (event) => {
    if (event.data?.type === 'config-update' && event.data?.config) {
      const currentConfig = useConfigStore.getState().config;
      if (currentConfig && JSON.stringify(currentConfig) !== JSON.stringify(event.data.config)) {
        useConfigStore.setState({ config: event.data.config });
      }
    }
  };
}

// beforeunload 保存 - 确保配置不丢失
if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => {
    const config = useConfigStore.getState().config;
    if (config) {
      saveConfigToStorage(config);
    }
    
    if (pendingUpdates.size > 0) {
      const updates: Partial<Config> = {};
      pendingUpdates.forEach((v, k) => {
        (updates as Record<string, unknown>)[k] = v;
      });
      navigator.sendBeacon?.('/api/config', JSON.stringify(updates));
    }
  });
}
