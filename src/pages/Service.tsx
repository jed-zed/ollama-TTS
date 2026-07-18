import { useState, useEffect, useCallback } from 'react';
import {
  Network,
  Copy,
  Volume2,
  Settings2,
  Mic,
  Send,
  Trash2,
  Save,
  Eye,
  EyeOff,
  Plus,
  Check,
  Loader2,
  Play,
  Square,
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
import { useTTSStore } from '@/stores/ttsStore';
import { ModelCombobox } from '@/components/ModelCombobox';
import PresetManager, { BUILTIN_PRESETS } from '@/components/PresetManager';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { checkBackendHealth, pollBackendHealth, type HealthCheckResponse, apiClient, getProxyStatus, startProxy, stopProxy, testProxy, testProxyStream, type TestStreamMetrics } from '@/services/api';
import type { Config } from '@/services/types';

const getConfirmationTargets = (config: Config | null, apiUrl: string): string[] => {
  const targets = new Set<string>();
  const mainTarget = apiUrl.trim();
  if (mainTarget) targets.add(mainTarget);

  const fallbackPresetId = config?.fallback_model_enabled
    ? config.fallback_preset_id?.trim()
    : '';
  if (fallbackPresetId) {
    const fallbackTarget = fallbackPresetId === 'custom'
      ? config?.custom_api_url?.trim()
      : config?.preset_configs?.[fallbackPresetId]?.api_url?.trim()
        || BUILTIN_PRESETS[fallbackPresetId]?.url?.trim();
    if (fallbackTarget) targets.add(fallbackTarget);
  }

  return Array.from(targets);
};

const Service = () => {
  const [showApiKey, setShowApiKey] = useState(false);
  const [copied, setCopied] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [confirmError, setConfirmError] = useState<string | null>(null);

  // 后端连接状态
  const [backendStatus, setBackendStatus] = useState<'unknown' | 'checking' | 'connected' | 'disconnected'>('unknown');
  const [backendInfo, setBackendInfo] = useState<HealthCheckResponse | null>(null);
  const [backendError, setBackendError] = useState<string | null>(null);
  const [isServiceOperating, setIsServiceOperating] = useState(false);

  // Ollama 代理服务状态
  const [proxyStatus, setProxyStatus] = useState<'unknown' | 'running' | 'stopped'>('unknown');
  const [isProxyOperating, setIsProxyOperating] = useState(false);
  const [proxyTestResult, setProxyTestResult] = useState<string | null>(null);
  const [isProxyTesting, setIsProxyTesting] = useState(false);
  const [testResponse, setTestResponse] = useState<string>('');
  const [testMetrics, setTestMetrics] = useState<TestStreamMetrics | null>(null);
  const [useStreamTest, setUseStreamTest] = useState(true);

  // 从 store 获取状态和方法
  const config = useConfig();
  const isLoading = useConfigLoading();
  const loadConfig = useConfigStore((state) => state.loadConfig);
  const applyAuthoritativeConfig = useConfigStore((state) => state.applyAuthoritativeConfig);
  const updateConfig = useConfigStore((state) => state.updateConfig);
  const updateConfigBatch = useConfigStore((state) => state.updateConfigBatch);

  // TTS store - 使用单独的 selector 避免无限循环
  const isPlaying = useTTSStore((state) => state.isPlaying);
  const engineStatus = useTTSStore((state) => state.engineStatus);
  const testEngine = useTTSStore((state) => state.testEngine);

  // 本地编辑状态（用于防抖）
  const [localApiUrl, setLocalApiUrl] = useState('');
  const [localApiKey, setLocalApiKey] = useState('');
  const [localModelName, setLocalModelName] = useState('');
  const [localSystemPrompt, setLocalSystemPrompt] = useState('');
  const confirmationTargets = getConfirmationTargets(config, localApiUrl);

  // 检查后端连接
  const handleCheckBackend = useCallback(async () => {
    setBackendStatus('checking');
    setBackendError(null);

    const result = await checkBackendHealth();

    if (result.connected && result.data) {
      setBackendStatus('connected');
      setBackendInfo(result.data);
    } else {
      setBackendStatus('disconnected');
      setBackendError(result.error || '连接失败');
      setBackendInfo(null);
    }
  }, []);

  // 启动服务 (事件驱动 + 轮询兜底)
  const handleStartService = async () => {
    setIsServiceOperating(true);
    setBackendError(null);
    setBackendStatus('checking');

    try {
      await apiClient.startBackend();

      const success = await pollBackendHealth(5000, 100, 300);

      if (success) {
        const result = await checkBackendHealth();
        if (result.connected && result.data) {
          setBackendStatus('connected');
          setBackendInfo(result.data);
        } else {
          setBackendStatus('connected');
        }
        await loadConfig();
        checkProxyStatus();
      } else {
        setBackendStatus('disconnected');
        setBackendError('启动超时，后端可能仍在初始化中');
      }
    } catch (error) {
      console.error('启动服务失败:', error);
      setBackendStatus('disconnected');
      setBackendError(error instanceof Error ? error.message : '启动失败');
    } finally {
      setIsServiceOperating(false);
    }
  };

  // 停止服务 (乐观更新，直接信任 Tauri 返回结果)
  const handleStopService = async () => {
    setIsServiceOperating(true);
    setBackendError(null);
    // 乐观更新：立即显示停止状态
    setBackendStatus('disconnected');
    setBackendInfo(null);

    try {
      const success = await apiClient.stopBackend();

      if (!success) {
        // 如果 Tauri 命令失败，检查实际状态
        const result = await checkBackendHealth();
        if (result.connected) {
          setBackendStatus('connected');
          setBackendInfo(result.data || null);
          setBackendError('停止服务失败');
        }
      }
      // 成功时不需要额外操作，状态已经更新
    } catch (error) {
      console.error('停止服务失败:', error);
      setBackendError(error instanceof Error ? error.message : '停止失败');
      // 停止失败时检查实际状态
      handleCheckBackend();
    } finally {
      setIsServiceOperating(false);
    }
  };

  // 检查代理服务状态
  const checkProxyStatus = useCallback(async () => {
    const result = await getProxyStatus();
    if (result.success && result.data) {
      setProxyStatus(result.data.running ? 'running' : 'stopped');
    } else {
      setProxyStatus('stopped');
    }
  }, []);

  // 启动代理服务
  const handleStartProxy = async () => {
    setIsProxyOperating(true);
    try {
      const result = await startProxy();
      if (result.success) {
        await Promise.all([
          checkProxyStatus(),
          handleCheckBackend(),
          loadConfig(),
        ]);
      } else {
        await checkProxyStatus();
      }
    } finally {
      setIsProxyOperating(false);
    }
  };

  // 停止代理服务
  const handleStopProxy = async () => {
    setIsProxyOperating(true);
    try {
      const result = await stopProxy();
      if (result.success) {
        await Promise.all([
          checkProxyStatus(),
          handleCheckBackend(),
          loadConfig(),
        ]);
      } else {
        await checkProxyStatus();
      }
    } finally {
      setIsProxyOperating(false);
    }
  };

  // 测试代理连接
  const handleTestProxy = async () => {
    if (config?.api_target_confirmation_required) {
      setProxyTestResult('✗ 请先确认 API URL');
      setTestResponse('');
      setTestMetrics(null);
      return;
    }

    setIsProxyTesting(true);
    setProxyTestResult(null);
    setTestResponse('');
    setTestMetrics(null);

    if (useStreamTest) {
      await testProxyStream({
        onChunk: (chunk) => {
          setTestResponse(prev => prev + chunk);
        },
        onComplete: (metrics) => {
          setProxyTestResult('✓ 测试成功');
          setTestMetrics(metrics);
          setIsProxyTesting(false);
        },
        onError: (error) => {
          setProxyTestResult(`✗ ${error}`);
          setIsProxyTesting(false);
        }
      });
    } else {
      try {
        const result = await testProxy();
        if (result.success) {
          setProxyTestResult('✓ 测试成功');
          setTestResponse(result.response || '');
        } else {
          setProxyTestResult(`✗ ${result.error}`);
          setTestResponse('');
        }
      } catch (error) {
        setProxyTestResult(`✗ 测试失败`);
        setTestResponse('');
      } finally {
        setIsProxyTesting(false);
      }
    }
  };

  // 初始化检查后端（延迟 500ms，让 UI 先渲染）
  useEffect(() => {
    const timer = setTimeout(() => {
      handleCheckBackend();
      checkProxyStatus();
    }, 500);

    const setupListeners = async () => {
      const unlistenStarted = await listen('backend-started', () => {
        setBackendStatus('connected');
        handleCheckBackend();
        void loadConfig();
        checkProxyStatus();
      });

      const unlistenStopped = await listen('backend-stopped', () => {
        setBackendStatus('disconnected');
        setBackendInfo(null);
      });

      return () => {
        unlistenStarted();
        unlistenStopped();
      };
    };

    const cleanupPromise = setupListeners();

    return () => {
      clearTimeout(timer);
      cleanupPromise.then(cleanup => cleanup());
    };
  }, [checkProxyStatus, handleCheckBackend, loadConfig]);

  // 同步配置到本地状态 - 配置变化时更新
  useEffect(() => {
    if (config) {
      setLocalApiUrl(config.api_url || '');
      setLocalApiKey(config.api_key || '');
      setLocalModelName(config.model_name || '');
      setLocalSystemPrompt(config.system_prompt_injection || '');
    }
  }, [config]);

  // 获取角色列表
  const characters = config?.gsv_characters || {};
  const characterList = Object.entries(characters).map(([id, char]) => ({
    id,
    name: char.name,
  }));

  // 获取当前角色名称
  const currentCharacterName = config?.gsv_current_character
    ? characters[config.gsv_current_character]?.name || '未设置'
    : '未设置';

  // 复制模型名称
  const handleCopyWithPreview = () => {
    const textToCopy = localModelName + '-preview';
    navigator.clipboard.writeText(textToCopy);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  // 保存配置
  const handleSave = async () => {
    setIsSaving(true);
    try {
      await updateConfigBatch({
        api_url: localApiUrl,
        api_key: localApiKey,
        model_name: localModelName,
        system_prompt_injection: localSystemPrompt,
      });
    } finally {
      setIsSaving(false);
    }
  };

  const handleConfirmApiTarget = async () => {
    if (!localApiUrl.trim()) return;

    setIsSaving(true);
    setConfirmError(null);
    try {
      const confirmedConfig = await invoke<Config>('confirm_api_target', {
        apiUrl: localApiUrl.trim(),
        expectedTargets: confirmationTargets,
      });
      applyAuthoritativeConfig(confirmedConfig);
      await loadConfig();
    } catch (error) {
      setConfirmError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsSaving(false);
    }
  };

  // 清空系统提示词
  const handleClearPrompt = () => {
    setLocalSystemPrompt('');
  };

  // 保存 API 配置到当前预设（实时自动保存）
  const saveApiConfigToCurrentPreset = useCallback(async (
    field: 'api_url' | 'api_key' | 'model_name',
    value: string
  ) => {
    const currentPreset = config?.current_api_preset || 'custom';

    if (currentPreset === 'custom') {
      // 自定义预设：同时保存到 api_* 和 custom_api_*
      const updates: Record<string, unknown> = {
        [field]: value,
      };
      if (field === 'api_url') updates.custom_api_url = value;
      if (field === 'api_key') updates.custom_api_key = value;
      if (field === 'model_name') updates.custom_model_name = value;
      await updateConfigBatch(updates);
    } else {
      // 其他预设：保存到 api_* 和 preset_configs
      const currentPresetConfigs = config?.preset_configs || {};
      const currentConfig = currentPresetConfigs[currentPreset] || { api_url: '', api_key: '', model_name: '' };

      const updates: Record<string, unknown> = {
        [field]: value,
        preset_configs: {
          ...currentPresetConfigs,
          [currentPreset]: {
            ...currentConfig,
            [field]: value,
          },
        },
      };

      await updateConfigBatch(updates);
    }
  }, [config, updateConfigBatch]);

  // 处理音量变化（防抖）
  const handleVolumeChange = useCallback(async (value: number[]) => {
    const volume = value[0] / 100;
    await updateConfig('tts_volume', volume);
  }, [updateConfig]);

  // AI 助手对话示例
  const [chatMessages] = useState([
    {
      role: 'user',
      content: '你好，请介绍一下你自己。'
    },
    {
      role: 'assistant',
      name: '星野',
      nameEn: 'Hoshino',
      content: '你好呀～我是星野，是 AI Voice Bridge 的虚拟助手。我可以将文字变成有感情的声音，还能理解你说的话哦！有什么想聊的吗？',
      emotion: '开心',
      engine: 'GPT-SoVITS',
      time: '1.2s'
    }
  ]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-8 h-8 animate-spin text-primary" />
        <span className="ml-2 text-muted-foreground">加载配置中...</span>
      </div>
    );
  }

  return (
    <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
      {/* 顶部状态栏 */}
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
          {/* API 状态徽章 */}
          <Badge variant="outline" className={`px-3 py-1.5 ${config?.api_url
            ? 'border-green-300 bg-green-50 text-green-600'
            : 'border-orange-300 bg-orange-50 text-orange-600'
            }`}>
            <div className={`w-2 h-2 rounded-full mr-2 ${config?.api_url ? 'bg-green-500 animate-pulse' : 'bg-orange-500'
              }`} />
            {config?.api_url ? 'API 已配置' : 'API 未配置'}
          </Badge>

          {/* TTS 状态徽章 */}
          <Badge variant="outline" className={`px-3 py-1.5 ${config?.tts_enabled
            ? isPlaying
              ? 'border-green-300 bg-green-50 text-green-600'
              : 'border-purple-300 bg-purple-50 text-purple-600'
            : 'border-gray-300 bg-gray-50 text-gray-600'
            }`}>
            <div className={`w-2 h-2 rounded-full mr-2 ${config?.tts_enabled
              ? isPlaying ? 'bg-green-500 animate-pulse' : 'bg-purple-500'
              : 'bg-gray-400'
              }`} />
            {config?.tts_enabled
              ? isPlaying ? 'TTS 播放中' : 'TTS 待机'
              : 'TTS 已关闭'
            }
          </Badge>

          {/* 角色徽章 */}
          <Badge variant="outline" className="px-3 py-1.5 border-amber-300 bg-amber-50 text-amber-600">
            <div className="w-2 h-2 rounded-full bg-amber-500 mr-2" />
            角色: {currentCharacterName}
          </Badge>
        </div>
      </div>

      {/* 主配置区域 - 左右两栏 */}
      <div className="grid grid-cols-2 gap-6">
        {/* Ollama 代理服务卡片 */}
        <Card className="col-span-2 glass border-none shadow-custom">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className={`p-2 rounded-lg ${proxyStatus === 'running'
                  ? 'bg-green-100 text-green-600'
                  : 'bg-gray-100 text-gray-600'
                  }`}>
                  <Network className="h-5 w-5" />
                </div>
                <div>
                  <CardTitle className="text-base">Ollama 代理服务</CardTitle>
                  <CardDescription className="text-xs">
                    在 11434 端口提供 Ollama 兼容 API
                  </CardDescription>
                </div>
              </div>
              <div className="flex items-center gap-3">
                <Badge variant="outline" className={
                  proxyStatus === 'running'
                    ? 'border-green-300 bg-green-50 text-green-600'
                    : 'border-gray-300 bg-gray-50 text-gray-600'
                }>
                  <div className={`w-2 h-2 rounded-full mr-2 ${proxyStatus === 'running' ? 'bg-green-500 animate-pulse' : 'bg-gray-400'
                    }`} />
                  {proxyStatus === 'running' ? '运行中 :11434' : '已停止'}
                </Badge>

                <Button
                  size="sm"
                  onClick={proxyStatus === 'running' ? handleStopProxy : handleStartProxy}
                  disabled={isProxyOperating}
                  className={`h-8 text-white ${proxyStatus === 'running'
                    ? 'bg-red-600 hover:bg-red-700'
                    : 'bg-green-600 hover:bg-green-700'
                    }`}
                >
                  {isProxyOperating ? (
                    <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  ) : proxyStatus === 'running' ? (
                    <Square className="w-4 h-4 mr-2 fill-current" />
                  ) : (
                    <Play className="w-4 h-4 mr-2" />
                  )}
                  {proxyStatus === 'running' ? '停止服务' : '启动服务'}
                </Button>

                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleTestProxy}
                  disabled={isProxyTesting || proxyStatus !== 'running' || config?.api_target_confirmation_required}
                  className="h-8"
                >
                  {isProxyTesting ? (
                    <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  ) : (
                    <Check className="w-4 h-4 mr-2" />
                  )}
                  测试
                </Button>
              </div>
            </div>
          </CardHeader>
          <CardContent className="pt-0 space-y-3">
            {config?.api_target_confirmation_required && (
              <div className="rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-900">
                API 目标已变更或旧桥接已移除；确认所有目标前，代理测试和所有 LLM 请求均已暂停。
              </div>
            )}
            <div className="flex items-center gap-4 text-sm text-muted-foreground">
              <span>外部应用访问地址:</span>
              <code className="bg-gray-100 px-2 py-1 rounded text-gray-700">
                http://localhost:11434
              </code>
              {proxyTestResult && (
                <span className={proxyTestResult.startsWith('✓') ? 'text-green-600' : 'text-red-500'}>
                  {proxyTestResult}
                </span>
              )}
            </div>
            {(testResponse || isProxyTesting) && (
              <div className="p-3 bg-gradient-to-r from-purple-50 to-pink-50 rounded-lg border border-purple-100">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <div className={`w-2 h-2 rounded-full ${isProxyTesting ? 'bg-purple-500 animate-pulse' : 'bg-green-500'}`} />
                    <span className="text-xs font-medium text-purple-600">AI 回复</span>
                  </div>
                  {testMetrics && (
                    <div className="flex items-center gap-3 text-xs text-muted-foreground">
                      <span>首字: {testMetrics.first_token_ms}ms</span>
                      <span>总耗时: {testMetrics.total_ms}ms</span>
                      <span>tokens: {testMetrics.token_count}</span>
                    </div>
                  )}
                </div>
                <p className="text-sm text-gray-700 whitespace-pre-wrap leading-relaxed">
                  {testResponse || (isProxyTesting ? '等待响应...' : '')}
                </p>
              </div>
            )}
          </CardContent>
        </Card>

        {/* 左侧 - API 配置 */}
        <Card className="glass border-none shadow-custom">
          <CardHeader className="pb-4">
            <div className="flex items-center gap-3">
              <div className="p-2 rounded-lg bg-blue-100 text-blue-600">
                <Network className="h-5 w-5" />
              </div>
              <div>
                <CardTitle>API 配置</CardTitle>
                <CardDescription>设置 LLM API 接口地址和密钥</CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {config?.api_target_confirmation_required && (
              <div className="space-y-3 rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900">
                <p>
                  API 目标已变更或旧桥接已移除。为避免误发，所有 LLM 请求已暂停；请核对并明确确认以下目标：
                </p>
                <ul className="space-y-1 rounded-md border border-amber-200 bg-white/70 p-2 font-mono text-xs">
                  {confirmationTargets.map((target) => (
                    <li key={target} className="break-all">{target}</li>
                  ))}
                </ul>
                {confirmError && (
                  <p role="alert" className="rounded-md border border-red-200 bg-red-50 px-2 py-1.5 text-xs text-red-700">
                    确认失败：{confirmError}
                  </p>
                )}
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={handleConfirmApiTarget}
                  disabled={isSaving || confirmationTargets.length === 0}
                  className="border-amber-400 bg-white hover:bg-amber-100"
                >
                  {isSaving && <Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" />}
                  确认使用以上 API 目标
                </Button>
              </div>
            )}

            <PresetManager
              config={config}
              disabled={proxyStatus === 'running'}
              localApiUrl={localApiUrl}
              localApiKey={localApiKey}
              localModelName={localModelName}
              setLocalApiUrl={setLocalApiUrl}
              setLocalApiKey={setLocalApiKey}
              setLocalModelName={setLocalModelName}
              updateConfigBatch={updateConfigBatch}
            />

            <div className="space-y-2">
              <Label>API URL</Label>
              <Input
                value={localApiUrl}
                onChange={(e) => setLocalApiUrl(e.target.value)}
                onBlur={() => saveApiConfigToCurrentPreset('api_url', localApiUrl)}
                className="bg-white/50"
                placeholder="http://127.0.0.1:11434/v1"
                disabled={proxyStatus === 'running'}
              />
            </div>

            <div className="space-y-2">
              <Label>API Key</Label>
              <div className="relative">
                <Input
                  type={showApiKey ? "text" : "password"}
                  value={localApiKey}
                  onChange={(e) => setLocalApiKey(e.target.value)}
                  onBlur={() => saveApiConfigToCurrentPreset('api_key', localApiKey)}
                  className="bg-white/50 pr-10"
                  placeholder="sk-xxxxxxxxxxxxxxxx"
                  disabled={proxyStatus === 'running'}
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
                <ModelCombobox
                  value={localModelName}
                  onChange={setLocalModelName}
                  onSelect={(value) => saveApiConfigToCurrentPreset('model_name', value)}
                  onBlur={() => saveApiConfigToCurrentPreset('model_name', localModelName)}
                  apiUrl={localApiUrl}
                  apiKey={localApiKey}
                  disabled={proxyStatus === 'running' || config?.api_target_confirmation_required}
                  placeholder="选择或输入模型名称"
                  className="flex-1"
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
                请直接点击上方按钮复制再粘贴入游戏
              </p>
              {proxyStatus === 'running' && (
                <p className="text-xs text-amber-600 mt-2">
                  ⚠️ 代理服务运行中，请先停止服务再修改配置
                </p>
              )}
            </div>
          </CardContent>
        </Card>

        {/* 右侧 - 语音合成 */}
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
                checked={config?.tts_enabled ?? true}
                onCheckedChange={(checked) => updateConfig('tts_enabled', checked)}
                className="data-[state=checked]:bg-primary"
              />
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {/* 引擎选择 */}
            <div className="space-y-2">
              <Label>选择引擎</Label>
              <div className="flex gap-3">
                <Button
                  className={`flex-1 h-11 ${config?.tts_engine === 'gpt_sovits'
                    ? 'bg-gradient-to-r from-pink-500 to-purple-500 hover:from-pink-600 hover:to-purple-600 text-white shadow-md'
                    : 'bg-white/50 text-muted-foreground hover:bg-white/80'
                    }`}
                  onClick={() => updateConfig('tts_engine', 'gpt_sovits')}
                >
                  <Settings2 className="mr-2 h-4 w-4" />
                  GPT-SoVITS
                </Button>
                <Button
                  variant="outline"
                  className="flex-1 h-11 border-dashed border-2 text-muted-foreground hover:text-foreground hover:border-primary/50"
                  disabled
                >
                  <Plus className="mr-2 h-4 w-4" />
                  待添加
                </Button>
              </div>
            </div>

            {/* 角色配置 */}
            <div className="space-y-3">
              <Label>角色配置</Label>
              <div className="space-y-2">
                <div className="flex items-center gap-3">
                  <span className="text-sm text-muted-foreground w-20">全局默认</span>
                  <Select
                    value={config?.gsv_current_character || ''}
                    onValueChange={(value) => updateConfig('gsv_current_character', value)}
                  >
                    <SelectTrigger className="flex-1">
                      <SelectValue placeholder="选择角色" />
                    </SelectTrigger>
                    <SelectContent>
                      {characterList.map((char) => (
                        <SelectItem key={char.id} value={char.id}>{char.name}</SelectItem>
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
                      <SelectValue placeholder="选择角色" />
                    </SelectTrigger>
                    <SelectContent>
                      {characterList.map((char) => (
                        <SelectItem key={char.id} value={char.id}>{char.name}</SelectItem>
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
                      <SelectValue placeholder="选择角色" />
                    </SelectTrigger>
                    <SelectContent>
                      {characterList.map((char) => (
                        <SelectItem key={char.id} value={char.id}>{char.name}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </div>
            </div>

            {/* 音量控制 */}
            <div className="space-y-2">
              <div className="flex justify-between items-center">
                <Label>音量</Label>
                <span className="text-sm font-medium text-primary">
                  {Math.round((config?.tts_volume ?? 0.8) * 100)}%
                </span>
              </div>
              <Slider
                value={[Math.round((config?.tts_volume ?? 0.8) * 100)]}
                onValueChange={handleVolumeChange}
                max={100}
                step={1}
                className="w-full"
              />
            </div>

            {/* 自动播放 */}
            <div className="flex items-center justify-between p-3 rounded-lg bg-white/50">
              <div className="flex items-center gap-2">
                <Mic className="h-4 w-4 text-muted-foreground" />
                <span className="text-sm">自动播放语音</span>
              </div>
              <Switch
                checked={config?.tts_auto_play ?? true}
                onCheckedChange={(checked) => updateConfig('tts_auto_play', checked)}
                className="data-[state=checked]:bg-primary"
              />
            </div>
          </CardContent>
        </Card>

        {/* 局域网开放 */}
        <Card className="glass border-none shadow-custom col-span-2">
          <CardContent className="pt-6">
            <div className="flex flex-row items-center justify-between rounded-lg border border-gray-200 p-3 bg-white">
              <div className="space-y-0.5">
                <Label className="text-base font-medium">🌐 局域网开放</Label>
                <p className="text-xs text-muted-foreground">
                  开启后仅 Ollama 兼容代理绑定 0.0.0.0，允许局域网设备访问；配置和管理 API 始终仅限本机。
                  <span className="text-amber-600 font-medium">修改后需重启代理服务器生效。</span>
                </p>
              </div>
              <Switch
                checked={config?.lan_access_enabled || false}
                onCheckedChange={(checked) => updateConfig('lan_access_enabled', checked)}
                className="data-[state=checked]:bg-green-500"
              />
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
                    {'custom' !== config?.current_api_preset && (
                      <SelectItem value="custom">
                        🔧 自定义 (Custom)
                        {config?.custom_model_name ? ` (${config.custom_model_name})` : ''}
                      </SelectItem>
                    )}
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

              <div className="rounded-lg bg-emerald-50 border border-emerald-200 p-3 text-sm">
                <p className="font-medium mb-2 text-emerald-700">💡 触发规则</p>
                <div className="space-y-1.5 text-emerald-600">
                  <p>当用户消息以固定前缀开头时，自动切换到辅助模型：</p>
                  <code className="block bg-white/60 rounded px-2 py-1 text-xs font-mono text-gray-700">
                    {'SYSTEM:\\n...'}
                  </code>
                </div>
              </div>
            </CardContent>
          )}
        </Card>
      </div>

      {/* 底部 - 提示词/AI助手 标签页 */}
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
                <Button variant="outline" size="sm" className="h-8" onClick={handleClearPrompt}>
                  <Trash2 className="mr-1.5 h-3.5 w-3.5" />
                  清空
                </Button>
                <Button
                  size="sm"
                  className="h-8 bg-primary hover:bg-primary/90"
                  onClick={handleSave}
                  disabled={isSaving}
                >
                  {isSaving ? (
                    <Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Save className="mr-1.5 h-3.5 w-3.5" />
                  )}
                  保存
                </Button>
              </div>
            </div>
          </CardHeader>

          <CardContent className="pt-4">
            {/* 系统提示词 */}
            <TabsContent value="prompt" className="mt-0">
              <Textarea
                placeholder="在此输入系统角色设定..."
                className="bg-white/50 min-h-[280px] resize-none text-sm leading-relaxed"
                value={localSystemPrompt}
                onChange={(e) => setLocalSystemPrompt(e.target.value)}
              />
            </TabsContent>

            {/* AI 助手对话 */}
            <TabsContent value="assistant" className="mt-0">
              <div className="bg-white/30 rounded-xl min-h-[280px] flex flex-col">
                {/* 对话区域 */}
                <div className="flex-1 p-4 space-y-4 overflow-auto">
                  {chatMessages.map((msg, index) => (
                    msg.role === 'user' ? (
                      /* 用户消息 */
                      <div key={index} className="flex justify-end">
                        <div className="bg-gradient-to-r from-pink-500 to-purple-500 text-white px-4 py-2.5 rounded-2xl rounded-tr-sm max-w-[70%] shadow-md">
                          {msg.content}
                        </div>
                      </div>
                    ) : (
                      /* AI 回复 */
                      <div key={index} className="flex gap-3">
                        <Avatar className="w-10 h-10 border-2 border-amber-200 shadow-md">
                          <AvatarFallback className="bg-gradient-to-br from-amber-100 to-orange-100 text-amber-600">
                            ☀️
                          </AvatarFallback>
                        </Avatar>
                        <div className="flex-1">
                          <div className="border-2 border-pink-200 rounded-2xl rounded-tl-sm p-4 bg-white/80 shadow-sm relative">
                            {/* 装饰星星 */}
                            <div className="absolute -top-1 -right-1 text-pink-300 text-xs">✦</div>
                            <div className="absolute top-1 right-3 text-pink-200 text-[10px]">✦</div>

                            {/* 头部信息 */}
                            <div className="flex items-center justify-between mb-2">
                              <div className="flex items-center gap-2">
                                <span className="font-bold text-pink-500">{msg.name}</span>
                                <span className="text-xs text-muted-foreground">{msg.nameEn}</span>
                              </div>
                              <span className="text-xs text-muted-foreground flex items-center gap-1">
                                ⏱ {msg.time}
                              </span>
                            </div>

                            {/* 消息内容 */}
                            <p className="text-gray-700 leading-relaxed">{msg.content}</p>

                            {/* 底部操作栏 */}
                            <div className="flex items-center justify-between mt-3 pt-3 border-t border-pink-100">
                              <div className="flex gap-2">
                                <Button size="sm" className="h-7 px-3 bg-gradient-to-r from-pink-500 to-purple-500 hover:from-pink-600 hover:to-purple-600 text-white text-xs">
                                  ▶ 播放语音
                                </Button>
                                <Button size="sm" variant="ghost" className="h-7 px-3 text-xs text-muted-foreground hover:text-foreground">
                                  ↻ 重新生成
                                </Button>
                                <Button size="sm" variant="ghost" className="h-7 px-3 text-xs text-muted-foreground hover:text-foreground">
                                  📋 复制
                                </Button>
                              </div>
                              <span className="text-xs text-muted-foreground">
                                {msg.engine} · 情感: {msg.emotion}
                              </span>
                            </div>
                          </div>
                        </div>
                      </div>
                    )
                  ))}
                </div>

                {/* 输入区域 */}
                <div className="p-4 border-t border-white/50">
                  <div className="flex gap-3 items-center">
                    <Input
                      placeholder="输入消息，或按住空格键说话..."
                      className="flex-1 bg-white/70 border-0 shadow-sm"
                    />
                    <Button size="icon" className="shrink-0 bg-primary hover:bg-primary/90 rounded-full h-10 w-10">
                      <Mic className="h-4 w-4" />
                    </Button>
                    <Button size="icon" className="shrink-0 bg-gradient-to-r from-pink-500 to-purple-500 hover:from-pink-600 hover:to-purple-600 rounded-full h-10 w-10">
                      <Send className="h-4 w-4" />
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

export default Service;
