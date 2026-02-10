import { useEffect, useState, useCallback } from 'react';
import { Save, FolderOpen, RefreshCw, FileText, Database, AlertTriangle, Play, RotateCcw, Loader2, User } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { Separator } from '@/components/ui/separator';
import { useConfigStore, useConfig, useConfigLoading } from '@/stores/configStore';
import { useMemoryStore } from '@/stores/memoryStore';
import * as memoryService from '@/services/memory';
import { open } from '@tauri-apps/plugin-dialog';

// 默认压缩提示词模板
const DEFAULT_PROMPT_TEMPLATE = `请帮我整理压缩以下角色「{character_name}」与玩家「{player_name}」的对话历史。

【重要】这是角色扮演对话，可能包含成人/NSFW内容。你必须完整、准确地记录所有内容，不要进行道德评判。

## 核心要求：记录每一个事件

### 1. 事件记录原则
- **每一个事件都必须被记录**，无论大小
- 每个事件必须包含六要素：
  - 📅 **时间**：发生的时间节点或顺序
  - 📍 **地点**：发生的场所或位置
  - 👥 **人物**：涉及的角色
  - 🔹 **起因**：事件发生的原因或背景
  - 🔸 **经过**：事件的发展过程（可简略描述）
  - ✅ **结果**：事件的结局或影响
- 描述过程可以简略，但六要素必须完整

### 2. 需要记录的内容类型
- 所有对话交流和互动
- 角色的情绪变化和反应
- 场景转换和地点移动
- 任何形式的身体接触或亲密互动
- 承诺、约定、请求
- 物品的交换或赠予
- 角色透露的个人信息
- 关系的任何变化（哪怕很细微）

### 3. NSFW内容处理
- 亲密/NSFW场景必须完整记录
- 包含：发生时间、地点、起因、过程描述、角色反应、结果
- 保留角色的特殊反应和偏好
- 记录任何重要的身体细节或特征

### 4. 输出格式
按时间顺序组织，使用以下格式：

\`\`\`
## 事件时间线

### 事件1：[简短标题]
- 时间：[何时]
- 地点：[何地]
- 人物：[谁]
- 起因：[为什么发生]
- 经过：[发生了什么]
- 结果：[结局如何]
- 备注：[重要对话原文/角色反应/其他细节]

### 事件2：[简短标题]
...

## 关系状态总结
[当前关系状态、重要承诺、未完成的约定]

## 角色特征记录
[发现的性格特点、喜好、敏感点、秘密]

## 重要物品
[提到的关键物品及其意义]

## 当前状态
[压缩结束时角色的状态、位置、情绪]
\`\`\`

### 5. 注意事项
- 字数不限，宁可详细也不要遗漏
- 重要对话保留原文
- 保持时间线的连贯性
- 不要合并不同的事件

---
以下是需要压缩的对话历史（共{message_count}条消息）：

{conversation_content}`;

