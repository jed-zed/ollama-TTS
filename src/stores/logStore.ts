import { create } from 'zustand';
import { apiClient } from '@/services/api';

export type LogLevel = 'DEBUG' | 'INFO' | 'WARN' | 'ERROR' | 'SUCCESS';

export interface LogEntry {
  id: string;
  timestamp: string;
  level: LogLevel;
  module: string;
  message: string;
  details?: Record<string, unknown>;
}

export interface LogFilter {
  level?: LogLevel;
  module?: string;
  search?: string;
}

export interface LogStats {
  total: number;
  by_level: Record<LogLevel, number>;
  subscribers: number;
}

interface LogState {
  logs: LogEntry[];
  stats: LogStats | null;
  filter: LogFilter;
  isLoading: boolean;
  isStreaming: boolean;
  isPaused: boolean;
  error: string | null;
  abortController: AbortController | null;

  fetchLogs: () => Promise<void>;
  fetchStats: () => Promise<void>;
  clearLogs: () => Promise<void>;
  exportLogs: (format: 'json' | 'csv' | 'text') => Promise<void>;
  setFilter: (filter: Partial<LogFilter>) => void;
  startStreaming: () => void;
  stopStreaming: () => void;
  togglePause: () => void;
  addLog: (log: LogEntry) => void;
  testLogs: () => Promise<void>;
}

export const useLogStore = create<LogState>((set, get) => ({
  logs: [],
  stats: null,
  filter: {},
  isLoading: false,
  isStreaming: false,
  isPaused: false,
  error: null,
  abortController: null,

  fetchLogs: async () => {
    set({ isLoading: true, error: null });
    try {
      const { filter } = get();
      const params = new URLSearchParams();
      if (filter.level) params.set('level', filter.level);
      if (filter.module) params.set('module', filter.module);
      if (filter.search) params.set('search', filter.search);
      params.set('limit', '200');

      const queryString = params.toString();
      const endpoint = `/api/logs${queryString ? `?${queryString}` : ''}`;
      
      const response = await apiClient.get<LogEntry[]>(endpoint);
      
      if (response.success && response.data) {
        set({ logs: response.data, isLoading: false });
      } else {
        set({ logs: [], error: response.error || '获取日志失败', isLoading: false });
      }
    } catch (error) {
      set({ logs: [], error: error instanceof Error ? error.message : '获取日志失败', isLoading: false });
    }
  },

  fetchStats: async () => {
    try {
      const response = await apiClient.get<LogStats>('/api/logs/stats');
      if (response.success && response.data) {
        set({ stats: response.data });
      }
    } catch {
    }
  },

  clearLogs: async () => {
    try {
      const response = await apiClient.delete('/api/logs');
      if (response.success) {
        set({ logs: [] });
        await get().fetchStats();
      }
    } catch (error) {
      set({ error: error instanceof Error ? error.message : '清空日志失败' });
    }
  },

  exportLogs: async (format: 'json' | 'csv' | 'text') => {
    try {
      const baseUrl = apiClient.getBaseUrl();
      const url = `${baseUrl}/api/logs/export?format=${format}`;
      
      const response = await fetch(url);
      const blob = await response.blob();
      
      const extensions: Record<string, string> = { json: 'json', csv: 'csv', text: 'txt' };
      const filename = `logs.${extensions[format]}`;
      
      const downloadUrl = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = downloadUrl;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(downloadUrl);
    } catch (error) {
      set({ error: error instanceof Error ? error.message : '导出日志失败' });
    }
  },

  setFilter: (filter: Partial<LogFilter>) => {
    set((state) => ({ filter: { ...state.filter, ...filter } }));
    get().fetchLogs();
  },

  startStreaming: () => {
    const { isStreaming, abortController: existingController } = get();
    if (isStreaming) return;

    if (existingController) {
      existingController.abort();
    }

    const controller = new AbortController();
    set({ isStreaming: true, abortController: controller });

    const baseUrl = apiClient.getBaseUrl();
    const eventSource = new EventSource(`${baseUrl}/api/logs/stream`);

    eventSource.onmessage = (event) => {
      const { isPaused } = get();
      if (isPaused) return;

      try {
        const log: LogEntry = JSON.parse(event.data);
        set((state) => ({
          logs: [log, ...state.logs].slice(0, 500),
        }));
      } catch {
      }
    };

    eventSource.onerror = () => {
      eventSource.close();
      set({ isStreaming: false, abortController: null });
    };

    controller.signal.addEventListener('abort', () => {
      eventSource.close();
    });
  },

  stopStreaming: () => {
    const { abortController } = get();
    if (abortController) {
      abortController.abort();
    }
    set({ isStreaming: false, abortController: null });
  },

  togglePause: () => {
    set((state) => ({ isPaused: !state.isPaused }));
  },

  addLog: (log: LogEntry) => {
    set((state) => ({
      logs: [log, ...state.logs].slice(0, 500),
    }));
  },

  testLogs: async () => {
    try {
      await apiClient.post('/api/logs/test');
    } catch (error) {
      set({ error: error instanceof Error ? error.message : '添加测试日志失败' });
    }
  },
}));
