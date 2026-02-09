/**
 * 独立字幕窗口
 * Tauri 原生无边框窗口，可浮于游戏上层
 * 
 * 功能：
 * - 字号、透明度、字体颜色调整
 * - 打字速度控制
 * - 自动关闭（可选开关与时间）
 * - 自动显示（有内容时）
 * - 点击穿透
 * - 语音输入
 * - 手动发送文本
 * - 拖动、调整大小
 */

import React, { useEffect, useState, useMemo, useCallback, useRef, CSSProperties } from 'react';
import { useTypewriter } from '@/hooks/useTypewriter';
import { 
  GripHorizontal, X, RotateCcw, MousePointer2, MousePointerClick, 
  Mic, MicOff, Send, Type, Palette, Clock, Loader2, History
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Slider } from '@/components/ui/slider';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { subtitleWindow, isTauri } from '@/services/tauri';
import { speechService } from '@/services/speech';
import { API_BASE_URL, SCROLL_THRESHOLD } from '@/lib/constants';
import { toast } from 'sonner';
import { Toaster } from '@/components/ui/sonner';

/**
 * 开始窗口拖动（Tauri 2.x）
 * 使用 getCurrentWindow().startDragging() 替代 data-tauri-drag-region
 */
async function startWindowDrag() {
  if (!isTauri()) return;
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().startDragging();
  } catch (e) {
    console.warn('[Overlay] Failed to start dragging:', e);
  }
}

interface SubtitleConfig {
  opacity: number;
  fontSize: number;
  fontColor: string;
  bgColor: string;
  charDelay: number;
  textOnly: boolean;
}

const DEFAULT_CONFIG: SubtitleConfig = {
  opacity: 90,
  fontSize: 24,
  fontColor: '#ec4899',
  bgColor: 'linear-gradient(135deg, #fce7f3, #f5d0fe)',
  charDelay: 30,
  textOnly: false,
};

const FONT_COLORS = [
  { name: '白色', value: '#ffffff' },
  { name: '粉色', value: '#ec4899' },
  { name: '蓝色', value: '#3b82f6' },
  { name: '绿色', value: '#22c55e' },
  { name: '黄色', value: '#eab308' },
];

