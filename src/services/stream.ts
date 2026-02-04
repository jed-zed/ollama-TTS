/**
 * 流式窗口服务
 * 管理实时 AI 回复显示和 SSE 通信
 */

import { apiClient } from './api';
import type {
  ApiResponse,
  StreamMessage,
  StreamEvent,
  ChatRequest,
} from './types';

// 流式连接状态
export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

/**
 * 获取历史消息
 */
export async function getStreamHistory(): Promise<ApiResponse<StreamMessage[]>> {
  return apiClient.get<StreamMessage[]>('/api/stream/history');
}

/**
 * 清空历史
 */
export async function clearStreamHistory(): Promise<ApiResponse<{ success: boolean }>> {
  return apiClient.post('/api/stream/clear');
}

/**
 * 发送聊天消息并获取流式响应
 */
export async function streamChat(
  request: ChatRequest,
  callbacks: {
    onChunk: (event: StreamEvent) => void;
    onError?: (error: string) => void;
    onComplete?: () => void;
  }
): Promise<void> {
  return apiClient.stream(
    '/api/chat',
    { ...request, stream: true },
    (data) => callbacks.onChunk(data as StreamEvent),
    callbacks.onError,
    callbacks.onComplete
  );
}

/**
 * 流式窗口管理器
 * 处理 SSE 连接和消息缓存
 */
export class StreamManager {
  private eventSource: EventSource | null = null;
  private status: ConnectionStatus = 'disconnected';
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 10;
  private baseUrl: string;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private lastEventTime = 0;

  public onStatusChange?: (status: ConnectionStatus) => void;
  public onMessage?: (event: StreamEvent) => void;
  public onError?: (error: string) => void;

  constructor(baseUrl: string = 'http://localhost:8765') {
    this.baseUrl = baseUrl;
  }

  connect(): void {
    if (this.eventSource) {
      this.disconnect();
    }

    this.setStatus('connecting');

    try {
      this.eventSource = new EventSource(`${this.baseUrl}/api/stream/events`);
      this.lastEventTime = Date.now();

      this.eventSource.onopen = () => {
        this.reconnectAttempts = 0;
        this.setStatus('connected');
        this.startHeartbeatCheck();
      };

      this.eventSource.onmessage = (event) => {
        this.lastEventTime = Date.now();
        try {
          const data = JSON.parse(event.data) as StreamEvent;
          this.onMessage?.(data);
        } catch {
        }
      };

      this.eventSource.onerror = () => {
        this.setStatus('error');
        this.cleanup();
        this.scheduleReconnect();
      };
    } catch (error) {
      this.setStatus('error');
      this.onError?.(error instanceof Error ? error.message : '连接失败');
      this.scheduleReconnect();
    }
  }

  disconnect(): void {
    this.cleanup();
    this.setStatus('disconnected');
  }

  private cleanup(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
  }

  private startHeartbeatCheck(): void {
    this.heartbeatTimer = setInterval(() => {
      const timeSinceLastEvent = Date.now() - this.lastEventTime;
      if (timeSinceLastEvent > 60000) {
        this.setStatus('error');
        this.cleanup();
        this.scheduleReconnect();
      }
    }, 15000);
  }

  getStatus(): ConnectionStatus {
    return this.status;
  }

  private setStatus(status: ConnectionStatus): void {
    this.status = status;
    this.onStatusChange?.(status);
  }

  private scheduleReconnect(): void {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      this.onError?.('达到最大重连次数');
      return;
    }

    const delay = Math.min(1000 * Math.pow(1.5, this.reconnectAttempts), 30000);
    this.reconnectAttempts++;

    this.reconnectTimer = setTimeout(() => {
      this.connect();
    }, delay);
  }

  resetReconnectAttempts(): void {
    this.reconnectAttempts = 0;
  }
}

// 导出单例
export const streamManager = new StreamManager();
