import { useEffect, useState, useCallback, useRef } from 'react';
import { Volume2, Mic, Loader2 } from 'lucide-react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import VoiceInputSettings from '@/components/VoiceInputSettings';
import { useConfigStore, useConfig, useConfigLoading } from '@/stores/configStore';
import { useTTSStore } from '@/stores/ttsStore';
import { synthesize } from '@/services/tts';
import type { GSVCharacter } from '@/services/types';
import { open } from '@tauri-apps/plugin-dialog';

import GlobalControlCard from './GlobalControlCard';
import VolumeMixerCard from './VolumeMixerCard';
import ConnectionCard from './ConnectionCard';
import CharacterManagement from './CharacterManagement';
import InferenceParams from './InferenceParams';
import SynthesisTest from './SynthesisTest';
import XunfeiSettings from './XunfeiSettings';

const Voice = () => {
  const config = useConfig();
  const isLoading = useConfigLoading();
  const loadConfig = useConfigStore((state) => state.loadConfig);
  const updateConfig = useConfigStore((state) => state.updateConfig);
  const updateConfigBatch = useConfigStore((state) => state.updateConfigBatch);

  const isPlaying = useTTSStore((state) => state.isPlaying);
  const isSynthesizing = useTTSStore((state) => state.isSynthesizing);
  const testEngine = useTTSStore((state) => state.testEngine);
  const stopPlayback = useTTSStore((state) => state.stopPlayback);
  const setTTSVolume = useTTSStore((state) => state.setVolume);

  const [localGsvApiUrl, setLocalGsvApiUrl] = useState('');
  const [isTesting, setIsTesting] = useState(false);
  const [testResult, setTestResult] = useState<'success' | 'fail' | null>(null);

  const [selectedCharacterId, setSelectedCharacterId] = useState<string>('');
  const [editingCharacter, setEditingCharacter] = useState<Partial<GSVCharacter>>({});

  const [localTopK, setLocalTopK] = useState('50');
  const [localTopP, setLocalTopP] = useState('0.9');
  const [localTemp, setLocalTemp] = useState('1.0');

  const [testText, setTestText] = useState('你好，这是一段语音合成测试。');
  const [testEmotion, setTestEmotion] = useState('calm');
  const [isSynthesizingTest, setIsSynthesizingTest] = useState(false);
  const [synthesizeResult, setSynthesizeResult] = useState<{ success: boolean; message: string } | null>(null);

  useEffect(() => {
    loadConfig();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const initializedRef = useRef(false);
  useEffect(() => {
    if (config && !initializedRef.current) {
      setLocalGsvApiUrl(config.gsv_api_url || '');
      setLocalTopK(String(config.gsv_top_k || 50));
      setLocalTopP(String(config.gsv_top_p || 0.9));
      setLocalTemp(String(config.gsv_temperature || 1.0));

      if (config.gsv_current_character) {
        setSelectedCharacterId(config.gsv_current_character);
      }
      initializedRef.current = true;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config]);

  useEffect(() => {
    const characterData = config?.gsv_characters?.[selectedCharacterId];
    if (selectedCharacterId && characterData) {
      setEditingCharacter(characterData);
    } else {
      setEditingCharacter({});
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedCharacterId]);

  const characters = config?.gsv_characters || {};
  const characterList = Object.entries(characters).map(([id, char]) => ({
    id,
    ...char,
  }));

  const handleTestGSV = async () => {
    setIsTesting(true);
    setTestResult(null);
    try {
      const available = await testEngine('gpt_sovits', localGsvApiUrl || undefined);
      setTestResult(available ? 'success' : 'fail');
    } finally {
      setIsTesting(false);
    }
  };

  const handleStopPlayback = async () => {
    await stopPlayback();
  };

  const handleVolumeChange = useCallback(async (value: number[]) => {
    const volume = value[0] / 100;
    await updateConfig('tts_volume', volume);
    await setTTSVolume(volume);
  }, [updateConfig, setTTSVolume]);

  const handleSaveCharacter = async () => {
    if (!selectedCharacterId || !editingCharacter.name) return;

    const newName = editingCharacter.name.trim();
    if (!newName) return;

    if (newName !== selectedCharacterId) {
      if (characters[newName] && newName !== selectedCharacterId) {
        return;
      }

      const { [selectedCharacterId]: _, ...rest } = characters;
      const updatedCharacters = {
        ...rest,
        [newName]: { ...editingCharacter, id: newName, name: newName } as GSVCharacter,
      };
      await updateConfig('gsv_characters', updatedCharacters);

      if (config?.gsv_current_character === selectedCharacterId) {
        await updateConfig('gsv_current_character', newName);
      }

      setSelectedCharacterId(newName);
    } else {
      const updatedCharacters = {
        ...characters,
        [selectedCharacterId]: { ...editingCharacter, id: selectedCharacterId } as GSVCharacter,
      };
      await updateConfig('gsv_characters', updatedCharacters);
    }
  };

  const handleAddCharacter = async () => {
    const hasEditingData = editingCharacter.name?.trim() ||
                           editingCharacter.ref_audio_dir ||
                           editingCharacter.gpt_model_path ||
                           editingCharacter.sovits_model_path ||
                           editingCharacter.emotion_audio_json;

    let newName: string;
    let newCharacter: GSVCharacter;

    if (hasEditingData && editingCharacter.name?.trim()) {
      newName = editingCharacter.name.trim();

      if (characters[newName]) {
        return;
      }

      newCharacter = {
        id: newName,
        name: newName,
        ref_audio_dir: editingCharacter.ref_audio_dir || '',
        gpt_model_path: editingCharacter.gpt_model_path || '',
        sovits_model_path: editingCharacter.sovits_model_path || '',
        emotion_audio_json: editingCharacter.emotion_audio_json,
      };
    } else {
      const baseName = '新角色';
      newName = baseName;
      let counter = 1;
      while (characters[newName]) {
        newName = `${baseName}${counter}`;
        counter++;
      }

      newCharacter = {
        id: newName,
        name: newName,
        ref_audio_dir: '',
        gpt_model_path: '',
        sovits_model_path: '',
      };
    }

    const updatedCharacters = {
      ...characters,
      [newName]: newCharacter,
    };
    await updateConfig('gsv_characters', updatedCharacters);
    setSelectedCharacterId(newName);
  };

  const handleDeleteCharacter = async () => {
    if (!selectedCharacterId) return;

    const { [selectedCharacterId]: _, ...rest } = characters;
    await updateConfig('gsv_characters', rest);

    const remainingIds = Object.keys(rest);
    setSelectedCharacterId(remainingIds[0] || '');
  };

  const handleApplyParams = async () => {
    await updateConfigBatch({
      gsv_top_k: parseInt(localTopK) || 50,
      gsv_top_p: parseFloat(localTopP) || 0.9,
      gsv_temperature: parseFloat(localTemp) || 1.0,
    });
  };

  const handleTestSynthesize = async () => {
    if (!testText.trim()) {
      setSynthesizeResult({ success: false, message: '请输入测试文本' });
      return;
    }

    setIsSynthesizingTest(true);
    setSynthesizeResult(null);

    try {
      const response = await synthesize({
        text: testText,
        emotion: testEmotion,
        speaker: selectedCharacterId || undefined,
        speed: 1.0,
      });

      if (response.success && response.data?.success) {
        setSynthesizeResult({
          success: true,
          message: `合成成功！使用角色: ${response.data.speaker_used || '默认'}, 情感: ${response.data.emotion_used || testEmotion}`,
        });
      } else {
        setSynthesizeResult({
          success: false,
          message: response.data?.error || response.error || '合成失败',
        });
      }
    } catch (error) {
      setSynthesizeResult({
        success: false,
        message: error instanceof Error ? error.message : '合成请求失败',
      });
    } finally {
      setIsSynthesizingTest(false);
    }
  };

  const handleBrowseDirectory = async (field: 'ref_audio_dir') => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: field === 'ref_audio_dir' ? '选择参考音频目录' : '选择目录',
      });
      if (selected && typeof selected === 'string') {
        setEditingCharacter({ ...editingCharacter, [field]: selected });
      }
    } catch (e) {
      console.error('选择目录失败:', e);
    }
  };

  const handleBrowseFile = async (field: 'emotion_audio_json' | 'gpt_model_path' | 'sovits_model_path') => {
    try {
      const filters = field === 'emotion_audio_json'
        ? [{ name: 'JSON', extensions: ['json'] }]
        : field === 'gpt_model_path'
          ? [{ name: 'Checkpoint', extensions: ['ckpt', 'pt', 'pth'] }]
          : [{ name: 'Model', extensions: ['pth', 'pt'] }];

      const selected = await open({
        directory: false,
        multiple: false,
        title: field === 'emotion_audio_json' ? '选择情感音频配置文件'
             : field === 'gpt_model_path' ? '选择 GPT 模型'
             : '选择 SoVITS 模型',
        filters,
      });
      if (selected && typeof selected === 'string') {
        setEditingCharacter({ ...editingCharacter, [field]: selected });
      }
    } catch (e) {
      console.error('选择文件失败:', e);
    }
  };

  const handleSynthesisCharacterSelect = (value: string) => {
    setSelectedCharacterId(value);
    updateConfig('gsv_current_character', value);
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
              <div className="col-span-12 lg:col-span-4 space-y-6">
                <GlobalControlCard config={config} updateConfig={updateConfig} />
                <VolumeMixerCard
                  volume={config?.tts_volume ?? 0.8}
                  isPlaying={isPlaying}
                  isSynthesizing={isSynthesizing}
                  onVolumeChange={handleVolumeChange}
                  onStopPlayback={handleStopPlayback}
                />
              </div>

              <div className="col-span-12 lg:col-span-8 space-y-6">
                 <Tabs defaultValue="gpt-sovits" className="w-full">
                    <TabsList className="w-full justify-start bg-transparent p-0 gap-6 border-b border-slate-200/60 mb-6 h-auto">
                       <TabsTrigger value="gpt-sovits" className="h-9 px-0 pb-2 rounded-none bg-transparent border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent data-[state=active]:shadow-none data-[state=active]:text-primary text-muted-foreground transition-all hover:text-primary/80">GPT-SoVITS</TabsTrigger>
                       <TabsTrigger value="xf" className="h-9 px-0 pb-2 rounded-none bg-transparent border-b-2 border-transparent data-[state=active]:border-blue-500 data-[state=active]:bg-transparent data-[state=active]:shadow-none data-[state=active]:text-blue-600 text-muted-foreground transition-all hover:text-blue-600/80">讯飞语音</TabsTrigger>
                    </TabsList>

                    <TabsContent value="gpt-sovits" className="space-y-6 animate-in slide-in-from-right-4 duration-300">
                       <ConnectionCard
                         localGsvApiUrl={localGsvApiUrl}
                         isTesting={isTesting}
                         testResult={testResult}
                         onUrlChange={setLocalGsvApiUrl}
                         onUrlBlur={() => updateConfig('gsv_api_url', localGsvApiUrl)}
                         onTest={handleTestGSV}
                       />

                       <CharacterManagement
                         config={config}
                         characterList={characterList}
                         selectedCharacterId={selectedCharacterId}
                         editingCharacter={editingCharacter}
                         onSelectCharacter={setSelectedCharacterId}
                         onEditCharacter={setEditingCharacter}
                         onSave={handleSaveCharacter}
                         onAdd={handleAddCharacter}
                         onDelete={handleDeleteCharacter}
                         onBrowseDirectory={handleBrowseDirectory}
                         onBrowseFile={handleBrowseFile}
                         updateConfig={updateConfig}
                       />

                       <InferenceParams
                         config={config}
                         localTopK={localTopK}
                         localTopP={localTopP}
                         localTemp={localTemp}
                         onTopKChange={setLocalTopK}
                         onTopPChange={setLocalTopP}
                         onTempChange={setLocalTemp}
                         onApply={handleApplyParams}
                         updateConfig={updateConfig}
                       />

                       <SynthesisTest
                         testText={testText}
                         testEmotion={testEmotion}
                         selectedCharacterId={selectedCharacterId}
                         characterList={characterList}
                         isSynthesizingTest={isSynthesizingTest}
                         isPlaying={isPlaying}
                         synthesizeResult={synthesizeResult}
                         onTestTextChange={setTestText}
                         onTestEmotionChange={setTestEmotion}
                         onSelectCharacter={handleSynthesisCharacterSelect}
                         onSynthesize={handleTestSynthesize}
                         onStopPlayback={handleStopPlayback}
                       />
                    </TabsContent>

                    <TabsContent value="xf" className="space-y-6 animate-in slide-in-from-right-4 duration-300">
                       <XunfeiSettings config={config} updateConfig={updateConfig} />
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
