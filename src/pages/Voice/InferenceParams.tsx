import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import type { Config } from '@/services/types';

interface InferenceParamsProps {
  config: Config | null;
  localTopK: string;
  localTopP: string;
  localTemp: string;
  onTopKChange: (value: string) => void;
  onTopPChange: (value: string) => void;
  onTempChange: (value: string) => void;
  onApply: () => void;
  updateConfig: (key: string, value: unknown) => Promise<void>;
}

const InferenceParams = ({
  config,
  localTopK,
  localTopP,
  localTemp,
  onTopKChange,
  onTopPChange,
  onTempChange,
  onApply,
  updateConfig,
}: InferenceParamsProps) => {
  const minRefAudios =
    config &&
    'gsv_min_ref_audios' in config &&
    typeof config.gsv_min_ref_audios === 'number'
      ? config.gsv_min_ref_audios
      : 2;
  const maxRefAudios =
    config &&
    'gsv_max_ref_audios' in config &&
    typeof config.gsv_max_ref_audios === 'number'
      ? config.gsv_max_ref_audios
      : 3;
  const emotionThreshold =
    config &&
    'gsv_emotion_threshold' in config &&
    typeof config.gsv_emotion_threshold === 'number'
      ? config.gsv_emotion_threshold
      : 0.3;

  return (
    <Card className="glass border-none shadow-custom">
      <CardContent className="pt-6">
        <div className="flex flex-col gap-4">
          <div className="flex items-center justify-between">
            <Label className="font-semibold text-sm">推理参数配置</Label>
            <Button size="sm" className="h-7 text-xs bg-slate-800 text-white" onClick={onApply}>
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
                onChange={(e) => onTopKChange(e.target.value)}
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
                onChange={(e) => onTopPChange(e.target.value)}
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
                onChange={(e) => onTempChange(e.target.value)}
              />
            </div>
          </div>

          <div className="mt-4 pt-4 border-t border-slate-200">
            <div className="flex items-center gap-2 mb-3">
              <span className="text-xs font-semibold text-slate-500 uppercase tracking-wider">参考音频配置</span>
              <span className="text-xs text-slate-400">(设置太多可能导致速度变慢)</span>
            </div>
            <div className="grid grid-cols-3 gap-6">
              <div className="space-y-2">
                <div className="flex justify-between text-xs">
                  <span className="text-slate-500">最少音频数</span>
                  <span className="font-mono bg-slate-100 px-1 rounded">{minRefAudios}</span>
                </div>
                <Input
                  type="number"
                  min={1}
                  max={5}
                  className="h-8 bg-white/50 text-center text-xs"
                  value={minRefAudios}
                  onChange={(e) => updateConfig('gsv_min_ref_audios', parseInt(e.target.value) || 2)}
                />
              </div>
              <div className="space-y-2">
                <div className="flex justify-between text-xs">
                  <span className="text-slate-500">最多音频数</span>
                  <span className="font-mono bg-slate-100 px-1 rounded">{maxRefAudios}</span>
                </div>
                <Input
                  type="number"
                  min={1}
                  max={5}
                  className="h-8 bg-white/50 text-center text-xs"
                  value={maxRefAudios}
                  onChange={(e) => updateConfig('gsv_max_ref_audios', parseInt(e.target.value) || 3)}
                />
              </div>
              <div className="space-y-2">
                <div className="flex justify-between text-xs">
                  <span className="text-slate-500">情感阈值</span>
                  <span className="font-mono bg-slate-100 px-1 rounded">{(emotionThreshold * 100).toFixed(0)}%</span>
                </div>
                <Input
                  type="number"
                  min={0.1}
                  max={0.9}
                  step={0.1}
                  className="h-8 bg-white/50 text-center text-xs"
                  value={emotionThreshold}
                  onChange={(e) => updateConfig('gsv_emotion_threshold', parseFloat(e.target.value) || 0.3)}
                />
              </div>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
};

export default InferenceParams;
