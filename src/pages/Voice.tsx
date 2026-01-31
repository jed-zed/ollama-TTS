import React from 'react';
import { Mic2, Play, Square, Settings, Volume2, Wand2, UserPlus, Save, RotateCcw, Mic } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle, CardDescription, CardFooter } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import { Label } from '@/components/ui/label';
import { Slider } from '@/components/ui/slider';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import VoiceInputSettings from '@/components/VoiceInputSettings';

const Voice = () => {
  return (
    <div className="h-full flex flex-col gap-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div>
        <h2 className="text-3xl font-bold bg-gradient-to-r from-pink-500 to-purple-600 bg-clip-text text-transparent">语音配置</h2>
        <p className="text-muted-foreground mt-2">管理语音合成 (TTS) 与语音识别 (ASR) 的相关设置。</p>
      </div>

      <Tabs defaultValue="tts" className="w-full">
         <TabsList className="bg-white/50 p-1 mb-6 h-12 w-full max-w-[400px] justify-start border border-white/40">
            <TabsTrigger value="tts" className="h-10 px-6 flex items-center gap-2">
               <Volume2 className="w-4 h-4" /> 语音合成 (TTS)
            </TabsTrigger>
            <TabsTrigger value="asr" className="h-10 px-6 flex items-center gap-2">
               <Mic className="w-4 h-4" /> 语音输入 (ASR)
            </TabsTrigger>
         </TabsList>

         <TabsContent value="tts" className="mt-0">
            <div className="grid grid-cols-12 gap-6">
              {/* Left Column: Basic Settings & Volume */}
              <div className="col-span-12 lg:col-span-4 space-y-6">
                  <Card className="glass border-none shadow-custom">
                     <CardHeader>
                       <CardTitle className="text-lg">基础设置</CardTitle>
                     </CardHeader>
                     <CardContent className="space-y-4">
                        <div className="flex items-center justify-between p-3 bg-white/50 rounded-lg">
                           <Label>启用 TTS 输出</Label>
                           <Switch defaultChecked />
                        </div>
                        <div className="flex items-center justify-between p-3 bg-white/50 rounded-lg">
                           <Label>自动播放</Label>
                           <Switch defaultChecked />
                        </div>
                        <div className="flex items-center justify-between p-3 bg-white/50 rounded-lg">
                           <Label>保存音频文件</Label>
                           <Switch />
                        </div>
                        <div className="space-y-2 pt-2">
                           <Label>播放模式</Label>
                           <Select defaultValue="dialog">
                              <SelectTrigger className="bg-white/50">
                                 <SelectValue />
                              </SelectTrigger>
                              <SelectContent>
                                 <SelectItem value="dialog">只播放对话 (Dialog Only)</SelectItem>
                                 <SelectItem value="full">播放全文 (Full Text)</SelectItem>
                              </SelectContent>
                           </Select>
                        </div>
                     </CardContent>
                  </Card>

                  <Card className="glass border-none shadow-custom">
                     <CardHeader>
                       <CardTitle className="text-lg flex items-center gap-2">
                          <Volume2 className="w-5 h-5 text-green-500" />
                          音量控制
                       </CardTitle>
                     </CardHeader>
                     <CardContent className="space-y-6">
                        <div className="space-y-2">
                           <div className="flex justify-between">
                              <Label>主音量</Label>
                              <span className="text-xs text-muted-foreground">80%</span>
                           </div>
                           <Slider defaultValue={[80]} max={100} step={1} className="py-2" />
                        </div>
                        <div className="flex gap-2">
                           <Button variant="outline" className="flex-1 border-red-200 text-red-500 hover:bg-red-50 hover:text-red-600">
                              <Square className="w-4 h-4 mr-2" /> 停止播放
                           </Button>
                           <span className="flex items-center px-3 text-xs text-muted-foreground bg-muted rounded-md flex-1 justify-center">
                              空闲
                           </span>
                        </div>
                     </CardContent>
                  </Card>
              </div>

              {/* Right Column: Engine Config */}
              <div className="col-span-12 lg:col-span-8 space-y-6">
                 <Tabs defaultValue="gpt-sovits" className="w-full">
                    <TabsList className="bg-white/50 p-1 mb-4 h-12 w-full justify-start border border-white/40">
                       <TabsTrigger value="gpt-sovits" className="h-10 px-6">GPT-SoVITS</TabsTrigger>
                       <TabsTrigger value="indextts" className="h-10 px-6">IndexTTS</TabsTrigger>
                       <TabsTrigger value="xf" className="h-10 px-6">讯飞语音</TabsTrigger>
                    </TabsList>

                    <TabsContent value="gpt-sovits" className="space-y-6">
                       {/* API Config */}
                       <Card className="glass border-none shadow-custom">
                          <CardHeader className="pb-3">
                             <div className="flex justify-between items-center">
                                <CardTitle className="text-base font-semibold text-primary">API 配置</CardTitle>
                                <Button size="sm" variant="ghost" className="h-8 text-xs text-blue-500 hover:text-blue-600">测试连接</Button>
                             </div>
                          </CardHeader>
                          <CardContent>
                             <div className="flex gap-3">
                                <Input defaultValue="http://192.168.100.171:9880/tts" className="bg-white/50 font-mono text-sm" />
                                <Button variant="secondary" className="bg-blue-50 text-blue-600 hover:bg-blue-100">配置</Button>
                             </div>
                          </CardContent>
                       </Card>

                       {/* Character Management */}
                       <Card className="glass border-none shadow-custom">
                          <CardHeader className="pb-2">
                             <CardTitle className="text-base font-semibold text-purple-600 flex items-center gap-2">
                                <UserPlus className="w-4 h-4" />
                                角色音色管理
                             </CardTitle>
                             <CardDescription>配置精调模型和参考音频以克隆目标声线</CardDescription>
                          </CardHeader>
                          <CardContent className="space-y-4">
                             <div className="grid grid-cols-2 gap-4">
                                <div className="space-y-2">
                                   <Label>当前角色</Label>
                                   <Select defaultValue="paimon">
                                      <SelectTrigger className="bg-white/50">
                                         <SelectValue placeholder="选择角色" />
                                      </SelectTrigger>
                                      <SelectContent>
                                         <SelectItem value="paimon">普通</SelectItem>
                                         <SelectItem value="nahida">纳西妲</SelectItem>
                                      </SelectContent>
                                   </Select>
                                </div>
                                <div className="space-y-2">
                                   <Label>角色名称</Label>
                                   <Input className="bg-white/50" placeholder="Display Name" />
                                </div>
                             </div>

                             <div className="space-y-2">
                                <Label>GPT模型路径 (.ckpt)</Label>
                                <div className="flex gap-2">
                                   <Input className="bg-white/50 text-xs text-muted-foreground" value="普连/普连-e18.ckpt" readOnly />
                                   <Button variant="outline" size="sm" className="w-16">浏览</Button>
                                </div>
                             </div>

                             <div className="space-y-2">
                                <Label>SoVITS模型路径 (.pth)</Label>
                                <div className="flex gap-2">
                                   <Input className="bg-white/50 text-xs text-muted-foreground" value="普连/普连_e25_s5350.pth" readOnly />
                                   <Button variant="outline" size="sm" className="w-16">浏览</Button>
                                </div>
                             </div>

                             <div className="flex gap-2 pt-2">
                                <Button className="flex-1 bg-gradient-to-r from-green-400 to-emerald-500 text-white border-0 shadow-sm hover:opacity-90">
                                   <UserPlus className="w-4 h-4 mr-2" /> 添加角色
                                </Button>
                                <Button variant="outline" className="flex-1 border-blue-200 text-blue-600 hover:bg-blue-50">
                                   <RotateCcw className="w-4 h-4 mr-2" /> 更新
                                </Button>
                                <Button variant="outline" className="flex-1 border-orange-200 text-orange-600 hover:bg-orange-50">
                                   <Save className="w-4 h-4 mr-2" /> 保存
                                </Button>
                             </div>
                          </CardContent>
                       </Card>

                       {/* Test Area */}
                       <Card className="glass border-none shadow-custom">
                          <CardHeader className="pb-3">
                             <CardTitle className="text-base font-semibold">合成测试</CardTitle>
                          </CardHeader>
                          <CardContent className="space-y-4">
                             <Textarea
                                className="bg-white/50 resize-none"
                                placeholder="输入测试文本..."
                                defaultValue="哈......你这家伙，真是有意思呢。"
                             />
                             <div className="flex gap-3 items-center">
                                <div className="flex-1">
                                   <Select defaultValue="calm">
                                      <SelectTrigger className="bg-white/50">
                                         <SelectValue />
                                      </SelectTrigger>
                                      <SelectContent>
                                         <SelectItem value="calm">冷静 (Calm)</SelectItem>
                                         <SelectItem value="angry">生气 (Angry)</SelectItem>
                                         <SelectItem value="happy">开心 (Happy)</SelectItem>
                                      </SelectContent>
                                   </Select>
                                </div>
                                <Button className="bg-pink-500 hover:bg-pink-600 text-white min-w-[120px]">
                                   <Wand2 className="w-4 h-4 mr-2" /> 测试合成
                                </Button>
                             </div>
                          </CardContent>
                       </Card>
                    </TabsContent>
                 </Tabs>
              </div>
            </div>
         </TabsContent>

         <TabsContent value="asr" className="mt-0">
            <div className="max-w-4xl mx-auto">
               <VoiceInputSettings />
            </div>
         </TabsContent>
      </Tabs>
    </div>
  );
};

export default Voice;