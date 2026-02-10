/**
 * 全局服务管理器
 * 单例模式，管理后端服务连接状态
 * 提供启动进度反馈和状态共享
 */

import { create } from 'zustand';
import { checkBackendHealth, type HealthCheckResponse } from './api';

export type ServiceStatus = 'idle' | 'starting' | 'connected' | 'disconnected' | 'error';

interface ServiceState {
  status: ServiceStatus;
  progress: number;
  progressMessage: string;
  backendInfo: HealthCheckResponse | null;
  error: string | null;
  startTime: number | null;
  
  setStatus: (status: ServiceStatus) => void;
  setProgress: (progress: number, message?: string) => void;
  setBackendInfo: (info: HealthCheckResponse | null) => void;
  setError: (error: string | null) => void;
  
  checkConnection: () => Promise<boolean>;
  waitForConnection: (timeout?: number) => Promise<boolean>;
}

let connectionCheckPromise: Promise<boolean> | null = null;

export const useServiceStore = create<ServiceState>((set, get) => ({
  status: 'idle',
  progress: 0,
  progressMessage: '',
  backendInfo: null,
  error: null,
  startTime: null,

  setStatus: (status) => set({ 
    status,
    startTime: status === 'starting' ? Date.now() : get().startTime,
  }),
  
  setProgress: (progress, message) => set({ 
    progress, 
    progressMessage: message || get().progressMessage,
  }),
  
  setBackendInfo: (info) => set({ backendInfo: info }),
  
  setError: (error) => set({ error, status: error ? 'error' : get().status }),

  checkConnection: async () => {
    if (connectionCheckPromise) {
      return connectionCheckPromise;
    }

    connectionCheckPromise = (async () => {
      try {
        const result = await checkBackendHealth();
        if (result.connected && result.data) {
          set({ 
            status: 'connected', 
            backendInfo: result.data,
            error: null,
            progress: 100,
            progressMessage: '已连接',
          });
          return true;
        } else {
          set({ 
            status: 'disconnected',
            backendInfo: null,
          });
          return false;
        }
      } catch (e) {
        set({ 
          status: 'error',
          error: e instanceof Error ? e.message : '连接检查失败',
        });
        return false;
      } finally {
        connectionCheckPromise = null;
      }
    })();

    return connectionCheckPromise;
  },

  waitForConnection: async (timeout = 5000) => {
    const startTime = Date.now();
    const checkInterval = 200;
    
    set({ status: 'starting', progress: 0, progressMessage: '正在连接后端...' });

    while (Date.now() - startTime < timeout) {
      const elapsed = Date.now() - startTime;
      const progress = Math.min(90, (elapsed / timeout) * 100);
      set({ progress, progressMessage: `正在连接... (${Math.round(elapsed / 1000)}s)` });

      const connected = await get().checkConnection();
      if (connected) {
        set({ progress: 100, progressMessage: '连接成功' });
        return true;
      }

      await new Promise(r => setTimeout(r, checkInterval));
    }

    set({ 
      status: 'disconnected', 
      progress: 0,
      progressMessage: '连接超时',
      error: '后端服务连接超时',
    });
    return false;
  },
}));

export const useServiceStatus = () => useServiceStore((state) => state.status);
export const useServiceProgress = () => useServiceStore((state) => ({ 
  progress: state.progress, 
  message: state.progressMessage,
}));
export const useBackendInfo = () => useServiceStore((state) => state.backendInfo);
export const useServiceError = () => useServiceStore((state) => state.error);
