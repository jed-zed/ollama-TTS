/**
 * 匀速打字效果 Hook
 * 提供流畅的逐字显示效果，支持流式输入
 * 
 * 使用 requestAnimationFrame 替代 setTimeout，避免后台节流问题
 * 现代浏览器会对后台标签页的 setTimeout 进行节流（最小间隔 1000ms），
 * 但 requestAnimationFrame 在窗口可见时不受此限制
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
  const rafRef = useRef<number | null>(null);
  const lastTimestampRef = useRef<number>(0);
  const lastTextRef = useRef('');
  const prevSourceTextRef = useRef('');
  const charDelayRef = useRef(charDelay);
  const isActiveRef = useRef(false);

  charDelayRef.current = charDelay;

  /**
   * 停止动画循环
   */
  const stopAnimation = useCallback(() => {
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    isActiveRef.current = false;
  }, []);

  /**
   * 重置所有状态（用于外部调用）
   */
  const reset = useCallback(() => {
    stopAnimation();
    bufferRef.current = [];
    lastTextRef.current = '';
    prevSourceTextRef.current = '';
    lastTimestampRef.current = 0;
    setIsTyping(false);
    setDisplayedText('');
  }, [stopAnimation]);

  /**
   * 立即显示全部内容
   */
  const complete = useCallback(() => {
    stopAnimation();
    bufferRef.current = [];
    setIsTyping(false);
    setDisplayedText(lastTextRef.current);
  }, [stopAnimation]);

  /**
   * 使用 requestAnimationFrame 的动画循环
   * 通过时间戳控制字符显示间隔，避免后台节流问题
   */
  const startAnimation = useCallback(() => {
    if (isActiveRef.current) return;
    isActiveRef.current = true;

    const animate = (timestamp: number) => {
      if (!isActiveRef.current) return;

      if (lastTimestampRef.current === 0) {
        lastTimestampRef.current = timestamp;
      }

      const elapsed = timestamp - lastTimestampRef.current;

      if (elapsed >= charDelayRef.current && bufferRef.current.length > 0) {
        setIsTyping(true);
        const char = bufferRef.current.shift()!;
        setDisplayedText(prev => prev + char);
        lastTimestampRef.current = timestamp;
      }

      if (bufferRef.current.length > 0) {
        rafRef.current = requestAnimationFrame(animate);
      } else {
        setIsTyping(false);
        isActiveRef.current = false;
        rafRef.current = null;
      }
    };

    rafRef.current = requestAnimationFrame(animate);
  }, []);

  useEffect(() => {
    const needsReset = sourceText.length < prevSourceTextRef.current.length;
    prevSourceTextRef.current = sourceText;

    if (!sourceText) {
      stopAnimation();
      bufferRef.current = [];
      lastTextRef.current = '';
      lastTimestampRef.current = 0;
      setIsTyping(false);
      setDisplayedText('');
      return;
    }

    if (needsReset) {
      stopAnimation();
      bufferRef.current = [];
      lastTextRef.current = sourceText;
      lastTimestampRef.current = 0;
      setDisplayedText('');
      
      for (const char of sourceText) {
        bufferRef.current.push(char);
      }
      
      startAnimation();
      return;
    }

    const newChars = sourceText.slice(lastTextRef.current.length);
    lastTextRef.current = sourceText;

    if (!newChars) return;

    for (const char of newChars) {
      bufferRef.current.push(char);
    }

    startAnimation();

    return () => {
      stopAnimation();
    };
  }, [sourceText, startAnimation, stopAnimation]);

  useEffect(() => {
    return () => {
      stopAnimation();
    };
  }, [stopAnimation]);

  return {
    displayedText,
    isTyping,
    reset,
    complete,
  };
}

export default useTypewriter;
