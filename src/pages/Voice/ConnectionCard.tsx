import { Check, AlertTriangle, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';

interface ConnectionCardProps {
  localGsvApiUrl: string;
  isTesting: boolean;
  testResult: 'success' | 'fail' | null;
  onUrlChange: (url: string) => void;
  onUrlBlur: () => void;
  onTest: () => void;
}

const ConnectionCard = ({
  localGsvApiUrl,
  isTesting,
  testResult,
  onUrlChange,
  onUrlBlur,
  onTest,
}: ConnectionCardProps) => {
  return (
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
              onChange={(e) => onUrlChange(e.target.value)}
              onBlur={onUrlBlur}
              className="pl-9 bg-white/50 border-slate-200 font-mono text-sm"
              placeholder="http://192.168.100.171:9880/tts"
            />
            <div className="absolute left-3 top-2.5 text-slate-400 text-xs font-bold">API</div>
          </div>
          <Button
            className="bg-primary hover:bg-primary/90 text-white min-w-[80px]"
            onClick={onTest}
            disabled={isTesting}
          >
            {isTesting ? <Loader2 className="w-4 h-4 animate-spin" /> : '测试'}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
};

export default ConnectionCard;
