import { useState, useEffect, useRef, useCallback } from 'react';
import {
  Network,
  Copy,
  Volume2,
  Settings2,
  Mic,
  MicOff,
  Send,
  Trash2,
  Save,
  Eye,
  EyeOff,
  Plus,
  Check,
  Loader2,
  Zap
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import { Switch } from '@/components/ui/switch';
import { Slider } from '@/components/ui/slider';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Badge } from '@/components/ui/badge';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { useConfigStore, useConfig, useConfigLoading } from '@/stores/configStore';
import { useStreamStore, useStreamHistory, useIsStreaming, useStreamContent, useConnectionStatus } from '@/stores/streamStore';
import { streamChat } from '@/services/stream';
import { checkBackendHealth } from '@/services/api';
import { HEALTH_CHECK_INTERVAL_MS, API_BASE_URL } from '@/lib/constants';
import type { ChatMessage, PresetApiConfig } from '@/services/types';
import { speechService } from '@/services/speech';
import { BUILTIN_PRESETS } from '@/components/PresetManager';

/**
 * 服务连接页面（首页）
 * SSE 连接已在 AppLayout 层级统一管理
 */
const Index = () => {
  const [showApiKey, setShowApiKey] = useState(false);
  const [copied, setCopied] = useState(false);
  const [inputMessage, setInputMessage] = useState('');
  const [isSending, setIsSending] = useState(false);
  const [backendConnected, setBackendConnected] = useState(false);
  const [isVoiceRecording, setIsVoiceRecording] = useState(false);
  const [isVoiceRecognizing, setIsVoiceRecognizing] = useState(false);
  const chatEndRef = useRef<HTMLDivElement>(null);

  const config = useConfig();
  const isConfigLoading = useConfigLoading();
  const updateConfig = useConfigStore((state) => state.updateConfig);
  const saveConfig = useConfigStore((state) => state.saveConfig);

  const history = useStreamHistory();
  const isStreaming = useIsStreaming();
  const currentContent = useStreamContent();
  const connectionStatus = useConnectionStatus();
  const clearHistory = useStreamStore((state) => state.clearHistory);
  const loadHistory = useStreamStore((state) => state.loadHistory);
  const appendContent = useStreamStore((state) => state.appendContent);
  const clearCurrent = useStreamStore((state) => state.clearCurrent);
  const finishStream = useStreamStore((state) => state.finishStream);

  // 只加载历史记录和检查后端健康状态，SSE 连接在 AppLayout 统一管理
  useEffect(() => {
    loadHistory();

    const checkHealth = async () => {
      const result = await checkBackendHealth();
      setBackendConnected(result.connected);
    };
    checkHealth();
    const healthInterval = setInterval(checkHealth, HEALTH_CHECK_INTERVAL_MS);

    return () => {
      clearInterval(healthInterval);
    };
  }, [loadHistory]);

  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [history, currentContent]);

  const handleCopyWithPreview = () => {
    const textToCopy = (config?.model_name || 'deepseek-ai/DeepSeek-V3') + '-preview';
    navigator.clipboard.writeText(textToCopy);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const resolvePresetConfig = useCallback((presetId: string): PresetApiConfig | null => {
    if (!presetId) return null;
    if (presetId === 'custom') {
      const apiUrl = config?.custom_api_url || '';
      const apiKey = config?.custom_api_key || '';
      const modelName = config?.custom_model_name || '';
      if (!apiUrl && !modelName) return null;
      return { api_url: apiUrl, api_key: apiKey, model_name: modelName };
    }
    const savedConfig = config?.preset_configs?.[presetId];
    const builtinPreset = BUILTIN_PRESETS[presetId];
    const fallbackUrl = builtinPreset?.url || '';
    const fallbackModel = builtinPreset?.defaultModel || '';
    const apiUrl = savedConfig?.api_url || fallbackUrl;
    const apiKey = savedConfig?.api_key || '';
    const modelName = savedConfig?.model_name || fallbackModel;
    if (!apiUrl && !modelName) return null;
    return { api_url: apiUrl, api_key: apiKey, model_name: modelName };
  }, [config]);

  interface ModelSelection {
    model: string;
    apiOverride?: { api_url: string; api_key: string };
  }

  const FALLBACK_TRIGGER_PREFIX = 'SYSTEM:\n';

  const selectModel = useCallback((userMessage: string): ModelSelection => {
    const defaultModel = config?.model_name || 'deepseek-ai/DeepSeek-V3';
    const defaultResult: ModelSelection = { model: defaultModel };

    if (!config?.fallback_model_enabled || !config?.fallback_preset_id) {
      return defaultResult;
    }

    if (!userMessage.startsWith(FALLBACK_TRIGGER_PREFIX)) {
      return defaultResult;
    }

    const presetConfig = resolvePresetConfig(config.fallback_preset_id);
    if (!presetConfig || !presetConfig.model_name) {
      console.warn('[模型切换] 辅助预设配置无效，回退到主模型');
      return defaultResult;
    }

    const presetLabel = BUILTIN_PRESETS[config.fallback_preset_id]?.label
      || config.user_presets?.[config.fallback_preset_id]?.name
      || config.fallback_preset_id;

    console.log(
      `[模型切换] 用户消息匹配辅助模型触发前缀`,
      `→ 切换至辅助预设: ${presetLabel} (${presetConfig.model_name})`
    );

    return {
      model: presetConfig.model_name,
      apiOverride: presetConfig.api_url !== config.api_url
        ? { api_url: presetConfig.api_url, api_key: presetConfig.api_key }
        : undefined,
    };
  }, [config, resolvePresetConfig]);

  const handleSendMessage = useCallback(async () => {
    if (!inputMessage.trim() || isSending) return;

    const userMessage = inputMessage.trim();
    setInputMessage('');
    setIsSending(true);
    clearCurrent();

    const messages: ChatMessage[] = [
      ...(config?.system_prompt_injection ? [{ role: 'system' as const, content: config.system_prompt_injection }] : []),
      ...history.map(msg => ({ role: 'assistant' as const, content: msg.content })),
      { role: 'user' as const, content: userMessage }
    ];

    const { model: selectedModel, apiOverride } = selectModel(userMessage);

    try {
      await streamChat(
        {
          messages,
          model: selectedModel,
          stream: true,
          ...(apiOverride ? { api_url: apiOverride.api_url, api_key: apiOverride.api_key } : {}),
        },
        {
          onChunk: (event) => {
            if (event.type === 'content' && event.data.content) {
              appendContent(event.data.content);
            }
          },
          onError: (error) => {
            console.error('Chat error:', error);
            setIsSending(false);
          },
          onComplete: () => {
            finishStream();
            setIsSending(false);
          }
        }
      );
    } catch (error) {
      console.error('Send message failed:', error);
      setIsSending(false);
    }
  }, [inputMessage, isSending, config, history, clearCurrent, appendContent, finishStream, selectModel]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSendMessage();
    }
  };

  const handleMicToggle = useCallback(async () => {
    if (isVoiceRecognizing) return;

    if (!isVoiceRecording) {
      const success = await speechService.startRecording();
      if (success) {
        setIsVoiceRecording(true);
      }
    } else {
      setIsVoiceRecording(false);
      setIsVoiceRecognizing(true);

      try {
        const audioBlob = await speechService.stopRecording();
        if (!audioBlob) {
          setIsVoiceRecognizing(false);
          return;
        }

        const cfgResp = await fetch(`${API_BASE_URL}/api/config`);
        if (!cfgResp.ok) {
          setIsVoiceRecognizing(false);
          return;
        }
        const cfgResult = await cfgResp.json();
        const cfgData = cfgResult.data || cfgResult;
        if (!cfgData.baidu_api_key || !cfgData.baidu_secret_key) {
          setIsVoiceRecognizing(false);
          return;
        }

        const result = await speechService.recognize(audioBlob, {
          apiKey: cfgData.baidu_api_key,
          secretKey: cfgData.baidu_secret_key,
        });

        if (result.success && result.text) {
          setInputMessage(prev => prev + result.text);
        }
      } catch {
        console.error('语音识别失败');
      } finally {
        setIsVoiceRecognizing(false);
      }
    }
  }, [isVoiceRecording, isVoiceRecognizing]);

  const handleClearChat = async () => {
    await clearHistory();
  };

  const handleSaveConfig = async () => {
    await saveConfig();
  };

  if (isConfigLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-8 h-8 animate-spin text-primary" />
        <span className="ml-2 text-muted-foreground">加载配置中...</span>
      </div>
    );
  }

  const modelName = config?.model_name || 'deepseek-ai/DeepSeek-V3';
  const ttsEnabled = config?.tts_enabled ?? true;
  const autoPlay = config?.tts_auto_play ?? true;
  const volume = Math.round((config?.tts_volume ?? 0.8) * 100);

  return (
    <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div className="flex justify-between items-center">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-xl bg-gradient-to-tr from-pink-500 to-purple-500 shadow-lg">
            <Settings2 className="text-white w-5 h-5" />
          </div>
          <h2 className="text-2xl font-bold bg-gradient-to-r from-pink-500 to-purple-600 bg-clip-text text-transparent">
            控制面板
          </h2>
        </div>
        <div className="flex gap-3">
          <Badge variant="outline" className={`px-3 py-1.5 ${backendConnected ? 'border-green-300 bg-green-50 text-green-600' : 'border-red-300 bg-red-50 text-red-600'}`}>
            <div className={`w-2 h-2 rounded-full mr-2 ${backendConnected ? 'bg-green-500 animate-pulse' : 'bg-red-500'}`} />
            {backendConnected ? 'API 已连接' : 'API 未连接'}
          </Badge>
          <Badge variant="outline" className={`px-3 py-1.5 ${ttsEnabled ? 'border-purple-300 bg-purple-50 text-purple-600' : 'border-gray-300 bg-gray-50 text-gray-600'}`}>
            <div className={`w-2 h-2 rounded-full mr-2 ${ttsEnabled ? 'bg-purple-500' : 'bg-gray-400'}`} />
            TTS {ttsEnabled ? '已启用' : '已禁用'}
          </Badge>
          <Badge variant="outline" className="px-3 py-1.5 border-amber-300 bg-amber-50 text-amber-600">
            <div className="w-2 h-2 rounded-full bg-amber-500 mr-2" />
            角色: {config?.gsv_current_character || '默认'}
          </Badge>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-6">
        <Card className="glass border-none shadow-custom">
          <CardHeader className="pb-4">
            <div className="flex items-center gap-3">
              <div className="p-2 rounded-lg bg-blue-100 text-blue-600">
                <Network className="h-5 w-5" />
              </div>
              <div>
                <CardTitle>API 配置</CardTitle>
                <CardDescription>设置 Ollama 的本地或远程接口地址</CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label>服务商预设</Label>
              <Select 
                value={config?.current_api_preset || 'custom'}
                onValueChange={(value) => updateConfig('current_api_preset', value)}
              >
                <SelectTrigger>
                  <SelectValue placeholder="选择预设" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="custom">自定义 (Custom)</SelectItem>
                  <SelectItem value="deepseek">🚀 DeepSeek (硅基流动)</SelectItem>
                  <SelectItem value="local">本地 Ollama</SelectItem>
                  <SelectItem value="openai">OpenAI</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-2">
              <Label>API URL</Label>
              <Input 
                value={config?.api_url || ''} 
                onChange={(e) => updateConfig('api_url', e.target.value)}
                className="bg-white/50" 
              />
            </div>

            <div className="space-y-2">
              <Label>API Key</Label>
              <div className="relative">
                <Input
                  type={showApiKey ? "text" : "password"}
                  value={config?.api_key || ''}
                  onChange={(e) => updateConfig('api_key', e.target.value)}
                  className="bg-white/50 pr-10"
                />
                <button
                  onClick={() => setShowApiKey(!showApiKey)}
                  className="absolute right-3 top-2.5 text-muted-foreground hover:text-foreground transition-colors"
                >
                  {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                </button>
              </div>
            </div>

            <div className="space-y-2">
              <Label>模型名称</Label>
              <div className="flex gap-2">
                <Input
                  value={config?.model_name || ''}
                  onChange={(e) => updateConfig('model_name', e.target.value)}
                  className="bg-white/50"
                />
                <Button
                  size="icon"
                  variant="outline"
                  className="shrink-0 hover:bg-primary/10 hover:text-primary hover:border-primary/30"
                  onClick={handleCopyWithPreview}
                  title="复制模型名称并添加 -preview 后缀"
                >
                  {copied ? <Check className="h-4 w-4 text-green-500" /> : <Copy className="h-4 w-4" />}
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                加 -preview: <span className="text-primary cursor-pointer hover:underline" onClick={handleCopyWithPreview}>{modelName}-preview</span>
              </p>
            </div>
          </CardContent>
        </Card>

        <Card className="glass border-none shadow-custom col-span-2">
          <CardHeader className="pb-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="p-2 rounded-lg bg-emerald-100 text-emerald-600">
                  <Zap className="h-5 w-5" />
                </div>
                <div>
                  <CardTitle>辅助模型</CardTitle>
                  <CardDescription>简单场景自动切换到低成本模型</CardDescription>
                </div>
              </div>
              <Switch
                checked={config?.fallback_model_enabled || false}
                onCheckedChange={(checked) => updateConfig('fallback_model_enabled', checked)}
                className="data-[state=checked]:bg-emerald-500"
              />
            </div>
          </CardHeader>
          {config?.fallback_model_enabled && (
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label>辅助预设</Label>
                <Select
                  value={config?.fallback_preset_id || ''}
                  onValueChange={(value) => updateConfig('fallback_preset_id', value)}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="选择一个预设作为辅助模型" />
                  </SelectTrigger>
                  <SelectContent>
                    {Object.entries(BUILTIN_PRESETS)
                      .filter(([id]) => id !== config?.current_api_preset)
                      .map(([id, preset]) => (
                        <SelectItem key={id} value={id}>
                          {preset.emoji} {preset.label}
                          {config?.preset_configs?.[id]?.model_name
                            ? ` (${config.preset_configs[id].model_name})`
                            : preset.defaultModel ? ` (${preset.defaultModel})` : ''}
                        </SelectItem>
                      ))}
                    {Object.entries(config?.user_presets || {})
                      .filter(([id]) => id !== config?.current_api_preset)
                      .map(([id, meta]) => (
                        <SelectItem key={id} value={id}>
                          {meta.emoji || '⭐'} {meta.name}
                          {config?.preset_configs?.[id]?.model_name
                            ? ` (${config.preset_configs[id].model_name})`
                            : ''}
                        </SelectItem>
                      ))}
                  </SelectContent>
                </Select>
                <p className="text-xs text-muted-foreground">
                  从已保存的预设中选择，将使用该预设的 API 地址、密钥和模型
                </p>
              </div>

              <div className="space-y-2">
                <div className="flex justify-between items-center">
                  <Label>系统提示词阈值</Label>
                  <span className="text-sm font-medium text-emerald-600">
                    {config?.fallback_trigger_threshold ?? 100} 字符
                  </span>
                </div>
                <Slider
                  value={[config?.fallback_trigger_threshold ?? 100]}
                  onValueChange={([value]) => updateConfig('fallback_trigger_threshold', value)}
                  min={0}
                  max={500}
                  step={10}
                  className="w-full"
                />
                <p className="text-xs text-muted-foreground">
                  系统提示词少于此字符数时视为"简单"
                </p>
              </div>

              <div className="space-y-2">
                <Label>触发模式（正则表达式）</Label>
                <Input
                  placeholder="^SYSTEM:\n"
                  value={config?.fallback_pattern || '^SYSTEM:\\n'}
                  onChange={(e) => updateConfig('fallback_pattern', e.target.value)}
                  className="bg-white/50 font-mono text-sm"
                />
                <p className="text-xs text-muted-foreground">
                  用户消息匹配此正则时触发切换，默认匹配 SYSTEM: 开头
                </p>
              </div>

              <div className="rounded-lg bg-emerald-50 border border-emerald-200 p-3 text-sm">
                <p className="font-medium mb-2 text-emerald-700">💡 双重检查机制</p>
                <div className="space-y-1.5 text-emerald-600">
                  <div className="flex gap-2">
                    <span className="font-semibold">✓ 第一段：</span>
                    <span>系统提示词为空或简单（&lt; {config?.fallback_trigger_threshold ?? 100} 字符）</span>
                  </div>
                  <div className="flex gap-2">
                    <span className="font-semibold">✓ 第二段：</span>
                    <span>用户消息匹配触发模式</span>
                  </div>
                  <div className="mt-2 pt-2 border-t border-emerald-200 text-xs">
                    <p className="font-semibold text-emerald-700">两个条件都满足 → 切换到辅助模型</p>
                  </div>
                </div>
              </div>
            </CardContent>
          )}
        </Card>

        <Card className="glass border-none shadow-custom">
          <CardHeader className="pb-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="p-2 rounded-lg bg-purple-100 text-purple-600">
                  <Volume2 className="h-5 w-5" />
                </div>
                <div>
                  <CardTitle>语音合成</CardTitle>
                </div>
              </div>
              <Switch
                checked={ttsEnabled}
                onCheckedChange={(checked) => updateConfig('tts_enabled', checked)}
                className="data-[state=checked]:bg-primary"
              />
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label>选择引擎</Label>
              <div className="flex gap-3">
                <Button
                  className={`flex-1 h-11 ${config?.tts_engine === 'gpt_sovits' ? 'bg-gradient-to-r from-pink-500 to-purple-500 hover:from-pink-600 hover:to-purple-600 text-white shadow-md' : ''}`}
                  variant={config?.tts_engine === 'gpt_sovits' ? 'default' : 'outline'}
                  onClick={() => updateConfig('tts_engine', 'gpt_sovits')}
                >
                  <Settings2 className="mr-2 h-4 w-4" />
                  GPT-SoVITS
                </Button>
                <Button
                  variant="outline"
                  className="flex-1 h-11 border-dashed border-2 text-muted-foreground hover:text-foreground hover:border-primary/50"
                >
                  <Plus className="mr-2 h-4 w-4" />
                  待添加
                </Button>
              </div>
            </div>

            <div className="space-y-3">
              <Label>角色配置</Label>
              <div className="space-y-2">
                <div className="flex items-center gap-3">
                  <span className="text-sm text-muted-foreground w-20">全局默认</span>
                  <Select 
                    value={config?.gsv_current_character || 'default'}
                    onValueChange={(value) => updateConfig('gsv_current_character', value)}
                  >
                    <SelectTrigger className="flex-1">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {Object.entries(config?.gsv_characters || {}).map(([id, char]) => (
                        <SelectItem key={id} value={id}>{(char as { name?: string }).name || id}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="flex items-center gap-3">
                  <span className="text-sm text-muted-foreground w-20">男性默认</span>
                  <Select 
                    value={config?.gsv_default_male_character || ''}
                    onValueChange={(value) => updateConfig('gsv_default_male_character', value)}
                  >
                    <SelectTrigger className="flex-1">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {Object.entries(config?.gsv_characters || {}).map(([id, char]) => (
                        <SelectItem key={id} value={id}>{(char as { name?: string }).name || id}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="flex items-center gap-3">
                  <span className="text-sm text-muted-foreground w-20">女性默认</span>
                  <Select 
                    value={config?.gsv_default_female_character || ''}
                    onValueChange={(value) => updateConfig('gsv_default_female_character', value)}
                  >
                    <SelectTrigger className="flex-1">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {Object.entries(config?.gsv_characters || {}).map(([id, char]) => (
                        <SelectItem key={id} value={id}>{(char as { name?: string }).name || id}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </div>
            </div>

            <div className="space-y-2">
              <div className="flex justify-between items-center">
                <Label>音量</Label>
                <span className="text-sm font-medium text-primary">{volume}%</span>
              </div>
              <Slider
                value={[volume]}
                onValueChange={(value) => updateConfig('tts_volume', value[0] / 100)}
                max={100}
                step={1}
                className="w-full"
              />
            </div>

            <div className="flex items-center justify-between p-3 rounded-lg bg-white/50">
              <div className="flex items-center gap-2">
                <Mic className="h-4 w-4 text-muted-foreground" />
                <span className="text-sm">自动播放语音</span>
              </div>
              <Switch
                checked={autoPlay}
                onCheckedChange={(checked) => updateConfig('tts_auto_play', checked)}
                className="data-[state=checked]:bg-primary"
              />
            </div>
          </CardContent>
        </Card>
      </div>

      <Card className="glass border-none shadow-custom">
        <Tabs defaultValue="prompt" className="w-full">
          <CardHeader className="pb-0">
            <div className="flex items-center justify-between">
              <TabsList className="bg-white/50">
                <TabsTrigger value="prompt" className="data-[state=active]:bg-primary data-[state=active]:text-white">
                  📝 系统提示词
                </TabsTrigger>
                <TabsTrigger value="assistant" className="data-[state=active]:bg-primary data-[state=active]:text-white">
                  🤖 AI 助手
                </TabsTrigger>
              </TabsList>
              <div className="flex gap-2">
                <Button variant="outline" size="sm" className="h-8" onClick={handleClearChat}>
                  <Trash2 className="mr-1.5 h-3.5 w-3.5" />
                  清空
                </Button>
                <Button size="sm" className="h-8 bg-primary hover:bg-primary/90" onClick={handleSaveConfig}>
                  <Save className="mr-1.5 h-3.5 w-3.5" />
                  保存
                </Button>
              </div>
            </div>
          </CardHeader>

          <CardContent className="pt-4">
            <TabsContent value="prompt" className="mt-0">
              <Textarea
                placeholder="在此输入系统角色设定..."
                className="bg-white/50 min-h-[280px] resize-none text-sm leading-relaxed"
                value={config?.system_prompt_injection || ''}
                onChange={(e) => updateConfig('system_prompt_injection', e.target.value)}
              />
            </TabsContent>

            <TabsContent value="assistant" className="mt-0">
              <div className="bg-white/30 rounded-xl min-h-[280px] flex flex-col">
                <div className="flex-1 p-4 space-y-4 overflow-auto max-h-[300px]">
                  {history.length === 0 && !isStreaming && (
                    <div className="text-center text-muted-foreground py-8">
                      开始与 AI 对话吧！发送消息即可开始。
                    </div>
                  )}
                  
                  {history.map((msg, index) => (
                    <div key={msg.id || index} className="flex gap-3">
                      <Avatar className="w-10 h-10 border-2 border-amber-200 shadow-md">
                        <AvatarFallback className="bg-gradient-to-br from-amber-100 to-orange-100 text-amber-600">
                          ☀️
                        </AvatarFallback>
                      </Avatar>
                      <div className="flex-1">
                        <div className="border-2 border-pink-200 rounded-2xl rounded-tl-sm p-4 bg-white/80 shadow-sm relative">
                          <div className="absolute -top-1 -right-1 text-pink-300 text-xs">✦</div>
                          <div className="flex items-center justify-between mb-2">
                            <div className="flex items-center gap-2">
                              <span className="font-bold text-pink-500">{msg.speaker || 'AI'}</span>
                            </div>
                          </div>
                          <p className="text-gray-700 leading-relaxed">{msg.content}</p>
                          <div className="flex items-center justify-between mt-3 pt-3 border-t border-pink-100">
                            <div className="flex gap-2">
                              <Button size="sm" className="h-7 px-3 bg-gradient-to-r from-pink-500 to-purple-500 hover:from-pink-600 hover:to-purple-600 text-white text-xs">
                                ▶ 播放语音
                              </Button>
                              <Button size="sm" variant="ghost" className="h-7 px-3 text-xs text-muted-foreground hover:text-foreground">
                                📋 复制
                              </Button>
                            </div>
                            <span className="text-xs text-muted-foreground">
                              {msg.tts_engine || 'GPT-SoVITS'} · 情感: {msg.emotion || '默认'}
                            </span>
                          </div>
                        </div>
                      </div>
                    </div>
                  ))}

                  {isStreaming && currentContent && (
                    <div className="flex gap-3">
                      <Avatar className="w-10 h-10 border-2 border-amber-200 shadow-md">
                        <AvatarFallback className="bg-gradient-to-br from-amber-100 to-orange-100 text-amber-600">
                          ☀️
                        </AvatarFallback>
                      </Avatar>
                      <div className="flex-1">
                        <div className="border-2 border-pink-300 rounded-2xl rounded-tl-sm p-4 bg-white/90 shadow-md relative">
                          <p className="text-gray-700 leading-relaxed">
                            {currentContent}
                            <span className="inline-block w-2 h-5 ml-1 bg-pink-400 align-middle animate-pulse" />
                          </p>
                          <div className="flex items-center mt-2 pt-2 border-t border-pink-100">
                            <span className="text-xs text-pink-500 flex items-center gap-1">
                              <Loader2 className="w-3 h-3 animate-spin" />
                              正在生成...
                            </span>
                          </div>
                        </div>
                      </div>
                    </div>
                  )}
                  <div ref={chatEndRef} />
                </div>

                <div className="p-4 border-t border-white/50">
                  <div className="flex gap-3 items-center">
                    <Input
                      placeholder="输入消息，按 Enter 发送..."
                      className="flex-1 bg-white/70 border-0 shadow-sm"
                      value={inputMessage}
                      onChange={(e) => setInputMessage(e.target.value)}
                      onKeyDown={handleKeyDown}
                      disabled={isSending}
                    />
                    <Button
                      size="icon"
                      className={`shrink-0 rounded-full h-10 w-10 ${
                        isVoiceRecording
                          ? 'bg-red-500 hover:bg-red-600 animate-pulse'
                          : isVoiceRecognizing
                            ? 'bg-blue-500'
                            : 'bg-primary hover:bg-primary/90'
                      }`}
                      onClick={handleMicToggle}
                      disabled={isVoiceRecognizing}
                      title={isVoiceRecording ? '点击停止录音' : '点击开始语音输入'}
                    >
                      {isVoiceRecognizing ? <Loader2 className="h-4 w-4 animate-spin" /> : isVoiceRecording ? <MicOff className="h-4 w-4" /> : <Mic className="h-4 w-4" />}
                    </Button>
                    <Button 
                      size="icon" 
                      className="shrink-0 bg-gradient-to-r from-pink-500 to-purple-500 hover:from-pink-600 hover:to-purple-600 rounded-full h-10 w-10"
                      onClick={handleSendMessage}
                      disabled={isSending || !inputMessage.trim()}
                    >
                      {isSending ? <Loader2 className="h-4 w-4 animate-spin" /> : <Send className="h-4 w-4" />}
                    </Button>
                  </div>
                </div>
              </div>
            </TabsContent>
          </CardContent>
        </Tabs>
      </Card>
    </div>
  );
};

export default Index;
