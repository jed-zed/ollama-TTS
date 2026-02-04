import React, { useEffect, useState, useCallback, useRef } from 'react';
import { Mic2, Play, Square, Settings, Volume2, Wand2, UserPlus, Save, RotateCcw, Mic, FolderOpen, Trash2, AlertTriangle, Plus, ChevronRight, Check, Loader2 } from 'lucide-react';
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
import { Checkbox } from '@/components/ui/checkbox';
import { Separator } from '@/components/ui/separator';
import { useConfigStore, useConfig, useConfigLoading } from '@/stores/configStore';
import { useTTSStore } from '@/stores/ttsStore';
import type { GSVCharacter } from '@/services/types';

const Voice = () => {
  // 从 store 获取状态
  const config = useConfig();
  const isLoading = useConfigLoading();
  const loadConfig = useConfigStore((state) => state.loadConfig);
  const updateConfig = useConfigStore((state) => state.updateConfig);
  const updateConfigBatch = useConfigStore((state) => state.updateConfigBatch);

  // TTS store - 使用单独的 selector 避免无限循环
  const isPlaying = useTTSStore((state) => state.isPlaying);
  const isSynthesizing = useTTSStore((state) => state.isSynthesizing);
  const engineStatus = useTTSStore((state) => state.engineStatus);
  const testEngine = useTTSStore((state) => state.testEngine);
  const stopPlayback = useTTSStore((state) => state.stopPlayback);
  const setTTSVolume = useTTSStore((state) => state.setVolume);

  // 本地编辑状态
  const [localGsvApiUrl, setLocalGsvApiUrl] = useState('');
  const [isTesting, setIsTesting] = useState(false);
  const [testResult, setTestResult] = useState<'success' | 'fail' | null>(null);

  // 当前编辑的角色
  const [selectedCharacterId, setSelectedCharacterId] = useState<string>('');
  const [editingCharacter, setEditingCharacter] = useState<Partial<GSVCharacter>>({});

  // 推理参数本地状态
  const [localTopK, setLocalTopK] = useState('50');
  const [localTopP, setLocalTopP] = useState('0.9');
  const [localTemp, setLocalTemp] = useState('1.0');

  // 初始化
  useEffect(() => {
    loadConfig();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 同步配置到本地状态 - 使用 useRef 避免触发重渲染
  const initializedRef = useRef(false);
  useEffect(() => {
    if (config && !initializedRef.current) {
      setLocalGsvApiUrl(config.gsv_api_url || '');
      setLocalTopK(String(config.gsv_top_k || 50));
      setLocalTopP(String(config.gsv_top_p || 0.9));
      setLocalTemp(String(config.gsv_temperature || 1.0));

      // 设置默认选中的角色
      if (config.gsv_current_character) {
        setSelectedCharacterId(config.gsv_current_character);
      }
      initializedRef.current = true;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config]);

  // 当选中角色变化时，更新编辑状态 - 只依赖 selectedCharacterId 避免无限循环
  useEffect(() => {
    const characterData = config?.gsv_characters?.[selectedCharacterId];
    if (selectedCharacterId && characterData) {
      setEditingCharacter(characterData);
    } else {
      setEditingCharacter({});
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedCharacterId]);

  // 获取角色列表
  const characters = config?.gsv_characters || {};
  const characterList = Object.entries(characters).map(([id, char]) => ({
    id,
    ...char,
  }));

  // 测试 GPT-SoVITS 连接
  const handleTestGSV = async () => {
    setIsTesting(true);
    setTestResult(null);
    try {
      const available = await testEngine('gpt_sovits');
      setTestResult(available ? 'success' : 'fail');
    } finally {
      setIsTesting(false);
    }
  };

  // 停止播放
  const handleStopPlayback = async () => {
    await stopPlayback();
  };

  // 音量变化
  const handleVolumeChange = useCallback(async (value: number[]) => {
    const volume = value[0] / 100;
    await updateConfig('tts_volume', volume);
    await setTTSVolume(volume);
  }, [updateConfig, setTTSVolume]);

  // 保存角色
  const handleSaveCharacter = async () => {
    if (!selectedCharacterId || !editingCharacter.name) return;

    const updatedCharacters = {
      ...characters,
      [selectedCharacterId]: editingCharacter as GSVCharacter,
    };
    await updateConfig('gsv_characters', updatedCharacters);
  };

  // 添加新角色
  const handleAddCharacter = async () => {
    const newId = `char_${Date.now()}`;
    const newCharacter: GSVCharacter = {
      id: newId,
      name: '新角色',
      ref_audio_dir: '',
      gpt_model_path: '',
      sovits_model_path: '',
    };
    const updatedCharacters = {
      ...characters,
      [newId]: newCharacter,
    };
    await updateConfig('gsv_characters', updatedCharacters);
    setSelectedCharacterId(newId);
  };

  // 删除角色
  const handleDeleteCharacter = async () => {
    if (!selectedCharacterId) return;

    const { [selectedCharacterId]: _, ...rest } = characters;
    await updateConfig('gsv_characters', rest);

    // 选择下一个角色
    const remainingIds = Object.keys(rest);
    setSelectedCharacterId(remainingIds[0] || '');
  };

  // 应用推理参数
  const handleApplyParams = async () => {
    await updateConfigBatch({
      gsv_top_k: parseInt(localTopK) || 50,
      gsv_top_p: parseFloat(localTopP) || 0.9,
      gsv_temperature: parseFloat(localTemp) || 1.0,
    });
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-8 h-8 animate-spin text-primary" />
        <span className="ml-2 text-muted-foreground">加载配置中...</span>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col gap-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div>
        <h2 className="text-3xl font-bold bg-gradient-to-r from-pink-500 to-purple-600 bg-clip-text text-transparent">语音配置</h2>
        <p className="text-muted-foreground mt-2">打造个性化的语音交互体验，支持多种高性能合成引擎。</p>
      </div>

      <Tabs defaultValue="tts" className="w-full space-y-6">
         <TabsList className="bg-white/50 p-1 h-12 w-full max-w-[400px] justify-start border border-white/40 shadow-sm rounded-xl">
            <TabsTrigger value="tts" className="h-10 px-6 rounded-lg data-[state=active]:bg-white data-[state=active]:shadow-md transition-all flex items-center gap-2">
               <Volume2 className="w-4 h-4" /> 语音合成 (TTS)
            </TabsTrigger>
            <TabsTrigger value="asr" className="h-10 px-6 rounded-lg data-[state=active]:bg-white data-[state=active]:shadow-md transition-all flex items-center gap-2">
               <Mic className="w-4 h-4" /> 语音输入 (ASR)
            </TabsTrigger>
         </TabsList>

         <TabsContent value="tts" className="mt-0 outline-none">
            <div className="grid grid-cols-12 gap-6">

              {/* === Left Column: Global Settings === */}
              <div className="col-span-12 lg:col-span-4 space-y-6">
                  {/* Global Control Card */}
                  <Card className="glass border-none shadow-custom overflow-hidden group hover:shadow-lg transition-all duration-300">
                     <CardHeader className="bg-gradient-to-r from-blue-50/50 to-transparent pb-4">
                       <CardTitle className="text-lg flex items-center gap-2 text-slate-800">
                          <Settings className="w-5 h-5 text-blue-500" />
                          全局控制
                       </CardTitle>
                     </CardHeader>
                     <CardContent className="space-y-5 pt-2">
                        <div className="flex items-center justify-between p-3 bg-white/60 rounded-xl border border-white/50 shadow-sm transition-colors hover:bg-white/80">
                           <div className="space-y-0.5">
                              <Label className="text-base font-medium">启用 TTS 输出</Label>
                              <p className="text-xs text-muted-foreground">开启后将朗读 AI 回复</p>
                           </div>
                           <Switch
                             checked={config?.tts_enabled ?? true}
                             onCheckedChange={(checked) => updateConfig('tts_enabled', checked)}
                             className="data-[state=checked]:bg-blue-500"
                           />
                        </div>

                        <div className="flex items-center justify-between px-2">
                           <Label className="text-sm text-slate-600">自动播放</Label>
                           <Switch
                             checked={config?.tts_auto_play ?? true}
                             onCheckedChange={(checked) => updateConfig('tts_auto_play', checked)}
                             className="scale-90"
                           />
                        </div>
                        <Separator className="bg-slate-100" />
                        <div className="flex items-center justify-between px-2">
                           <Label className="text-sm text-slate-600">保存音频文件</Label>
                           <Switch
                             checked={config?.tts_save_audio ?? false}
                             onCheckedChange={(checked) => updateConfig('tts_save_audio', checked)}
                             className="scale-90"
                           />
                        </div>
                        <Separator className="bg-slate-100" />

                        <div className="space-y-2 pt-1">
                           <Label className="text-sm text-slate-600">播放模式</Label>
                           <Select
                             value={config?.tts_play_mode || 'dialogue'}
                             onValueChange={(value: 'dialogue' | 'fulltext') => updateConfig('tts_play_mode', value)}
                           >
                              <SelectTrigger className="bg-white/50 border-slate-200 focus:ring-blue-200 h-9">
                                 <SelectValue />
                              </SelectTrigger>
                              <SelectContent>
                                 <SelectItem value="dialogue">只播放对话 (Dialog Only)</SelectItem>
                                 <SelectItem value="fulltext">播放全文 (Full Text)</SelectItem>
                              </SelectContent>
                           </Select>
                        </div>
                     </CardContent>
                  </Card>

                  {/* Volume Mixer Card */}
                  <Card className="glass border-none shadow-custom overflow-hidden group hover:shadow-lg transition-all duration-300">
                     <CardHeader className="bg-gradient-to-r from-green-50/50 to-transparent pb-4">
                       <CardTitle className="text-lg flex items-center gap-2 text-slate-800">
                          <Volume2 className="w-5 h-5 text-green-500" />
                          混音控制
                       </CardTitle>
                     </CardHeader>
                     <CardContent className="space-y-6 pt-2">
                        <div className="space-y-4 bg-white/40 p-4 rounded-xl border border-white/50">
                           <div className="flex justify-between items-center">
                              <Label className="text-sm font-medium">主音量</Label>
                              <span className="text-xs font-mono bg-white px-2 py-0.5 rounded text-slate-500 border">
                                {Math.round((config?.tts_volume ?? 0.8) * 100)}%
                              </span>
                           </div>
                           <Slider
                             value={[Math.round((config?.tts_volume ?? 0.8) * 100)]}
                             onValueChange={handleVolumeChange}
                             max={100}
                             step={1}
                             className="py-1"
                           />
                        </div>

                        <div className="flex gap-2">
                           <Button
                             variant="outline"
                             className="flex-1 border-red-100 text-red-500 hover:bg-red-50 hover:text-red-600 h-9 transition-colors"
                             onClick={handleStopPlayback}
                             disabled={!isPlaying}
                           >
                              <Square className="w-4 h-4 mr-2 fill-current opacity-80" /> 停止播放
                           </Button>
                           <div className={`flex-1 flex items-center justify-center gap-2 text-xs rounded-md border ${
                             isPlaying
                               ? 'text-green-600 bg-green-50/50 border-green-100'
                               : isSynthesizing
                                 ? 'text-blue-600 bg-blue-50/50 border-blue-100'
                                 : 'text-muted-foreground bg-slate-50/50 border-slate-100'
                           }`}>
                              <span className={`w-2 h-2 rounded-full ${
                                isPlaying
                                  ? 'bg-green-500 animate-pulse'
                                  : isSynthesizing
                                    ? 'bg-blue-500 animate-pulse'
                                    : 'bg-slate-300'
                              }`}></span>
                              {isPlaying ? '播放中' : isSynthesizing ? '合成中' : '空闲'}
                           </div>
                        </div>
                     </CardContent>
                  </Card>
              </div>

              {/* === Right Column: Engine Config === */}
              <div className="col-span-12 lg:col-span-8 space-y-6">
                 <Tabs defaultValue="gpt-sovits" className="w-full">
                    <TabsList className="w-full justify-start bg-transparent p-0 gap-6 border-b border-slate-200/60 mb-6 h-auto">
                       <TabsTrigger value="gpt-sovits" className="h-9 px-0 pb-2 rounded-none bg-transparent border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent data-[state=active]:shadow-none data-[state=active]:text-primary text-muted-foreground transition-all hover:text-primary/80">GPT-SoVITS</TabsTrigger>
                       <TabsTrigger value="xf" className="h-9 px-0 pb-2 rounded-none bg-transparent border-b-2 border-transparent data-[state=active]:border-blue-500 data-[state=active]:bg-transparent data-[state=active]:shadow-none data-[state=active]:text-blue-600 text-muted-foreground transition-all hover:text-blue-600/80">讯飞语音</TabsTrigger>
                    </TabsList>

                    {/* === GPT-SoVITS Tab === */}
                    <TabsContent value="gpt-sovits" className="space-y-6 animate-in slide-in-from-right-4 duration-300">
                       <Card className="glass border-none shadow-custom overflow-hidden">
                          <CardHeader className="pb-4">
                             <div className="flex justify-between items-center">
                                <CardTitle className="text-base font-semibold flex items-center gap-2">
                                    <span className="w-2 h-6 bg-primary rounded-full"></span>
                                    连接设置
                                </CardTitle>
                                <div className="flex items-center gap-2">
                                    {testResult === 'success' && (
                                      <span className="text-xs px-2 py-1 rounded-full bg-green-100 text-green-600 flex items-center gap-1 border border-green-200">
                                        <Check className="w-3 h-3" /> 已连接
                                      </span>
                                    )}
                                    {testResult === 'fail' && (
                                      <span className="text-xs px-2 py-1 rounded-full bg-red-100 text-red-600 flex items-center gap-1 border border-red-200">
                                        <AlertTriangle className="w-3 h-3" /> 连接失败
                                      </span>
                                    )}
                                    {testResult === null && (
                                      <span className="text-xs px-2 py-1 rounded-full bg-orange-100 text-orange-600 flex items-center gap-1 border border-orange-200">
                                        <AlertTriangle className="w-3 h-3" /> 未检测
                                      </span>
                                    )}
                                </div>
                             </div>
                          </CardHeader>
                          <CardContent className="pt-0">
                             <div className="flex gap-2">
                                <div className="relative flex-1">
                                    <Input
                                      value={localGsvApiUrl}
                                      onChange={(e) => setLocalGsvApiUrl(e.target.value)}
                                      onBlur={() => updateConfig('gsv_api_url', localGsvApiUrl)}
                                      className="pl-9 bg-white/50 border-slate-200 font-mono text-sm"
                                      placeholder="http://192.168.100.171:9880/tts"
                                    />
                                    <div className="absolute left-3 top-2.5 text-slate-400 text-xs font-bold">API</div>
                                </div>
                                <Button
                                  className="bg-primary hover:bg-primary/90 text-white min-w-[80px]"
                                  onClick={handleTestGSV}
                                  disabled={isTesting}
                                >
                                  {isTesting ? <Loader2 className="w-4 h-4 animate-spin" /> : '测试'}
                                </Button>
                             </div>
                          </CardContent>
                       </Card>

                       <Card className="glass border-none shadow-custom overflow-hidden">
                          <CardHeader className="pb-4 border-b border-slate-100">
                             <CardTitle className="text-base font-semibold flex items-center gap-2">
                                <span className="w-2 h-6 bg-purple-500 rounded-full"></span>
                                角色音色管理
                             </CardTitle>
                             <CardDescription>配置精调模型和参考音频以克隆目标声线</CardDescription>
                          </CardHeader>
                          <CardContent className="pt-6 space-y-6">
                             {/* Current Character Selection */}
                             <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                                <div className="space-y-2">
                                   <Label className="text-xs font-semibold text-slate-500 uppercase tracking-wider">当前角色</Label>
                                   <Select
                                     value={selectedCharacterId}
                                     onValueChange={setSelectedCharacterId}
                                   >
                                      <SelectTrigger className="bg-white/60 border-slate-200 h-10 shadow-sm">
                                         <SelectValue placeholder="选择角色" />
                                      </SelectTrigger>
                                      <SelectContent>
                                         {characterList.map((char) => (
                                           <SelectItem key={char.id} value={char.id}>{char.name}</SelectItem>
                                         ))}
                                      </SelectContent>
                                   </Select>
                                </div>
                                <div className="space-y-2">
                                   <Label className="text-xs font-semibold text-slate-500 uppercase tracking-wider">参考音频配置</Label>
                                   <div className="flex items-center gap-2 h-10 bg-white/60 px-3 rounded-md border border-slate-200 shadow-sm">
                                      <Checkbox
                                        checked={!!editingCharacter.ref_audio_json}
                                        className="data-[state=checked]:bg-purple-500 data-[state=checked]:border-purple-500"
                                      />
                                      <Input
                                        className="border-none shadow-none bg-transparent h-8 text-sm focus-visible:ring-0 p-0"
                                        value={editingCharacter.ref_audio_json || ''}
                                        onChange={(e) => setEditingCharacter({ ...editingCharacter, ref_audio_json: e.target.value })}
                                        placeholder="参考音频配置文件"
                                      />
                                   </div>
                                </div>
                             </div>

                             {/* Path Configuration Grid */}
                             <div className="bg-slate-50/50 rounded-xl p-5 border border-slate-100 space-y-4">
                                <div className="grid grid-cols-[100px_1fr_auto] gap-3 items-center">
                                   <Label className="text-right text-xs text-slate-500">角色名称</Label>
                                   <Input
                                     className="bg-white h-9 border-slate-200"
                                     placeholder="Display Name"
                                     value={editingCharacter.name || ''}
                                     onChange={(e) => setEditingCharacter({ ...editingCharacter, name: e.target.value })}
                                   />
                                   <div className="w-16"></div>
                                </div>

                                <div className="grid grid-cols-[100px_1fr_auto] gap-3 items-center">
                                   <Label className="text-right text-xs text-slate-500">参考音频目录</Label>
                                   <Input
                                     className="bg-white h-9 border-slate-200 text-xs font-mono text-slate-600"
                                     value={editingCharacter.ref_audio_dir || ''}
                                     onChange={(e) => setEditingCharacter({ ...editingCharacter, ref_audio_dir: e.target.value })}
                                   />
                                   <Button variant="outline" size="sm" className="w-16 h-9"><FolderOpen className="w-4 h-4" /></Button>
                                </div>

                                <div className="grid grid-cols-[100px_1fr_auto] gap-3 items-center">
                                   <Label className="text-right text-xs text-slate-500">情感音频配置</Label>
                                   <Input
                                     className="bg-white h-9 border-slate-200 text-xs font-mono text-slate-600"
                                     value={editingCharacter.emotion_audio_json || ''}
                                     onChange={(e) => setEditingCharacter({ ...editingCharacter, emotion_audio_json: e.target.value })}
                                   />
                                   <Button variant="outline" size="sm" className="w-16 h-9"><FolderOpen className="w-4 h-4" /></Button>
                                </div>

                                <div className="grid grid-cols-[100px_1fr_auto] gap-3 items-center">
                                   <Label className="text-right text-xs text-slate-500">GPT模型路径</Label>
                                   <Input
                                     className="bg-white h-9 border-slate-200 text-xs font-mono text-slate-600"
                                     value={editingCharacter.gpt_model_path || ''}
                                     onChange={(e) => setEditingCharacter({ ...editingCharacter, gpt_model_path: e.target.value })}
                                   />
                                   <Button variant="outline" size="sm" className="w-16 h-9"><FolderOpen className="w-4 h-4" /></Button>
                                </div>

                                <div className="grid grid-cols-[100px_1fr_auto] gap-3 items-center">
                                   <Label className="text-right text-xs text-slate-500">SoVITS模型</Label>
                                   <Input
                                     className="bg-white h-9 border-slate-200 text-xs font-mono text-slate-600"
                                     value={editingCharacter.sovits_model_path || ''}
                                     onChange={(e) => setEditingCharacter({ ...editingCharacter, sovits_model_path: e.target.value })}
                                   />
                                   <Button variant="outline" size="sm" className="w-16 h-9"><FolderOpen className="w-4 h-4" /></Button>
                                </div>
                             </div>

                             {/* Server Mode */}
                             <div className="flex items-center gap-4 bg-purple-50/50 p-3 rounded-lg border border-purple-100">
                                <div className="flex items-center gap-2">
                                    <Checkbox
                                      checked={config?.gsv_use_server_path ?? false}
                                      onCheckedChange={(checked) => updateConfig('gsv_use_server_path', !!checked)}
                                      className="data-[state=checked]:bg-purple-600 data-[state=checked]:border-purple-600"
                                    />
                                    <Label className="text-sm font-medium text-purple-900">服务器部署模式</Label>
                                </div>
                                <div className="h-4 w-px bg-purple-200"></div>
                                <div className="flex items-center gap-2 flex-1">
                                    <Label className="text-xs text-purple-700 whitespace-nowrap">路径前缀:</Label>
                                    <Input
                                      className="h-7 bg-white/80 border-purple-200 text-xs"
                                      value={config?.gsv_server_path_prefix || ''}
                                      onChange={(e) => updateConfig('gsv_server_path_prefix', e.target.value)}
                                    />
                                </div>
                             </div>

                             {/* Default Gender Roles */}
                             <div className="grid grid-cols-2 gap-4">
                                <div className="space-y-1.5">
                                   <Label className="text-xs text-slate-500">默认男性角色</Label>
                                   <Select
                                     value={config?.gsv_default_male_character || ''}
                                     onValueChange={(value) => updateConfig('gsv_default_male_character', value)}
                                   >
                                      <SelectTrigger className="bg-white/50 h-9 text-sm"><SelectValue placeholder="选择角色" /></SelectTrigger>
                                      <SelectContent>
                                        {characterList.map((char) => (
                                          <SelectItem key={char.id} value={char.id}>{char.name}</SelectItem>
                                        ))}
                                      </SelectContent>
                                   </Select>
                                </div>
                                <div className="space-y-1.5">
                                   <Label className="text-xs text-slate-500">默认女性角色</Label>
                                   <Select
                                     value={config?.gsv_default_female_character || ''}
                                     onValueChange={(value) => updateConfig('gsv_default_female_character', value)}
                                   >
                                      <SelectTrigger className="bg-white/50 h-9 text-sm"><SelectValue placeholder="选择角色" /></SelectTrigger>
                                      <SelectContent>
                                        {characterList.map((char) => (
                                          <SelectItem key={char.id} value={char.id}>{char.name}</SelectItem>
                                        ))}
                                      </SelectContent>
                                   </Select>
                                </div>
                             </div>

                             <Separator />

                             {/* Action Buttons */}
                             <div className="flex gap-3">
                                <Button
                                  className="flex-1 bg-gradient-to-r from-green-500 to-emerald-600 text-white shadow-md hover:shadow-lg hover:opacity-90 transition-all"
                                  onClick={handleAddCharacter}
                                >
                                   <Plus className="w-4 h-4 mr-2" /> 添加角色
                                </Button>
                                <Button
                                  variant="outline"
                                  className="flex-1 border-blue-200 text-blue-600 hover:bg-blue-50"
                                  onClick={handleSaveCharacter}
                                  disabled={!selectedCharacterId}
                                >
                                   <Save className="w-4 h-4 mr-2" /> 保存
                                </Button>
                                <Button
                                  variant="outline"
                                  className="flex-1 border-red-200 text-red-600 hover:bg-red-50"
                                  onClick={handleDeleteCharacter}
                                  disabled={!selectedCharacterId}
                                >
                                   <Trash2 className="w-4 h-4 mr-2" /> 删除
                                </Button>
                             </div>
                          </CardContent>
                       </Card>

                       {/* Inference Params */}
                       <Card className="glass border-none shadow-custom">
                          <CardContent className="pt-6">
                             <div className="flex flex-col gap-4">
                                <div className="flex items-center justify-between">
                                   <Label className="font-semibold text-sm">推理参数配置</Label>
                                   <Button size="sm" className="h-7 text-xs bg-slate-800 text-white" onClick={handleApplyParams}>
                                     应用参数
                                   </Button>
                                </div>
                                <div className="grid grid-cols-3 gap-6">
                                   <div className="space-y-2">
                                      <div className="flex justify-between text-xs">
                                         <span className="text-slate-500">top_k</span>
                                         <span className="font-mono bg-slate-100 px-1 rounded">{localTopK}</span>
                                      </div>
                                      <Input
                                        className="h-8 bg-white/50 text-center text-xs"
                                        value={localTopK}
                                        onChange={(e) => setLocalTopK(e.target.value)}
                                      />
                                   </div>
                                   <div className="space-y-2">
                                      <div className="flex justify-between text-xs">
                                         <span className="text-slate-500">top_p</span>
                                         <span className="font-mono bg-slate-100 px-1 rounded">{localTopP}</span>
                                      </div>
                                      <Input
                                        className="h-8 bg-white/50 text-center text-xs"
                                        value={localTopP}
                                        onChange={(e) => setLocalTopP(e.target.value)}
                                      />
                                   </div>
                                   <div className="space-y-2">
                                      <div className="flex justify-between text-xs">
                                         <span className="text-slate-500">temp</span>
                                         <span className="font-mono bg-slate-100 px-1 rounded">{localTemp}</span>
                                      </div>
                                      <Input
                                        className="h-8 bg-white/50 text-center text-xs"
                                        value={localTemp}
                                        onChange={(e) => setLocalTemp(e.target.value)}
                                      />
                                   </div>
                                </div>
                             </div>
                          </CardContent>
                       </Card>
                    </TabsContent>

                    {/* === Xunfei Tab === */}
                    <TabsContent value="xf" className="space-y-6 animate-in slide-in-from-right-4 duration-300">
                        <Card className="glass border-none shadow-custom">
                          <CardHeader className="pb-4 border-b border-slate-100">
                             <CardTitle className="text-base font-semibold flex items-center gap-2">
                                <span className="w-2 h-6 bg-blue-500 rounded-full"></span>
                                讯飞语音配置
                             </CardTitle>
                          </CardHeader>
                          <CardContent className="pt-6 space-y-4">
                             <div className="bg-slate-50/50 rounded-xl p-5 border border-slate-100 space-y-4">
                                <div className="grid grid-cols-[80px_1fr] gap-3 items-center">
                                   <Label className="text-right text-xs text-slate-500">APP ID</Label>
                                   <Input
                                     className="bg-white h-9 border-slate-200 font-mono text-xs"
                                     value={config?.xunfei_app_id || ''}
                                     onChange={(e) => updateConfig('xunfei_app_id', e.target.value)}
                                   />
                                </div>
                                <div className="grid grid-cols-[80px_1fr] gap-3 items-center">
                                   <Label className="text-right text-xs text-slate-500">API Key</Label>
                                   <Input
                                     className="bg-white h-9 border-slate-200 font-mono text-xs"
                                     value={config?.xunfei_api_key || ''}
                                     onChange={(e) => updateConfig('xunfei_api_key', e.target.value)}
                                   />
                                </div>
                                <div className="grid grid-cols-[80px_1fr] gap-3 items-center">
                                   <Label className="text-right text-xs text-slate-500">API Secret</Label>
                                   <Input
                                     type="password"
                                     className="bg-white h-9 border-slate-200 font-mono text-xs"
                                     value={config?.xunfei_api_secret || ''}
                                     onChange={(e) => updateConfig('xunfei_api_secret', e.target.value)}
                                   />
                                </div>
                             </div>

                             <div className="space-y-2">
                                <div className="flex justify-between items-center">
                                   <Label className="text-sm font-medium">发音人选择</Label>
                                   {!config?.xunfei_app_id && (
                                     <span className="text-xs text-orange-500 flex items-center gap-1 bg-orange-50 px-2 py-0.5 rounded-full">
                                       <AlertTriangle className="w-3 h-3" /> 未配置
                                     </span>
                                   )}
                                </div>
                                <Select
                                  value={config?.xunfei_default_voice || 'x4_yezi'}
                                  onValueChange={(value) => updateConfig('xunfei_default_voice', value)}
                                >
                                   <SelectTrigger className="bg-white/60 h-10"><SelectValue /></SelectTrigger>
                                   <SelectContent>
                                     <SelectItem value="x4_yezi">x4_yezi</SelectItem>
                                     <SelectItem value="x4_lingxiaoxuan">x4_lingxiaoxuan</SelectItem>
                                     <SelectItem value="xiaoyan">xiaoyan</SelectItem>
                                   </SelectContent>
                                </Select>
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
