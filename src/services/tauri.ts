/**
 * Tauri 原生 API 封装
 * 提供字幕窗口管理、桌面穿透等原生功能
 */

import { getBackendBaseUrl } from '@/lib/backendUrl';

// 检测是否在 Tauri 环境中运行（同步检测，更可靠）
export const isTauri = (): boolean => {
  if (typeof window === 'undefined') return false;

  const tauriWindow = window as typeof window & {
    __TAURI_INTERNALS__?: { ipc?: unknown };
  };
  
  // 检测 Tauri 2.x 的 __TAURI_INTERNALS__ 或 __TAURI__
  const hasTauriInternals = '__TAURI_INTERNALS__' in window;
  const hasTauriGlobal = '__TAURI__' in window;
  
  // 额外检测：Tauri 会注入 ipc 通信对象
  const hasTauriIpc = !!tauriWindow.__TAURI_INTERNALS__?.ipc;
  
  return hasTauriInternals || hasTauriGlobal || hasTauriIpc;
};

// 动态导入 Tauri API（避免在非 Tauri 环境报错）
let invoke: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null = null;
let listen: ((event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>) | null = null;
let emit: ((event: string, payload?: unknown) => Promise<void>) | null = null;
let emitTo: ((target: string, event: string, payload?: unknown) => Promise<void>) | null = null;

async function initTauriApi() {
  if (isTauri() && !invoke) {
    try {
      const core = await import('@tauri-apps/api/core');
      const event = await import('@tauri-apps/api/event');
      invoke = core.invoke;
      listen = event.listen;
      emit = event.emit;
      emitTo = event.emitTo;
    } catch (e) {
      console.warn('[Tauri] Failed to load Tauri API:', e);
    }
  }
}

// 初始化
initTauriApi();

export interface SubtitleContentUpdate {
  /** 完整文本快照；首次同步、重置或文本回退时发送。 */
  text?: string;
  /** 相对上一条已发送内容的新增文本。 */
  delta?: string;
  /** 收到后应先丢弃字幕窗口中的旧内容。 */
  reset?: boolean;
  /** 本次内容应写入字幕窗口历史记录。 */
  final?: boolean;
  speaker?: string;
  isStreaming: boolean;
}

/**
 * 字幕窗口管理 API
 */
export const subtitleWindow = {
  /**
   * 创建/显示字幕窗口
   */
  async create(): Promise<void> {
    if (!isTauri()) {
      console.warn('[Tauri] Not in Tauri environment, subtitle window not available');
      return;
    }
    await initTauriApi();
    if (invoke) {
      await invoke('create_subtitle_window');
    }
  },

  /**
   * 关闭字幕窗口
   */
  async close(): Promise<void> {
    if (!isTauri() || !invoke) return;
    await invoke('close_subtitle_window');
  },

  /**
   * 显示字幕窗口
   */
  async show(): Promise<void> {
    if (!isTauri() || !invoke) return;
    await invoke('show_subtitle_window');
  },

  /**
   * 隐藏字幕窗口
   */
  async hide(): Promise<void> {
    if (!isTauri() || !invoke) return;
    await invoke('hide_subtitle_window');
  },

  /**
   * 设置鼠标穿透（桌面穿透）
   * @param enabled 是否启用穿透
   */
  async setClickThrough(enabled: boolean): Promise<void> {
    if (!isTauri() || !invoke) return;
    await invoke('set_subtitle_click_through', { enabled });
  },

  /**
   * 获取穿透状态
   */
  async getClickThrough(): Promise<boolean> {
    if (!isTauri() || !invoke) return false;
    return (await invoke('get_subtitle_click_through')) as boolean;
  },

  /**
   * 设置窗口位置
   */
  async setPosition(x: number, y: number): Promise<void> {
    if (!isTauri() || !invoke) return;
    await invoke('set_subtitle_position', { x, y });
  },

  /**
   * 设置窗口大小
   */
  async setSize(width: number, height: number): Promise<void> {
    if (!isTauri() || !invoke) return;
    await invoke('set_subtitle_size', { width, height });
  },

  /**
   * 设置窗口透明度
   */
  async setOpacity(opacity: number): Promise<void> {
    if (!isTauri() || !invoke) return;
    await invoke('set_subtitle_opacity', { opacity });
  },

  /**
   * 监听透明度变化事件
   */
  async onOpacityChange(handler: (opacity: number) => void): Promise<() => void> {
    if (!isTauri()) return () => {};
    await initTauriApi();
    if (listen) {
      return listen('set-opacity', (event) => {
        handler(event.payload as number);
      });
    }
    return () => {};
  },

  /**
   * 发送配置更新事件到字幕窗口
   * 使用 emitTo 直接发送到 subtitle 窗口
   */
  async emitConfigUpdate(config: Record<string, unknown>): Promise<void> {
    if (!isTauri()) return;
    await initTauriApi();
    if (emitTo) {
      await emitTo('subtitle', 'subtitle-config-update', config);
    }
  },

  /**
   * 监听配置更新事件
   */
  async onConfigUpdate(handler: (config: Record<string, unknown>) => void): Promise<() => void> {
    if (!isTauri()) return () => {};
    await initTauriApi();
    if (listen) {
      return listen('subtitle-config-update', (event) => {
        handler(event.payload as Record<string, unknown>);
      });
    }
    return () => {};
  },

  /**
   * 发送字幕内容更新事件
   * 使用 emitTo 直接发送到 subtitle 窗口
   */
  async emitContentUpdate(content: SubtitleContentUpdate): Promise<void> {
    if (!isTauri()) return;
    await initTauriApi();
    if (emitTo) {
      await emitTo('subtitle', 'subtitle-content-update', content);
    }
  },

  /**
   * 监听字幕内容更新事件
   */
  async onContentUpdate(handler: (content: SubtitleContentUpdate) => void): Promise<() => void> {
    if (!isTauri()) return () => {};
    await initTauriApi();
    if (listen) {
      return listen('subtitle-content-update', (event) => {
        handler(event.payload as SubtitleContentUpdate);
      });
    }
    return () => {};
  },

  /**
   * 发送隐藏字幕事件
   * 使用 emitTo 直接发送到 subtitle 窗口
   */
  async emitHide(): Promise<void> {
    if (!isTauri()) return;
    await initTauriApi();
    if (emitTo) {
      await emitTo('subtitle', 'subtitle-hide', {});
    }
    await this.hide();
  },

  /**
   * 从字幕窗口发送用户隐藏事件到主窗口
   * 用于通知主窗口更新 userHiddenSubtitle 状态
   */
  async emitUserHide(): Promise<void> {
    if (!isTauri()) return;
    await initTauriApi();
    if (emitTo) {
      await emitTo('main', 'subtitle-user-hide', {});
    }
    await this.hide();
  },

  /**
   * 监听用户隐藏字幕事件（主窗口监听）
   */
  async onUserHide(handler: () => void): Promise<() => void> {
    if (!isTauri()) return () => {};
    await initTauriApi();
    if (listen) {
      return listen('subtitle-user-hide', () => {
        handler();
      });
    }
    return () => {};
  },

  /**
   * 监听隐藏字幕事件
   */
  async onHide(handler: () => void): Promise<() => void> {
    if (!isTauri()) return () => {};
    await initTauriApi();
    if (listen) {
      return listen('subtitle-hide', () => {
        handler();
      });
    }
    return () => {};
  },

  /**
   * 发送历史记录同步事件到字幕窗口
   */
  async emitHistorySync(history: Array<{ id: string; content: string; speaker?: string; timestamp: number }>): Promise<void> {
    if (!isTauri()) return;
    await initTauriApi();
    if (emitTo) {
      await emitTo('subtitle', 'subtitle-history-sync', history);
    }
  },

  /**
   * 监听历史记录同步事件
   */
  async onHistorySync(handler: (history: Array<{ id: string; content: string; speaker?: string; timestamp: number }>) => void): Promise<() => void> {
    if (!isTauri()) return () => {};
    await initTauriApi();
    if (listen) {
      return listen('subtitle-history-sync', (event) => {
        handler(event.payload as Array<{ id: string; content: string; speaker?: string; timestamp: number }>);
      });
    }
    return () => {};
  },

  /**
   * 请求历史记录同步（从字幕窗口发送到主窗口）
   */
  async requestHistorySync(): Promise<void> {
    if (!isTauri()) return;
    await initTauriApi();
    if (emitTo) {
      await emitTo('main', 'subtitle-request-history', {});
    }
  },

  /**
   * 监听历史记录请求事件（主窗口监听）
   */
  async onHistoryRequest(handler: () => void): Promise<() => void> {
    if (!isTauri()) return () => {};
    await initTauriApi();
    if (listen) {
      return listen('subtitle-request-history', () => {
        handler();
      });
    }
    return () => {};
  },

  /**
   * 请求当前字幕内容快照（从字幕窗口发送到主窗口）。
   * 字幕窗口先注册内容监听器再发送该事件，避免窗口创建期间丢失流片段。
   */
  async requestContentSync(): Promise<void> {
    if (!isTauri()) return;
    await initTauriApi();
    if (emitTo) {
      await emitTo('main', 'subtitle-request-content', {});
    }
  },

  /**
   * 监听字幕内容快照请求（主窗口监听）。
   */
  async onContentRequest(handler: () => void): Promise<() => void> {
    if (!isTauri()) return () => {};
    await initTauriApi();
    if (listen) {
      return listen('subtitle-request-content', () => {
        handler();
      });
    }
    return () => {};
  },
};

/**
 * 后端管理 API
 */
export const backend = {
  /**
   * 启动后端
   */
  async start(): Promise<number> {
    if (!isTauri() || !invoke) return 8765;
    return (await invoke('start_backend')) as number;
  },

  /**
   * 停止后端
   */
  async stop(): Promise<void> {
    if (!isTauri() || !invoke) return;
    await invoke('stop_backend');
  },

  /**
   * 获取后端 URL
   */
  async getUrl(): Promise<string> {
    if (!isTauri() || !invoke) return getBackendBaseUrl();
    return (await invoke('get_backend_url')) as string;
  },
};

export default {
  isTauri,
  subtitleWindow,
  backend,
};
