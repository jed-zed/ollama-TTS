import { FolderOpen, Plus, Save, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Checkbox } from '@/components/ui/checkbox';
import { Separator } from '@/components/ui/separator';
import type { Config, GSVCharacter } from '@/services/types';

interface CharacterListItem {
  id: string;
  name: string;
}

interface CharacterManagementProps {
  config: Config | null;
  characterList: CharacterListItem[];
  selectedCharacterId: string;
  editingCharacter: Partial<GSVCharacter>;
  onSelectCharacter: (id: string) => void;
  onEditCharacter: (char: Partial<GSVCharacter>) => void;
  onSave: () => void;
  onAdd: () => void;
  onDelete: () => void;
  onBrowseDirectory: (field: 'ref_audio_dir') => void;
  onBrowseFile: (field: 'emotion_audio_json' | 'gpt_model_path' | 'sovits_model_path') => void;
  updateConfig: (key: string, value: unknown) => Promise<void>;
}

const CharacterManagement = ({
  config,
  characterList,
  selectedCharacterId,
  editingCharacter,
  onSelectCharacter,
  onEditCharacter,
  onSave,
  onAdd,
  onDelete,
  onBrowseDirectory,
  onBrowseFile,
  updateConfig,
}: CharacterManagementProps) => {
  return (
    <Card className="glass border-none shadow-custom overflow-hidden">
      <CardHeader className="pb-4 border-b border-slate-100">
        <CardTitle className="text-base font-semibold flex items-center gap-2">
          <span className="w-2 h-6 bg-purple-500 rounded-full"></span>
          角色音色管理
        </CardTitle>
        <CardDescription>配置精调模型和参考音频以克隆目标声线</CardDescription>
      </CardHeader>
      <CardContent className="pt-6 space-y-6">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div className="space-y-2">
            <Label className="text-xs font-semibold text-slate-500 uppercase tracking-wider">当前角色</Label>
            <Select
              value={selectedCharacterId}
              onValueChange={onSelectCharacter}
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
        </div>

        <div className="bg-slate-50/50 rounded-xl p-5 border border-slate-100 space-y-4">
          <div className="grid grid-cols-[100px_1fr_auto] gap-3 items-center">
            <Label className="text-right text-xs text-slate-500">角色名称</Label>
            <Input
              className="bg-white h-9 border-slate-200"
              placeholder="Display Name"
              value={editingCharacter.name || ''}
              onChange={(e) => onEditCharacter({ ...editingCharacter, name: e.target.value })}
            />
            <div className="w-16"></div>
          </div>

          <div className="grid grid-cols-[100px_1fr_auto] gap-3 items-center">
            <Label className="text-right text-xs text-slate-500">参考音频目录</Label>
            <Input
              className="bg-white h-9 border-slate-200 text-xs font-mono text-slate-600"
              value={editingCharacter.ref_audio_dir || ''}
              onChange={(e) => onEditCharacter({ ...editingCharacter, ref_audio_dir: e.target.value })}
            />
            <Button variant="outline" size="sm" className="w-16 h-9" onClick={() => onBrowseDirectory('ref_audio_dir')}><FolderOpen className="w-4 h-4" /></Button>
          </div>

          <div className="grid grid-cols-[100px_1fr_auto] gap-3 items-center">
            <Label className="text-right text-xs text-slate-500">情感音频配置</Label>
            <Input
              className="bg-white h-9 border-slate-200 text-xs font-mono text-slate-600"
              value={editingCharacter.emotion_audio_json || ''}
              onChange={(e) => onEditCharacter({ ...editingCharacter, emotion_audio_json: e.target.value })}
            />
            <Button variant="outline" size="sm" className="w-16 h-9" onClick={() => onBrowseFile('emotion_audio_json')}><FolderOpen className="w-4 h-4" /></Button>
          </div>

          <div className="grid grid-cols-[100px_1fr_auto] gap-3 items-center">
            <Label className="text-right text-xs text-slate-500">GPT模型路径</Label>
            <Input
              className="bg-white h-9 border-slate-200 text-xs font-mono text-slate-600"
              value={editingCharacter.gpt_model_path || ''}
              onChange={(e) => onEditCharacter({ ...editingCharacter, gpt_model_path: e.target.value })}
            />
            <Button variant="outline" size="sm" className="w-16 h-9" onClick={() => onBrowseFile('gpt_model_path')}><FolderOpen className="w-4 h-4" /></Button>
          </div>

          <div className="grid grid-cols-[100px_1fr_auto] gap-3 items-center">
            <Label className="text-right text-xs text-slate-500">SoVITS模型</Label>
            <Input
              className="bg-white h-9 border-slate-200 text-xs font-mono text-slate-600"
              value={editingCharacter.sovits_model_path || ''}
              onChange={(e) => onEditCharacter({ ...editingCharacter, sovits_model_path: e.target.value })}
            />
            <Button variant="outline" size="sm" className="w-16 h-9" onClick={() => onBrowseFile('sovits_model_path')}><FolderOpen className="w-4 h-4" /></Button>
          </div>
        </div>

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

        <div className="flex gap-3">
          <Button
            className="flex-1 bg-gradient-to-r from-green-500 to-emerald-600 text-white shadow-md hover:shadow-lg hover:opacity-90 transition-all"
            onClick={onAdd}
          >
            <Plus className="w-4 h-4 mr-2" /> 添加角色
          </Button>
          <Button
            variant="outline"
            className="flex-1 border-blue-200 text-blue-600 hover:bg-blue-50"
            onClick={onSave}
            disabled={!selectedCharacterId}
          >
            <Save className="w-4 h-4 mr-2" /> 保存
          </Button>
          <Button
            variant="outline"
            className="flex-1 border-red-200 text-red-600 hover:bg-red-50"
            onClick={onDelete}
            disabled={!selectedCharacterId}
          >
            <Trash2 className="w-4 h-4 mr-2" /> 删除
          </Button>
        </div>
      </CardContent>
    </Card>
  );
};

export default CharacterManagement;
