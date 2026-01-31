import React, { useState } from 'react';
import { Mic, Save, CheckCircle2, AlertTriangle, Settings, Globe, Download, FolderOpen, Play, Square, FileAudio } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import { Textarea } from '@/components/ui/textarea';

const VoiceInputSettings = () => {
  const [engine, setEngine] = useState('auto');
  const [isRecording, setIsRecording] = useState(false);

  return (
    <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">

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
                 <Input defaultValue="121915668" className="bg-white/50" />
              </div>
              <div className="space-y-2">
                 <Label>API_KEY</Label>
                 <Input defaultValue="9lwircwCxqKkwnam4VCmCMfT" className="bg-white/50" />
              </div>
           </div>
           <div className="space-y-2">
              <Label>SECRET_KEY</Label>
              <Input type="password" defaultValue="************************" className="bg-white/50" />
           </div>

           <div className="flex items-center gap-3 pt-2">
              <span className="text-sm text-green-600 flex items-center gap-1 bg-green-50 px-2 py-1 rounded">
                 <CheckCircle2 className="w-4 h-4" /> 已配置
              </span>
              <Button className="bg-green-500 hover:bg-green-600 text-white h-8">保存配置</Button>
              <Button variant="outline" className="border-blue-200 text-blue-600 hover:bg-blue-50 h-8">申请API</Button>
           </div>

           <div className="bg-yellow-50/50 p-3 rounded-md text-xs text-muted-foreground border border-yellow-100">
              <p className="font-semibold mb-1 flex items-center gap-1 text-yellow-700">
                 <AlertTriangle className="w-3 h-3" /> 百度语音 API 申请指引:
              </p>
              <ol className="list-decimal pl-4 space-y-1">
                 <li>访问: <a href="#" className="text-blue-500 hover:underline">https://console.bce.baidu.com/ai/#/ai/speech/app/list</a></li>
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
              <Input className="bg-white/50" />
           </div>
           <div className="space-y-2">
              <Label>API_KEY</Label>
              <Input className="bg-white/50" />
           </div>
           <div className="space-y-2">
              <Label>API_SECRET</Label>
              <Input className="bg-white/50" />
           </div>

           <div className="flex items-center gap-3 pt-2">
              <span className="text-sm text-orange-500 flex items-center gap-1 bg-orange-50 px-2 py-1 rounded">
                 <AlertTriangle className="w-4 h-4" /> 未配置
              </span>
              <Button className="bg-green-500 hover:bg-green-600 text-white h-8">保存配置</Button>
              <Button variant="outline" className="border-blue-200 text-blue-600 hover:bg-blue-50 h-8">申请API</Button>
              <Button className="bg-blue-500 hover:bg-blue-600 text-white h-8">测试连接</Button>
           </div>

           <div className="text-center text-xs text-muted-foreground pt-1">
              📍 访问讯飞开放平台申请语音听写（流式版）API
           </div>
        </CardContent>
      </Card>

      {/* 引擎选择 & 状态 */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
         {/* 引擎选择 */}
         <Card className="glass border-none shadow-custom">
            <CardHeader className="pb-3">
               <CardTitle className="text-base font-semibold">引擎选择</CardTitle>
            </CardHeader>
            <CardContent>
               <RadioGroup defaultValue="auto" value={engine} onValueChange={setEngine} className="space-y-3">
                  <div className="flex items-center space-x-2">
                     <RadioGroupItem value="auto" id="r-auto" />
                     <Label htmlFor="r-auto" className="cursor-pointer font-normal">自动切换 (推荐) <span className="text-xs text-muted-foreground ml-2">- 优先使用百度/讯飞，失败时自动切换</span></Label>
                  </div>
                  <div className="flex items-center space-x-2">
                     <RadioGroupItem value="baidu" id="r-baidu" />
                     <Label htmlFor="r-baidu" className="cursor-pointer font-normal">仅百度 <span className="text-xs text-muted-foreground ml-2">- 需要网络连接，识别率高</span></Label>
                  </div>
                  <div className="flex items-center space-x-2">
                     <RadioGroupItem value="xf" id="r-xf" />
                     <Label htmlFor="r-xf" className="cursor-pointer font-normal">仅讯飞 <span className="text-xs text-muted-foreground ml-2">- 需要网络连接，识别率高</span></Label>
                  </div>
                  <div className="flex items-center space-x-2">
                     <RadioGroupItem value="vosk" id="r-vosk" />
                     <Label htmlFor="r-vosk" className="cursor-pointer font-normal">仅 Vosk <span className="text-xs text-muted-foreground ml-2">- 离线识别，需要下载模型</span></Label>
                  </div>
               </RadioGroup>
            </CardContent>
         </Card>

         {/* 引擎状态 */}
         <Card className="glass border-none shadow-custom">
            <CardHeader className="pb-3">
               <CardTitle className="text-base font-semibold">引擎状态</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
               <div className="flex items-center justify-between">
                  <span className="text-sm">百度语音:</span>
                  <div className="flex items-center gap-2">
                     <span className="text-xs text-green-600 flex items-center gap-1 bg-green-50 px-2 py-0.5 rounded border border-green-100"><CheckCircle2 className="w-3 h-3" /> 可用</span>
                     <Button size="sm" variant="outline" className="h-6 text-xs text-blue-600 border-blue-200">测试连接</Button>
                  </div>
               </div>
               <div className="flex items-center justify-between">
                  <span className="text-sm">讯飞语音:</span>
                  <div className="flex items-center gap-2">
                     <span className="text-xs text-muted-foreground flex items-center gap-1 bg-gray-100 px-2 py-0.5 rounded border border-gray-200">⌛ 未检测</span>
                     <Button size="sm" variant="outline" className="h-6 text-xs text-blue-600 border-blue-200">测试连接</Button>
                  </div>
               </div>
               <div className="flex items-center justify-between">
                  <span className="text-sm">Vosk 状态:</span>
                  <span className="text-xs text-orange-500 flex items-center gap-1 bg-orange-50 px-2 py-0.5 rounded border border-orange-100"><AlertTriangle className="w-3 h-3" /> 未配置</span>
               </div>
               <Separator />
               <div className="flex items-center justify-between">
                  <span className="text-sm font-medium">当前引擎:</span>
                  <span className="text-xs text-blue-600 flex items-center gap-1 font-bold bg-blue-50 px-2 py-1 rounded">🤖 自动</span>
               </div>
            </CardContent>
         </Card>
      </div>

      {/* Vosk 离线模型配置 */}
      <Card className="glass border-none shadow-custom">
        <CardHeader className="pb-3 border-b border-border/40">
           <CardTitle className="text-base font-semibold text-primary flex items-center gap-2">
              <FolderOpen className="w-4 h-4" />
              Vosk 离线模型配置
           </CardTitle>
        </CardHeader>
        <CardContent className="pt-4 space-y-4">
           <div className="flex gap-2">
              <Input placeholder="未配置" disabled className="bg-gray-50" />
              <Button className="bg-green-600 hover:bg-green-700 text-white whitespace-nowrap">选择文件夹</Button>
              <Button className="bg-orange-500 hover:bg-orange-600 text-white whitespace-nowrap">下载模型</Button>
           </div>

           <div className="bg-blue-50/50 p-3 rounded-md text-xs text-muted-foreground border border-blue-100">
              <p className="font-semibold mb-1 flex items-center gap-1 text-blue-700">
                 <Download className="w-3 h-3" /> Vosk 模型下载指引:
              </p>
              <ol className="list-decimal pl-4 space-y-1">
                 <li>群文件下载或访问: <a href="#" className="text-blue-500 hover:underline">https://alphacephei.com/vosk/models</a></li>
                 <li>下载对应语言的模型:
                    <ul className="list-disc pl-4 mt-1 space-y-1 text-gray-500">
                       <li>中文: vosk-model-cn-0.22 (约 1.3GB)</li>
                       <li>英文: vosk-model-en-us-0.22 (约 1.8GB)</li>
                       <li>小型中文: vosk-model-small-cn-0.22 (约 42MB, 识别率稍低)</li>
                    </ul>
                 </li>
                 <li>解压到任意文件夹，路径不要有任何中文</li>
                 <li>点击"选择文件夹"选择解压后的模型目录</li>
              </ol>
           </div>
        </CardContent>
      </Card>

      {/* 语音输入控制 */}
      <Card className="glass border-none shadow-custom border-2 border-primary/20">
        <CardHeader className="pb-3">
           <CardTitle className="text-base font-semibold flex items-center gap-2">
              <Mic className="w-5 h-5 text-primary" />
              语音输入控制
           </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
           <div className="flex gap-4 items-center">
              <Button
                 className={`h-12 px-8 text-lg font-bold transition-all duration-300 ${isRecording ? 'bg-red-500 hover:bg-red-600 animate-pulse' : 'bg-green-600 hover:bg-green-700'}`}
                 onClick={() => setIsRecording(!isRecording)}
              >
                 {isRecording ? (
                    <><Square className="w-5 h-5 mr-2" /> 停止录音</>
                 ) : (
                    <><Play className="w-5 h-5 mr-2" /> 开始录音</>
                 )}
              </Button>

              <div className="flex items-center gap-2 bg-white/50 px-3 py-2 rounded-md border border-border/50">
                 <span className="text-sm text-muted-foreground">语言:</span>
                 <Select defaultValue="zh-CN">
                    <SelectTrigger className="w-[100px] h-8 bg-transparent border-none shadow-none focus:ring-0">
                       <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                       <SelectItem value="zh-CN">简体中文</SelectItem>
                       <SelectItem value="en-US">English</SelectItem>
                       <SelectItem value="ja-JP">日本語</SelectItem>
                    </SelectContent>
                 </Select>
              </div>
           </div>

           <div className="relative">
              <Input
                 className="bg-white/80 h-10 pr-10 font-mono text-sm"
                 placeholder="等待语音输入..."
                 readOnly
                 value={isRecording ? "正在聆听..." : ""}
              />
              <FileAudio className={`absolute right-3 top-2.5 w-5 h-5 text-muted-foreground ${isRecording ? 'text-green-500 animate-pulse' : ''}`} />
           </div>
        </CardContent>
      </Card>

    </div>
  );
};

export default VoiceInputSettings;
