/**
 * TTS 状态管理
 * 管理语音合成的播放状态
 */

import { create } from 'zustand';
import * as ttsService from '../services/tts';

interface TTSState {
  // 播放状态
  isPlaying: boolean;
  isSynthesizing: boolean;
  queueSize: number;

  // 当前播放信息
  currentText: string | null;
  currentSpeaker: string | null;
  currentEmotion: string | null;

  // 引擎状态
  engineStatus: Record<string, boolean>;

  // 操作
  setPlaying: (playing: boolean) => void;
  setSynthesizing: (synthesizing: boolean) => void;
  setQueueSize: (size: number) => void;
  setCurrentInfo: (text: string | null, speaker?: string, emotion?: string) => void;

  // API 操作
  testEngine: (engine: string, url?: string) => Promise<boolean>;
  stopPlayback: () => Promise<void>;
  stopAll: () => Promise<void>;
  setVolume: (volume: number) => Promise<void>;
  refreshStatus: () => Promise<void>;
}

export const useTTSStore = create<TTSState>((set, get) => ({
  isPlaying: false,
  isSynthesizing: false,
  queueSize: 0,
  currentText: null,
  currentSpeaker: null,
  currentEmotion: null,
  engineStatus: {},

  setPlaying: (playing) => set({ isPlaying: playing }),
  setSynthesizing: (synthesizing) => set({ isSynthesizing: synthesizing }),
  setQueueSize: (size) => set({ queueSize: size }),

  setCurrentInfo: (text, speaker, emotion) =>
    set({
      currentText: text,
      currentSpeaker: speaker ?? null,
      currentEmotion: emotion ?? null,
    }),

  testEngine: async (engine, url) => {
    const response = await ttsService.testTTSEngine(engine, url);
    const available = response.success && response.data?.available === true;

    set((state) => ({
      engineStatus: { ...state.engineStatus, [engine]: available },
    }));

    return available;
  },

  stopPlayback: async () => {
    await ttsService.stopPlayback();
    set({ isPlaying: false, currentText: null });
  },

  stopAll: async () => {
    await ttsService.stopAll();
    set({
      isPlaying: false,
      isSynthesizing: false,
      queueSize: 0,
      currentText: null,
    });
  },

  setVolume: async (volume) => {
    await ttsService.setVolume(volume);
  },

  refreshStatus: async () => {
    const response = await ttsService.getPlaybackStatus();
    if (response.success && response.data) {
      set({
        isPlaying: response.data.is_playing,
        isSynthesizing: response.data.is_synthesizing,
        queueSize: response.data.queue_size,
      });
    }
  },
}));

// 选择器 hooks
export const useTTSPlaying = () => useTTSStore((state) => state.isPlaying);
export const useTTSSynthesizing = () => useTTSStore((state) => state.isSynthesizing);
export const useTTSQueueSize = () => useTTSStore((state) => state.queueSize);
