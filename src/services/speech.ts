/**
 * 语音识别服务
 * 提供录音和语音识别功能，支持百度和讯飞双引擎
 */

import { getBackendBaseUrl } from '@/lib/backendUrl';

export type SpeechEngine = 'baidu' | 'xunfei';

export interface RecognizeResponse {
  success: boolean;
  text?: string;
  error?: string;
}

export interface BaiduSpeechConfig {
  apiKey: string;
  secretKey: string;
  sampleRate?: number;
}

export interface XunfeiSpeechConfig {
  appId: string;
  apiKey: string;
  apiSecret: string;
}

export type SpeechConfig = {
  engine: 'baidu';
  baidu: BaiduSpeechConfig;
} | {
  engine: 'xunfei';
  xunfei: XunfeiSpeechConfig;
};

class SpeechService {
  private mediaRecorder: MediaRecorder | null = null;
  private audioChunks: Blob[] = [];
  private stream: MediaStream | null = null;
  private isRecording = false;

  async startRecording(): Promise<boolean> {
    if (this.isRecording) {
      console.warn('Already recording');
      return false;
    }

    try {
      this.stream = await navigator.mediaDevices.getUserMedia({ 
        audio: {
          sampleRate: 16000,
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
        } 
      });
      
      this.mediaRecorder = new MediaRecorder(this.stream, {
        mimeType: 'audio/webm;codecs=opus'
      });
      
      this.audioChunks = [];

      this.mediaRecorder.ondataavailable = (event) => {
        if (event.data.size > 0) {
          this.audioChunks.push(event.data);
        }
      };

      this.mediaRecorder.start(100);
      this.isRecording = true;
      return true;
    } catch (error) {
      console.error('Failed to start recording:', error);
      return false;
    }
  }

  async stopRecording(): Promise<Blob | null> {
    if (!this.isRecording || !this.mediaRecorder) {
      console.warn('Not recording');
      return null;
    }

    return new Promise((resolve) => {
      if (!this.mediaRecorder) {
        resolve(null);
        return;
      }

      this.mediaRecorder.onstop = () => {
        const audioBlob = new Blob(this.audioChunks, { type: 'audio/webm' });
        this.audioChunks = [];
        this.isRecording = false;
        
        if (this.stream) {
          this.stream.getTracks().forEach(track => track.stop());
          this.stream = null;
        }
        
        resolve(audioBlob);
      };

      this.mediaRecorder.stop();
    });
  }

  async convertWebmToPcm(webmBlob: Blob): Promise<ArrayBuffer> {
    const audioContext = new AudioContext({ sampleRate: 16000 });
    const arrayBuffer = await webmBlob.arrayBuffer();
    const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);
    
    const channelData = audioBuffer.getChannelData(0);
    const pcmData = new Int16Array(channelData.length);
    
    for (let i = 0; i < channelData.length; i++) {
      const sample = Math.max(-1, Math.min(1, channelData[i]));
      pcmData[i] = sample < 0 ? sample * 0x8000 : sample * 0x7FFF;
    }
    
    await audioContext.close();
    return pcmData.buffer;
  }

  async recognize(audioBlob: Blob, config: SpeechConfig): Promise<RecognizeResponse> {
    try {
      const pcmBuffer = await this.convertWebmToPcm(audioBlob);
      const base64Audio = this.arrayBufferToBase64(pcmBuffer);

      if (config.engine === 'xunfei') {
        return await this.recognizeWithXunfei(base64Audio, config.xunfei);
      }

      return await this.recognizeWithBaidu(base64Audio, config.baidu);
    } catch (error) {
      console.error('Recognition failed:', error);
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
      };
    }
  }

  private async recognizeWithBaidu(base64Audio: string, config: BaiduSpeechConfig): Promise<RecognizeResponse> {
    const response = await fetch(`${getBackendBaseUrl()}/api/speech/recognize`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        audio_data: base64Audio,
        format: 'pcm',
        sample_rate: config.sampleRate || 16000,
        api_key: config.apiKey,
        secret_key: config.secretKey,
      }),
    });

    if (!response.ok) {
      return { success: false, error: `HTTP error: ${response.status}` };
    }

    return await response.json();
  }

  private async recognizeWithXunfei(base64Audio: string, config: XunfeiSpeechConfig): Promise<RecognizeResponse> {
    const response = await fetch(`${getBackendBaseUrl()}/api/speech/xunfei/recognize`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        audio_data: base64Audio,
        app_id: config.appId,
        api_key: config.apiKey,
        api_secret: config.apiSecret,
      }),
    });

    if (!response.ok) {
      return { success: false, error: `HTTP error: ${response.status}` };
    }

    return await response.json();
  }

  private arrayBufferToBase64(buffer: ArrayBuffer): string {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.byteLength; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    return btoa(binary);
  }

  getIsRecording(): boolean {
    return this.isRecording;
  }

  cleanup(): void {
    if (this.mediaRecorder && this.isRecording) {
      this.mediaRecorder.stop();
    }
    if (this.stream) {
      this.stream.getTracks().forEach(track => track.stop());
      this.stream = null;
    }
    this.isRecording = false;
    this.audioChunks = [];
  }
}

export const speechService = new SpeechService();