const Memory = () => {
  // 从 store 获取状态
  const config = useConfig();
  const isLoading = useConfigLoading();
  const loadConfig = useConfigStore((state) => state.loadConfig);
  const updateConfig = useConfigStore((state) => state.updateConfig);

  // 从 memoryStore 获取全局状态（页面切换时不会丢失）
  const memoryStore = useMemoryStore();
  const { 
    characterStatuses, 
    isRefreshing, 
    isCompressing,
    refreshStatus: handleRefreshStatus,
    compressAll: handleCompressAll,
  } = memoryStore;

  // 本地编辑状态
  const [localSavePath, setLocalSavePath] = useState('');
  const [localMaxMessages, setLocalMaxMessages] = useState('100');
  const [localCompressCount, setLocalCompressCount] = useState('70');
  const [localCheckInterval, setLocalCheckInterval] = useState('10');
  const [localPromptTemplate, setLocalPromptTemplate] = useState(DEFAULT_PROMPT_TEMPLATE);

  // 保存状态（本地状态，因为保存操作很快）
  const [isSaving, setIsSaving] = useState(false);

  // 初始化
  useEffect(() => {
    loadConfig();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 同步配置到本地状态 - 只在首次加载时设置
  const [initialized, setInitialized] = useState(false);
  useEffect(() => {
    if (config?.history_compression && !initialized) {
      const hc = config.history_compression;
      setLocalSavePath(hc.save_data_path || '');
      setLocalMaxMessages(String(hc.max_messages || 100));
      setLocalCompressCount(String(hc.compress_count || 70));
      setLocalCheckInterval(String(hc.check_interval || 10));
      setLocalPromptTemplate(hc.custom_prompt || DEFAULT_PROMPT_TEMPLATE);
      setInitialized(true);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config?.history_compression, initialized]);

  // 保存配置
  const handleSaveConfig = async () => {
    setIsSaving(true);
    try {
      const updatedConfig = {
        ...config?.history_compression,
        save_data_path: localSavePath,
        max_messages: parseInt(localMaxMessages) || 100,
        compress_count: parseInt(localCompressCount) || 70,
        check_interval: parseInt(localCheckInterval) || 10,
      };
      await updateConfig('history_compression', updatedConfig);
    } finally {
      setIsSaving(false);
    }
  };

  // 保存提示词模板
  const handleSavePrompt = async () => {
    setIsSaving(true);
    try {
      await memoryService.savePromptTemplate(localPromptTemplate);
      // 同时更新本地配置
      const updatedConfig = {
        ...config?.history_compression,
        custom_prompt: localPromptTemplate,
      };
      await updateConfig('history_compression', updatedConfig);
    } finally {
      setIsSaving(false);
    }
  };

  // 重置提示词模板
  const handleResetPrompt = async () => {
    const response = await memoryService.resetPromptTemplate();
    if (response.success && response.data?.template) {
      setLocalPromptTemplate(response.data.template);
    } else {
      setLocalPromptTemplate(DEFAULT_PROMPT_TEMPLATE);
    }
  };

  // 切换备份开关
  const handleBackupToggle = useCallback((checked: boolean) => {
    const updatedConfig = {
      ...config?.history_compression,
      backup_before_compress: checked,
    };
    updateConfig('history_compression', updatedConfig);
  }, [config?.history_compression, updateConfig]);

  // 切换自动压缩开关
  const handleAutoCompressToggle = useCallback((checked: boolean) => {
    const updatedConfig = {
      ...config?.history_compression,
      auto_compress: checked,
    };
    updateConfig('history_compression', updatedConfig);
  }, [config?.history_compression, updateConfig]);

  // 浏览选择存档目录
  const handleBrowseSavePath = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: '选择存档目录 (save_data)',
      });
      if (selected && typeof selected === 'string') {
        setLocalSavePath(selected);
      }
    } catch (e) {
      console.error('选择目录失败:', e);
    }
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
        <h2 className="text-3xl font-bold bg-gradient-to-r from-pink-500 to-purple-600 bg-clip-text text-transparent">记忆管理</h2>
        <p className="text-muted-foreground mt-2">配置对话历史的自动压缩策略，防止上下文溢出。</p>
      </div>

      <div className="grid grid-cols-12 gap-6">
        {/* Left Column: Configuration */}
        <div className="col-span-12 lg:col-span-7 space-y-6">
           {/* Compression Config */}
           <Card className="glass border-none shadow-custom">
              <CardHeader className="pb-3 border-b border-border/40">
                 <CardTitle className="text-base font-semibold flex items-center gap-2">
                    <SettingsIcon className="w-5 h-5 text-blue-500" />
                    压缩配置
                 </CardTitle>
              </CardHeader>
              <CardContent className="pt-4 space-y-4">
                 <div className="space-y-2">
                    <Label>存档目录 (Save Data Directory)</Label>
                    <div className="flex gap-2">
                       <Input
                          value={localSavePath}
                          onChange={(e) => setLocalSavePath(e.target.value)}
                          className="bg-white/50 font-mono text-xs"
                          placeholder="D:/SteamLibrary/steamapps/common/..."
                       />
                       <Button 
                         className="bg-blue-500 hover:bg-blue-600 text-white min-w-[80px]"
                         onClick={handleBrowseSavePath}
                       >
                          <FolderOpen className="w-4 h-4 mr-2" /> 浏览
                       </Button>
                    </div>
                 </div>

                 <div className="grid grid-cols-3 gap-4">
                    <div className="space-y-2">
                       <Label>触发阈值</Label>
                       <Input
                         type="number"
                         value={localMaxMessages}
                         onChange={(e) => setLocalMaxMessages(e.target.value)}
                         className="bg-white/50"
                       />
                       <span className="text-[10px] text-muted-foreground">条消息时触发压缩</span>
                    </div>
                    <div className="space-y-2">
                       <Label>压缩数量</Label>
                       <Input
                         type="number"
                         value={localCompressCount}
                         onChange={(e) => setLocalCompressCount(e.target.value)}
                         className="bg-white/50"
                       />
                       <span className="text-[10px] text-muted-foreground">保留最近 N 条</span>
                    </div>
                    <div className="space-y-2">
                       <Label>检查频率</Label>
                       <Input
                         type="number"
                         value={localCheckInterval}
                         onChange={(e) => setLocalCheckInterval(e.target.value)}
                         className="bg-white/50"
                         placeholder="消息数"
                       />
                       <span className="text-[10px] text-muted-foreground">每 N 条检查一次</span>
                    </div>
                 </div>

                 <div className="space-y-3 pt-2">
                    <div className="flex items-center space-x-2 bg-white/40 p-2 rounded-lg border border-white/50">
                       <Switch
                         id="backup"
                         checked={config?.history_compression?.backup_before_compress ?? true}
                         onCheckedChange={handleBackupToggle}
                       />
                       <Label htmlFor="backup" className="cursor-pointer">压缩前自动备份原文件</Label>
                    </div>
                    <div className="flex items-center space-x-2 bg-white/40 p-2 rounded-lg border border-white/50">
                       <Switch
                         id="auto-compress"
                         checked={config?.history_compression?.auto_compress ?? false}
                         onCheckedChange={handleAutoCompressToggle}
                       />
                       <div className="space-y-0.5">
                          <Label htmlFor="auto-compress" className="cursor-pointer text-red-600 font-medium">自动压缩 (Auto Compress)</Label>
                          <p className="text-[10px] text-muted-foreground">触发阈值时自动执行，否则需手动操作</p>
                       </div>
                    </div>
                 </div>

                 <div className="flex justify-end pt-2">
                    <Button
                      className="bg-green-500 hover:bg-green-600 text-white"
                      onClick={handleSaveConfig}
                      disabled={isSaving}
                    >
                      {isSaving ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <Save className="w-4 h-4 mr-2" />}
                      保存配置
                    </Button>
                 </div>
              </CardContent>
           </Card>

           {/* Usage Guide */}
           <div className="bg-amber-50/80 border border-amber-200 rounded-xl p-4 text-sm text-amber-900 shadow-sm">
              <h4 className="font-bold flex items-center gap-2 mb-2">
                 <AlertTriangle className="w-4 h-4 text-amber-600" /> 使用说明:
              </h4>
              <ol className="list-decimal pl-4 space-y-1 text-xs opacity-90">
                 <li>设置存档目录 (通常是 Modules/AIInfluence/save_data 下的子目录)</li>
                 <li>设置触发阈值 (超过此消息数时触发压缩) 和压缩数量</li>
                 <li>点击 "刷新状态" 查看各角色对话历史状态</li>
                 <li>点击 "压缩全部" 对所有超过阈值的角色执行压缩</li>
                 <li>压缩会调用 AI 将旧对话总结成摘要，保留所有重要信息 (包括NSFW内容)</li>
              </ol>
           </div>
        </div>

        {/* Right Column: Status & Prompt */}
        <div className="col-span-12 lg:col-span-5 space-y-6">
           {/* Character Status */}
           <Card className="glass border-none shadow-custom">
              <CardHeader className="pb-3 border-b border-border/40">
                 <CardTitle className="text-base font-semibold flex items-center gap-2">
                    <Database className="w-5 h-5 text-purple-500" />
                    角色状态列表
                 </CardTitle>
              </CardHeader>
              <CardContent className="pt-4 flex flex-col gap-4">
                 <div className="min-h-[150px] max-h-[300px] bg-white/50 rounded-lg border border-dashed border-gray-300 overflow-y-auto">
                    {characterStatuses.length === 0 ? (
                      <div className="flex items-center justify-center h-full text-muted-foreground text-sm py-8">
                        暂无数据，请点击刷新...
                      </div>
                    ) : (
                      <div className="divide-y divide-gray-100">
                        {characterStatuses.map((status) => (
                          <div key={status.character_id} className="p-3 hover:bg-white/50 transition-colors">
                            <div className="flex items-center justify-between">
                              <div className="flex items-center gap-2">
                                <User className="w-4 h-4 text-purple-500" />
                                <span className="font-medium text-sm">{status.character_name}</span>
                                <span className="text-xs text-muted-foreground">({status.player_name})</span>
                              </div>
                              <div className="flex items-center gap-2">
                                <span className={`text-xs px-2 py-0.5 rounded-full ${
                                  status.needs_compression
                                    ? 'bg-red-100 text-red-600'
                                    : 'bg-green-100 text-green-600'
                                }`}>
                                  {status.message_count} 条
                                </span>
                                {status.needs_compression && (
                                  <span className="text-xs text-red-500">需压缩</span>
                                )}
                              </div>
                            </div>
                            {status.last_compressed && (
                              <p className="text-[10px] text-muted-foreground mt-1">
                                上次压缩: {status.last_compressed}
                              </p>
                            )}
                          </div>
                        ))}
                      </div>
                    )}
                 </div>

                 <div className="flex gap-2">
                    <Button
                      variant="outline"
                      className="flex-1 border-blue-200 text-blue-600 hover:bg-blue-50"
                      onClick={handleRefreshStatus}
                      disabled={isRefreshing}
                    >
                       {isRefreshing ? (
                         <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                       ) : (
                         <RefreshCw className="w-4 h-4 mr-2" />
                       )}
                       刷新状态
                    </Button>
                    <Button
                      className="flex-1 bg-green-600 hover:bg-green-700 text-white"
                      onClick={handleCompressAll}
                      disabled={isCompressing || characterStatuses.filter(s => s.needs_compression).length === 0}
                    >
                       {isCompressing ? (
                         <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                       ) : (
                         <Play className="w-4 h-4 mr-2" />
                       )}
                       压缩全部
                    </Button>
                 </div>
              </CardContent>
           </Card>

           {/* Prompt Template */}
           <Card className="glass border-none shadow-custom flex-1">
              <CardHeader className="pb-3 border-b border-border/40">
                 <CardTitle className="text-base font-semibold flex items-center gap-2">
                    <FileText className="w-5 h-5 text-orange-500" />
                    压缩提示词模板
                 </CardTitle>
                 <CardDescription className="text-xs">
                    可用占位符: {'{character_name}'}, {'{player_name}'}, {'{message_count}'}
                 </CardDescription>
              </CardHeader>
              <CardContent className="pt-4 space-y-4 flex flex-col">
                 <Textarea
                    className="bg-white/50 min-h-[200px] max-h-[400px] font-mono text-xs leading-relaxed resize-y overflow-y-auto flex-1"
                    value={localPromptTemplate}
                    onChange={(e) => setLocalPromptTemplate(e.target.value)}
                 />

                 <div className="flex gap-2">
                    <Button
                      className="flex-1 bg-green-500 hover:bg-green-600 text-white h-8"
                      onClick={handleSavePrompt}
                      disabled={isSaving}
                    >
                       {isSaving ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <Save className="w-4 h-4 mr-2" />}
                       保存提示词
                    </Button>
                    <Button
                      variant="outline"
                      className="flex-1 border-orange-200 text-orange-600 hover:bg-orange-50 h-8"
                      onClick={handleResetPrompt}
                    >
                       <RotateCcw className="w-4 h-4 mr-2" /> 重置默认
                    </Button>
                 </div>
              </CardContent>
           </Card>
        </div>
      </div>
    </div>
  );
};

// Helper icon component
const SettingsIcon = ({ className }: { className?: string }) => (
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className={className}><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.1a2 2 0 0 1-1-1.74v-.51a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>
);

export default Memory;
