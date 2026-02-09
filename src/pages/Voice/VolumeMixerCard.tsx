import { Square, Volume2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Slider } from '@/components/ui/slider';

interface VolumeMixerCardProps {
  volume: number;
  isPlaying: boolean;
  isSynthesizing: boolean;
  onVolumeChange: (value: number[]) => void;
  onStopPlayback: () => void;
}

const VolumeMixerCard = ({
  volume,
  isPlaying,
  isSynthesizing,
  onVolumeChange,
  onStopPlayback,
}: VolumeMixerCardProps) => {
  return (
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
              {Math.round(volume * 100)}%
            </span>
          </div>
          <Slider
            value={[Math.round(volume * 100)]}
            onValueChange={onVolumeChange}
            max={100}
            step={1}
            className="py-1"
          />
        </div>

        <div className="flex gap-2">
          <Button
            variant="outline"
            className="flex-1 border-red-100 text-red-500 hover:bg-red-50 hover:text-red-600 h-9 transition-colors"
            onClick={onStopPlayback}
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
  );
};

export default VolumeMixerCard;
