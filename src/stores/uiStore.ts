/**
 * UI 状态管理
 * 管理加载状态、错误提示等全局 UI 状态
 */

import { create } from 'zustand';

interface Toast {
  id: string;
  type: 'success' | 'error' | 'warning' | 'info';
  message: string;
  duration?: number;
}

interface UIState {
  // 全局加载状态
  isLoading: boolean;
  loadingText: string | null;

  // Toast 通知
  toasts: Toast[];

  // 模态框状态
  activeModal: string | null;
  modalData: unknown;

  // 操作
  setLoading: (loading: boolean, text?: string) => void;

  // Toast 操作
  showToast: (type: Toast['type'], message: string, duration?: number) => void;
  hideToast: (id: string) => void;
  showSuccess: (message: string) => void;
  showError: (message: string) => void;
  showWarning: (message: string) => void;

  // 模态框操作
  openModal: (modalId: string, data?: unknown) => void;
  closeModal: () => void;
}

export const useUIStore = create<UIState>((set, get) => ({
  isLoading: false,
  loadingText: null,
  toasts: [],
  activeModal: null,
  modalData: null,

  setLoading: (loading, text) =>
    set({
      isLoading: loading,
      loadingText: text ?? null,
    }),

  showToast: (type, message, duration = 3000) => {
    const id = `toast_${Date.now()}`;
    const toast: Toast = { id, type, message, duration };

    set((state) => ({
      toasts: [...state.toasts, toast],
    }));

    // 自动隐藏
    if (duration > 0) {
      setTimeout(() => {
        get().hideToast(id);
      }, duration);
    }
  },

  hideToast: (id) =>
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    })),

  showSuccess: (message) => get().showToast('success', message),
  showError: (message) => get().showToast('error', message, 5000),
  showWarning: (message) => get().showToast('warning', message),

  openModal: (modalId, data) =>
    set({
      activeModal: modalId,
      modalData: data,
    }),

  closeModal: () =>
    set({
      activeModal: null,
      modalData: null,
    }),
}));

// 选择器 hooks
export const useIsLoading = () => useUIStore((state) => state.isLoading);
export const useLoadingText = () => useUIStore((state) => state.loadingText);
export const useToasts = () => useUIStore((state) => state.toasts);
export const useActiveModal = () => useUIStore((state) => state.activeModal);
