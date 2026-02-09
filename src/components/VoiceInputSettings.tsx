import { useState, useEffect } from 'react';
import { Mic, CheckCircle2, AlertTriangle, Globe, Square, Loader2, MessageSquare } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useConfig, useConfigStore } from '@/stores/configStore';
import { API_BASE_URL } from '@/lib/constants';
import { toast } from 'sonner';
import { speechService } from '@/services/speech';

const VoiceInputSettings = () => {
  const config = useConfig();
  const updateConfig = useConfigStore((state) => state.updateConfig);
  
  const [isRecording, setIsRecording] = useState(false);
  const [testStatus, setTestStatus] = useState<'idle' | 'testing' | 'success' | 'error'>('idle');
  const [testMessage, setTestMessage] = useState('');
  const [recognizedText, setRecognizedText] = useState<string>('');
  const [isRecognizing, setIsRecognizing] = useState(false);

  // 百度配置状态
  const [baiduAppId, setBaiduAppId] = useState('');
  const [baiduApiKey, setBaiduApiKey] = useState('');
  const [baiduSecretKey, setBaiduSecretKey] = useState('');

  // 讯飞配置状态
  const [xunfeiAppId, setXunfeiAppId] = useState('');
  const [xunfeiApiKey, setXunfeiApiKey] = useState('');
  const [xunfeiApiSecret, setXunfeiApiSecret] = useState('');

  // 初始化配置
  useEffect(() => {
    if (config) {
      setBaiduAppId(config.baidu_app_id || '');
      setBaiduApiKey(config.baidu_api_key || '');
      setBaiduSecretKey(config.baidu_secret_key || '');
      
      setXunfeiAppId(config.xunfei_app_id || '');
      setXunfeiApiKey(config.xunfei_api_key || '');
      setXunfeiApiSecret(config.xunfei_api_secret || '');
    }
  }, [config]);

  // 保存百度配置
  const handleSaveBaidu = async () => {
    await updateConfig('baidu_app_id', baiduAppId);
    await updateConfig('baidu_api_key', baiduApiKey);
    await updateConfig('baidu_secret_key', baiduSecretKey);
    toast.success('百度语音配置已保存');
  };

  // 保存讯飞配置
  const handleSaveXunfei = async () => {
    await updateConfig('xunfei_app_id', xunfeiAppId);
    await updateConfig('xunfei_api_key', xunfeiApiKey);
    await updateConfig('xunfei_api_secret', xunfeiApiSecret);
    toast.success('讯飞语音配置已保存');
  };

  // 语音输入测试
  const startVoiceInput = async () => {
    if (!baiduApiKey || !baiduSecretKey) {
      toast.error('请先配置百度语音 API');
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
      
      const result = await speechService.recognize(audioBlob, {
        apiKey: baiduApiKey,
        secretKey: baiduSecretKey,
      });

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

  return (
    <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
      
      {/* 语音输入测试区域 */}
      <Card className="glass border-none shadow-custom bg-gradient-to-br from-blue-50/50 to-purple-50/50">
        <CardHeader className="pb-3 border-b border-border/40">
           <CardTitle className="text-base font-semibold text-primary flex items-center gap-2">
              <MessageSquare className="w-4 h-4" />
              语音输入测试
           </CardTitle>
           <CardDescription className="text-xs">
             录音并识别语音内容，测试语音识别功能是否正常
           </CardDescription>
        </CardHeader>
        <CardContent className="pt-4 space-y-4">
          <div className="flex items-center gap-4">
            <Button 
              variant={isRecording ? "destructive" : "default"}
              className={`w-32 transition-all ${isRecording ? 'animate-pulse' : ''}`}
              onClick={isRecording ? stopVoiceInput : startVoiceInput}
              disabled={isRecognizing || (!baiduApiKey || !baiduSecretKey)}
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
          
          {(!baiduApiKey || !baiduSecretKey) && (
            <div className="p-3 rounded-md bg-yellow-50 border border-yellow-200 text-xs text-yellow-700">
              请先在下方配置百度语音 API 后再进行测试
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
                   onChange={(e) => setBaiduAppId(e.target.value)}
                 />
              </div>
              <div className="space-y-2">
                 <Label>API_KEY</Label>
                 <Input 
                   placeholder="请输入 API_KEY" 
                   className="bg-white/50" 
                   value={baiduApiKey}
                   onChange={(e) => setBaiduApiKey(e.target.value)}
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
                onChange={(e) => setBaiduSecretKey(e.target.value)}
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
                className="bg-green-500 hover:bg-green-600 text-white h-8"
                onClick={handleSaveBaidu}
              >
                保存配置
              </Button>
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

           {testStatus !== 'idle' && (
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
                 <li>填入上方输入框并保存</li>
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
                onChange={(e) => setXunfeiAppId(e.target.value)}
              />
           </div>
           <div className="space-y-2">
              <Label>API_KEY</Label>
              <Input 
                className="bg-white/50" 
                value={xunfeiApiKey}
                onChange={(e) => setXunfeiApiKey(e.target.value)}
              />
           </div>
           <div className="space-y-2">
              <Label>API_SECRET</Label>
              <Input 
                className="bg-white/50" 
                value={xunfeiApiSecret}
                onChange={(e) => setXunfeiApiSecret(e.target.value)}
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
                className="bg-green-500 hover:bg-green-600 text-white h-8"
                onClick={handleSaveXunfei}
              >
                保存配置
              </Button>
              <Button variant="outline" className="border-blue-200 text-blue-600 hover:bg-blue-50 h-8" onClick={() => window.open('https://www.xfyun.cn/', '_blank')}>
                申请API
              </Button>
           </div>

           <div className="text-center text-xs text-muted-foreground pt-1">
              📍 访问讯飞开放平台申请语音听写（流式版）API
           </div>
        </CardContent>
      </Card>
    </div>
  );
};

export default VoiceInputSettings;