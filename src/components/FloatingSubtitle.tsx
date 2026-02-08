/**
 * 浮动字幕组件
 * 在 Tauri 桌面环境下自动使用独立窗口
 * 在 Web 环境下使用 Portal 模式
 */

import React, { useState, useEffect, useCallback, useMemo, useRef, CSSProperties } from 'react';
import { createPortal } from 'react-dom';
import { GripHorizontal, Minus, X, Maximize2, MousePointer2, MousePointerClick, RotateCcw, History, Palette } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Slider } from '@/components/ui/slider';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useConfig, useConfigStore } from '@/stores/configStore';
import { useStreamStore, useSubtitleVisible, useStreamHistory } from '@/stores/streamStore';
import { subtitleWindow, isTauri } from '@/services/tauri';
import { useTypewriter } from '@/hooks/useTypewriter';

const STORAGE_KEY = 'floating-subtitle-position';
const SIZE_STORAGE_KEY = 'floating-subtitle-size';
const THEME_STORAGE_KEY = 'floating-subtitle-theme';

type ThemeKey = 'classic' | 'dark' | 'aurora' | 'light';

interface ThemeConfig {
  name: string;
  containerBorder: string;
  toolbarBorder: string;
  iconMain: string;
  textMain: string;
  buttonHover: string;
  buttonHoverSoft: string;
  resizeHandle: string;
  decoration: string;
  decorationCorner: string;
  progressBar: string;
  checkbox: string;
  bgColor: string;
  fontColor: string;
  svgFill: string;
}

const THEMES: Record<ThemeKey, ThemeConfig> = {
  classic: {
    name: '经典粉',
    containerBorder: 'border-pink-200',
    toolbarBorder: 'border-pink-200/50',
    iconMain: 'text-pink-400',
    textMain: 'text-pink-400',
    buttonHover: 'hover:bg-pink-100 hover:text-pink-600',
    buttonHoverSoft: 'hover:bg-pink-100/50',
    resizeHandle: 'text-pink-300 hover:text-pink-500',
    decoration: 'text-pink-300',
    decorationCorner: 'border-pink-400',
    progressBar: 'bg-pink-400',
    checkbox: 'data-[state=checked]:bg-pink-500 data-[state=checked]:border-pink-500',
    bgColor: 'rgba(255,255,255,0.1)',
    fontColor: '#ffffff',
    svgFill: '#EC4899'
  },
  dark: {
    name: '深邃黑金',
    containerBorder: 'border-yellow-600/30',
    toolbarBorder: 'border-yellow-600/20',
    iconMain: 'text-yellow-500',
    textMain: 'text-yellow-500',
    buttonHover: 'hover:bg-yellow-900/30 hover:text-yellow-400',
    buttonHoverSoft: 'hover:bg-yellow-900/20',
    resizeHandle: 'text-yellow-600 hover:text-yellow-400',
    decoration: 'text-yellow-600',
    decorationCorner: 'border-yellow-500',
    progressBar: 'bg-yellow-500',
    checkbox: 'data-[state=checked]:bg-yellow-600 data-[state=checked]:border-yellow-600',
    bgColor: 'rgba(0,0,0,0.8)',
    fontColor: '#fbbf24',
    svgFill: '#D97706'
  },
  aurora: {
    name: '科技极光',
    containerBorder: 'border-emerald-400/40',
    toolbarBorder: 'border-cyan-400/30',
    iconMain: 'text-emerald-400',
    textMain: 'text-cyan-300',
    buttonHover: 'hover:bg-cyan-500/20 hover:text-emerald-300',
    buttonHoverSoft: 'hover:bg-emerald-500/15',
    resizeHandle: 'text-cyan-400 hover:text-emerald-300',
    decoration: 'text-emerald-400',
    decorationCorner: 'border-cyan-400',
    progressBar: 'bg-gradient-to-r from-cyan-400 to-emerald-400',
    checkbox: 'data-[state=checked]:bg-emerald-500 data-[state=checked]:border-emerald-500',
    bgColor: 'linear-gradient(135deg, rgba(15, 23, 42, 0.85), rgba(20, 40, 60, 0.9), rgba(10, 50, 60, 0.85))',
    fontColor: '#5eead4',
    svgFill: '#34d399'
  },
  light: {
    name: '亚克力白',
    containerBorder: 'border-slate-200',
    toolbarBorder: 'border-slate-200/50',
    iconMain: 'text-slate-500',
    textMain: 'text-slate-600',
    buttonHover: 'hover:bg-slate-100 hover:text-slate-700',
    buttonHoverSoft: 'hover:bg-slate-100/50',
    resizeHandle: 'text-slate-400 hover:text-slate-600',
    decoration: 'text-slate-300',
    decorationCorner: 'border-slate-400',
    progressBar: 'bg-slate-500',
    checkbox: 'data-[state=checked]:bg-slate-600 data-[state=checked]:border-slate-600',
    bgColor: 'rgba(255,255,255,0.85)',
    fontColor: '#1e293b',
    svgFill: '#64748B'
  }
};