const Overlay: React.FC = () => {
  const [config, setConfig] = useState<SubtitleConfig>(() => {
    try {
      const stored = localStorage.getItem('subtitle-overlay-config');
      return stored ? { ...DEFAULT_CONFIG, ...JSON.parse(stored) } : DEFAULT_CONFIG;
    } catch {
      return DEFAULT_CONFIG;
    }
  });
  
  const [streamContent, setStreamContent] = useState('');
  const [isStreaming, setIsStreaming] = useState(false);
  
  const [history, setHistory] = useState<Array<{ id: string; content: string; speaker?: string; timestamp: number }>>([]);

  const [clickThrough, setClickThrough] = useState(false);
  const [autoClose, setAutoClose] = useState(true);
  const [autoCloseDelay, setAutoCloseDelay] = useState(5);
  const [autoShow, setAutoShow] = useState(true);
  const [isRecording, setIsRecording] = useState(false);
  const [isRecognizing, setIsRecognizing] = useState(false);
  const [inputText, setInputText] = useState('');
  const [showSettings, setShowSettings] = useState(false);
  const [speechConfig, setSpeechConfig] = useState<{ apiKey: string; secretKey: string } | null>(null);
  const [isUserSent, setIsUserSent] = useState(false);
  const [selectedHistoryId, setSelectedHistoryId] = useState<string | null>(null);
  
  const autoCloseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const isNearBottomRef = useRef(true);
  
  

  const { displayedText, isTyping, reset: resetTypewriter } = useTypewriter(streamContent, {
    charDelay: config.charDelay,
  });

  /**
   * 组件挂载时添加 overlay-window class 到 html 元素
   * 用于隐藏滚动条和设置全屏透明背景
   */
  useEffect(() => {
    document.documentElement.classList.add('overlay-window');
    return () => {
      document.documentElement.classList.remove('overlay-window');
    };
  }, []);

  useEffect(() => {
    const loadSpeechConfig = async () => {
      try {
        const response = await fetch(`${API_BASE_URL}/api/config`);
        if (response.ok) {
          const result = await response.json();
          const configData = result.data || result;
          if (configData.baidu_api_key && configData.baidu_secret_key) {
            setSpeechConfig({
              apiKey: configData.baidu_api_key,
              secretKey: configData.baidu_secret_key,
            });
          }
        }
      } catch (error) {
        console.warn('Failed to load speech config:', error);
      }
    };
    loadSpeechConfig();
  }, []);

  useEffect(() => {
    localStorage.setItem('subtitle-overlay-config', JSON.stringify(config));
  }, [config]);

  useEffect(() => {
    if (!isTauri()) return;

    const unsubscribers: Array<() => void> = [];

    subtitleWindow.onConfigUpdate((newConfig) => {
      setConfig(prev => ({
        ...prev,
        opacity: typeof newConfig.subtitle_opacity === 'number' 
          ? Math.round(newConfig.subtitle_opacity * 100) 
          : prev.opacity,
        fontSize: typeof newConfig.subtitle_font_size === 'number' 
          ? newConfig.subtitle_font_size 
          : prev.fontSize,
        fontColor: typeof newConfig.subtitle_font_color === 'string' 
          ? newConfig.subtitle_font_color 
          : prev.fontColor,
        bgColor: typeof newConfig.subtitle_bg_color === 'string' 
          ? newConfig.subtitle_bg_color 
          : prev.bgColor,
        charDelay: typeof newConfig.char_delay === 'number' 
          ? newConfig.char_delay 
          : prev.charDelay,
        textOnly: typeof newConfig.subtitle_text_only === 'boolean'
          ? newConfig.subtitle_text_only
          : prev.textOnly,
      }));
    }).then(unsub => unsubscribers.push(unsub));

    subtitleWindow.onContentUpdate((content) => {
      setStreamContent(content.text);
      setIsStreaming(content.isStreaming);
      setIsUserSent(false);
      if (content.isStreaming) {
        setSelectedHistoryId(null);
      }
      
      if (content.text && !content.isStreaming) {
        setHistory(prev => {
          const newItem = {
            id: Date.now().toString(),
            content: content.text,
            speaker: content.speaker,
            timestamp: Date.now(),
          };
          return [newItem, ...prev].slice(0, 20);
        });
      }
    }).then(unsub => unsubscribers.push(unsub));

    subtitleWindow.onHide(() => {
      handleClose();
    }).then(unsub => unsubscribers.push(unsub));

    return () => {
      unsubscribers.forEach(unsub => unsub());
    };
  }, []);

  useEffect(() => {
    if (isTauri()) {
      subtitleWindow.setClickThrough(clickThrough);
    }
  }, [clickThrough]);

  useEffect(() => {
    if (autoCloseTimerRef.current) {
      clearTimeout(autoCloseTimerRef.current);
      autoCloseTimerRef.current = null;
    }
    
    if (!isStreaming && !isTyping && streamContent && autoClose && !isUserSent) {
      autoCloseTimerRef.current = setTimeout(() => {
        handleClose();
      }, autoCloseDelay * 1000);
    }
    
    return () => {
      if (autoCloseTimerRef.current) {
        clearTimeout(autoCloseTimerRef.current);
        autoCloseTimerRef.current = null;
      }
    };
  }, [isStreaming, isTyping, streamContent, autoClose, autoCloseDelay, isUserSent]);

  /**
   * 自动滚动到底部（新内容时）
   * 仅当用户处于底部附近时才自动滚动，避免打断用户浏览历史
   */
  useEffect(() => {
    if (scrollRef.current && (isStreaming || displayedText) && isNearBottomRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [displayedText, isStreaming]);

  const handleScroll = useCallback(() => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    isNearBottomRef.current = scrollHeight - scrollTop - clientHeight < SCROLL_THRESHOLD;
  }, [SCROLL_THRESHOLD]);

  const selectedHistoryContent = useMemo(() => {
    if (!selectedHistoryId) return null;
    return history.find(item => item.id === selectedHistoryId)?.content || null;
  }, [selectedHistoryId, history]);

  /**
   * 背景层样式 - 只用于背景层，内容层不受影响
   * 支持格式: #ffffff, rgb(r,g,b), rgba(r,g,b,a), linear-gradient(...)
   */
  const bgLayerStyle = useMemo((): CSSProperties => {
    if (config.textOnly) {
      return {
        backgroundColor: 'transparent',
        backdropFilter: 'none',
        opacity: 0,
      };
    }

    const bgOpacity = config.opacity / 100;
    
    if (bgOpacity === 0) {
      return {
        backgroundColor: 'transparent',
        backdropFilter: 'none',
        opacity: 0,
      };
    }
    
    const bgColor = config.bgColor;
    
    // 处理渐变（linear-gradient, radial-gradient 等）
    if (bgColor.includes('gradient')) {
      return {
        background: bgColor,
        backdropFilter: `blur(${Math.round(bgOpacity * 12)}px)`,
        opacity: bgOpacity,
      };
    }
    
    // 处理透明
    if (bgColor === 'transparent') {
      return {
        backgroundColor: 'transparent',
        backdropFilter: `blur(${Math.round(bgOpacity * 12)}px)`,
        opacity: bgOpacity,
      };
    }
    
    // 处理十六进制颜色 #ffffff 或 #fff
    let background: string;
    if (bgColor.startsWith('#')) {
      let hex = bgColor.slice(1);
      if (hex.length === 3) {
        hex = hex[0] + hex[0] + hex[1] + hex[1] + hex[2] + hex[2];
      }
      const r = parseInt(hex.slice(0, 2), 16);
      const g = parseInt(hex.slice(2, 4), 16);
      const b = parseInt(hex.slice(4, 6), 16);
      background = `rgb(${r}, ${g}, ${b})`;
    } 
    // 处理 rgb(r, g, b)
    else if (bgColor.startsWith('rgb(')) {
      background = bgColor;
    } 
    // 处理 rgba(r, g, b, a) - 提取 rgb 部分
    else if (bgColor.startsWith('rgba(')) {
      const match = bgColor.match(/rgba\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*[\d.]+\s*\)/);
      if (match) {
        background = `rgb(${match[1]}, ${match[2]}, ${match[3]})`;
      } else {
        background = bgColor;
      }
    } 
    else {
      background = bgColor;
    }
    
    return {
      backgroundColor: background,
      backdropFilter: `blur(${Math.round(bgOpacity * 12)}px)`,
      opacity: bgOpacity,
    };
  }, [config.textOnly, config.opacity, config.bgColor]);

  const handleClose = useCallback(async () => {
    if (isTauri()) {
      await subtitleWindow.hide();
    }
  }, []);

  const handleSend = useCallback(() => {
    if (inputText.trim()) {
      setIsUserSent(true);
      setStreamContent(inputText.trim());
      navigator.clipboard.writeText(inputText.trim());
      setInputText('');
      
      setHistory(prev => [{
        id: Date.now().toString(),
        content: inputText.trim(),
        timestamp: Date.now(),
      }, ...prev].slice(0, 20));
    }
  }, [inputText]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }, [handleSend]);

  const handleMicToggle = useCallback(async () => {
    if (isRecognizing) return;
    
    if (!isRecording) {
      if (!speechConfig) {
        toast.warning('请先在设置中配置百度语音 API');
        return;
      }
      
      const success = await speechService.startRecording();
      if (success) {
        setIsRecording(true);
        toast.info('开始录音，请说话...');
      } else {
        toast.error('无法访问麦克风，请检查权限');
      }
    } else {
      setIsRecording(false);
      setIsRecognizing(true);
      
      try {
        const audioBlob = await speechService.stopRecording();
        if (!audioBlob || !speechConfig) {
          setIsRecognizing(false);
          return;
        }

        const result = await speechService.recognize(audioBlob, speechConfig);
        
        if (result.success && result.text) {
          setInputText(prev => prev + result.text);
        }
      } catch (error) {
        console.error('语音识别失败:', error);
      } finally {
        setIsRecognizing(false);
      }
    }
  }, [isRecording, isRecognizing, speechConfig]);

  const handleClear = useCallback(() => {
    resetTypewriter();
    setStreamContent('');
  }, [resetTypewriter]);

  const handleResetDefaults = useCallback(() => {
    setConfig(DEFAULT_CONFIG);
  }, []);

  return (
    <div
      className="w-screen h-screen flex flex-col select-none"
      style={{ 
        pointerEvents: clickThrough ? 'none' : 'auto',
        overflow: 'hidden',
        background: 'transparent',
      }}
    >
      {!config.textOnly && (
      <div className="relative" style={{ pointerEvents: 'auto' }}>
        <div className="absolute inset-0 pointer-events-none" style={bgLayerStyle} />
        <div className="relative z-10 flex items-center justify-between px-3 py-1.5" style={{ color: config.fontColor }}>
          <div 
            className="flex items-center gap-2 cursor-move opacity-50"
            onMouseDown={startWindowDrag}
          >
            <GripHorizontal className="w-4 h-4" />
            <span className="text-xs">拖动</span>
          </div>

        {/* 中间：控件 */}
        <div className="flex items-center gap-3 text-xs">
          {/* 字号 */}
          <div className="flex items-center gap-1.5">
            <Type className="w-3 h-3 opacity-70" />
            <Slider 
              value={[config.fontSize]} 
              onValueChange={(v) => setConfig(prev => ({ ...prev, fontSize: v[0] }))} 
              max={48} min={12} step={2} 
              className="w-14" 
            />
            <span className="opacity-50 w-6">{config.fontSize}</span>
          </div>

          {/* 透明度 */}
          <div className="flex items-center gap-1.5">
            <span className="opacity-70">透明</span>
            <Slider 
              value={[config.opacity]} 
              onValueChange={(v) => setConfig(prev => ({ ...prev, opacity: v[0] }))} 
              max={100} min={0} step={5} 
              className="w-14" 
            />
            <span className="opacity-50 w-6">{config.opacity}%</span>
          </div>

          {/* 速度 */}
          <div className="flex items-center gap-1.5">
            <Clock className="w-3 h-3 opacity-70" />
            <span className="opacity-70">速度</span>
            <Slider 
              value={[config.charDelay]} 
              onValueChange={(v) => setConfig(prev => ({ ...prev, charDelay: v[0] }))} 
              max={100} min={10} step={5} 
              className="w-12" 
            />
            <span className="opacity-50 w-8">{config.charDelay}ms</span>
          </div>

          {/* 颜色选择 */}
          <div className="flex items-center gap-1">
            <Palette className="w-3 h-3 opacity-70" />
            {FONT_COLORS.map(color => (
              <button
                key={color.value}
                className={`w-4 h-4 rounded-full border-2 ${config.fontColor === color.value ? 'border-white' : 'border-transparent'}`}
                style={{ backgroundColor: color.value }}
                onClick={() => setConfig(prev => ({ ...prev, fontColor: color.value }))}
                title={color.name}
              />
            ))}
          </div>

          {/* 自动关闭 */}
          <div className="flex items-center gap-1">
            <Checkbox
              id="auto-close"
              checked={autoClose}
              onCheckedChange={(checked) => setAutoClose(checked as boolean)}
              className="w-3 h-3 data-[state=checked]:bg-pink-500 border-white/30"
            />
            <Slider 
              value={[autoCloseDelay]} 
              onValueChange={(v) => setAutoCloseDelay(v[0])} 
              max={30} min={1} step={1} 
              className="w-10" 
              disabled={!autoClose}
            />
            <span className="opacity-50 w-5">{autoCloseDelay}s</span>
          </div>

          {/* 穿透 */}
          <div className="flex items-center gap-1">
            <Checkbox
              id="click-through"
              checked={clickThrough}
              onCheckedChange={(checked) => setClickThrough(checked as boolean)}
              className="w-3 h-3 data-[state=checked]:bg-pink-500 border-white/30"
            />
            <Label htmlFor="click-through" className="opacity-70 text-xs cursor-pointer flex items-center gap-0.5">
              {clickThrough ? <MousePointer2 className="w-3 h-3" /> : <MousePointerClick className="w-3 h-3" />}
              穿透
            </Label>
          </div>

          {/* 流式状态 */}
          {isStreaming && (
            <span className="flex items-center gap-1 text-green-400">
              <span className="w-1.5 h-1.5 rounded-full bg-green-400 animate-pulse" />
              输出中
            </span>
          )}

          {/* 历史记录选择器 */}
          {history.length > 0 && (
            <div className="flex items-center gap-1">
              <History className="w-3 h-3 opacity-70" />
              <Select 
                value={selectedHistoryId || 'current'} 
                onValueChange={(v) => setSelectedHistoryId(v === 'current' ? null : v)}
              >
                <SelectTrigger className="h-6 w-20 text-xs bg-white/10 border-white/20">
                  <SelectValue placeholder="历史" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="current">当前</SelectItem>
                  {history.slice(0, 6).map((item, index) => (
                    <SelectItem key={item.id} value={item.id}>
                      {index + 1}. {item.content.slice(0, 10)}...
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}
        </div>

        {/* 右侧：操作按钮 */}
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="sm" className="h-6 px-2 opacity-70 hover:opacity-100 hover:bg-white/10" onClick={handleClear}>
            清空
          </Button>
          <Button variant="ghost" size="sm" className="h-6 px-2 opacity-70 hover:opacity-100 hover:bg-white/10" onClick={handleResetDefaults}>
            <RotateCcw className="w-3 h-3" />
          </Button>
          <Button variant="ghost" size="icon" className="w-6 h-6 opacity-70 hover:text-red-400 hover:bg-red-500/20" onClick={handleClose}>
            <X className="w-4 h-4" />
          </Button>
        </div>
        </div>
      </div>
      )}

      {/* 字幕内容区 - 分层结构 + 滚动功能 */}
      <div className="flex-1 relative min-h-0">
        {!config.textOnly && (
        <div className="absolute inset-0 pointer-events-none" style={bgLayerStyle} />
        )}
        {/* 内容层 - 使用 absolute 填满容器，确保滚动正常工作 */}
        <div 
          ref={scrollRef}
          className="absolute inset-0 z-10 overflow-y-auto p-6"
          onScroll={handleScroll}
          style={{ 
            scrollbarWidth: 'thin',
            scrollbarColor: 'rgba(255,255,255,0.3) transparent',
          }}
        >
          {/* 使用 flexbox 实现内容底部对齐 */}
          <div className="min-h-full flex flex-col justify-end">
            {/* 历史字幕 */}
            {!config.textOnly && history.length > 0 && (
              <div className="w-full space-y-1 mb-2">
                {history.slice(0, 10).reverse().map(item => (
                  <p 
                    key={item.id}
                    className="leading-relaxed font-medium text-center max-w-full opacity-50 whitespace-pre-wrap"
                    style={{ 
                      fontSize: `${Math.max(12, config.fontSize - 4)}px`, 
                      color: config.fontColor,
                      textShadow: '0 1px 2px rgba(0,0,0,0.3)',
                    }}
                  >
                    {item.content}
                  </p>
                ))}
              </div>
            )}
            
            {/* 当前字幕文本 */}
            <p 
              className={`leading-relaxed font-medium max-w-full whitespace-pre-wrap ${(isStreaming || isTyping) ? 'text-left' : 'text-center'}`}
              style={{ 
                fontSize: `${config.fontSize}px`, 
                color: config.fontColor,
                textShadow: '0 2px 4px rgba(0,0,0,0.5)',
              }}
            >
              {selectedHistoryContent || displayedText || '等待内容...'}
              {!selectedHistoryContent && (isStreaming || isTyping) && (
                <span className="inline-block w-0.5 ml-1 bg-pink-400 animate-pulse" style={{ height: `${config.fontSize}px` }} />
              )}
            </p>
          </div>
        </div>
        
        {!config.textOnly && (
        <>
        <div className="absolute top-2 right-4 opacity-10 pointer-events-none z-20">
          <svg width="60" height="60" viewBox="0 0 100 100" fill="none">
            <path d="M50 0C50 0 50 50 100 50C50 50 50 100 50 100C50 100 50 50 0 50C50 50 50 0 50 0Z" fill="#EC4899"/>
          </svg>
        </div>
        <div className="absolute bottom-2 left-2 w-1/4 h-0.5 bg-gradient-to-r from-pink-400/30 to-transparent rounded-full pointer-events-none z-20" />
        <div className="absolute bottom-4 right-6 pointer-events-none z-20">
          <span className="text-pink-300/50 text-lg animate-pulse">✦</span>
        </div>
        </>
        )}
      </div>

      {!config.textOnly && (
      <div className="relative" style={{ pointerEvents: 'auto' }}>
        <div className="absolute inset-0 pointer-events-none" style={bgLayerStyle} />
        <div className="relative z-10 flex items-center gap-2 px-3 py-2" style={{ color: config.fontColor }}>
          {/* 麦克风按钮 */}
          <Button
            variant="ghost"
            size="icon"
            className={`w-8 h-8 ${isRecording ? 'text-red-400 bg-red-500/20 animate-pulse' : isRecognizing ? 'text-blue-400 bg-blue-500/20' : 'opacity-70 hover:opacity-100 hover:bg-white/10'}`}
            onClick={handleMicToggle}
            disabled={isRecognizing}
            title={isRecording ? '点击停止录音' : '点击开始语音输入'}
          >
            {isRecognizing ? <Loader2 className="w-4 h-4 animate-spin" /> : isRecording ? <MicOff className="w-4 h-4" /> : <Mic className="w-4 h-4" />}
          </Button>

          {/* 输入框 */}
          <Input
            value={inputText}
            onChange={(e) => setInputText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="输入文本，回车发送并复制..."
            className="flex-1 h-8 text-sm bg-white/10 border-white/20 placeholder:opacity-40 focus:border-pink-400"
            style={{ color: config.fontColor }}
          />

          {/* 发送按钮 */}
          <Button
            size="sm"
            className="h-8 px-4 bg-gradient-to-r from-pink-500 to-purple-500 hover:from-pink-600 hover:to-purple-600 text-white"
            onClick={handleSend}
          >
            <Send className="w-3 h-3 mr-1" />
            发送
          </Button>

          {/* 调整大小手柄 */}
          <div className="w-4 h-4 cursor-se-resize flex items-end justify-end opacity-30 hover:opacity-50">
            <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
              <path d="M9 1L1 9M9 5L5 9M9 9L9 9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
            </svg>
          </div>
        </div>
      </div>
      )}

      {!config.textOnly && (
      <>
      <div className="absolute -bottom-0.5 -left-0.5 w-3 h-3 border-b-2 border-l-2 border-pink-400/50 rounded-bl-lg pointer-events-none" />
      <div className="absolute -top-0.5 -right-0.5 w-3 h-3 border-t-2 border-r-2 border-pink-400/50 rounded-tr-lg pointer-events-none" />
      </>
      )}
      
      {/* Toast 通知 */}
      <Toaster position="top-center" richColors />
    </div>
  );
};

export default Overlay;
