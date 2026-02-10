import { AlertTriangle } from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import type { Config } from '@/services/types';

interface XunfeiSettingsProps {
  config: Config | null;
  updateConfig: (key: string, value: unknown) => Promise<void>;
}

const XunfeiSettings = ({ config, updateConfig }: XunfeiSettingsProps) => {
  return (
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
  );
};

export default XunfeiSettings;
