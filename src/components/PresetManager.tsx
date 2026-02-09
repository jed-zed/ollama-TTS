import { useState, useCallback } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import type { Config } from '@/services/types';

/**
 * 内置预设的默认 URL 和模型名称映射
 * 用户修改后会保存到 preset_configs，切换时优先使用已保存值
 */
export const BUILTIN_PRESETS: Record<string, { url: string; defaultModel?: string; label: string; emoji: string }> = {
  local: { url: 'http://127.0.0.1:11434/v1/chat/completions', defaultModel: '', label: '本地 Ollama', emoji: '🏠' },
  siliconflow: { url: 'https://api.siliconflow.cn/v1/chat/completions', defaultModel: '', label: '硅基流动 (SiliconFlow)', emoji: '🚀' },
  deepseek: { url: 'https://api.deepseek.com/v1/chat/completions', defaultModel: 'deepseek-chat', label: 'DeepSeek', emoji: '🔮' },
  volcengine: { url: 'https://ark.cn-beijing.volces.com/api/v3/chat/completions', defaultModel: '', label: '火山引擎 (Volcengine)', emoji: '🌋' },
  ggchan: { url: 'https://gcli.ggchan.dev/v1/chat/completions', defaultModel: 'gemini-3-pro-preview', label: 'gg公益站 (Gemini)', emoji: '🎁' },
  openrouter: { url: 'https://openrouter.ai/api/v1/chat/completions', defaultModel: '', label: 'OpenRouter', emoji: '🌐' },
  openai: { url: 'https://api.openai.com/v1/chat/completions', defaultModel: '', label: 'OpenAI', emoji: '💎' },
};

const RESERVED_PRESET_IDS = new Set(['custom', ...Object.keys(BUILTIN_PRESETS)]);

interface PresetManagerProps {
  config: Config | null;
  disabled?: boolean;
  localApiUrl: string;
  localApiKey: string;
  localModelName: string;
  setLocalApiUrl: (url: string) => void;
  setLocalApiKey: (key: string) => void;
  setLocalModelName: (model: string) => void;
  updateConfigBatch: (updates: Partial<Config>) => Promise<void>;
}

/**
 * 预设管理器组件
 * 负责预设选择、新建、删除和切换逻辑
 */
