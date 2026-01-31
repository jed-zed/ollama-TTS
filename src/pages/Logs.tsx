import React from 'react';
import { Terminal, Download, Trash2, PauseCircle, PlayCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';

const Logs = () => {
  const dummyLogs = [
    { time: '10:23:45', level: 'INFO', module: 'System', message: 'Application started successfully.' },
    { time: '10:23:46', level: 'INFO', module: 'Ollama', message: 'Connected to local API endpoint.' },
    { time: '10:24:12', level: 'DEBUG', module: 'TTS', message: 'Loading model: paimon.pth' },
    { time: '10:24:14', level: 'SUCCESS', module: 'TTS', message: 'Model loaded in 1.2s' },
    { time: '10:25:30', level: 'WARN', module: 'Network', message: 'Latency spike detected: 150ms' },
    { time: '10:26:01', level: 'INFO', module: 'Stream', message: 'Received chunks: 450 bytes' },
    { time: '10:26:02', level: 'INFO', module: 'Stream', message: 'Subtitle queue update: +1 item' },
    { time: '10:26:05', level: 'INFO', module: 'Audio', message: 'Playback started: dialog_001.wav' },
  ];

  const getBadgeVariant = (level: string) => {
    switch(level) {
      case 'INFO': return 'secondary';
      case 'WARN': return 'outline'; // Outline orange usually
      case 'DEBUG': return 'secondary';
      case 'SUCCESS': return 'default';
      default: return 'secondary';
    }
  };

  const getLevelColor = (level: string) => {
      switch(level) {
          case 'INFO': return 'text-blue-500';
          case 'WARN': return 'text-orange-500';
          case 'DEBUG': return 'text-gray-500';
          case 'SUCCESS': return 'text-green-500';
          default: return 'text-gray-700';
      }
  };

  return (
    <div className="h-full flex flex-col gap-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div className="flex justify-between items-end">
        <div>
          <h2 className="text-3xl font-bold bg-gradient-to-r from-pink-500 to-purple-600 bg-clip-text text-transparent">系统日志</h2>
          <p className="text-muted-foreground mt-2">监控系统运行状态与调试信息。</p>
        </div>
        <div className="flex gap-2">
             <Button size="sm" variant="outline" className="gap-2">
                <PauseCircle className="w-4 h-4" /> 暂停滚动
             </Button>
             <Button size="sm" variant="outline" className="gap-2">
                <Trash2 className="w-4 h-4" /> 清空
             </Button>
             <Button size="sm" variant="outline" className="gap-2">
                <Download className="w-4 h-4" /> 导出
             </Button>
        </div>
      </div>

      <Card className="flex-1 glass border-none shadow-custom overflow-hidden flex flex-col">
        <CardHeader className="pb-2 border-b border-border/50 bg-white/30">
           <div className="flex items-center gap-2">
             <Terminal className="w-4 h-4 text-muted-foreground" />
             <span className="text-sm font-mono text-muted-foreground">console_output.log</span>
           </div>
        </CardHeader>
        <CardContent className="flex-1 overflow-auto p-0 font-mono text-sm">
           <div className="p-4 space-y-1">
             {dummyLogs.map((log, i) => (
                <div key={i} className="flex gap-4 hover:bg-black/5 p-1 rounded px-2 transition-colors">
                   <span className="text-muted-foreground opacity-60 w-20 shrink-0 select-none">{log.time}</span>
                   <span className={`w-16 shrink-0 font-bold ${getLevelColor(log.level)}`}>{log.level}</span>
                   <span className="w-20 shrink-0 text-purple-600 opacity-80">[{log.module}]</span>
                   <span className="text-slate-700">{log.message}</span>
                </div>
             ))}
             <div className="flex gap-4 p-1 px-2 animate-pulse">
                <span className="w-2 opacity-0"></span>
                <span className="text-muted-foreground">_</span>
             </div>
           </div>
        </CardContent>
      </Card>
    </div>
  );
};

export default Logs;