import { Settings } from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import type { Config } from '@/services/types';

interface GlobalControlCardProps {
  config: Config | null;
  updateConfig: (key: string, value: unknown) => Promise<void>;
}

const GlobalControlCard = ({ config, updateConfig }: GlobalControlCardProps) => {
  return (
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
  );
};

export default GlobalControlCard;