const PresetManager = ({
  config,
  disabled = false,
  localApiUrl,
  localApiKey,
  localModelName,
  setLocalApiUrl,
  setLocalApiKey,
  setLocalModelName,
  updateConfigBatch,
}: PresetManagerProps) => {
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [newPresetName, setNewPresetName] = useState('');
  const [newPresetEmoji, setNewPresetEmoji] = useState('⭐');
  const [createError, setCreateError] = useState('');

  const currentPreset = config?.current_api_preset || 'custom';
  const userPresets = config?.user_presets || {};
  const isUserPreset = currentPreset in userPresets;

  const generatePresetId = (name: string): string => {
    const base = name
      .toLowerCase()
      .replace(/[^a-z0-9\u4e00-\u9fff]/g, '_')
      .replace(/_+/g, '_')
      .replace(/^_|_$/g, '');
    const id = `user_${base || 'preset'}`;
    if (!RESERVED_PRESET_IDS.has(id) && !(id in userPresets)) {
      return id;
    }
    let counter = 1;
    while (RESERVED_PRESET_IDS.has(`${id}_${counter}`) || `${id}_${counter}` in userPresets) {
      counter++;
    }
    return `${id}_${counter}`;
  };

  const handlePresetChange = useCallback(async (preset: string) => {
    if (preset === '__create_new__') {
      setShowCreateDialog(true);
      return;
    }

    try {
      const currentPresetConfigs = config?.preset_configs || {};

      const updates: Record<string, unknown> = {
        current_api_preset: preset,
      };

      if (currentPreset === 'custom') {
        updates.custom_api_url = localApiUrl;
        updates.custom_api_key = localApiKey;
        updates.custom_model_name = localModelName;
      } else {
        updates.preset_configs = {
          ...currentPresetConfigs,
          [currentPreset]: {
            api_url: localApiUrl,
            api_key: localApiKey,
            model_name: localModelName,
          },
        };
      }

      if (preset === 'custom') {
        const customUrl = config?.custom_api_url || '';
        const customKey = config?.custom_api_key || '';
        const customModel = config?.custom_model_name || '';
        updates.api_url = customUrl;
        updates.api_key = customKey;
        updates.model_name = customModel;
        setLocalApiUrl(customUrl);
        setLocalApiKey(customKey);
        setLocalModelName(customModel);
      } else {
        const builtinPreset = BUILTIN_PRESETS[preset];
        const savedPresetConfig = currentPresetConfigs[preset] || { api_url: '', api_key: '', model_name: '' };

        const defaultUrl = builtinPreset?.url || '';
        const defaultModel = builtinPreset?.defaultModel || '';

        const targetUrl = savedPresetConfig.api_url || defaultUrl;
        const targetKey = savedPresetConfig.api_key || '';
        const targetModel = savedPresetConfig.model_name || defaultModel;

        updates.api_url = targetUrl;
        updates.api_key = targetKey;
        updates.model_name = targetModel;

        setLocalApiUrl(targetUrl);
        setLocalApiKey(targetKey);
        setLocalModelName(targetModel);
      }

      await updateConfigBatch(updates);
    } catch (error) {
      console.error('切换预设失败:', error);
    }
  }, [config, currentPreset, localApiUrl, localApiKey, localModelName, setLocalApiUrl, setLocalApiKey, setLocalModelName, updateConfigBatch]);

  const handleCreatePreset = async () => {
    const trimmedName = newPresetName.trim();
    if (!trimmedName) {
      setCreateError('请输入预设名称');
      return;
    }

    const presetId = generatePresetId(trimmedName);

    const newUserPresets = {
      ...userPresets,
      [presetId]: {
        name: trimmedName,
        emoji: newPresetEmoji || '⭐',
      },
    };

    const currentPresetConfigs = config?.preset_configs || {};

    const updates: Record<string, unknown> = {
      current_api_preset: presetId,
      user_presets: newUserPresets,
      preset_configs: {
        ...currentPresetConfigs,
        [currentPreset]: {
          api_url: localApiUrl,
          api_key: localApiKey,
          model_name: localModelName,
        },
        [presetId]: {
          api_url: '',
          api_key: '',
          model_name: '',
        },
      },
      api_url: '',
      api_key: '',
      model_name: '',
    };

    setLocalApiUrl('');
    setLocalApiKey('');
    setLocalModelName('');

    await updateConfigBatch(updates);

    setShowCreateDialog(false);
    setNewPresetName('');
    setNewPresetEmoji('⭐');
    setCreateError('');
  };

  const handleDeletePreset = async () => {
    if (!isUserPreset) return;

    const { [currentPreset]: _removed, ...remainingUserPresets } = userPresets;
    const currentPresetConfigs = config?.preset_configs || {};
    const { [currentPreset]: _removedConfig, ...remainingPresetConfigs } = currentPresetConfigs;

    const customUrl = config?.custom_api_url || '';
    const customKey = config?.custom_api_key || '';
    const customModel = config?.custom_model_name || '';

    const updates: Record<string, unknown> = {
      current_api_preset: 'custom',
      user_presets: remainingUserPresets,
      preset_configs: remainingPresetConfigs,
      api_url: customUrl,
      api_key: customKey,
      model_name: customModel,
    };

    setLocalApiUrl(customUrl);
    setLocalApiKey(customKey);
    setLocalModelName(customModel);

    await updateConfigBatch(updates);
    setShowDeleteDialog(false);
  };

  const userPresetEntries = Object.entries(userPresets);

  return (
    <>
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <Label>服务商预设</Label>
          {isUserPreset && (
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2 text-xs text-destructive hover:text-destructive hover:bg-destructive/10"
              onClick={() => setShowDeleteDialog(true)}
              disabled={disabled}
            >
              <Trash2 className="h-3 w-3 mr-1" />
              删除预设
            </Button>
          )}
        </div>
        <Select
          value={currentPreset}
          onValueChange={handlePresetChange}
          disabled={disabled}
        >
          <SelectTrigger>
            <SelectValue placeholder="选择预设" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="custom">🔧 自定义 (Custom)</SelectItem>
            {Object.entries(BUILTIN_PRESETS).map(([id, preset]) => (
              <SelectItem key={id} value={id}>
                {preset.emoji} {preset.label}
              </SelectItem>
            ))}
            {userPresetEntries.length > 0 && (
              <>
                <div className="px-2 py-1.5 text-xs font-medium text-muted-foreground border-t mt-1 pt-2">
                  我的预设
                </div>
                {userPresetEntries.map(([id, meta]) => (
                  <SelectItem key={id} value={id}>
                    {meta.emoji || '⭐'} {meta.name}
                  </SelectItem>
                ))}
              </>
            )}
            <div className="border-t mt-1 pt-1">
              <SelectItem value="__create_new__">
                <span className="flex items-center gap-1 text-primary">
                  <Plus className="h-3.5 w-3.5" />
                  新建预设...
                </span>
              </SelectItem>
            </div>
          </SelectContent>
        </Select>
      </div>

      <Dialog open={showCreateDialog} onOpenChange={setShowCreateDialog}>
        <DialogContent className="sm:max-w-[400px]">
          <DialogHeader>
            <DialogTitle>新建 API 预设</DialogTitle>
            <DialogDescription>
              创建一个新的 API 配置预设，方便快速切换不同的服务商
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-2">
            <div className="space-y-2">
              <Label>预设名称</Label>
              <Input
                value={newPresetName}
                onChange={(e) => {
                  setNewPresetName(e.target.value);
                  setCreateError('');
                }}
                placeholder="例如：Claude API"
                autoFocus
                onKeyDown={(e) => {
                  if (e.key === 'Enter') handleCreatePreset();
                }}
              />
              {createError && (
                <p className="text-xs text-destructive">{createError}</p>
              )}
            </div>
            <div className="space-y-2">
              <Label>图标 (Emoji)</Label>
              <Input
                value={newPresetEmoji}
                onChange={(e) => setNewPresetEmoji(e.target.value)}
                placeholder="⭐"
                className="w-20"
                maxLength={4}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowCreateDialog(false)}>
              取消
            </Button>
            <Button onClick={handleCreatePreset}>
              创建
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>删除预设</AlertDialogTitle>
            <AlertDialogDescription>
              确定要删除预设「{userPresets[currentPreset]?.emoji} {userPresets[currentPreset]?.name}」吗？
              该预设保存的 API 配置将被永久删除，此操作不可撤销。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDeletePreset}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
};

export default PresetManager;
