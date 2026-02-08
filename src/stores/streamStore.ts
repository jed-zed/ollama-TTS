/**
 * 流式窗口状态管理
 * 管理实时 AI 回复和历史记录
 */

import { create } from 'zustand';
import type { StreamMessage, StreamEvent, ConnectionStatus } from '../services/types';
import * as streamService from '../services/stream';
import { subtitleWindow, isTauri } from '../services/tauri';

// 发送内容更新到独立窗口
const emitContentToOverlay = (text: string, speaker: string | null, isStreaming: boolean) => {
  if (isTauri()) {
    subtitleWindow.emitContentUpdate({
      text,
      speaker: speaker ?? undefined,
      isStreaming,
    });
  }
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
  showSubtitle: () => void;
  hideSubtitle: () => void;

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
  history: [],
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
    emitContentToOverlay(text, null, false);
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

  clearCurrent: () =>
    set({
      currentContent: '',
      currentSpeaker: null,
      currentEmotion: null,
      isStreaming: false,
    }),

  finishStream: () => {
    const state = get();
    if (state.currentContent) {
      const message: StreamMessage = {
        id: `msg_${Date.now()}`,
        content: state.currentContent,
        speaker: state.currentSpeaker ?? undefined,
        emotion: state.currentEmotion ?? undefined,
        timestamp: Date.now(),
      };
      emitContentToOverlay(state.currentContent, state.currentSpeaker, false);
      set((s) => ({
        history: [...s.history, message].slice(-10),
        isStreaming: false,
      }));
    }
  },

  setSubtitleVisible: (visible) => set({ subtitleVisible: visible }),
  showSubtitle: () => {
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

  addToHistory: (message) =>
    set((state) => ({
      history: [...state.history, message].slice(-10),
    })),

  loadHistory: async () => {
    const response = await streamService.getStreamHistory();
    if (response.success && response.data) {
      set({ history: response.data });
    }
  },

  clearHistory: async () => {
    const response = await streamService.clearStreamHistory();
    if (response.success) {
      set({ history: [] });
    }
  },

  setCharDelay: (delay) => set({ charDelay: delay }),
  setAutoScroll: (auto) => set({ autoScroll: auto }),

  handleStreamEvent: (event) => {
    const state = get();

    switch (event.type) {
      case 'stream_start':
        // 新消息开始时：归档当前内容到历史，清空当前内容，自动显示字幕
        if (isTauri()) {
          subtitleWindow.show();
        }
        if (state.currentContent && state.currentContent.length > 10) {
          const message: StreamMessage = {
            id: `msg_${Date.now()}`,
            content: state.currentContent,
            speaker: state.currentSpeaker ?? undefined,
            emotion: state.currentEmotion ?? undefined,
            timestamp: Date.now(),
          };
          set((s) => ({
            history: [...s.history, message].slice(-10),
            currentContent: '',
            currentSpeaker: event.data.speaker ?? null,
            currentEmotion: event.data.emotion ?? null,
            isStreaming: true,
            subtitleVisible: true,
          }));
        } else {
          set({
            currentContent: '',
            currentSpeaker: event.data.speaker ?? null,
            currentEmotion: event.data.emotion ?? null,
            isStreaming: true,
            subtitleVisible: true,
          });
        }
        break;

      case 'content':
        if (event.data.full_content !== undefined) {
          emitContentToOverlay(event.data.full_content, state.currentSpeaker, true);
          set({
            currentContent: event.data.full_content,
            isStreaming: true,
          });
        } else if (event.data.content) {
          set((s) => {
            const newContent = s.currentContent + event.data.content;
            emitContentToOverlay(newContent, s.currentSpeaker, true);
            return {
              currentContent: newContent,
              isStreaming: true,
            };
          });
        }
        if (event.data.speaker || event.data.emotion) {
          state.setCurrentInfo(event.data.speaker, event.data.emotion);
        }
        break;

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
