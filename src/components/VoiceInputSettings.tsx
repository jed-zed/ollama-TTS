import { useState, useEffect, useCallback, useRef } from 'react';
import { Mic, CheckCircle2, AlertTriangle, Globe, Square, Loader2, MessageSquare, Keyboard, Power, PowerOff } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { useConfig, useConfigStore } from '@/stores/configStore';
import { API_BASE_URL } from '@/lib/constants';
import { toast } from 'sonner';
import { speechService } from '@/services/speech';
import type { SpeechEngine } from '@/services/speech';
import { globalShortcutService } from '@/services/globalShortcut';
import { isTauri } from '@/services/tauri';

const VoiceInputSettings = () => {
  const config = useConfig();
  const updateConfig = useConfigStore((state) => state.updateConfig);
  
  const [isRecording, setIsRecording] = useState(false);
  const [testStatus, setTestStatus] = useState<'idle' | 'testing' | 'success' | 'error'>('idle');
  const [testMessage, setTestMessage] = useState('');
  const [recognizedText, setRecognizedText] = useState<string>('');
  const [isRecognizing, setIsRecognizing] = useState(false);

  const [shortcutEnabled, setShortcutEnabled] = useState(false);
  const [shortcutKey, setShortcutKey] = useState('Ctrl+Shift+,');
  const [shortcutState, setShortcutState] = useState<'idle' | 'recording' | 'recognizing'>('idle');
  const [isCapturing, setIsCapturing] = useState(false);
  const capturedKeysRef = useRef<string[]>([]);

  const speechEngine = (config?.speech_engine || 'baidu') as SpeechEngine;
  const baiduAppId = config?.baidu_app_id || '';
  const baiduApiKey = config?.baidu_api_key || '';
  const baiduSecretKey = config?.baidu_secret_key || '';
  const xunfeiAppId = config?.xunfei_app_id || '';
  const xunfeiApiKey = config?.xunfei_api_key || '';
  const xunfeiApiSecret = config?.xunfei_api_secret || '';

  useEffect(() => {
    if (config) {
      setShortcutEnabled(config.voice_shortcut_enabled || false);
      setShortcutKey(config.voice_shortcut || 'Ctrl+Shift+,');
    }
  }, [config]);

  useEffect(() => {
    globalShortcutService.setHandlers({
      onStateChange: setShortcutState,
      onResult: (text) => toast.success(`识别结果: ${text}`),
      onError: (error) => toast.error(error),
    });
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    if (shortcutEnabled && shortcutKey) {
      globalShortcutService.register(shortcutKey);
    } else {
      globalShortcutService.unregister();
    }
    return () => {
      globalShortcutService.unregister();
    };
  }, [shortcutEnabled, shortcutKey]);

  const handleShortcutToggle = useCallback(async (enabled: boolean) => {
    setShortcutEnabled(enabled);
    await updateConfig('voice_shortcut_enabled', enabled);
    if (enabled) {
      toast.success(`全局快捷键已启用: ${shortcutKey}`);
    } else {
      toast.info('全局快捷键已禁用');
    }
  }, [shortcutKey, updateConfig]);

  const handleStartCapture = useCallback(() => {
    setIsCapturing(true);
    capturedKeysRef.current = [];
  }, []);

  const handleCaptureKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (!isCapturing) return;
    e.preventDefault();
    e.stopPropagation();

    const parts: string[] = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.shiftKey) parts.push('Shift');
    if (e.altKey) parts.push('Alt');
    if (e.metaKey) parts.push('Super');

    const key = e.key;
    if (!['Control', 'Shift', 'Alt', 'Meta'].includes(key)) {
      if (key === ',') parts.push(',');
      else if (key === '.') parts.push('.');
      else if (key === '/') parts.push('/');
      else if (key === ';') parts.push(';');
      else if (key === "'") parts.push("'");
      else if (key === '[') parts.push('[');
      else if (key === ']') parts.push(']');
      else if (key === '\\') parts.push('\\');
      else if (key === '`') parts.push('`');
      else if (key === '-') parts.push('-');
      else if (key === '=') parts.push('=');
      else if (key.length === 1) parts.push(key.toUpperCase());
      else parts.push(key);

      if (parts.length >= 2) {
        const newShortcut = parts.join('+');
        setShortcutKey(newShortcut);
        setIsCapturing(false);
        updateConfig('voice_shortcut', newShortcut);
        toast.success(`快捷键已设置为: ${newShortcut}`);
      }
    }
  }, [isCapturing, updateConfig]);

  const handleEngineChange = useCallback(async (engine: SpeechEngine) => {
    await updateConfig('speech_engine', engine);
    globalShortcutService.loadSpeechConfig();
    toast.success(`语音识别引擎已切换为: ${engine === 'baidu' ? '百度' : '讯飞'}`);
  }, [updateConfig]);

  const startVoiceInput = async () => {
    if (speechEngine === 'baidu' && (!baiduApiKey || !baiduSecretKey)) {
      toast.error('请先配置百度语音 API');
      return;
    }
    if (speechEngine === 'xunfei' && (!xunfeiAppId || !xunfeiApiKey || !xunfeiApiSecret)) {
      toast.error('请先配置讯飞语音 API');
      return;
    }

    const success = await speechService.startRecording();
    if (success) {
      setIsRecording(true);
      setRecognizedText('');
      toast.info('开始录音，请说话...');
    } else {
      toast.error('无法访问麦克风，请检查权限');
    }
  };

  const stopVoiceInput = async () => {
    if (!isRecording) return;
    
    setIsRecording(false);
    setIsRecognizing(true);
    
    try {
      const audioBlob = await speechService.stopRecording();
      if (!audioBlob) {
        toast.error('未录制到音频');
        setIsRecognizing(false);
        return;
      }

      toast.info('正在识别...');
      
      const speechConfig = speechEngine === 'xunfei'
        ? { engine: 'xunfei' as const, xunfei: { appId: xunfeiAppId, apiKey: xunfeiApiKey, apiSecret: xunfeiApiSecret } }
        : { engine: 'baidu' as const, baidu: { apiKey: baiduApiKey, secretKey: baiduSecretKey } };

      const result = await speechService.recognize(audioBlob, speechConfig);

      if (result.success && result.text) {
        setRecognizedText(result.text);
        toast.success('识别成功');
      } else {
        toast.error(result.error || '识别失败');
      }
    } catch (error) {
      console.error('语音识别失败:', error);
      toast.error('语音识别失败');
    } finally {
      setIsRecognizing(false);
    }
  };

  // 测试百度连接（通过后端代理，避免 CORS 问题）
  const testBaiduConnection = async () => {
    if (!baiduApiKey || !baiduSecretKey) {
      toast.error('请先填写 API Key 和 Secret Key');
      return;
    }

    setTestStatus('testing');
    setTestMessage('正在通过后端代理连接百度鉴权接口...');

    try {
      const response = await fetch(`${API_BASE_URL}/api/speech/baidu/test`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          api_key: baiduApiKey,
          secret_key: baiduSecretKey,
        }),
      });

      if (response.ok) {
        const data = await response.json();
        if (data.success) {
          setTestStatus('success');
          setTestMessage(`连接成功！Access Token 获取正常 (有效期: ${data.expires_in}秒)`);
          toast.success('百度语音连接测试成功');
        } else {
          setTestStatus('error');
          setTestMessage(`连接失败: ${data.error_description || data.error}`);
        }
      } else {
        setTestStatus('error');
        setTestMessage(`后端服务错误: HTTP ${response.status}`);
      }
    } catch (error) {
      console.error('测试失败:', error);
      setTestStatus('error');
      setTestMessage('连接失败 (请确保后端服务已启动)');
    }
  };

  const testXunfeiConnection = async () => {
    if (!xunfeiAppId || !xunfeiApiKey || !xunfeiApiSecret) {
      toast.error('请先填写讯飞 APP ID、API Key 和 API Secret');
      return;
    }

    setTestStatus('testing');
    setTestMessage('正在通过后端代理连接讯飞语音接口...');

    try {
      const response = await fetch(`${API_BASE_URL}/api/speech/xunfei/test`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          app_id: xunfeiAppId,
          api_key: xunfeiApiKey,
          api_secret: xunfeiApiSecret,
        }),
      });

      if (response.ok) {
        const data = await response.json();
        if (data.success) {
          setTestStatus('success');
          setTestMessage(data.message || '讯飞语音 API 连接成功');
          toast.success('讯飞语音连接测试成功');
        } else {
          setTestStatus('error');
          setTestMessage(`连接失败: ${data.error}`);
        }
      } else {
        setTestStatus('error');
        setTestMessage(`后端服务错误: HTTP ${response.status}`);
      }
    } catch (error) {
      console.error('测试失败:', error);
      setTestStatus('error');
      setTestMessage('连接失败 (请确保后端服务已启动)');
    }
  };

  return (
    <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
      
      {isTauri() && (
      <Card className="glass border-none shadow-custom bg-gradient-to-br from-violet-50/50 to-indigo-50/50">
        <CardHeader className="pb-3 border-b border-border/40">
           <CardTitle className="text-base font-semibold text-primary flex items-center gap-2">
              <Keyboard className="w-4 h-4" />
              全局语音快捷键
           </CardTitle>
           <CardDescription className="text-xs">
             设置全局快捷键，在游戏中按下即可语音输入，识别后自动粘贴到当前窗口
           </CardDescription>
        </CardHeader>
        <CardContent className="pt-4 space-y-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Label htmlFor="shortcut-toggle" className="text-sm font-medium">
                启用全局快捷键
              </Label>
              {shortcutState !== 'idle' && (
                <span className={`text-xs px-2 py-0.5 rounded-full font-medium ${
                  shortcutState === 'recording' 
                    ? 'bg-red-100 text-red-600 animate-pulse' 
                    : 'bg-blue-100 text-blue-600'
                }`}>
                  {shortcutState === 'recording' ? '录音中...' : '识别中...'}
                </span>
              )}
            </div>
            <div className="flex items-center gap-2">
              {shortcutEnabled ? (
                <Power className="w-4 h-4 text-green-500" />
              ) : (
                <PowerOff className="w-4 h-4 text-muted-foreground" />
              )}
              <Switch
                id="shortcut-toggle"
                checked={shortcutEnabled}
                onCheckedChange={handleShortcutToggle}
              />
            </div>
          </div>

          <div className="flex items-center gap-3">
            <Label className="text-sm shrink-0">快捷键</Label>
            <div
              tabIndex={0}
              onKeyDown={handleCaptureKeyDown}
              onBlur={() => setIsCapturing(false)}
              className={`flex-1 h-9 px-3 rounded-md border text-sm flex items-center cursor-pointer transition-all ${
                isCapturing
                  ? 'border-violet-500 bg-violet-50 ring-2 ring-violet-200 text-violet-700'
                  : 'border-input bg-white/50 text-foreground'
              }`}
              onClick={handleStartCapture}
            >
              {isCapturing ? (
                <span className="text-violet-500 animate-pulse">请按下组合键...</span>
              ) : (
                <span className="font-mono">{shortcutKey}</span>
              )}
            </div>
            <Button
              variant="outline"
              size="sm"
              className="h-9 shrink-0"
              onClick={handleStartCapture}
            >
              修改
            </Button>
          </div>

          <div className="bg-indigo-50/50 p-3 rounded-md text-xs text-muted-foreground border border-indigo-100 space-y-1">
            <p className="font-semibold text-indigo-700">使用说明:</p>
            <ul className="list-disc pl-4 space-y-0.5">
              <li>按下快捷键开始录音，再按一次停止录音并识别</li>
              <li>识别完成后自动复制到剪贴板并粘贴到当前焦点窗口</li>
              <li>需要先配置语音识别 API（百度或讯飞）才能使用</li>
              <li>快捷键必须是组合键（如 Ctrl+Shift+,）</li>
            </ul>
          </div>
        </CardContent>
      </Card>
      )}

      {/* 语音识别引擎选择 */}
      <Card className="glass border-none shadow-custom bg-gradient-to-br from-emerald-50/50 to-teal-50/50">
        <CardHeader className="pb-3 border-b border-border/40">
           <CardTitle className="text-base font-semibold text-primary flex items-center gap-2">
              <Mic className="w-4 h-4" />
              语音识别引擎
           </CardTitle>
           <CardDescription className="text-xs">
             选择用于语音识别的引擎，百度和讯飞均支持中文识别
           </CardDescription>
        </CardHeader>
        <CardContent className="pt-4 space-y-4">
          <div className="flex items-center gap-4">
            <Label className="text-sm shrink-0">当前引擎</Label>
            <Select value={speechEngine} onValueChange={(v) => handleEngineChange(v as SpeechEngine)}>
              <SelectTrigger className="bg-white/60 h-10 w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="baidu">百度语音</SelectItem>
                <SelectItem value="xunfei">讯飞语音</SelectItem>
              </SelectContent>
            </Select>
            <span className={`text-xs px-2 py-1 rounded-full font-medium ${
              speechEngine === 'baidu'
                ? (baiduApiKey && baiduSecretKey ? 'bg-green-100 text-green-700' : 'bg-orange-100 text-orange-600')
                : (xunfeiAppId && xunfeiApiKey && xunfeiApiSecret ? 'bg-green-100 text-green-700' : 'bg-orange-100 text-orange-600')
            }`}>
              {speechEngine === 'baidu'
                ? (baiduApiKey && baiduSecretKey ? '已配置' : '未配置')
                : (xunfeiAppId && xunfeiApiKey && xunfeiApiSecret ? '已配置' : '未配置')
              }
            </span>
          </div>
        </CardContent>
      </Card>

      {/* 语音输入测试区域 */}
      <Card className="glass border-none shadow-custom bg-gradient-to-br from-blue-50/50 to-purple-50/50">
        <CardHeader className="pb-3 border-b border-border/40">
           <CardTitle className="text-base font-semibold text-primary flex items-center gap-2">
              <MessageSquare className="w-4 h-4" />
              语音输入测试
           </CardTitle>
           <CardDescription className="text-xs">
             使用 {speechEngine === 'baidu' ? '百度' : '讯飞'} 引擎录音并识别语音内容
           </CardDescription>
        </CardHeader>
        <CardContent className="pt-4 space-y-4">
          <div className="flex items-center gap-4">
            <Button 
              variant={isRecording ? "destructive" : "default"}
              className={`w-32 transition-all ${isRecording ? 'animate-pulse' : ''}`}
              onClick={isRecording ? stopVoiceInput : startVoiceInput}
              disabled={isRecognizing || (speechEngine === 'baidu' ? (!baiduApiKey || !baiduSecretKey) : (!xunfeiAppId || !xunfeiApiKey || !xunfeiApiSecret))}
            >
              {isRecording ? (
                <>
                  <Square className="w-4 h-4 mr-2" /> 停止录音
                </>
              ) : isRecognizing ? (
                <>
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" /> 识别中...
                </>
              ) : (
                <>
                  <Mic className="w-4 h-4 mr-2" /> 开始测试
                </>
              )}
            </Button>
            
            {isRecording && (
              <span className="text-xs text-red-500 font-medium animate-pulse">
                正在录音，点击停止后开始识别...
              </span>
            )}
          </div>
          
          {recognizedText && (
            <div className="p-3 rounded-md bg-green-50 border border-green-200">
              <p className="text-xs text-muted-foreground mb-1">识别结果：</p>
              <p className="text-sm font-medium text-green-700">{recognizedText}</p>
            </div>
          )}
          
          {(!baiduApiKey || !baiduSecretKey) && speechEngine === 'baidu' && (
            <div className="p-3 rounded-md bg-yellow-50 border border-yellow-200 text-xs text-yellow-700">
              请先在下方配置百度语音 API 后再进行测试
            </div>
          )}
          {(!xunfeiAppId || !xunfeiApiKey || !xunfeiApiSecret) && speechEngine === 'xunfei' && (
            <div className="p-3 rounded-md bg-yellow-50 border border-yellow-200 text-xs text-yellow-700">
              请先在下方配置讯飞语音 API 后再进行测试
            </div>
          )}
        </CardContent>
      </Card>

      {/* 百度语音 API 配置 */}
      <Card className="glass border-none shadow-custom">
        <CardHeader className="pb-3 border-b border-border/40">
           <CardTitle className="text-base font-semibold text-primary flex items-center gap-2">
              <Globe className="w-4 h-4" />
              百度语音 API 配置
           </CardTitle>
        </CardHeader>
        <CardContent className="pt-4 space-y-4">
           <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div className="space-y-2">
                 <Label>APP_ID</Label>
                 <Input 
                   placeholder="请输入 APP_ID" 
                   className="bg-white/50" 
                   value={baiduAppId}
                   onChange={(e) => updateConfig('baidu_app_id', e.target.value)}
                 />
              </div>
              <div className="space-y-2">
                 <Label>API_KEY</Label>
                 <Input 
                   placeholder="请输入 API_KEY" 
                   className="bg-white/50" 
                   value={baiduApiKey}
                   onChange={(e) => updateConfig('baidu_api_key', e.target.value)}
                 />
              </div>
           </div>
           <div className="space-y-2">
              <Label>SECRET_KEY</Label>
              <Input 
                type="password" 
                placeholder="请输入 SECRET_KEY" 
                className="bg-white/50" 
                value={baiduSecretKey}
                onChange={(e) => updateConfig('baidu_secret_key', e.target.value)}
              />
           </div>

           <div className="flex items-center gap-3 pt-2">
              <span className={`text-sm flex items-center gap-1 px-2 py-1 rounded ${
                baiduApiKey && baiduSecretKey 
                  ? 'text-green-600 bg-green-50' 
                  : 'text-orange-500 bg-orange-50'
              }`}>
                 {baiduApiKey && baiduSecretKey ? (
                   <><CheckCircle2 className="w-4 h-4" /> 已配置</>
                 ) : (
                   <><AlertTriangle className="w-4 h-4" /> 未配置</>
                 )}
              </span>
              <Button 
                className="bg-blue-500 hover:bg-blue-600 text-white h-8"
                onClick={testBaiduConnection}
                disabled={testStatus === 'testing' || !baiduApiKey || !baiduSecretKey}
              >
                {testStatus === 'testing' ? (
                  <><Loader2 className="w-4 h-4 mr-2 animate-spin" /> 测试中...</>
                ) : (
                  '测试连接'
                )}
              </Button>
              <Button variant="outline" className="border-blue-200 text-blue-600 hover:bg-blue-50 h-8" onClick={() => window.open('https://console.bce.baidu.com/ai/#/ai/speech/app/list', '_blank')}>
                申请API
              </Button>
           </div>

           {testStatus !== 'idle' && speechEngine === 'baidu' && (
             <div className={`p-3 rounded-md text-xs border ${
               testStatus === 'success' 
                 ? 'bg-green-50 border-green-200 text-green-700' 
                 : testStatus === 'error'
                   ? 'bg-red-50 border-red-200 text-red-700'
                   : 'bg-blue-50 border-blue-200 text-blue-700'
             }`}>
               {testMessage}
             </div>
           )}

           <div className="bg-yellow-50/50 p-3 rounded-md text-xs text-muted-foreground border border-yellow-100">
              <p className="font-semibold mb-1 flex items-center gap-1 text-yellow-700">
                 <AlertTriangle className="w-3 h-3" /> 百度语音 API 申请指引:
              </p>
              <ol className="list-decimal pl-4 space-y-1">
                 <li>访问: <a href="https://console.bce.baidu.com/ai/#/ai/speech/app/list" target="_blank" rel="noreferrer" className="text-blue-500 hover:underline">百度智能云控制台</a></li>
                 <li>登录百度账号，创建应用</li>
                 <li>获取 APP_ID, API_KEY, SECRET_KEY</li>
                 <li>填入上方输入框即可自动保存</li>
              </ol>
           </div>
        </CardContent>
      </Card>

      {/* 讯飞语音识别 API 配置 */}
      <Card className="glass border-none shadow-custom">
        <CardHeader className="pb-3 border-b border-border/40">
           <CardTitle className="text-base font-semibold text-primary flex items-center gap-2">
              <Globe className="w-4 h-4" />
              讯飞语音识别 API 配置
           </CardTitle>
        </CardHeader>
        <CardContent className="pt-4 space-y-4">
           <div className="space-y-2">
              <Label>APP_ID</Label>
              <Input 
                className="bg-white/50" 
                value={xunfeiAppId}
                onChange={(e) => updateConfig('xunfei_app_id', e.target.value)}
              />
           </div>
           <div className="space-y-2">
              <Label>API_KEY</Label>
              <Input 
                className="bg-white/50" 
                value={xunfeiApiKey}
                onChange={(e) => updateConfig('xunfei_api_key', e.target.value)}
              />
           </div>
           <div className="space-y-2">
              <Label>API_SECRET</Label>
              <Input 
                className="bg-white/50" 
                value={xunfeiApiSecret}
                onChange={(e) => updateConfig('xunfei_api_secret', e.target.value)}
              />
           </div>

           <div className="flex items-center gap-3 pt-2">
              <span className={`text-sm flex items-center gap-1 px-2 py-1 rounded ${
                xunfeiAppId && xunfeiApiKey && xunfeiApiSecret
                  ? 'text-green-600 bg-green-50' 
                  : 'text-orange-500 bg-orange-50'
              }`}>
                 {xunfeiAppId && xunfeiApiKey && xunfeiApiSecret ? (
                   <><CheckCircle2 className="w-4 h-4" /> 已配置</>
                 ) : (
                   <><AlertTriangle className="w-4 h-4" /> 未配置</>
                 )}
              </span>
              <Button 
                className="bg-blue-500 hover:bg-blue-600 text-white h-8"
                onClick={testXunfeiConnection}
                disabled={testStatus === 'testing' || !xunfeiAppId || !xunfeiApiKey || !xunfeiApiSecret}
              >
                {testStatus === 'testing' ? (
                  <><Loader2 className="w-4 h-4 mr-2 animate-spin" /> 测试中...</>
                ) : (
                  '测试连接'
                )}
              </Button>
              <Button variant="outline" className="border-blue-200 text-blue-600 hover:bg-blue-50 h-8" onClick={() => window.open('https://www.xfyun.cn/', '_blank')}>
                申请API
              </Button>
           </div>

           <div className="text-center text-xs text-muted-foreground pt-1">
              📍 访问讯飞开放平台申请语音听写（流式版）API
           </div>

           {testStatus !== 'idle' && speechEngine === 'xunfei' && (
             <div className={`p-3 rounded-md text-xs border ${
               testStatus === 'success' 
                 ? 'bg-green-50 border-green-200 text-green-700' 
                 : testStatus === 'error'
                   ? 'bg-red-50 border-red-200 text-red-700'
                   : 'bg-blue-50 border-blue-200 text-blue-700'
             }`}>
               {testMessage}
             </div>
           )}
        </CardContent>
      </Card>
    </div>
  );
};

export default VoiceInputSettings;