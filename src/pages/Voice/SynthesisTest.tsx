import { Play, Square, Check, AlertTriangle, Loader2, MessageSquare } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';

interface CharacterListItem {
  id: string;
  name: string;
}

interface SynthesisTestProps {
  testText: string;
  testEmotion: string;
  selectedCharacterId: string;
  characterList: CharacterListItem[];
  isSynthesizingTest: boolean;
  isPlaying: boolean;
  synthesizeResult: { success: boolean; message: string } | null;
  onTestTextChange: (text: string) => void;
  onTestEmotionChange: (emotion: string) => void;
  onSelectCharacter: (id: string) => void;
  onSynthesize: () => void;
  onStopPlayback: () => void;
}

const SynthesisTest = ({
  testText,
  testEmotion,
  selectedCharacterId,
  characterList,
  isSynthesizingTest,
  isPlaying,
  synthesizeResult,
  onTestTextChange,
  onTestEmotionChange,
  onSelectCharacter,
  onSynthesize,
  onStopPlayback,
}: SynthesisTestProps) => {
  return (
    <Card className="glass border-none shadow-custom overflow-hidden">
      <CardHeader className="pb-4">
        <CardTitle className="text-base font-semibold flex items-center gap-2">
          <span className="w-2 h-6 bg-green-500 rounded-full"></span>
          <MessageSquare className="w-4 h-4 text-green-600" />
          语音合成测试
        </CardTitle>
        <CardDescription>输入文本并选择情感进行语音合成测试</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <Label className="text-xs font-semibold text-slate-500 uppercase tracking-wider">测试文本</Label>
          <Textarea
            className="bg-white/60 border-slate-200 min-h-[80px] resize-none"
            placeholder="输入要合成的文本..."
            value={testText}
            onChange={(e) => onTestTextChange(e.target.value)}
          />
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-2">
            <Label className="text-xs font-semibold text-slate-500 uppercase tracking-wider">情感</Label>
            <Select value={testEmotion} onValueChange={onTestEmotionChange}>
              <SelectTrigger className="bg-white/60 border-slate-200 h-10">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="calm">平静 (calm)</SelectItem>
                <SelectItem value="happy">开心 (happy)</SelectItem>
                <SelectItem value="angry">愤怒 (angry)</SelectItem>
                <SelectItem value="sad">悲伤 (sad)</SelectItem>
                <SelectItem value="fear">恐惧 (fear)</SelectItem>
                <SelectItem value="surprised">惊讶 (surprised)</SelectItem>
                <SelectItem value="depressed">沮丧 (depressed)</SelectItem>
                <SelectItem value="disgust">厌恶 (disgust)</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label className="text-xs font-semibold text-slate-500 uppercase tracking-wider">使用角色</Label>
            <Select value={selectedCharacterId} onValueChange={onSelectCharacter}>
              <SelectTrigger className="bg-white/60 border-slate-200 h-10">
                <SelectValue placeholder="当前选中角色" />
              </SelectTrigger>
              <SelectContent>
                {characterList.map((char) => (
                  <SelectItem key={char.id} value={char.id}>{char.name}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <div className="flex gap-3 items-center">
          <Button
            className="flex-1 bg-gradient-to-r from-green-500 to-emerald-600 text-white shadow-md hover:shadow-lg hover:opacity-90 transition-all"
            onClick={onSynthesize}
            disabled={isSynthesizingTest || !testText.trim()}
          >
            {isSynthesizingTest ? (
              <>
                <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                合成中...
              </>
            ) : (
              <>
                <Play className="w-4 h-4 mr-2" />
                合成并播放
              </>
            )}
          </Button>
          <Button
            variant="outline"
            className="border-red-200 text-red-600 hover:bg-red-50"
            onClick={onStopPlayback}
            disabled={!isPlaying}
          >
            <Square className="w-4 h-4 mr-1 fill-current" />
            停止
          </Button>
        </div>

        {synthesizeResult && (
          <div className={`p-3 rounded-lg border text-sm ${
            synthesizeResult.success
              ? 'bg-green-50 border-green-200 text-green-700'
              : 'bg-red-50 border-red-200 text-red-700'
          }`}>
            {synthesizeResult.success ? (
              <div className="flex items-center gap-2">
                <Check className="w-4 h-4" />
                {synthesizeResult.message}
              </div>
            ) : (
              <div className="flex items-center gap-2">
                <AlertTriangle className="w-4 h-4" />
                {synthesizeResult.message}
              </div>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
};

export default SynthesisTest;
