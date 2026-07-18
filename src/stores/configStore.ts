/**
 * 配置状态管理
 * 管理应用配置的加载、更新和持久化
 * 
 * 三级存储：内存缓存 → localStorage → 后端API
 * 多页面同步：BroadcastChannel
 */

import { create } from 'zustand';
import { useShallow } from 'zustand/react/shallow';
import type { Config, GSVCharacter } from '../services/types';
import * as configService from '../services/config';
import { subtitleWindow, isTauri } from '../services/tauri';
import { OLLAMA_DEFAULT_URL, GSV_DEFAULT_TTS_URL, CONFIG_DEBOUNCE_MS } from '@/lib/constants';

let debounceTimer: ReturnType<typeof setTimeout> | null = null;
const pendingUpdates: Map<string, unknown> = new Map();
let pendingUpdatesBase: Config | null = null;
let configMutationSequence = 0;
let configMutationsInFlight = 0;
let configLoadSequence = 0;
let configMutationQueue: Promise<void> = Promise.resolve();
let mutationCycleBase: Config | null = null;
let mutationCycleServerConfig: Config | null = null;
const CONFIG_STORAGE_KEY = 'ollama-tts-config-v2';
const CONFIG_VERSION_KEY = 'ollama-tts-config-version-v2';
const BROADCAST_CHANNEL_NAME = 'ollama-tts-config-sync-v2';

try {
  localStorage.removeItem('ollama-tts-config');
  localStorage.removeItem('ollama-tts-config-version');
} catch (e) {
  console.warn('[Config] Failed to clean legacy config cache:', e);
}

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

const mergePendingUpdates = (config: Config): Config => {
  const merged = { ...config } as Record<string, unknown>;
  pendingUpdates.forEach((value, key) => {
    merged[key] = value;
  });
  return merged as unknown as Config;
};

const enqueueConfigMutation = <T>(operation: () => Promise<T>): Promise<T> => {
  const result = configMutationQueue.then(operation, operation);
  configMutationQueue = result.then(() => undefined, () => undefined);
  return result;
};

interface ConfigState {
  // 配置数据
  config: Config | null;
  isLoading: boolean;
  error: string | null;

  // 操作
  loadConfig: () => Promise<void>;
  applyAuthoritativeConfig: (config: Config) => void;
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
  api_target_confirmation_required: false,
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

  baidu_app_id: '',
  baidu_api_key: '',
  baidu_secret_key: '',

  char_delay: 30,
  subtitle_font_color: '#ffffff',
  subtitle_bg_color: 'rgba(255,255,255,0.1)',
  subtitle_opacity: 0.8,
  subtitle_font_size: 24,
  subtitle_auto_show: true,
  subtitle_auto_hide: true,
  subtitle_auto_hide_delay: 5,
  subtitle_window_x: 100,
  subtitle_window_y: 100,
  subtitle_window_width: 800,
  subtitle_window_height: 200,

  subtitle_text_only: false,
  subtitle_hide_input: false,
  subtitle_hide_controls: false,

  custom_font_colors: {},
  custom_bg_colors: {},

  voice_shortcut_enabled: false,
  voice_shortcut: 'Ctrl+Shift+,',

  speech_engine: 'baidu',

  paste_mode: 'direct',

  lan_access_enabled: false,

  fallback_model_enabled: false,
  fallback_preset_id: '',
  fallback_trigger_threshold: 100,
  fallback_pattern: '^SYSTEM:\\n',

  history_compression: {
    enabled: false,
    save_data_path: '',
    max_messages: 100,
    compress_count: 70,
    check_interval: 10,
    backup_before_compress: true,
    backup_dir: 'compression_backups',
    custom_prompt: '',
  },
};

