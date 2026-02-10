import { isTauri } from '@/services/tauri';
import { speechService } from '@/services/speech';
import type { SpeechConfig, SpeechEngine } from '@/services/speech';
import { API_BASE_URL } from '@/lib/constants';

type VoiceShortcutState = 'idle' | 'recording' | 'recognizing';

type StateChangeHandler = (state: VoiceShortcutState) => void;
type ResultHandler = (text: string) => void;
type ErrorHandler = (error: string) => void;

class GlobalShortcutService {
  private currentShortcut: string | null = null;
  private state: VoiceShortcutState = 'idle';
  private onStateChange: StateChangeHandler | null = null;
  private onResult: ResultHandler | null = null;
  private onError: ErrorHandler | null = null;
  private speechConfig: SpeechConfig | null = null;
  private autoPaste = true;

  setHandlers(handlers: {
    onStateChange?: StateChangeHandler;
    onResult?: ResultHandler;
    onError?: ErrorHandler;
  }) {
    if (handlers.onStateChange) this.onStateChange = handlers.onStateChange;
    if (handlers.onResult) this.onResult = handlers.onResult;
    if (handlers.onError) this.onError = handlers.onError;
  }

  private setState(newState: VoiceShortcutState) {
    this.state = newState;
    this.onStateChange?.(newState);
  }

  async loadSpeechConfig() {
    try {
      const response = await fetch(`${API_BASE_URL}/api/config`);
      if (response.ok) {
        const result = await response.json();
        const configData = result.data || result;
        const engine: SpeechEngine = configData.speech_engine || 'baidu';

        if (engine === 'xunfei' && configData.xunfei_app_id && configData.xunfei_api_key && configData.xunfei_api_secret) {
          this.speechConfig = {
            engine: 'xunfei',
            xunfei: {
              appId: configData.xunfei_app_id,
              apiKey: configData.xunfei_api_key,
              apiSecret: configData.xunfei_api_secret,
            },
          };
        } else if (configData.baidu_api_key && configData.baidu_secret_key) {
          this.speechConfig = {
            engine: 'baidu',
            baidu: {
              apiKey: configData.baidu_api_key,
              secretKey: configData.baidu_secret_key,
            },
          };
        }
      }
    } catch {
      console.warn('[GlobalShortcut] Failed to load speech config');
    }
  }

  async register(shortcut: string): Promise<boolean> {
    if (!isTauri()) return false;

    try {
      if (this.currentShortcut) {
        await this.unregister();
      }

      const { register } = await import('@tauri-apps/plugin-global-shortcut');
      await register(shortcut, async (event) => {
        if (event.state === 'Pressed') {
          await this.handleShortcutPressed();
        }
      });

      this.currentShortcut = shortcut;
      await this.loadSpeechConfig();
      console.log(`[GlobalShortcut] Registered: ${shortcut}`);
      return true;
    } catch (e) {
      console.error('[GlobalShortcut] Register failed:', e);
      this.onError?.(`快捷键注册失败: ${e}`);
      return false;
    }
  }

  async unregister(): Promise<void> {
    if (!isTauri() || !this.currentShortcut) return;

    try {
      const { unregister } = await import('@tauri-apps/plugin-global-shortcut');
      await unregister(this.currentShortcut);
      console.log(`[GlobalShortcut] Unregistered: ${this.currentShortcut}`);
      this.currentShortcut = null;
    } catch (e) {
      console.warn('[GlobalShortcut] Unregister failed:', e);
    }
  }

  private async handleShortcutPressed() {
    if (this.state === 'recognizing') return;

    if (this.state === 'idle') {
      await this.startRecording();
    } else if (this.state === 'recording') {
      await this.stopAndRecognize();
    }
  }

  private async startRecording() {
    if (!this.speechConfig) {
      await this.loadSpeechConfig();
      if (!this.speechConfig) {
        this.onError?.('请先配置语音识别 API（百度或讯飞）');
        return;
      }
    }

    const success = await speechService.startRecording();
    if (success) {
      this.setState('recording');
    } else {
      this.onError?.('无法访问麦克风，请检查权限');
    }
  }

  private async stopAndRecognize() {
    this.setState('recognizing');

    try {
      const audioBlob = await speechService.stopRecording();
      if (!audioBlob || !this.speechConfig) {
        this.setState('idle');
        return;
      }

      const result = await speechService.recognize(audioBlob, this.speechConfig);

      if (result.success && result.text) {
        this.onResult?.(result.text);

        if (this.autoPaste) {
          await this.clipboardPaste(result.text);
        }
      } else {
        this.onError?.(result.error || '语音识别失败');
      }
    } catch (e) {
      console.error('[GlobalShortcut] Recognition failed:', e);
      this.onError?.('语音识别失败');
    } finally {
      this.setState('idle');
    }
  }

  private async clipboardPaste(text: string) {
    if (!isTauri()) return;

    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('clipboard_paste', { text });
    } catch (e) {
      console.error('[GlobalShortcut] Clipboard paste failed:', e);
    }
  }

  getState(): VoiceShortcutState {
    return this.state;
  }

  isRegistered(): boolean {
    return this.currentShortcut !== null;
  }

  getCurrentShortcut(): string | null {
    return this.currentShortcut;
  }

  setAutoPaste(enabled: boolean) {
    this.autoPaste = enabled;
  }

  async destroy() {
    if (this.state === 'recording') {
      await speechService.stopRecording();
    }
    await this.unregister();
    this.setState('idle');
  }
}

export const globalShortcutService = new GlobalShortcutService();
