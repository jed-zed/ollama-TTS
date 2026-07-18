import React, { useCallback, useEffect, useState } from 'react';
import { NavLink, useLocation, Outlet } from 'react-router-dom';
import { MessageSquareText, Mic, Settings2, Power, PlugZap, Save, FileText, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useStreamStore } from '@/stores/streamStore';
import { streamManager } from '@/services/stream';
import { subtitleWindow, isTauri } from '@/services/tauri';
import { checkBackendHealth, stopProxy, getProxyStatus } from '@/services/api';
import { getBackendBaseUrl } from '@/lib/backendUrl';

const AppLayout = () => {
  const location = useLocation();
  const setConnectionStatus = useStreamStore((state) => state.setConnectionStatus);
  const handleStreamEvent = useStreamStore((state) => state.handleStreamEvent);
  const hideSubtitle = useStreamStore((state) => state.hideSubtitle);
  const syncContentToOverlay = useStreamStore((state) => state.syncContentToOverlay);

  const [backendConnected, setBackendConnected] = useState(false);
  const [proxyRunning, setProxyRunning] = useState(false);
  const [isStopping, setIsStopping] = useState(false);

  const mainPort = new URL(getBackendBaseUrl()).port || '8765';

  const checkStatus = useCallback(async () => {
    try {
      const [backendResult, proxyResult] = await Promise.all([
        checkBackendHealth(),
        getProxyStatus(),
      ]);
      setBackendConnected(backendResult.connected);

      if (proxyResult.success && proxyResult.data) {
        setProxyRunning(proxyResult.data.running);
      } else {
        setProxyRunning(false);
      }
    } catch (error) {
      console.error('Failed to check status:', error);
      setBackendConnected(false);
      setProxyRunning(false);
    }
  }, []);

  const handleStopService = async () => {
    setIsStopping(true);
    try {
      await stopProxy();
      await checkStatus();
    } catch (error) {
      console.error('Failed to stop proxy:', error);
    } finally {
      setIsStopping(false);
    }
  };

  useEffect(() => {
    if (!isTauri()) return;

    let disposed = false;
    const unsubscribers: Array<() => void> = [];

    const register = async (subscription: Promise<() => void>) => {
      try {
        const unsubscribe = await subscription;
        if (disposed) {
          unsubscribe();
        } else {
          unsubscribers.push(unsubscribe);
        }
      } catch (error) {
        console.warn('[AppLayout] Failed to register subtitle listener:', error);
      }
    };

    void register(subtitleWindow.onUserHide(hideSubtitle));
    void register(subtitleWindow.onContentRequest(syncContentToOverlay));

    return () => {
      disposed = true;
      unsubscribers.forEach((unsubscribe) => unsubscribe());
    };
  }, [hideSubtitle, syncContentToOverlay]);

  useEffect(() => {
    streamManager.onStatusChange = (status) => {
      setConnectionStatus(status);
    };
    streamManager.onMessage = (event) => {
      handleStreamEvent(event);
    };
    streamManager.connect();

    // 初始检查状态
    void checkStatus();
    // 轮询状态
    const timer = setInterval(() => {
      void checkStatus();
    }, 5000);

    return () => {
      streamManager.disconnect();
      clearInterval(timer);
    };
  }, [checkStatus, setConnectionStatus, handleStreamEvent]);

  const navItems = [
    { path: '/', label: '服务连接', icon: PlugZap },
    { path: '/subtitles', label: '字幕视觉', icon: MessageSquareText },
    { path: '/voice', label: '语音配置', icon: Mic },
    { path: '/memory', label: '记忆管理', icon: Save },
    { path: '/logs', label: '运行日志', icon: FileText },
  ];

  return (
    <div className="h-screen w-screen flex overflow-hidden">
      {/* 主容器：填充整个窗口 */}
      <div className="w-full h-full flex bg-white/95 overflow-hidden">

        {/* Sidebar */}
        <div className="w-64 min-w-[256px] bg-white/50 border-r border-border flex flex-col p-6">
          <div className="flex items-center gap-3 mb-8 px-2">
            <div className="w-10 h-10 rounded-xl bg-gradient-to-tr from-pink-500 to-purple-500 flex items-center justify-center shadow-lg transform rotate-3">
              <Settings2 className="text-white w-6 h-6" />
            </div>
            <div>
              <h1 className="font-bold text-lg leading-none tracking-tight">O2O Adapter</h1>
              <p className="text-xs text-muted-foreground mt-1">Ollama to OpenAI</p>
              <p className="text-xs text-muted-foreground mt-1">作者：杰</p>
            </div>
          </div>

          <nav className="flex-1 space-y-2">
            {navItems.map((item) => {
              const Icon = item.icon;
              const isActive = location.pathname === item.path;
              return (
                <NavLink
                  key={item.path}
                  to={item.path}
                  className={`flex items-center gap-3 px-4 py-3 rounded-xl transition-all duration-200 group ${isActive
                    ? 'bg-primary text-primary-foreground shadow-md'
                    : 'text-muted-foreground hover:bg-white hover:text-foreground hover:shadow-sm'
                    }`}
                >
                  <Icon className={`w-5 h-5 ${isActive ? 'text-white' : 'text-muted-foreground group-hover:text-primary transition-colors'}`} />
                  <span className="font-medium">{item.label}</span>
                </NavLink>
              );
            })}
          </nav>

          <div className="mt-auto pt-6">
            <div className="p-4 rounded-xl bg-secondary/30 border border-secondary/50">
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-semibold text-secondary-foreground">系统状态</span>
                <div className={`w-2 h-2 rounded-full ${backendConnected ? 'bg-green-500 animate-pulse' : 'bg-red-500'}`}></div>
              </div>
              <p className="text-xs text-muted-foreground mb-3">
                {backendConnected ? `核心服务已连接 :${mainPort}` : '核心服务未响应'}
              </p>

              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <span className="text-[10px] text-muted-foreground font-medium uppercase tracking-wider">Ollama 代理</span>
                  <span className={`text-[10px] px-1.5 py-0.5 rounded-full ${proxyRunning ? 'bg-green-100 text-green-600' : 'bg-gray-100 text-gray-500'}`}>
                    {proxyRunning ? '运行中' : '已停止'}
                  </span>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  className="w-full h-8 text-xs gap-2 border-red-200 hover:bg-red-50 hover:text-red-500"
                  onClick={handleStopService}
                  disabled={!proxyRunning || isStopping}
                >
                  {isStopping ? <Loader2 className="w-3 h-3 animate-spin" /> : <Power className="w-3 h-3" />}
                  停止代理
                </Button>
              </div>
            </div>
          </div>
        </div>

        {/* Main Content */}
        <div className="flex-1 flex flex-col overflow-hidden bg-slate-50/30 relative">
          {/* Decorative background elements */}
          <div className="absolute top-0 right-0 w-96 h-96 bg-blue-100/10 rounded-full -translate-y-1/2 translate-x-1/2 pointer-events-none"></div>
          <div className="absolute bottom-0 left-0 w-96 h-96 bg-pink-100/10 rounded-full translate-y-1/2 -translate-x-1/2 pointer-events-none"></div>

          <div className="flex-1 overflow-auto p-8 relative z-10">
            <Outlet />
          </div>
        </div>
      </div>
    </div>
  );
};

export default AppLayout;
