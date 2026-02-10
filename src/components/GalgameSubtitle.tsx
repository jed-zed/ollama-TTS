import React, { useId, useState, useMemo, CSSProperties } from 'react';
import { GripHorizontal, Minus, X, Send, RotateCcw } from 'lucide-react';
import { Slider } from '@/components/ui/slider';
import { Checkbox } from '@/components/ui/checkbox';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useConfig, useConfigStore } from '@/stores/configStore';
import { useStreamContent, useIsStreaming } from '@/stores/streamStore';
import { useTypewriter } from '@/hooks/useTypewriter';

interface GalgameSubtitleProps {
  text?: string;
  selectedHistoryContent?: string | null;
  className?: string;
  onClose?: () => void;
  onMinimize?: () => void;
  onSend?: (text: string) => void;
}

const DEFAULT_SUBTITLE_CONFIG = {
  subtitle_font_color: '#ffffff',
  subtitle_bg_color: 'rgba(255,255,255,0.1)',
  subtitle_opacity: 0.9,
  subtitle_font_size: 18,
  char_delay: 30,
};

const GalgameSubtitle = ({
  text,
  selectedHistoryContent,
  className,
  onClose,
  onMinimize,
  onSend
}: GalgameSubtitleProps) => {
  const transparencyId = useId();
  const speedId = useId();
  const autoCloseId = useId();
  const passThroughId = useId();
  const inputId = useId();

  const config = useConfig();
  const updateConfig = useConfigStore((state) => state.updateConfig);
  const streamContent = useStreamContent();
  const isStreaming = useIsStreaming();

  const [autoClose, setAutoClose] = useState(true);
  const [passThrough, setPassThrough] = useState(false);
  const [inputText, setInputText] = useState('');

  const opacity = Math.round((config?.subtitle_opacity ?? 0.9) * 100);
  const speed = config?.char_delay ?? 30;
  const fontSize = config?.subtitle_font_size ?? 18;
  const fontColor = config?.subtitle_font_color ?? '#ffffff';
  const bgColor = config?.subtitle_bg_color ?? 'rgba(255,255,255,0.1)';
  const textOnly = config?.subtitle_text_only ?? false;

  const isShowingHistory = !!selectedHistoryContent;
  const rawStreamText = text || streamContent || "哈......你这家伙，真是有意思呢。竟然能找到这里来，看来我也不能小看你了。";
  
  // 只对非历史记录内容使用打字机效果
  const { displayedText: typedText, isTyping } = useTypewriter(rawStreamText, {
    charDelay: speed,
  });

  // 历史记录直接显示，当前内容使用打字机效果
  const displayText = isShowingHistory ? selectedHistoryContent : typedText;

  const subtitleStyle = useMemo((): CSSProperties => {
    if (textOnly) {
      return {
        fontSize: `${fontSize}px`,
        backgroundColor: 'transparent',
        backdropFilter: 'none',
        WebkitBackdropFilter: 'none',
        border: 'none',
        borderWidth: 0,
        boxShadow: 'none',
        outline: 'none',
      };
    }

    const bgOpacity = opacity / 100;
    const isGradient = bgColor.startsWith('linear-gradient');
    
    if (isGradient) {
      return {
        fontSize: `${fontSize}px`,
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
      fontSize: `${fontSize}px`,
      backgroundColor: bgWithOpacity,
      backdropFilter: 'blur(12px)',
      WebkitBackdropFilter: 'blur(12px)',
    };
  }, [textOnly, opacity, fontSize, bgColor]);

  const handleOpacityChange = (value: number[]) => {
    updateConfig('subtitle_opacity', value[0] / 100);
  };

  const handleSpeedChange = (value: number[]) => {
    updateConfig('char_delay', value[0]);
  };

  const handleResetDefaults = () => {
    updateConfig('subtitle_opacity', DEFAULT_SUBTITLE_CONFIG.subtitle_opacity);
    updateConfig('subtitle_font_size', DEFAULT_SUBTITLE_CONFIG.subtitle_font_size);
    updateConfig('char_delay', DEFAULT_SUBTITLE_CONFIG.char_delay);
    updateConfig('subtitle_font_color', DEFAULT_SUBTITLE_CONFIG.subtitle_font_color);
    updateConfig('subtitle_bg_color', DEFAULT_SUBTITLE_CONFIG.subtitle_bg_color);
  };

  const handleSend = () => {
    if (inputText.trim() && onSend) {
      onSend(inputText);
      setInputText('');
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <figure
      role="dialog"
      aria-label="字幕窗口"
      aria-live="polite"
      aria-atomic="true"
      className={`
        relative w-full max-w-3xl mx-auto select-none
        motion-safe:animate-in motion-safe:fade-in motion-safe:slide-in-from-bottom-4 motion-safe:duration-500
        ${className}
      `}
    >
      <div 
        className={textOnly ? 'relative' : 'relative rounded-2xl overflow-hidden border-2 border-pink-200 shadow-2xl'}
        style={subtitleStyle}
      >

        {!textOnly && (
        <div
          className="flex flex-wrap items-center gap-2 px-3 py-2 bg-gradient-to-r from-pink-50 to-white border-b border-pink-100 cursor-move"
          role="toolbar"
          aria-label="字幕控制工具栏"
        >
          <div className="flex items-center gap-1 text-pink-400" aria-hidden="true">
            <GripHorizontal className="w-4 h-4" />
            <span className="text-xs text-muted-foreground">拖动</span>
          </div>

          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <Label htmlFor={transparencyId} className="text-muted-foreground text-xs">
                透明
              </Label>
              <Slider
                id={transparencyId}
                value={[opacity]}
                onValueChange={handleOpacityChange}
                max={100}
                min={20}
                step={5}
                className="w-16"
                aria-label="透明度"
              />
            </div>

            <div className="flex items-center gap-1.5">
              <Checkbox
                id={autoCloseId}
                checked={autoClose}
                onCheckedChange={(checked) => setAutoClose(checked as boolean)}
                className="w-3.5 h-3.5 data-[state=checked]:bg-pink-500 data-[state=checked]:border-pink-500"
              />
              <Label htmlFor={autoCloseId} className="text-muted-foreground text-xs cursor-pointer">
                5秒关闭
              </Label>
            </div>

            <div className="flex items-center gap-1.5">
              <Checkbox
                id={passThroughId}
                checked={passThrough}
                onCheckedChange={(checked) => setPassThrough(checked as boolean)}
                className="w-3.5 h-3.5 data-[state=checked]:bg-pink-500 data-[state=checked]:border-pink-500"
              />
              <Label htmlFor={passThroughId} className="text-muted-foreground text-xs cursor-pointer">
                穿透
              </Label>
            </div>

            <div className="flex items-center gap-2">
              <Label htmlFor={speedId} className="text-muted-foreground text-xs">
                速度
              </Label>
              <Slider
                id={speedId}
                value={[speed]}
                onValueChange={handleSpeedChange}
                max={100}
                min={10}
                step={5}
                className="w-16"
                aria-label="打字速度"
              />
              <span className="text-muted-foreground w-8">{speed}ms</span>
            </div>

            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2 text-xs text-muted-foreground hover:text-pink-600"
              onClick={handleResetDefaults}
              title="重置默认样式"
            >
              <RotateCcw className="w-3 h-3 mr-1" />
              重置
            </Button>
          </div>

          <div className="flex items-center gap-1 ml-auto">
            <Button
              variant="ghost"
              size="icon"
              className="w-6 h-6 hover:bg-pink-100 hover:text-pink-600"
              onClick={onMinimize}
              aria-label="最小化"
            >
              <Minus className="w-3 h-3" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="w-6 h-6 hover:bg-red-100 hover:text-red-600"
              onClick={onClose}
              aria-label="关闭"
            >
              <X className="w-3 h-3" />
            </Button>
          </div>
        </div>
        )}

        <div className="relative p-6 min-h-[100px]">
          {!textOnly && (
          <>
          <div className="absolute top-0 right-0 p-4 opacity-10" aria-hidden="true">
            <svg width="80" height="80" viewBox="0 0 100 100" fill="none" xmlns="http://www.w3.org/2000/svg">
              <path d="M50 0C50 0 50 50 100 50C50 50 50 100 50 100C50 100 50 50 0 50C50 50 50 0 50 0Z" fill="#EC4899"/>
            </svg>
          </div>

          <div className="absolute bottom-2 left-2 w-1/4 h-1 bg-gradient-to-r from-pink-200 to-transparent rounded-full" aria-hidden="true" />
          </>
          )}

          {isShowingHistory && (
            <div className="mb-2 flex items-center gap-2">
              <span className="text-xs text-muted-foreground">📜 查看历史记录</span>
            </div>
          )}

          <p
            className="leading-relaxed font-medium relative z-10"
            role="status"
            style={{
              fontSize: `${fontSize}px`,
              color: fontColor,
              ...(textOnly ? { textShadow: '0 2px 4px rgba(0,0,0,0.5)' } : {}),
            }}
          >
            {displayText}
            {!isShowingHistory && (isStreaming || isTyping) && (
              <span
                className="inline-block w-2 h-5 ml-1 bg-pink-400 align-middle motion-safe:animate-pulse"
                aria-hidden="true"
              />
            )}
          </p>

          {!textOnly && (
          <>
          <div className="absolute bottom-4 right-6" aria-hidden="true">
            <span className="text-pink-300 text-lg motion-safe:animate-pulse">✦</span>
          </div>
          <div className="absolute bottom-8 right-10" aria-hidden="true">
            <span className="text-pink-200 text-sm">✦</span>
          </div>
          </>
          )}
        </div>

        {!textOnly && (
        <div className="flex items-center gap-2 px-3 py-2 bg-gradient-to-r from-white to-pink-50 border-t border-pink-100">
          <span className="text-pink-400 text-sm" aria-hidden="true">✏️</span>
          <Label htmlFor={inputId} className="sr-only">输入测试字幕</Label>
          <Input
            id={inputId}
            value={inputText}
            onChange={(e) => setInputText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="输入测试字幕..."
            className="flex-1 h-8 text-sm bg-white/70 border-pink-200 focus:border-pink-400 focus:ring-pink-400"
          />
          <Button
            size="sm"
            className="h-8 px-4 bg-gradient-to-r from-pink-500 to-purple-500 hover:from-pink-600 hover:to-purple-600 text-white text-xs"
            onClick={handleSend}
          >
            <Send className="w-3 h-3 mr-1" />
            发送
          </Button>

          <div
            className="w-4 h-4 cursor-se-resize flex items-end justify-end text-pink-300 hover:text-pink-500"
            aria-label="拖动调整大小"
            role="separator"
          >
            <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
              <path d="M9 1L1 9M9 5L5 9M9 9L9 9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
            </svg>
          </div>
        </div>
        )}
      </div>

      {!textOnly && (
      <>
      <div className="absolute -bottom-1 -left-1 w-4 h-4 border-b-2 border-l-2 border-pink-400 rounded-bl-lg" aria-hidden="true" />
      <div className="absolute -top-1 -right-1 w-4 h-4 border-t-2 border-r-2 border-pink-400 rounded-tr-lg" aria-hidden="true" />
      </>
      )}
    </figure>
  );
};

export default GalgameSubtitle;
