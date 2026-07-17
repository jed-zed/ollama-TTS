/**
 * 记忆压缩状态管理
 * 管理压缩操作的全局状态，确保页面切换时状态不丢失
 */

import { create } from 'zustand';
import type { MemoryStatus } from '../services/types';
import * as memoryService from '../services/memory';

interface MemoryState {
  characterStatuses: MemoryStatus[];
  isRefreshing: boolean;
  isCompressing: boolean;
  lastRefreshTime: number | null;
  compressionProgress: string | null;

  refreshStatus: () => Promise<void>;
  compressAll: () => Promise<void>;
  setCharacterStatuses: (statuses: MemoryStatus[]) => void;
}

export const useMemoryStore = create<MemoryState>((set, get) => ({
  characterStatuses: [],
  isRefreshing: false,
  isCompressing: false,
  lastRefreshTime: null,
  compressionProgress: null,

  setCharacterStatuses: (statuses) => set({ characterStatuses: statuses }),

  refreshStatus: async () => {
    if (get().isRefreshing) return;
    
    set({ isRefreshing: true });
    try {
      const response = await memoryService.getMemoryStatus();
      if (response.success && response.data) {
        set({ 
          characterStatuses: response.data,
          lastRefreshTime: Date.now(),
        });
      }
    } finally {
      set({ isRefreshing: false });
    }
  },

  compressAll: async () => {
    if (get().isCompressing) return;
    
    set({ isCompressing: true, compressionProgress: '正在压缩...' });
    try {
      const response = await memoryService.compressAll();
      if (response.success && response.data) {
        const message =
          'message' in response.data && typeof response.data.message === 'string'
            ? response.data.message
            : '压缩完成';
        set({ compressionProgress: message });
      }
      await get().refreshStatus();
    } finally {
      set({ isCompressing: false, compressionProgress: null });
    }
  },
}));

export const useCharacterStatuses = () => useMemoryStore((state) => state.characterStatuses);
export const useIsRefreshing = () => useMemoryStore((state) => state.isRefreshing);
export const useIsCompressing = () => useMemoryStore((state) => state.isCompressing);
export const useCompressionProgress = () => useMemoryStore((state) => state.compressionProgress);