export const useConfigStore = create<ConfigState>((set, get) => ({
  config: getStoredConfig() || defaultConfig,
  isLoading: false,
  error: null,

  loadConfig: async () => {
    const storedConfig = getStoredConfig();
    const loadId = ++configLoadSequence;
    const mutationSequence = configMutationSequence;

    if (!storedConfig) {
      set({ isLoading: true, error: null });
    }

    // Store 初始化时已经读取过本地缓存；这里保留当前内存态，避免旧缓存
    // 在配置 mutation 或 IPC 确认期间短暂覆盖更晚的状态。
    const response = await configService.getConfig({ forceFresh: true });

    if (
      loadId !== configLoadSequence
      || mutationSequence !== configMutationSequence
    ) {
      if (loadId === configLoadSequence) {
        set({ isLoading: false });
      }
      return;
    }

    if (response.success && response.data) {
      set({ config: response.data, isLoading: false });
      saveConfigToStorage(response.data);
      if (configMutationsInFlight === 0 && pendingUpdates.size === 0) {
        mutationCycleBase = null;
        mutationCycleServerConfig = null;
      }
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

  applyAuthoritativeConfig: (config) => {
    // IPC 返回的是已经持久化的后端快照。推进两个代际，阻止此前的 GET
    // 或 HTTP mutation 响应在稍后覆盖它。
    configMutationSequence += 1;
    configLoadSequence += 1;
    mutationCycleServerConfig = config;
    set({ config, isLoading: false, error: null });
    saveConfigToStorage(config);
    broadcastConfigUpdate(config);

    if (configMutationsInFlight === 0 && pendingUpdates.size === 0) {
      mutationCycleBase = null;
      mutationCycleServerConfig = null;
    }
  },

  updateConfig: async (key, value) => {
    const currentConfig = get().config;
    if (!currentConfig) return;

    const mutationId = ++configMutationSequence;
    if (!mutationCycleBase) {
      mutationCycleBase = currentConfig;
    }

    const newConfig = { ...currentConfig, [key]: value };
    set({ config: newConfig });
    saveConfigToStorage(newConfig);
    broadcastConfigUpdate(newConfig);

    const subtitleKeys = ['subtitle_opacity', 'subtitle_font_color', 'subtitle_bg_color', 'subtitle_font_size', 'char_delay', 'subtitle_text_only', 'subtitle_auto_show', 'subtitle_auto_hide', 'subtitle_auto_hide_delay', 'subtitle_hide_input', 'subtitle_hide_controls'];
    if (subtitleKeys.includes(key as string)) {
      emitConfigToOverlay({ [key]: value });
    }

    pendingUpdates.set(key as string, value);
    if (!pendingUpdatesBase) {
      pendingUpdatesBase = currentConfig;
    }

    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }

    debounceTimer = setTimeout(async () => {
      debounceTimer = null;
      if (pendingUpdates.size === 0) return;

      const updates: Partial<Config> = {};
      pendingUpdates.forEach((v, k) => {
        (updates as Record<string, unknown>)[k] = v;
      });
      pendingUpdates.clear();
      const rollbackConfig = pendingUpdatesBase;
      pendingUpdatesBase = null;

      configMutationsInFlight += 1;
      try {
        const response = await enqueueConfigMutation(
          () => configService.updateConfig(updates),
        );
        if (response.success && response.data) {
          mutationCycleServerConfig = response.data;
        }
        if (mutationId === configMutationSequence) {
          if (response.success && response.data) {
            const nextConfig = mergePendingUpdates(response.data);
            set({ config: nextConfig, error: null });
            saveConfigToStorage(nextConfig);
            broadcastConfigUpdate(nextConfig);
          } else if (rollbackConfig) {
            const nextConfig = mergePendingUpdates(rollbackConfig);
            set({ config: nextConfig, error: response.error });
            saveConfigToStorage(nextConfig);
            broadcastConfigUpdate(nextConfig);
          } else {
            set({ error: response.error });
          }
        }
      } finally {
        configMutationsInFlight -= 1;
        if (configMutationsInFlight === 0 && pendingUpdates.size === 0) {
          const syncSequence = configMutationSequence;
          const syncLoadId = ++configLoadSequence;
          const authoritative = await configService.getConfig({ forceFresh: true });
          if (syncSequence === configMutationSequence && syncLoadId === configLoadSequence) {
            if (authoritative.success && authoritative.data) {
              set({ config: authoritative.data, error: null });
              saveConfigToStorage(authoritative.data);
              broadcastConfigUpdate(authoritative.data);
            } else {
              const fallback = mutationCycleServerConfig ?? mutationCycleBase;
              if (fallback) {
                set({ config: fallback, error: authoritative.error });
                saveConfigToStorage(fallback);
                broadcastConfigUpdate(fallback);
              } else {
                set({ error: authoritative.error });
              }
            }
            mutationCycleBase = null;
            mutationCycleServerConfig = null;
          }
        }
      }
    }, CONFIG_DEBOUNCE_MS);
  },

  updateConfigBatch: async (updates) => {
    const currentConfig = get().config;
    if (!currentConfig) return;

    const updateKeys = Object.keys(updates);
    let rollbackConfig = currentConfig;
    if (pendingUpdatesBase) {
      const rollbackRecord = { ...currentConfig } as Record<string, unknown>;
      let overlapsPendingUpdate = false;
      for (const key of updateKeys) {
        if (pendingUpdates.has(key)) {
          overlapsPendingUpdate = true;
          rollbackRecord[key] = (pendingUpdatesBase as unknown as Record<string, unknown>)[key];
          pendingUpdates.delete(key);
        }
      }
      if (overlapsPendingUpdate) {
        rollbackConfig = rollbackRecord as unknown as Config;
      }
      if (pendingUpdates.size === 0) {
        if (debounceTimer) {
          clearTimeout(debounceTimer);
          debounceTimer = null;
        }
        pendingUpdatesBase = null;
      }
    }

    const mutationId = ++configMutationSequence;
    if (!mutationCycleBase) {
      mutationCycleBase = rollbackConfig;
    }

    const newConfig = { ...currentConfig, ...updates };
    set({ config: newConfig });
    saveConfigToStorage(newConfig);
    broadcastConfigUpdate(newConfig);

    configMutationsInFlight += 1;
    try {
      const response = await enqueueConfigMutation(
        () => configService.updateConfig(updates),
      );
      if (response.success && response.data) {
        mutationCycleServerConfig = response.data;
      }
      if (mutationId === configMutationSequence) {
        if (response.success && response.data) {
          const nextConfig = mergePendingUpdates(response.data);
          set({ config: nextConfig, error: null });
          saveConfigToStorage(nextConfig);
          broadcastConfigUpdate(nextConfig);
        } else {
          const nextConfig = mergePendingUpdates(rollbackConfig);
          set({ config: nextConfig, error: response.error });
          saveConfigToStorage(nextConfig);
          broadcastConfigUpdate(nextConfig);
        }
      }
    } finally {
      configMutationsInFlight -= 1;
      if (configMutationsInFlight === 0 && pendingUpdates.size === 0) {
        const syncSequence = configMutationSequence;
        const syncLoadId = ++configLoadSequence;
        const authoritative = await configService.getConfig({ forceFresh: true });
        if (syncSequence === configMutationSequence && syncLoadId === configLoadSequence) {
          if (authoritative.success && authoritative.data) {
            set({ config: authoritative.data, error: null });
            saveConfigToStorage(authoritative.data);
            broadcastConfigUpdate(authoritative.data);
          } else {
            const fallback = mutationCycleServerConfig ?? mutationCycleBase;
            if (fallback) {
              set({ config: fallback, error: authoritative.error });
              saveConfigToStorage(fallback);
              broadcastConfigUpdate(fallback);
            } else {
              set({ error: authoritative.error });
            }
          }
          mutationCycleBase = null;
          mutationCycleServerConfig = null;
        }
      }
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
    useShallow((state) => ({
      enabled: state.config?.tts_enabled ?? true,
      engine: state.config?.tts_engine ?? 'gpt_sovits',
      autoPlay: state.config?.tts_auto_play ?? true,
      volume: state.config?.tts_volume ?? 0.8,
      playMode: state.config?.tts_play_mode ?? 'dialogue',
    }))
  );

// GSV 相关选择器 - 使用 shallow 比较防止无限循环
export const useGSVConfig = () =>
  useConfigStore(
    useShallow((state) => ({
      apiUrl: state.config?.gsv_api_url ?? '',
      currentCharacter: state.config?.gsv_current_character ?? '',
      characters: state.config?.gsv_characters ?? {},
      topK: state.config?.gsv_top_k ?? 50,
      topP: state.config?.gsv_top_p ?? 0.9,
      temperature: state.config?.gsv_temperature ?? 1.0,
    }))
  );

// 字幕相关选择器 - 使用 shallow 比较防止无限循环
export const useSubtitleConfig = () =>
  useConfigStore(
    useShallow((state) => ({
      charDelay: state.config?.char_delay ?? 30,
      fontColor: state.config?.subtitle_font_color ?? '#ffffff',
      bgColor: state.config?.subtitle_bg_color ?? '#000000',
      opacity: state.config?.subtitle_opacity ?? 0.8,
      fontSize: state.config?.subtitle_font_size ?? 24,
    }))
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
