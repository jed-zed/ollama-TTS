import React, { useId, useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { Eye, EyeOff, LayoutTemplate, Eraser, Palette, AppWindow, Type, Zap, ScrollText, Loader2, RotateCcw, History } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import { Label } from '@/components/ui/label';
import { Slider } from '@/components/ui/slider';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import GalgameSubtitle from '@/components/GalgameSubtitle';
import { useConfigStore, useConfig, useConfigLoading } from '@/stores/configStore';
import { useStreamStore, useStreamHistory, useIsStreaming, useConnectionStatus, useStreamContent } from '@/stores/streamStore';
import { subtitleWindow, isTauri } from '@/services/tauri';

// 颜色映射
const fontColorMap: Record<string, string> = {
  white: '#ffffff',
  black: '#000000',
  pink: '#ec4899',
  yellow: '#eab308',
  cyan: '#06b6d4',
  gold: '#fbbf24',
  slate: '#1e293b',
};

const bgColorMap: Record<string, string> = {
  glass: 'rgba(255,255,255,0.1)',
  white: '#ffffff',
  dark: '#1e1e1e',
  transparent: 'transparent',
  pink: 'linear-gradient(135deg, #fce7f3, #f5d0fe)',
  darkGold: 'rgba(0,0,0,0.8)',
  aurora: 'linear-gradient(135deg, rgba(15, 23, 42, 0.85), rgba(20, 40, 60, 0.9), rgba(10, 50, 60, 0.85))',
  acrylic: 'rgba(255,255,255,0.85)',
};

const DEFAULT_SUBTITLE_CONFIG = {
  subtitle_font_color: '#ec4899',
  subtitle_bg_color: 'linear-gradient(135deg, #fce7f3, #f5d0fe)',
  subtitle_opacity: 0.9,
  subtitle_font_size: 18,
  char_delay: 30,
};

const Subtitles = () => {
  // 生成唯一 ID 用于无障碍标签关联
  const mousePassId = useId();
  const autoShowId = useId();
  const opacityId = useId();
  const fontSizeId = useId();
  const fontColorId = useId();
  const bgColorId = useId();
  const speedId = useId();
  const autoCloseId = useId();

  // 从 store 获取状态
  const config = useConfig();
  const isLoading = useConfigLoading();
  const loadConfig = useConfigStore((state) => state.loadConfig);
  const updateConfig = useConfigStore((state) => state.updateConfig);

  // Stream store
  const history = useStreamHistory();
  const isStreaming = useIsStreaming();
  const connectionStatus = useConnectionStatus();
  const currentContent = useStreamContent();
  const currentSpeaker = useStreamStore((state) => state.currentSpeaker);
  const currentEmotion = useStreamStore((state) => state.currentEmotion);
  const clearHistory = useStreamStore((state) => state.clearHistory);
  const loadHistory = useStreamStore((state) => state.loadHistory);
  const addToHistory = useStreamStore((state) => state.addToHistory);
  const showSubtitle = useStreamStore((state) => state.showSubtitle);
  const hideSubtitle = useStreamStore((state) => state.hideSubtitle);
  const setContent = useStreamStore((state) => state.setContent);

  // 本地 UI 状态
  const [autoCloseDelay, setAutoCloseDelay] = useState([5]);
  const [previewMode, setPreviewMode] = useState('subtitle');
  const [mousePassThrough, setMousePassThrough] = useState(false);
  const [autoShow, setAutoShow] = useState(true);
  const [autoClose, setAutoClose] = useState(true);
  const [selectedHistoryId, setSelectedHistoryId] = useState<string>('');
  
  const prevIsStreamingRef = useRef(isStreaming);
  const prevContentRef = useRef(currentContent);

  // 初始化 - 只在组件挂载时执行一次
  useEffect(() => {
    loadConfig();
    loadHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 监听流式结束，自动保存历史记录
  useEffect(() => {
    if (prevIsStreamingRef.current && !isStreaming && prevContentRef.current && prevContentRef.current.length > 10) {
      addToHistory({
        id: `msg_${Date.now()}`,
        content: prevContentRef.current,
        speaker: currentSpeaker ?? undefined,
        emotion: currentEmotion ?? undefined,
        timestamp: Date.now(),
      });
    }
    prevIsStreamingRef.current = isStreaming;
    prevContentRef.current = currentContent;
  }, [isStreaming, currentContent, currentSpeaker, currentEmotion, addToHistory]);

  // 同步鼠标穿透到 Tauri 窗口
  useEffect(() => {
    if (isTauri()) {
      subtitleWindow.setClickThrough(mousePassThrough);
    }
  }, [mousePassThrough]);

  // 处理历史记录选择
  const handleHistorySelect = useCallback((historyId: string) => {
    setSelectedHistoryId(historyId);
    if (historyId) {
      const selectedItem = history.find(item => item.id === historyId);
      if (selectedItem) {
        setContent(selectedItem.content);
        showSubtitle();
      }
    }
  }, [history, setContent, showSubtitle]);

  // 从配置派生颜色选择状态 - 使用 useMemo 避免无限循环
  const selectedFontColor = useMemo(() => {
    const entry = Object.entries(fontColorMap).find(
      ([, hex]) => hex === config?.subtitle_font_color
    );
    return entry?.[0] ?? 'white';
  }, [config?.subtitle_font_color]);

  const selectedBgColor = useMemo(() => {
    const entry = Object.entries(bgColorMap).find(
      ([, value]) => value === config?.subtitle_bg_color
    );
    return entry?.[0] ?? 'glass';
  }, [config?.subtitle_bg_color]);

  // 处理字体颜色变化
  const handleFontColorChange = useCallback((value: string) => {
    const hexColor = fontColorMap[value] || '#ffffff';
    updateConfig('subtitle_font_color', hexColor);
  }, [updateConfig]);

  // 处理背景颜色变化
  const handleBgColorChange = useCallback((value: string) => {
    const bgValue = bgColorMap[value] || 'rgba(255,255,255,0.1)';
    updateConfig('subtitle_bg_color', bgValue);
  }, [updateConfig]);

  // 处理不透明度变化
  const handleOpacityChange = useCallback((value: number[]) => {
    updateConfig('subtitle_opacity', value[0] / 100);
  }, [updateConfig]);

  // 处理字体大小变化
  const handleFontSizeChange = useCallback((value: number[]) => {
    updateConfig('subtitle_font_size', value[0]);
  }, [updateConfig]);

  // 处理打字速度变化
  const handleSpeedChange = useCallback((value: number[]) => {
    updateConfig('char_delay', value[0]);
  }, [updateConfig]);

  // 清空内容
  const handleClearContent = async () => {
    await clearHistory();
  };

  // 重置默认样式
  const handleResetDefaults = useCallback(() => {
    updateConfig('subtitle_opacity', DEFAULT_SUBTITLE_CONFIG.subtitle_opacity);
    updateConfig('subtitle_font_size', DEFAULT_SUBTITLE_CONFIG.subtitle_font_size);
    updateConfig('char_delay', DEFAULT_SUBTITLE_CONFIG.char_delay);
    updateConfig('subtitle_font_color', DEFAULT_SUBTITLE_CONFIG.subtitle_font_color);
    updateConfig('subtitle_bg_color', DEFAULT_SUBTITLE_CONFIG.subtitle_bg_color);
  }, [updateConfig]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-8 h-8 animate-spin text-primary" />
        <span className="ml-2 text-muted-foreground">加载配置中...</span>
      </div>
    );
  }

  const opacity = Math.round((config?.subtitle_opacity ?? 0.9) * 100);
  const fontSize = config?.subtitle_font_size ?? 18;
  const speed = config?.char_delay ?? 30;

  return (
    <div className="h-full flex flex-col gap-6 motion-safe:animate-in motion-safe:fade-in motion-safe:slide-in-from-bottom-4 motion-safe:duration-500">
      <div>
        <h2 className="text-3xl font-bold bg-gradient-to-r from-pink-500 to-purple-600 bg-clip-text text-transparent">字幕视觉</h2>
        <p className="text-muted-foreground mt-2">自定义悬浮窗外观与行为，打造沉浸式阅读体验。</p>
      </div>

      <div className="grid grid-cols-12 gap-6 flex-1 min-h-0">
        {/* Controls Section */}
        <div className="col-span-4 flex flex-col gap-6 overflow-auto pr-2">
            {/* 窗口控制 */}
            <Card className="glass border-none shadow-custom">
              <CardHeader className="pb-3">
                <CardTitle className="text-lg flex items-center gap-2">
                  <AppWindow className="w-4 h-4 text-pink-500" aria-hidden="true" />
                  窗口控制
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 gap-3" role="group" aria-label="字幕窗口控制按钮">
                   <Button
                    variant="outline"
                    className="w-full shadow-sm hover:bg-pink-50 hover:text-pink-600 hover:border-pink-200"
                    onClick={showSubtitle}
                  >
                    <Eye className="w-4 h-4 mr-2" aria-hidden="true" /> 显示字幕
                  </Button>
                   <Button
                     variant="outline"
                     className="w-full text-muted-foreground hover:bg-slate-100"
                     onClick={hideSubtitle}
                   >
                     <EyeOff className="w-4 h-4 mr-2" aria-hidden="true" /> 隐藏字幕
                   </Button>
                   <Button
                     variant="outline"
                     className="w-full text-muted-foreground"
                     aria-pressed="false"
                   >
                     <LayoutTemplate className="w-4 h-4 mr-2" aria-hidden="true" /> 显示控件
                   </Button>
                   <Button
                     variant="outline"
                     className="w-full text-muted-foreground hover:text-destructive hover:bg-red-50 hover:border-red-200"
                     onClick={handleClearContent}
                   >
                     <Eraser className="w-4 h-4 mr-2" aria-hidden="true" /> 清空内容
                   </Button>
                </div>

                {/* 历史记录 */}
                <div className="pt-3 border-t">
                  <Label className="flex items-center gap-2 mb-2 text-sm">
                    <History className="w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" />
                    历史记录
                  </Label>
                  <Select value={selectedHistoryId} onValueChange={handleHistorySelect}>
                    <SelectTrigger className="w-full">
                      <SelectValue placeholder={history.length > 0 ? "选择历史记录查看" : "暂无历史记录"} />
                    </SelectTrigger>
                    <SelectContent>
                      {history.length === 0 ? (
                        <SelectItem value="_empty" disabled>暂无历史记录</SelectItem>
                      ) : (
                        history.map((item, index) => (
                          <SelectItem key={item.id} value={item.id}>
                            <div className="flex items-center gap-2">
                              <span className="text-xs text-muted-foreground">#{history.length - index}</span>
                              <span className="truncate max-w-[180px]">
                                {item.content.slice(0, 30)}{item.content.length > 30 ? '...' : ''}
                              </span>
                            </div>
                          </SelectItem>
                        ))
                      )}
                    </SelectContent>
                  </Select>
                  {history.length > 0 && (
                    <p className="text-xs text-muted-foreground mt-1">
                      共 {history.length} 条记录，选择后可在字幕窗口中查看
                    </p>
                  )}
                </div>
              </CardContent>
            </Card>

            {/* 字幕设置 */}
            <Card className="glass border-none shadow-custom flex-1">
              <CardHeader className="pb-3">
                <div className="flex items-center justify-between">
                  <CardTitle className="text-lg flex items-center gap-2">
                    <Palette className="w-4 h-4 text-purple-500" aria-hidden="true" />
                    字幕设置
                  </CardTitle>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-xs text-muted-foreground hover:text-pink-600"
                    onClick={handleResetDefaults}
                  >
                    <RotateCcw className="w-3 h-3 mr-1" />
                    重置默认
                  </Button>
                </div>
              </CardHeader>
              <CardContent className="space-y-5">
                 {/* 字体颜色 */}
                 <div className="space-y-2">
                    <Label htmlFor={fontColorId} className="flex items-center gap-2">
                      <Type className="w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" />
                      字体颜色
                    </Label>
                    <Select value={selectedFontColor} onValueChange={handleFontColorChange}>
                      <SelectTrigger id={fontColorId}>
                        <SelectValue placeholder="选择颜色" />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="white">⚪ 白色</SelectItem>
                        <SelectItem value="black">⚫ 黑色</SelectItem>
                        <SelectItem value="pink">🩷 粉色</SelectItem>
                        <SelectItem value="yellow">🟡 黄色</SelectItem>
                        <SelectItem value="cyan">🩵 青色</SelectItem>
                        <SelectItem value="gold">🟠 金色</SelectItem>
                        <SelectItem value="slate">🔵 深蓝灰</SelectItem>
                      </SelectContent>
                    </Select>
                 </div>

                 {/* 背景颜色 */}
                 <div className="space-y-2">
                    <Label htmlFor={bgColorId} className="flex items-center gap-2">
                      <Palette className="w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" />
                      背景颜色
                    </Label>
                    <Select value={selectedBgColor} onValueChange={handleBgColorChange}>
                      <SelectTrigger id={bgColorId}>
                        <SelectValue placeholder="选择颜色" />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="glass">🪟 磨砂玻璃</SelectItem>
                        <SelectItem value="white">⚪ 白色</SelectItem>
                        <SelectItem value="dark">⚫ 深色</SelectItem>
                        <SelectItem value="transparent">🔍 透明</SelectItem>
                        <SelectItem value="pink">🩷 粉色渐变</SelectItem>
                        <SelectItem value="darkGold">🟤 深邃黑金</SelectItem>
                        <SelectItem value="aurora">🌌 科技极光</SelectItem>
                        <SelectItem value="acrylic">💎 亚克力白</SelectItem>
                      </SelectContent>
                    </Select>
                 </div>

                 {/* 不透明度 */}
                 <div className="space-y-2">
                    <div className="flex justify-between items-center">
                      <Label htmlFor={opacityId}>不透明度</Label>
                      <span className="text-xs text-primary font-medium">{opacity}%</span>
                    </div>
                    <Slider
                      id={opacityId}
                      value={[opacity]}
                      onValueChange={handleOpacityChange}
                      max={100}
                      min={0}
                      step={5}
                      className="py-2"
                      aria-label="不透明度"
                    />
                 </div>

                 {/* 字体大小 */}
                 <div className="space-y-2">
                    <div className="flex justify-between items-center">
                      <Label htmlFor={fontSizeId}>字体大小</Label>
                      <span className="text-xs text-primary font-medium">{fontSize}px</span>
                    </div>
                    <Slider
                      id={fontSizeId}
                      value={[fontSize]}
                      onValueChange={handleFontSizeChange}
                      min={12}
                      max={36}
                      step={1}
                      className="py-2"
                      aria-label="字体大小"
                    />
                 </div>

                 {/* 打字速度 */}
                 <div className="space-y-2">
                    <div className="flex justify-between items-center">
                      <Label htmlFor={speedId} className="flex items-center gap-2">
                        <Zap className="w-3.5 h-3.5 text-muted-foreground" aria-hidden="true" />
                        打字速度
                      </Label>
                      <span className="text-xs text-primary font-medium">{speed}ms</span>
                    </div>
                    <Slider
                      id={speedId}
                      value={[speed]}
                      onValueChange={handleSpeedChange}
                      min={10}
                      max={100}
                      step={5}
                      className="py-2"
                      aria-label="打字速度"
                    />
                 </div>

                 {/* 自动关闭延迟 */}
                 <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                       <Label htmlFor={autoCloseId} className="text-sm cursor-pointer">完成后自动关闭</Label>
                       <p className="text-xs text-muted-foreground" id={`${autoCloseId}-desc`}>{autoCloseDelay[0]} 秒后隐藏</p>
                    </div>
                    <div className="flex items-center gap-2">
                      <Slider
                        value={autoCloseDelay}
                        onValueChange={setAutoCloseDelay}
                        min={1}
                        max={30}
                        step={1}
                        className="w-20"
                        aria-label="自动关闭延迟"
                      />
                      <Switch
                        id={autoCloseId}
                        checked={autoClose}
                        onCheckedChange={setAutoClose}
                        aria-describedby={`${autoCloseId}-desc`}
                      />
                    </div>
                 </div>

                 {/* 鼠标穿透 */}
                 <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                       <Label htmlFor={mousePassId} className="text-sm cursor-pointer">鼠标穿透</Label>
                       <p className="text-xs text-muted-foreground" id={`${mousePassId}-desc`}>点击穿透到下层窗口</p>
                    </div>
                    <Switch
                      id={mousePassId}
                      checked={mousePassThrough}
                      onCheckedChange={setMousePassThrough}
                      aria-describedby={`${mousePassId}-desc`}
                    />
                 </div>

                 {/* 自动显示 */}
                 <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                       <Label htmlFor={autoShowId} className="text-sm cursor-pointer">自动显示</Label>
                       <p className="text-xs text-muted-foreground" id={`${autoShowId}-desc`}>有新内容时自动弹出</p>
                    </div>
                    <Switch
                      id={autoShowId}
                      checked={autoShow}
                      onCheckedChange={setAutoShow}
                      aria-describedby={`${autoShowId}-desc`}
                    />
                 </div>
              </CardContent>
            </Card>
        </div>

        {/* Preview Section */}
        <div className="col-span-8 flex flex-col gap-6">
            <Card className="flex-1 glass border-none shadow-custom relative overflow-hidden flex flex-col">
               <div className="absolute inset-0 bg-gradient-to-br from-blue-50 to-pink-50 opacity-50 z-0" aria-hidden="true" />
               {/* Background decoration */}
               <div
                 className="absolute inset-0 z-0 opacity-20"
                 style={{
                   background: 'radial-gradient(ellipse at top right, rgba(147, 51, 234, 0.3), transparent 60%), radial-gradient(ellipse at bottom left, rgba(59, 130, 246, 0.3), transparent 60%)',
                 }}
                 aria-hidden="true"
               />

               <Tabs value={previewMode} onValueChange={setPreviewMode} className="flex-1 flex flex-col">
               <CardHeader className="relative z-10 border-b border-white/20 bg-white/30 backdrop-blur-sm">
                 <div className="flex justify-between items-center">
                    <CardTitle>实时预览</CardTitle>
                    <TabsList className="grid w-[240px] grid-cols-2 h-8">
                      <TabsTrigger value="subtitle" className="text-xs">📜 字幕页面</TabsTrigger>
                      <TabsTrigger value="stream" className="text-xs">💬 流式窗口</TabsTrigger>
                    </TabsList>
                 </div>
               </CardHeader>

               <CardContent className="relative z-10 flex-1 flex flex-col">
                    {/* 字幕页面模式 */}
                    <TabsContent value="subtitle" className="flex-1 flex flex-col items-center justify-end pb-8 px-8 mt-0">
                      <div className="w-full text-center mb-6 text-muted-foreground text-sm font-medium bg-white/40 px-3 py-1 rounded-full inline-block backdrop-blur-sm">
                        字幕页面 - Galgame 风格悬浮字幕
                      </div>
                      <GalgameSubtitle />
                    </TabsContent>

                    {/* 流式窗口模式 */}
                    <TabsContent value="stream" className="flex-1 flex flex-col mt-0">
                      <div className="w-full text-center mb-4 text-muted-foreground text-sm font-medium bg-white/40 px-3 py-1 rounded-full inline-block backdrop-blur-sm">
                        流式窗口 - 实时 AI 回复显示
                      </div>

                      {/* 流式窗口 */}
                      <div className="flex-1 bg-white/80 backdrop-blur-xl rounded-xl border border-pink-100 shadow-lg overflow-hidden flex flex-col">
                        {/* 顶部控制栏 */}
                        <div className="px-4 py-2 bg-gradient-to-r from-pink-50 to-purple-50 border-b border-pink-100 flex items-center justify-between">
                          <div className="flex items-center gap-2">
                            <span className="text-sm font-medium text-slate-700">AI 回复</span>
                            <span className="flex items-center gap-1 text-xs text-muted-foreground">
                              <span className={`w-2 h-2 rounded-full ${
                                connectionStatus === 'connected'
                                  ? 'bg-green-500 motion-safe:animate-pulse'
                                  : connectionStatus === 'connecting'
                                    ? 'bg-yellow-500 motion-safe:animate-pulse'
                                    : 'bg-gray-400'
                              }`} />
                              {connectionStatus === 'connected' ? '已连接' : connectionStatus === 'connecting' ? '连接中' : '未连接'}
                            </span>
                          </div>
                          <div className="flex items-center gap-2">
                            <Select value={selectedHistoryId} onValueChange={handleHistorySelect}>
                              <SelectTrigger className="h-7 w-[140px] text-xs">
                                <ScrollText className="w-3.5 h-3.5 mr-1" aria-hidden="true" />
                                <SelectValue placeholder={history.length > 0 ? `历史(${history.length})` : "暂无历史记录"} />
                              </SelectTrigger>
                              <SelectContent>
                                {history.length === 0 ? (
                                  <SelectItem value="_empty" disabled>暂无历史记录</SelectItem>
                                ) : (
                                  history.map((item, index) => (
                                    <SelectItem key={item.id} value={item.id}>
                                      <div className="flex items-center gap-2">
                                        <span className="text-xs text-muted-foreground">#{history.length - index}</span>
                                        <span className="truncate max-w-[120px]">
                                          {item.content.slice(0, 20)}{item.content.length > 20 ? '...' : ''}
                                        </span>
                                      </div>
                                    </SelectItem>
                                  ))
                                )}
                              </SelectContent>
                            </Select>
                            <Button
                              variant="ghost"
                              size="sm"
                              className="h-7 px-2 text-xs text-muted-foreground hover:text-destructive hover:bg-red-50"
                              onClick={handleClearContent}
                            >
                              <Eraser className="w-3.5 h-3.5 mr-1" aria-hidden="true" />
                              清空
                            </Button>
                          </div>
                        </div>

                        {/* AI 回复内容区域 - 只显示当前内容 */}
                        <div className="flex-1 p-6 overflow-auto" role="log" aria-live="polite">
                          <div className="space-y-4">
                            {!currentContent && !isStreaming ? (
                              <div className="text-center text-muted-foreground py-8">
                                暂无消息，等待 AI 回复...
                              </div>
                            ) : (
                              <div className="p-4 bg-white rounded-xl border-2 border-pink-200 shadow-sm">
                                <p className="text-slate-700 leading-relaxed">
                                  {currentContent}
                                  {isStreaming && (
                                    <span className="inline-block w-2 h-5 ml-1 bg-pink-400 align-middle motion-safe:animate-pulse" aria-hidden="true" />
                                  )}
                                </p>
                                {currentSpeaker && (
                                  <div className="flex items-center justify-between mt-3 pt-2 border-t border-pink-100/50">
                                    <span className="text-xs text-muted-foreground">
                                      {currentSpeaker} {currentEmotion && `· ${currentEmotion}`}
                                    </span>
                                  </div>
                                )}
                              </div>
                            )}
                          </div>
                        </div>

                        {/* 底部状态栏 */}
                        <div className="px-4 py-2 bg-slate-50 border-t border-pink-100 flex items-center justify-between text-xs text-muted-foreground">
                          <span>共 {history.length} 条历史记录</span>
                          <span>{isStreaming ? '正在输出...' : '等待中'}</span>
                        </div>
                      </div>
                    </TabsContent>
               </CardContent>
               </Tabs>
            </Card>
        </div>
      </div>
    </div>
  );
};

export default Subtitles;
