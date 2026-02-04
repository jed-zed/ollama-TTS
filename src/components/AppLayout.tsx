import React, { useEffect } from 'react';
import { NavLink, useLocation, Outlet } from 'react-router-dom';
import { LayoutDashboard, MessageSquareText, Mic, ScrollText, Settings2, Power, PlugZap, Save, FileText } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { useConfigStore } from '@/stores/configStore';
import { useStreamStore } from '@/stores/streamStore';
import { streamManager } from '@/services/stream';

const AppLayout = () => {
  const location = useLocation();
  const loadConfig = useConfigStore((state) => state.loadConfig);
  const config = useConfigStore((state) => state.config);
  const setConnectionStatus = useStreamStore((state) => state.setConnectionStatus);
  const handleStreamEvent = useStreamStore((state) => state.handleStreamEvent);

  // 初始化：加载配置和建立 SSE 连接
  useEffect(() => {
    // 加载配置
    if (!config) {
      loadConfig();
    }

    // 建立 SSE 连接
    streamManager.onStatusChange = (status) => {
      setConnectionStatus(status);
    };
    streamManager.onMessage = (event) => {
      handleStreamEvent(event);
    };
    streamManager.connect();

    return () => {
      streamManager.disconnect();
    };
  }, [config, loadConfig, setConnectionStatus, handleStreamEvent]);

  const navItems = [
    { path: '/', label: '服务连接', icon: PlugZap },
    { path: '/subtitles', label: '字幕视觉', icon: MessageSquareText },
    { path: '/voice', label: '语音配置', icon: Mic },
    { path: '/memory', label: '记忆管理', icon: Save },
    { path: '/logs', label: '运行日志', icon: FileText },
  ];

  return (
    <div className="min-h-screen w-full flex justify-center py-8">
      {/* Constraints text from user: Desktop interface width fixed at 1440px */}
      <div className="w-[1440px] h-[900px] flex bg-white/80 backdrop-blur-xl rounded-3xl shadow-2xl overflow-hidden border border-white/50">
        
        {/* Sidebar */}
        <div className="w-64 bg-white/50 border-r border-border flex flex-col p-6">
          <div className="flex items-center gap-3 mb-8 px-2">
            <div className="w-10 h-10 rounded-xl bg-gradient-to-tr from-pink-500 to-purple-500 flex items-center justify-center shadow-lg transform rotate-3">
              <Settings2 className="text-white w-6 h-6" />
            </div>
            <div>
              <h1 className="font-bold text-lg leading-none tracking-tight">O2O Adapter</h1>
              <p className="text-xs text-muted-foreground mt-1">Ollama to OpenAI</p>
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
                  className={`flex items-center gap-3 px-4 py-3 rounded-xl transition-all duration-200 group ${
                    isActive
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
                   <span className="text-xs font-semibold text-secondary-foreground">服务状态</span>
                   <div className="w-2 h-2 rounded-full bg-green-500 animate-pulse"></div>
                </div>
                <p className="text-xs text-muted-foreground mb-3">服务正在运行端口: 8080</p>
                <Button size="sm" variant="outline" className="w-full h-8 text-xs gap-2 border-red-200 hover:bg-red-50 hover:text-red-500">
                  <Power className="w-3 h-3" /> 停止服务
                </Button>
             </div>
          </div>
        </div>

        {/* Main Content */}
        <div className="flex-1 flex flex-col overflow-hidden bg-slate-50/30 relative">
           {/* Decorative background elements */}
           <div className="absolute top-0 right-0 w-96 h-96 bg-blue-200/20 rounded-full blur-3xl -translate-y-1/2 translate-x-1/2 pointer-events-none"></div>
           <div className="absolute bottom-0 left-0 w-96 h-96 bg-pink-200/20 rounded-full blur-3xl translate-y-1/2 -translate-x-1/2 pointer-events-none"></div>
           
           <div className="flex-1 overflow-auto p-8 relative z-10">
              <Outlet />
           </div>
        </div>
      </div>
    </div>
  );
};

export default AppLayout;