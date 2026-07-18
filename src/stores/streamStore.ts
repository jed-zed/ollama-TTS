/**
 * 流式窗口状态管理
 * 管理实时 AI 回复和历史记录
 */

import { create } from 'zustand';
import type { StreamMessage, StreamEvent, ConnectionStatus } from '../services/types';
import * as streamService from '../services/stream';
import { subtitleWindow, isTauri } from '../services/tauri';
import { subtitleStreamBridge } from '../services/subtitleStreamBridge';
import { MAX_HISTORY_SIZE, MIN_CONTENT_LENGTH } from '@/lib/constants';
import { useConfigStore } from './configStore';

const HISTORY_STORAGE_KEY = 'stream-history';

const createMessageId = () => `msg_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;

const normalizeHistory = (history: StreamMessage[]): StreamMessage[] => {
  const seenIds = new Set<string>();
  const dedupedDesc: StreamMessage[] = [];

  for (let index = history.length - 1; index >= 0; index--) {
    const item = history[index];
    if (!item || typeof item !== 'object') continue;

    const normalizedId = typeof item.id === 'string' && item.id.trim().length > 0
      ? item.id
      : createMessageId();

    if (seenIds.has(normalizedId)) continue;
    seenIds.add(normalizedId);

    dedupedDesc.push({
      ...item,
      id: normalizedId,
    });
  }

  return dedupedDesc.reverse().slice(-MAX_HISTORY_SIZE);
};

const loadHistoryFromStorage = (): StreamMessage[] => {
  try {
    const stored = localStorage.getItem(HISTORY_STORAGE_KEY);
    if (!stored) return [];
    const parsed = JSON.parse(stored);
    return Array.isArray(parsed) ? normalizeHistory(parsed as StreamMessage[]) : [];
  } catch {
    return [];
  }
};

const saveHistoryToStorage = (history: StreamMessage[]) => {
  try {
    localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(normalizeHistory(history)));
  } catch {
    return;
  }
};

interface OverlayEmitOptions {
  forceSnapshot?: boolean;
  final?: boolean;
  boundary?: boolean;
}

// 内容在主窗口保留全量状态，桥接层负责按帧合并并向独立窗口发送增量。
const emitContentToOverlay = (
  text: string,
  speaker: string | null,
  isStreaming: boolean,
  options?: OverlayEmitOptions,
) => {
  subtitleStreamBridge.publish(text, speaker, isStreaming, options);
};

interface StreamState {
  // 连接状态
  connectionStatus: ConnectionStatus;

  // 当前流式内容
  currentContent: string;
  currentSpeaker: string | null;
  currentEmotion: string | null;
  isStreaming: boolean;

  // 字幕窗口可见性
  subtitleVisible: boolean;

  // 历史记录
  history: StreamMessage[];

  // 显示设置
  charDelay: number;
  autoScroll: boolean;

  // 操作
  setConnectionStatus: (status: ConnectionStatus) => void;
  appendContent: (text: string) => void;
  setContent: (text: string) => void;
  setCurrentInfo: (speaker?: string, emotion?: string) => void;
  clearCurrent: () => void;
  finishStream: () => void;

  // 字幕可见性
  setSubtitleVisible: (visible: boolean) => void;
  showSubtitle: (force?: boolean) => void;
  hideSubtitle: () => void;
  syncContentToOverlay: () => void;

  // 历史操作
  addToHistory: (message: StreamMessage) => void;
  loadHistory: () => Promise<void>;
  clearHistory: () => Promise<void>;

  // 设置
  setCharDelay: (delay: number) => void;
  setAutoScroll: (auto: boolean) => void;

  // SSE 事件处理
  handleStreamEvent: (event: StreamEvent) => void;
}

export const useStreamStore = create<StreamState>((set, get) => ({
  connectionStatus: 'disconnected',
  currentContent: '',
  currentSpeaker: null,
  currentEmotion: null,
  isStreaming: false,
  subtitleVisible: false,
  history: loadHistoryFromStorage(),
  charDelay: 30,
  autoScroll: true,

  setConnectionStatus: (status) => set({ connectionStatus: status }),

  appendContent: (text) =>
    set((state) => {
      const newContent = state.currentContent + text;
      emitContentToOverlay(newContent, state.currentSpeaker, true);
      return {
        currentContent: newContent,
        isStreaming: true,
      };
    }),

  setContent: (text) => {
    emitContentToOverlay(text, null, false, {
      forceSnapshot: true,
      final: Boolean(text),
      boundary: true,
    });
    set({
      currentContent: text,
      isStreaming: false,
    });
  },

  setCurrentInfo: (speaker, emotion) =>
    set({
      currentSpeaker: speaker ?? null,
      currentEmotion: emotion ?? null,
    }),

  clearCurrent: () => {
    emitContentToOverlay('', null, false, {
      forceSnapshot: true,
      boundary: true,
    });
    set({
      currentContent: '',
      currentSpeaker: null,
      currentEmotion: null,
      isStreaming: false,
    });
  },

  finishStream: () => {
    const state = get();
    if (!state.isStreaming) {
      return;
    }

    const shouldArchive = state.currentContent.length > MIN_CONTENT_LENGTH;
    emitContentToOverlay(state.currentContent, state.currentSpeaker, false, {
      final: shouldArchive,
    });

    if (shouldArchive) {
      const message: StreamMessage = {
        id: createMessageId(),
        content: state.currentContent,
        speaker: state.currentSpeaker ?? undefined,
        emotion: state.currentEmotion ?? undefined,
        timestamp: Date.now(),
      };
      set((s) => {
        const newHistory = [...s.history, message].slice(-MAX_HISTORY_SIZE);
        saveHistoryToStorage(newHistory);
        return {
          history: newHistory,
          isStreaming: false,
        };
      });
    } else {
      set({ isStreaming: false });
    }
  },

  setSubtitleVisible: (visible) => set({ subtitleVisible: visible }),
  showSubtitle: (force = false) => {
    const state = get();
    if (state.subtitleVisible) {
      return;
    }
    const shouldAutoShow = useConfigStore.getState().config?.subtitle_auto_show ?? true;
    if (!force && !shouldAutoShow) {
      return;
    }
    if (isTauri()) {
      subtitleWindow.show();
    }
    set({ subtitleVisible: true });
  },
  hideSubtitle: () => {
    if (isTauri()) {
      subtitleWindow.hide();
    }
    set({ subtitleVisible: false });
  },
  syncContentToOverlay: () => {
    const state = get();
    subtitleStreamBridge.sync(
      state.currentContent,
      state.currentSpeaker,
      state.isStreaming,
    );
  },

  addToHistory: (message) =>
    set((state) => {
      const newHistory = [...state.history, message].slice(-MAX_HISTORY_SIZE);
      saveHistoryToStorage(newHistory);
      return { history: newHistory };
    }),

  loadHistory: async () => {
    const localHistory = loadHistoryFromStorage();
    if (localHistory.length > 0) {
      set({ history: localHistory });
      return;
    }
    const response = await streamService.getStreamHistory();
    if (response.success && response.data) {
      set({ history: response.data });
      saveHistoryToStorage(response.data);
    }
  },

  clearHistory: async () => {
    const response = await streamService.clearStreamHistory();
    if (response.success) {
      set({ history: [] });
      saveHistoryToStorage([]);
    }
  },

  setCharDelay: (delay) => set({ charDelay: delay }),
  setAutoScroll: (auto) => set({ autoScroll: auto }),

  handleStreamEvent: (event) => {
    const state = get();

    switch (event.type) {
      case 'stream_start': {
        // 新消息开始时：归档当前内容到历史，清空当前内容，根据配置决定是否自动显示字幕
        const shouldAutoShow = useConfigStore.getState().config?.subtitle_auto_show ?? true;
        if (shouldAutoShow && !state.subtitleVisible) {
          state.showSubtitle();
        }
        const newSpeaker = event.data.speaker ?? null;
        if (state.isStreaming && state.currentContent && state.currentContent.length > MIN_CONTENT_LENGTH) {
          const message: StreamMessage = {
            id: createMessageId(),
            content: state.currentContent,
            speaker: state.currentSpeaker ?? undefined,
            emotion: state.currentEmotion ?? undefined,
            timestamp: Date.now(),
          };
          set((s) => {
            const newHistory = [...s.history, message].slice(-MAX_HISTORY_SIZE);
            saveHistoryToStorage(newHistory);
            return {
              history: newHistory,
              currentContent: '',
              currentSpeaker: newSpeaker,
              currentEmotion: event.data.emotion ?? null,
              isStreaming: true,
              subtitleVisible: shouldAutoShow ? true : s.subtitleVisible,
            };
          });
        } else {
          set({
            currentContent: '',
            currentSpeaker: newSpeaker,
            currentEmotion: event.data.emotion ?? null,
            isStreaming: true,
            subtitleVisible: shouldAutoShow ? true : state.subtitleVisible,
          });
        }
        emitContentToOverlay('', newSpeaker, true, {
          forceSnapshot: true,
          boundary: true,
        });
        break;
      }

      case 'content': {
        const shouldAutoShow = useConfigStore.getState().config?.subtitle_auto_show ?? true;
        if (shouldAutoShow && !state.subtitleVisible) {
          state.showSubtitle();
        }
        if (event.data.full_content !== undefined) {
          const snapshotStreaming = event.data.is_streaming ?? true;
          const nextSpeaker = event.data.speaker ?? state.currentSpeaker;
          emitContentToOverlay(event.data.full_content, nextSpeaker, snapshotStreaming, {
            forceSnapshot: true,
            final: !snapshotStreaming && event.data.full_content.length > MIN_CONTENT_LENGTH,
          });
          set({
            currentContent: event.data.full_content,
            isStreaming: snapshotStreaming,
            subtitleVisible: snapshotStreaming && shouldAutoShow ? true : state.subtitleVisible,
          });
        } else if (event.data.content) {
          set((s) => {
            const newContent = s.currentContent + event.data.content;
            emitContentToOverlay(newContent, s.currentSpeaker, true);
            return {
              currentContent: newContent,
              isStreaming: true,
              subtitleVisible: shouldAutoShow ? true : s.subtitleVisible,
            };
          });
        }
        if (event.data.speaker || event.data.emotion) {
          state.setCurrentInfo(event.data.speaker, event.data.emotion);
        }
        break;
      }

      case 'stream_end':
        state.finishStream();
        break;

      case 'tts_status':
        break;

      case 'complete':
        state.finishStream();
        break;

      case 'ping':
        break;

      case 'error':
        console.error('Stream error:', event.data.error);
        set({ isStreaming: false });
        break;
    }
  },
}));

// 选择器 hooks
export const useStreamContent = () => useStreamStore((state) => state.currentContent);
export const useStreamHistory = () => useStreamStore((state) => state.history);
export const useIsStreaming = () => useStreamStore((state) => state.isStreaming);
export const useConnectionStatus = () => useStreamStore((state) => state.connectionStatus);
export const useSubtitleVisible = () => useStreamStore((state) => state.subtitleVisible);
