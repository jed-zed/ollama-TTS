import { useEffect, useRef, useState, useCallback } from 'react';
import { Terminal, Download, Trash2, PauseCircle, PlayCircle, RefreshCw, Radio, Search, Filter, TestTube, ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { useLogStore, type LogLevel } from '@/stores/logStore';

const Logs = () => {
  const {
    logs,
    stats,
    filter,
    pagination,
    isLoading,
    isStreaming,
    isPaused,
    error,
    fetchLogs,
    fetchStats,
    clearLogs,
    exportLogs,
    setFilter,
    setPage,
    setPageSize,
    startStreaming,
    stopStreaming,
    togglePause,
    testLogs,
  } = useLogStore();

  const logsContainerRef = useRef<HTMLDivElement>(null);
  const [isAtTop, setIsAtTop] = useState(true);

  // 检测用户是否在顶部（日志是倒序显示，最新的在顶部）
  const handleScroll = useCallback(() => {
    if (logsContainerRef.current) {
      // 如果 scrollTop 接近 0，认为用户在顶部
      setIsAtTop(logsContainerRef.current.scrollTop < 50);
    }
  }, []);

  useEffect(() => {
    fetchLogs();
    fetchStats();
    startStreaming();

    return () => {
      stopStreaming();
    };
  }, []);

  // 只有当用户在顶部且未暂停时才自动滚动到最新
  useEffect(() => {
    if (!isPaused && isAtTop && logsContainerRef.current) {
      logsContainerRef.current.scrollTop = 0;
    }
  }, [logs, isPaused, isAtTop]);

  const getLevelColor = (level: LogLevel) => {
    switch (level) {
      case 'INFO': return 'text-blue-500';
      case 'WARN': return 'text-orange-500';
      case 'DEBUG': return 'text-gray-500';
      case 'SUCCESS': return 'text-green-500';
      case 'ERROR': return 'text-red-500';
      default: return 'text-gray-700';
    }
  };

  const getLevelBgColor = (level: LogLevel) => {
    switch (level) {
      case 'INFO': return 'bg-blue-500/10';
      case 'WARN': return 'bg-orange-500/10';
      case 'DEBUG': return 'bg-gray-500/10';
      case 'SUCCESS': return 'bg-green-500/10';
      case 'ERROR': return 'bg-red-500/10';
      default: return 'bg-gray-500/10';
    }
  };

  const formatTime = (timestamp: string) => {
    try {
      const date = new Date(timestamp);
      return date.toLocaleTimeString('zh-CN', {
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
      });
    } catch {
      return timestamp;
    }
  };

  const handleExport = async (format: 'json' | 'csv' | 'text') => {
    await exportLogs(format);
  };

  return (
    <div className="h-full flex flex-col gap-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div className="flex justify-between items-end">
        <div>
          <h2 className="text-3xl font-bold bg-gradient-to-r from-pink-500 to-purple-600 bg-clip-text text-transparent">系统日志</h2>
          <p className="text-muted-foreground mt-2">监控系统运行状态与调试信息。</p>
        </div>
        <div className="flex gap-2">
          <Button
            size="sm"
            variant={isStreaming ? "default" : "outline"}
            className="gap-2"
            onClick={isStreaming ? stopStreaming : startStreaming}
          >
            <Radio className={`w-4 h-4 ${isStreaming ? 'animate-pulse' : ''}`} />
            {isStreaming ? '实时' : '离线'}
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="gap-2"
            onClick={togglePause}
            disabled={!isStreaming}
          >
            {isPaused ? <PlayCircle className="w-4 h-4" /> : <PauseCircle className="w-4 h-4" />}
            {isPaused ? '继续' : '暂停'}
          </Button>
          <Button size="sm" variant="outline" className="gap-2" onClick={() => fetchLogs()}>
            <RefreshCw className={`w-4 h-4 ${isLoading ? 'animate-spin' : ''}`} />
            刷新
          </Button>
          <Button size="sm" variant="outline" className="gap-2" onClick={() => clearLogs()}>
            <Trash2 className="w-4 h-4" /> 清空
          </Button>
          <Select onValueChange={(v) => handleExport(v as 'json' | 'csv' | 'text')}>
            <SelectTrigger className="w-[100px] h-8">
              <Download className="w-4 h-4 mr-1" />
              导出
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="json">JSON</SelectItem>
              <SelectItem value="csv">CSV</SelectItem>
              <SelectItem value="text">TXT</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      <div className="flex gap-4 items-center">
        <div className="relative flex-1 max-w-xs">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <Input
            placeholder="搜索日志内容..."
            className="pl-9"
            value={filter.search || ''}
            onChange={(e) => setFilter({ search: e.target.value || undefined })}
          />
        </div>
        <Select
          value={filter.level || 'all'}
          onValueChange={(v) => setFilter({ level: v === 'all' ? undefined : v as LogLevel })}
        >
          <SelectTrigger className="w-[120px]">
            <Filter className="w-4 h-4 mr-1" />
            级别
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">全部</SelectItem>
            <SelectItem value="DEBUG">DEBUG</SelectItem>
            <SelectItem value="INFO">INFO</SelectItem>
            <SelectItem value="WARN">WARN</SelectItem>
            <SelectItem value="ERROR">ERROR</SelectItem>
            <SelectItem value="SUCCESS">SUCCESS</SelectItem>
          </SelectContent>
        </Select>
        <Input
          placeholder="模块筛选..."
          className="w-[120px]"
          value={filter.module || ''}
          onChange={(e) => setFilter({ module: e.target.value || undefined })}
        />
        <Button size="sm" variant="outline" className="gap-2" onClick={testLogs}>
          <TestTube className="w-4 h-4" />
          测试日志
        </Button>

        {stats && (
          <div className="flex gap-4 text-sm text-muted-foreground ml-auto">
            <span>总计: <strong>{pagination.total}</strong></span>
            <span className="text-red-500">错误: {stats.by_level?.ERROR || 0}</span>
            <span className="text-orange-500">警告: {stats.by_level?.WARN || 0}</span>
          </div>
        )}
      </div>

      {error && (
        <div className="bg-red-50 border border-red-200 text-red-700 px-4 py-2 rounded-md text-sm">
          {error}
        </div>
      )}

      <Card className="flex-1 glass border-none shadow-custom overflow-hidden flex flex-col">
        <CardHeader className="pb-2 border-b border-border/50 bg-white/30">
          <div className="flex items-center gap-2">
            <Terminal className="w-4 h-4 text-muted-foreground" />
            <span className="text-sm font-mono text-muted-foreground">console_output.log</span>
            {isStreaming && (
              <span className="flex items-center gap-1 text-xs text-green-600">
                <span className="w-2 h-2 rounded-full bg-green-500 animate-pulse" />
                实时监听中
              </span>
            )}
          </div>
        </CardHeader>
        <CardContent ref={logsContainerRef} onScroll={handleScroll} className="flex-1 overflow-auto p-0 font-mono text-sm">
          <div className="p-4 space-y-1">
            {logs.length === 0 && !isLoading && (
              <div className="text-center text-muted-foreground py-8">
                暂无日志数据
              </div>
            )}
            {isLoading && logs.length === 0 && (
              <div className="text-center text-muted-foreground py-8">
                加载中...
              </div>
            )}
            {logs.map((log) => (
              <div
                key={log.id}
                className={`flex gap-4 hover:bg-black/5 p-1 rounded px-2 transition-colors ${getLevelBgColor(log.level)}`}
              >
                <span className="text-muted-foreground opacity-60 w-20 shrink-0 select-none">
                  {formatTime(log.timestamp)}
                </span>
                <span className={`w-16 shrink-0 font-bold ${getLevelColor(log.level)}`}>
                  {log.level}
                </span>
                <span className="w-24 shrink-0 text-purple-600 opacity-80 truncate">
                  [{log.module}]
                </span>
                <span className="text-slate-700 flex-1 break-all">{log.message}</span>
              </div>
            ))}
            {isStreaming && !isPaused && (
              <div className="flex gap-4 p-1 px-2 animate-pulse">
                <span className="w-2 opacity-0"></span>
                <span className="text-muted-foreground">_</span>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {pagination.totalPages > 1 && (
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <span>每页</span>
            <Select
              value={String(pagination.pageSize)}
              onValueChange={(v) => setPageSize(Number(v))}
            >
              <SelectTrigger className="w-[80px] h-8">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="25">25</SelectItem>
                <SelectItem value="50">50</SelectItem>
                <SelectItem value="100">100</SelectItem>
                <SelectItem value="200">200</SelectItem>
              </SelectContent>
            </Select>
            <span>条</span>
            <span className="ml-4">
              第 {pagination.page} / {pagination.totalPages} 页
            </span>
          </div>
          
          <div className="flex items-center gap-1">
            <Button
              variant="outline"
              size="icon"
              className="h-8 w-8"
              onClick={() => setPage(1)}
              disabled={pagination.page <= 1 || isLoading}
            >
              <ChevronsLeft className="w-4 h-4" />
            </Button>
            <Button
              variant="outline"
              size="icon"
              className="h-8 w-8"
              onClick={() => setPage(pagination.page - 1)}
              disabled={pagination.page <= 1 || isLoading}
            >
              <ChevronLeft className="w-4 h-4" />
            </Button>
            <span className="px-3 text-sm">
              {pagination.page}
            </span>
            <Button
              variant="outline"
              size="icon"
              className="h-8 w-8"
              onClick={() => setPage(pagination.page + 1)}
              disabled={pagination.page >= pagination.totalPages || isLoading}
            >
              <ChevronRight className="w-4 h-4" />
            </Button>
            <Button
              variant="outline"
              size="icon"
              className="h-8 w-8"
              onClick={() => setPage(pagination.totalPages)}
              disabled={pagination.page >= pagination.totalPages || isLoading}
            >
              <ChevronsRight className="w-4 h-4" />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
};

export default Logs;
