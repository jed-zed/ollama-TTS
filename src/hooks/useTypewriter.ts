/**
 * 匀速打字效果 Hook
 * 提供流畅的逐字显示效果，支持流式输入
 */

import { useState, useEffect, useRef, useCallback } from 'react';

interface UseTypewriterOptions {
  /** 每个字符的延迟时间(ms) */
  charDelay?: number;
}

interface UseTypewriterResult {
  /** 当前显示的文本 */
  displayedText: string;
  /** 是否正在打字中 */
  isTyping: boolean;
  /** 重置显示状态 */
  reset: () => void;
  /** 立即显示全部内容 */
  complete: () => void;
}

/**
 * 匀速打字效果 Hook
 * @param sourceText 源文本（可以是流式增量更新的）
 * @param options 配置选项
 */
export function useTypewriter(
  sourceText: string,
  options: UseTypewriterOptions = {}
): UseTypewriterResult {
  const { charDelay = 30 } = options;

  const [displayedText, setDisplayedText] = useState('');
  const [isTyping, setIsTyping] = useState(false);
  const bufferRef = useRef<string[]>([]);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastTextRef = useRef('');
  const prevSourceTextRef = useRef('');
  const charDelayRef = useRef(charDelay);

  charDelayRef.current = charDelay;

  /**
   * 重置所有状态（用于外部调用）
   */
  const reset = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    bufferRef.current = [];
    lastTextRef.current = '';
    prevSourceTextRef.current = '';
    setIsTyping(false);
    setDisplayedText('');
  }, []);

  /**
   * 立即显示全部内容
   */
  const complete = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    bufferRef.current = [];
    setIsTyping(false);
    setDisplayedText(lastTextRef.current);
  }, []);

  useEffect(() => {
    // 检测是否需要重置（新消息开始：源文本变短）
    const needsReset = sourceText.length < prevSourceTextRef.current.length;
    prevSourceTextRef.current = sourceText;

    // 源文本为空时使用 setTimeout 避免同步 setState
    if (!sourceText) {
      const resetTimer = setTimeout(() => {
        if (timerRef.current) {
          clearTimeout(timerRef.current);
          timerRef.current = null;
        }
        bufferRef.current = [];
        lastTextRef.current = '';
        setIsTyping(false);
        setDisplayedText('');
      }, 0);
      return () => clearTimeout(resetTimer);
    }

    // 如果需要重置（新消息开始）- 使用 setTimeout 避免同步 setState
    if (needsReset && lastTextRef.current) {
      const resetTimer = setTimeout(() => {
        if (timerRef.current) {
          clearTimeout(timerRef.current);
          timerRef.current = null;
        }
        bufferRef.current = [];
        lastTextRef.current = sourceText;
        setDisplayedText('');
        
        // 将所有字符添加到缓冲区并开始打字
        for (const char of sourceText) {
          bufferRef.current.push(char);
        }
        
        const typeNextChar = () => {
          if (bufferRef.current.length > 0) {
            setIsTyping(true);
            const char = bufferRef.current.shift()!;
            setDisplayedText(prev => prev + char);
            timerRef.current = setTimeout(typeNextChar, charDelayRef.current);
          } else {
            setIsTyping(false);
            timerRef.current = null;
          }
        };
        
        timerRef.current = setTimeout(typeNextChar, 0);
      }, 0);
      return () => clearTimeout(resetTimer);
    }

    // 计算新增的字符
    const newChars = sourceText.slice(lastTextRef.current.length);
    lastTextRef.current = sourceText;

    if (!newChars) return;

    // 将新字符添加到缓冲区
    for (const char of newChars) {
      bufferRef.current.push(char);
    }

    // 启动打字定时器
    const typeNextChar = () => {
      if (bufferRef.current.length > 0) {
        setIsTyping(true);
        const char = bufferRef.current.shift()!;
        setDisplayedText(prev => prev + char);
        timerRef.current = setTimeout(typeNextChar, charDelayRef.current);
      } else {
        setIsTyping(false);
        timerRef.current = null;
      }
    };

    if (!timerRef.current) {
      timerRef.current = setTimeout(typeNextChar, 0);
    }

    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [sourceText]);

  return {
    displayedText,
    isTyping,
    reset,
    complete,
  };
}

export default useTypewriter;