const FloatingSubtitleInner: React.FC = () => {
  const config = useConfig();
  const updateConfig = useConfigStore((state) => state.updateConfig);
  
  const streamContent = useStreamStore((state) => state.currentContent);
  const isStreaming = useStreamStore((state) => state.isStreaming);
  const currentSpeaker = useStreamStore((state) => state.currentSpeaker);
  const subtitleVisible = useSubtitleVisible();
  const hideSubtitle = useStreamStore((state) => state.hideSubtitle);
  const history = useStreamHistory();
  const addToHistory = useStreamStore((state) => state.addToHistory);
  const prevIsStreamingRef = useRef(false);

  const configCharDelay = config?.char_delay ?? 30;
  const { displayedText, isTyping, reset: resetTypewriter } = useTypewriter(streamContent, {
    charDelay: configCharDelay,
  });

  const [isMinimized, setIsMinimized] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const [selectedHistoryId, setSelectedHistoryId] = useState<string | null>(null);
  const [position, setPosition] = useState({ x: 100, y: 100 });
  const [size, setSize] = useState({ width: 800, height: 200 });
  const [isDragging, setIsDragging] = useState(false);
  const [isResizing, setIsResizing] = useState(false);
  const [dragOffset, setDragOffset] = useState({ x: 0, y: 0 });
  const [resizeStart, setResizeStart] = useState({ x: 0, y: 0, width: 0, height: 0 });
  const [clickThrough, setClickThrough] = useState(false);
  const [autoClose, setAutoClose] = useState(true);
  const [autoCloseDelay, setAutoCloseDelay] = useState(5);
  const autoCloseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const tauriWindowOpenedRef = useRef(false);

  const opacity = Math.round((config?.subtitle_opacity ?? 0.9) * 100);
  const fontSize = config?.subtitle_font_size ?? 18;
  const fontColor = config?.subtitle_font_color ?? '#ffffff';
  const bgColor = config?.subtitle_bg_color ?? 'rgba(255,255,255,0.1)';

  const [currentTheme, setCurrentTheme] = useState<ThemeKey>('classic');
  const theme = THEMES[currentTheme];

  // 初始化主题
  useEffect(() => {
    const savedTheme = localStorage.getItem(THEME_STORAGE_KEY) as ThemeKey;
    if (savedTheme && THEMES[savedTheme]) {
      setCurrentTheme(savedTheme);
    }
  }, []);

  const handleThemeChange = (key: ThemeKey) => {
    setCurrentTheme(key);
    localStorage.setItem(THEME_STORAGE_KEY, key);
    
    // 同步更新配置中的颜色
    const newTheme = THEMES[key];
    updateConfig('subtitle_bg_color', newTheme.bgColor);
    updateConfig('subtitle_font_color', newTheme.fontColor);
  };

  // Tauri 环境下自动打开独立窗口（仅在首次需要显示时创建）
  useEffect(() => {
    if (!isTauri()) return;
    
    if (subtitleVisible && !tauriWindowOpenedRef.current) {
      tauriWindowOpenedRef.current = true;
      subtitleWindow.create().then(() => {
        setTimeout(() => {
          if (config) {
            subtitleWindow.emitConfigUpdate({
              subtitle_opacity: config.subtitle_opacity,
              subtitle_font_color: config.subtitle_font_color,
              subtitle_bg_color: config.subtitle_bg_color,
              subtitle_font_size: config.subtitle_font_size,
              char_delay: config.char_delay,
            });
          }
        }, 500);
      }).catch(console.error);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [subtitleVisible]);

  // 隐藏字幕窗口（不关闭，只是隐藏）- 由 store 的 hideSubtitle 直接处理
  // 这个 useEffect 作为备用，确保状态同步
  useEffect(() => {
    if (!isTauri()) return;
    
    if (!subtitleVisible && tauriWindowOpenedRef.current) {
      subtitleWindow.hide().catch(console.error);
    }
  }, [subtitleVisible]);

  useEffect(() => {
    const savedPos = localStorage.getItem(STORAGE_KEY);
    const savedSize = localStorage.getItem(SIZE_STORAGE_KEY);
    
    const defaultX = Math.max(0, (window.innerWidth - 800) / 2);
    const defaultY = Math.max(100, window.innerHeight - 280);
    
    if (savedPos) {
      try {
        const parsed = JSON.parse(savedPos);
        setPosition({
          x: Math.max(0, Math.min(parsed.x, window.innerWidth - 100)),
          y: Math.max(0, Math.min(parsed.y, window.innerHeight - 100)),
        });
      } catch {
        setPosition({ x: defaultX, y: defaultY });
      }
    } else {
      setPosition({ x: defaultX, y: defaultY });
    }
    
    if (savedSize) {
      try {
        setSize(JSON.parse(savedSize));
      } catch { /* ignore */ }
    }
  }, []);

  useEffect(() => {
    if (autoCloseTimerRef.current) {
      clearTimeout(autoCloseTimerRef.current);
      autoCloseTimerRef.current = null;
    }
    
    if (!isStreaming && !isTyping && streamContent && autoClose) {
      autoCloseTimerRef.current = setTimeout(() => {
        hideSubtitle();
      }, autoCloseDelay * 1000);
    }
    
    return () => {
      if (autoCloseTimerRef.current) {
        clearTimeout(autoCloseTimerRef.current);
        autoCloseTimerRef.current = null;
      }
    };
  }, [isStreaming, isTyping, streamContent, autoClose, autoCloseDelay, hideSubtitle]);

  useEffect(() => {
    if (isStreaming) {
      setSelectedHistoryId(null);
    }
  }, [isStreaming]);

  useEffect(() => {
    if (prevIsStreamingRef.current && !isStreaming && streamContent && streamContent.length > 10) {
      addToHistory({
        id: `msg_${Date.now()}`,
        content: streamContent,
        speaker: currentSpeaker ?? undefined,
        timestamp: Date.now(),
      });
    }
    prevIsStreamingRef.current = isStreaming;
  }, [isStreaming, streamContent, currentSpeaker, addToHistory]);

  useEffect(() => {
    if (position.x !== 0 || position.y !== 0) {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(position));
    }
  }, [position]);

  useEffect(() => {
    localStorage.setItem(SIZE_STORAGE_KEY, JSON.stringify(size));
  }, [size]);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (clickThrough) return;
    e.preventDefault();
    e.stopPropagation();
    
    const rect = containerRef.current?.getBoundingClientRect();
    if (rect) {
      setDragOffset({ x: e.clientX - rect.left, y: e.clientY - rect.top });
      setIsDragging(true);
    }
  }, [clickThrough]);

  const handleResizeMouseDown = useCallback((e: React.MouseEvent) => {
    if (clickThrough) return;
    e.preventDefault();
    e.stopPropagation();
    setResizeStart({ x: e.clientX, y: e.clientY, width: size.width, height: size.height });
    setIsResizing(true);
  }, [clickThrough, size]);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (isDragging) {
        const newX = Math.max(0, Math.min(window.innerWidth - 100, e.clientX - dragOffset.x));
        const newY = Math.max(0, Math.min(window.innerHeight - 50, e.clientY - dragOffset.y));
        setPosition({ x: newX, y: newY });
      } else if (isResizing) {
        const dx = e.clientX - resizeStart.x;
        const dy = e.clientY - resizeStart.y;
        setSize({
          width: Math.max(400, resizeStart.width + dx),
          height: Math.max(120, resizeStart.height + dy),
        });
      }
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      setIsResizing(false);
    };

    if (isDragging || isResizing) {
      window.addEventListener('mousemove', handleMouseMove);
      window.addEventListener('mouseup', handleMouseUp);
    }

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging, isResizing, dragOffset, resizeStart]);

  const handleOpacityChange = useCallback((value: number[]) => {
    updateConfig('subtitle_opacity', value[0] / 100);
  }, [updateConfig]);

  const handleSpeedChange = useCallback((value: number[]) => {
    updateConfig('char_delay', value[0]);
  }, [updateConfig]);

  const handleAutoCloseDelayChange = useCallback((value: number[]) => {
    setAutoCloseDelay(value[0]);
  }, []);

  const handleClickThroughChange = useCallback(async (checked: boolean) => {
    setClickThrough(checked);
    if (isTauri()) {
      await subtitleWindow.setClickThrough(checked);
    }
  }, []);

  const handleClose = useCallback(() => {
    hideSubtitle();
    if (isTauri()) {
      subtitleWindow.hide();
    }
  }, [hideSubtitle]);

  const handleClear = useCallback(() => {
    resetTypewriter();
  }, [resetTypewriter]);

  const handleResetDefaults = useCallback(() => {
    setCurrentTheme('classic');
    localStorage.setItem(THEME_STORAGE_KEY, 'classic');
    updateConfig('subtitle_opacity', 0.9);
    updateConfig('subtitle_font_size', 18);
    updateConfig('char_delay', 30);
    updateConfig('subtitle_font_color', '#ffffff');
    updateConfig('subtitle_bg_color', 'rgba(255,255,255,0.1)');
  }, [updateConfig]);

  const selectedHistoryContent = useMemo(() => {
    if (!selectedHistoryId) return null;
    return history.find(item => item.id === selectedHistoryId)?.content || null;
  }, [selectedHistoryId, history]);

  const handleHistoryItemClick = useCallback((id: string) => {
    setSelectedHistoryId(id);
    setShowHistory(false);
  }, []);

  const handleBackToCurrent = useCallback(() => {
    setSelectedHistoryId(null);
  }, []);

  const containerStyle = useMemo((): CSSProperties => {
    const bgOpacity = opacity / 100;
    
    // 透明度为 0 时，背景完全透明，仅显示文字
    if (bgOpacity === 0) {
      return {
        backgroundColor: 'transparent',
        backdropFilter: 'none',
        WebkitBackdropFilter: 'none',
      };
    }
    
    const isGradient = bgColor.startsWith('linear-gradient');
    
    if (isGradient) {
      return {
        background: bgColor,
        backdropFilter: 'blur(12px)',
        WebkitBackdropFilter: 'blur(12px)',
      };
    }
    
    let bgWithOpacity = bgColor;
    if (bgColor.startsWith('rgba')) {
      bgWithOpacity = bgColor.replace(/,\s*[\d.]+\)$/, `, ${bgOpacity})`);
    } else if (bgColor.startsWith('rgb(')) {
      bgWithOpacity = bgColor.replace('rgb(', 'rgba(').replace(')', `, ${bgOpacity})`);
    } else if (bgColor.startsWith('#')) {
      const r = parseInt(bgColor.slice(1, 3), 16);
      const g = parseInt(bgColor.slice(3, 5), 16);
      const b = parseInt(bgColor.slice(5, 7), 16);
      bgWithOpacity = `rgba(${r}, ${g}, ${b}, ${bgOpacity})`;
    } else if (bgColor === 'transparent') {
      bgWithOpacity = `rgba(255, 255, 255, ${bgOpacity * 0.1})`;
    }
    
    return {
      backgroundColor: bgWithOpacity,
      backdropFilter: 'blur(12px)',
      WebkitBackdropFilter: 'blur(12px)',
    };
  }, [opacity, bgColor]);

  const contentStyle = useMemo((): CSSProperties => {
    return {
      fontSize: `${fontSize}px`,
    };
  }, [fontSize]);

  if (!subtitleVisible) {
    return null;
  }

  if (isMinimized) {
    const minimizedContent = (
      <div
        ref={containerRef}
        className="fixed select-none"
        style={{
          left: 0,
          top: 0,
          transform: `translate3d(${position.x}px, ${position.y}px, 0)`,
          zIndex: 99999,
          willChange: 'transform',
        }}
      >
        <div 
          className={`border-2 ${theme.containerBorder} rounded-xl shadow-lg px-3 py-2 flex items-center gap-3`}
          style={containerStyle}
        >
          <div className={`cursor-grab active:cursor-grabbing ${theme.iconMain}`} onMouseDown={handleMouseDown}>
            <GripHorizontal className="w-4 h-4" />
          </div>
          <span className="text-sm max-w-[200px] truncate" style={{ color: fontColor }}>
            {currentSpeaker || 'AI'}: {displayedText?.slice(0, 30)}...
          </span>
          <Button variant="ghost" size="icon" className={`w-6 h-6 ${theme.buttonHover}`} onClick={() => setIsMinimized(false)}>
            <Maximize2 className="w-3 h-3" />
          </Button>
          <Button variant="ghost" size="icon" className="w-6 h-6 hover:bg-red-100 hover:text-red-600" onClick={handleClose}>
            <X className="w-3 h-3" />
          </Button>
        </div>
      </div>
    );
    return typeof document !== 'undefined' ? createPortal(minimizedContent, document.body) : null;
  }

  const fullContent = (
    <figure
      ref={containerRef}
      role="dialog"
      aria-label="字幕窗口"
      aria-live="polite"
      aria-atomic="true"
      className="fixed select-none"
      style={{
        left: 0,
        top: 0,
        transform: `translate3d(${position.x}px, ${position.y}px, 0)`,
        zIndex: 99999,
        width: `${size.width}px`,
        maxWidth: '95vw',
        willChange: 'transform',
        cursor: isDragging ? 'grabbing' : isResizing ? 'se-resize' : 'default',
        pointerEvents: clickThrough ? 'none' : 'auto',
      }}
    >
      <div 
        className={`relative border-2 ${theme.containerBorder} rounded-2xl shadow-2xl overflow-hidden`}
        style={{ ...containerStyle, pointerEvents: clickThrough ? 'none' : 'auto' }}
      >
        {/* 工具栏 */}
        <div
          className={`flex items-center justify-between px-3 py-2 border-b ${theme.toolbarBorder}`}
          role="toolbar"
          aria-label="字幕控制工具栏"
          style={{ cursor: clickThrough ? 'default' : 'move', pointerEvents: clickThrough ? 'none' : 'auto' }}
          onMouseDown={handleMouseDown}
        >
          <div className={`flex items-center gap-1 ${theme.iconMain}`} aria-hidden="true">
            <GripHorizontal className="w-4 h-4" />
            <span className="text-xs" style={{ color: fontColor, opacity: 0.7 }}>拖动</span>
          </div>

          <div className="flex items-center gap-4 text-xs" style={{ pointerEvents: 'auto' }}>
            {/* 透明度滑块 - 支持 0-100% 全区间 */}
            <div className="flex items-center gap-2">
              <span style={{ color: fontColor, opacity: 0.7 }}>透明</span>
              <Slider value={[opacity]} onValueChange={handleOpacityChange} max={100} min={0} step={5} className="w-16" aria-label="透明度" />
              <span style={{ color: fontColor, opacity: 0.7 }} className="w-6">{opacity}%</span>
            </div>

            {/* 打字速度 */}
            <div className="flex items-center gap-2">
              <span style={{ color: fontColor, opacity: 0.7 }}>速度</span>
              <Slider value={[configCharDelay]} onValueChange={handleSpeedChange} max={100} min={10} step={5} className="w-16" aria-label="打字速度" />
              <span style={{ color: fontColor, opacity: 0.7 }} className="w-8">{configCharDelay}ms</span>
            </div>

            {/* 自动关闭 */}
            <div className="flex items-center gap-1.5">
              <Checkbox
                id="auto-close"
                checked={autoClose}
                onCheckedChange={(checked) => setAutoClose(checked as boolean)}
                className={`w-3.5 h-3.5 ${theme.checkbox}`}
              />
              <Label htmlFor="auto-close" style={{ color: fontColor, opacity: 0.7 }} className="text-xs cursor-pointer">
                自动关闭
              </Label>
              {autoClose && (
                <div className="flex items-center gap-1">
                  <Slider value={[autoCloseDelay]} onValueChange={handleAutoCloseDelayChange} max={30} min={1} step={1} className="w-12" aria-label="自动关闭延迟" />
                  <span style={{ color: fontColor, opacity: 0.7 }} className="w-6">{autoCloseDelay}s</span>
                </div>
              )}
            </div>

            {/* 鼠标穿透 */}
            <div className="flex items-center gap-1.5">
              <Checkbox
                id="click-through"
                checked={clickThrough}
                onCheckedChange={handleClickThroughChange}
                className={`w-3.5 h-3.5 ${theme.checkbox}`}
              />
              <Label htmlFor="click-through" style={{ color: fontColor, opacity: 0.7 }} className="text-xs cursor-pointer flex items-center gap-1">
                {clickThrough ? <MousePointer2 className="w-3 h-3" /> : <MousePointerClick className="w-3 h-3" />}
                穿透
              </Label>
            </div>

            {/* 流式状态指示 */}
            {isStreaming && (
              <span className="flex items-center gap-1 text-xs text-green-400">
                <span className="w-1.5 h-1.5 rounded-full bg-green-400 animate-pulse" />
                输出中
              </span>
            )}

            {/* 主题切换 */}
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="sm" className={`h-6 px-2 text-xs ${theme.buttonHoverSoft}`} title="切换主题">
                  <Palette className="w-3 h-3" style={{ color: fontColor }} />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {Object.entries(THEMES).map(([key, config]) => (
                  <DropdownMenuItem 
                    key={key} 
                    onClick={() => handleThemeChange(key as ThemeKey)}
                    className={currentTheme === key ? 'bg-accent' : ''}
                  >
                    {config.name}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>

            {/* 重置按钮 */}
            <Button variant="ghost" size="sm" className={`h-6 px-2 text-xs ${theme.buttonHoverSoft}`} onClick={handleResetDefaults} title="重置默认样式">
              <RotateCcw className="w-3 h-3" style={{ color: fontColor }} />
            </Button>
          </div>

          <div className="flex items-center gap-1" style={{ pointerEvents: 'auto' }}>
            {/* 历史记录按钮 */}
            <Button variant="ghost" size="sm" className={`h-6 px-2 text-xs ${theme.buttonHoverSoft}`} onClick={() => setShowHistory(!showHistory)}>
              <History className="w-3 h-3 mr-1" style={{ color: fontColor }} />
              <span style={{ color: fontColor, opacity: 0.7 }}>历史({history.length})</span>
            </Button>
            <Button variant="ghost" size="sm" className={`h-6 px-2 text-xs ${theme.buttonHoverSoft}`} onClick={handleClear}>
              <span style={{ color: fontColor, opacity: 0.7 }}>清空</span>
            </Button>
            <Button variant="ghost" size="icon" className={`w-6 h-6 ${theme.buttonHoverSoft}`} onClick={() => setIsMinimized(true)} aria-label="最小化">
              <Minus className="w-3 h-3" style={{ color: fontColor }} />
            </Button>
            <Button variant="ghost" size="icon" className="w-6 h-6 hover:bg-red-100/50" onClick={handleClose} aria-label="关闭">
              <X className="w-3 h-3" style={{ color: fontColor }} />
            </Button>
          </div>
        </div>

        {/* 历史记录面板 */}
        {showHistory && history.length > 0 && (
          <div className={`max-h-40 overflow-auto border-b ${theme.toolbarBorder}`}>
            <div 
              className={`px-4 py-2 hover:bg-white/10 cursor-pointer border-b ${theme.toolbarBorder} ${!selectedHistoryId ? 'bg-white/20' : ''}`}
              onClick={handleBackToCurrent}
            >
              <p className="text-sm font-medium" style={{ color: fontColor }}>📍 当前内容</p>
            </div>
            {history.slice(0, 6).map((msg, index) => (
              <div 
                key={msg.id} 
                className={`px-4 py-2 hover:bg-white/10 cursor-pointer border-b ${theme.toolbarBorder} last:border-b-0 ${selectedHistoryId === msg.id ? 'bg-white/20' : ''}`}
                onClick={() => handleHistoryItemClick(msg.id)}
              >
                <p className="text-sm line-clamp-2 whitespace-pre-wrap" style={{ color: fontColor }}>{index + 1}. {msg.content}</p>
                <span className="text-xs" style={{ color: fontColor, opacity: 0.5 }}>{msg.speaker || 'AI'}</span>
              </div>
            ))}
          </div>
        )}

        {/* 内容区 */}
        <div className="relative p-6" style={{ ...contentStyle, minHeight: `${Math.max(100, size.height - 80)}px` }}>
          {/* 装饰元素 */}
          <div className="absolute top-0 right-0 p-4 opacity-10" aria-hidden="true">
            <svg width="80" height="80" viewBox="0 0 100 100" fill="none" xmlns="http://www.w3.org/2000/svg">
              <path d="M50 0C50 0 50 50 100 50C50 50 50 100 50 100C50 100 50 50 0 50C50 50 50 0 50 0Z" fill={theme.svgFill}/>
            </svg>
          </div>
          <div className={`absolute bottom-2 left-2 w-1/4 h-1 bg-gradient-to-r from-current to-transparent rounded-full opacity-20`} style={{ color: theme.svgFill }} aria-hidden="true" />

          {/* 说话人标识 */}
          {currentSpeaker && !selectedHistoryContent && (
            <div className="mb-2">
              <span className={`text-sm font-bold ${theme.textMain}`}>{currentSpeaker}</span>
            </div>
          )}

          {/* 查看历史提示 */}
          {selectedHistoryContent && (
            <div className="mb-2 flex items-center gap-2">
              <span className={`text-xs ${theme.textMain} opacity-70`}>📜 查看历史记录</span>
              <Button 
                variant="ghost" 
                size="sm" 
                className={`h-5 px-2 text-xs ${theme.buttonHoverSoft}`}
                onClick={handleBackToCurrent}
              >
                返回当前
              </Button>
            </div>
          )}

          {/* 字幕文本 - 匀速显示 */}
          <p className="leading-relaxed font-medium relative z-10 whitespace-pre-wrap" role="status" style={{ fontSize: `${fontSize}px`, color: fontColor }}>
            {selectedHistoryContent || displayedText || '等待 AI 回复...'}
            {!selectedHistoryContent && (isStreaming || isTyping) && (
              <span className={`inline-block w-2 h-5 ml-1 ${theme.progressBar} align-middle animate-pulse`} aria-hidden="true" />
            )}
          </p>

          {/* 装饰星星 */}
          <div className="absolute bottom-4 right-6" aria-hidden="true">
            <span className={`${theme.decoration} text-lg animate-pulse`}>✦</span>
          </div>
          <div className="absolute bottom-8 right-10" aria-hidden="true">
            <span className={`${theme.decoration} text-sm opacity-60`}>✦</span>
          </div>
        </div>

        {/* 调整大小手柄 */}
        <div 
          className={`absolute bottom-0 right-0 w-4 h-4 cursor-se-resize flex items-end justify-end ${theme.resizeHandle}`}
          onMouseDown={handleResizeMouseDown}
          style={{ pointerEvents: clickThrough ? 'none' : 'auto' }}
          aria-label="拖动调整大小"
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
            <path d="M9 1L1 9M9 5L5 9M9 9L9 9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
          </svg>
        </div>
      </div>

      {/* 边角装饰 */}
      <div className={`absolute -bottom-1 -left-1 w-4 h-4 border-b-2 border-l-2 ${theme.decorationCorner} rounded-bl-lg`} aria-hidden="true" />
      <div className={`absolute -top-1 -right-1 w-4 h-4 border-t-2 border-r-2 ${theme.decorationCorner} rounded-tr-lg`} aria-hidden="true" />
    </figure>
  );
  
  return typeof document !== 'undefined' ? createPortal(fullContent, document.body) : null;
};

const FloatingSubtitle: React.FC = () => {
  if (isTauri()) {
    return null;
  }
  return <FloatingSubtitleInner />;
};

export default FloatingSubtitle;
