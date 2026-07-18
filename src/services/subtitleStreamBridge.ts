import { isTauri, subtitleWindow, type SubtitleContentUpdate } from './tauri';

const IPC_FRAME_INTERVAL_MS = 16;

interface SubtitleSnapshot {
  text: string;
  speaker: string | null;
  isStreaming: boolean;
  forceSnapshot: boolean;
  final: boolean;
  boundary: boolean;
}

interface PublishOptions {
  forceSnapshot?: boolean;
  final?: boolean;
  boundary?: boolean;
}

/**
 * 将高频字幕片段合并为每帧最多一次 IPC，并在连续文本场景下只传输增量。
 * 完整文本始终保留在主窗口 Store 中；字幕窗口重建时可主动请求快照恢复状态。
 */
class SubtitleStreamBridge {
  private queue: SubtitleSnapshot[] = [];
  private frameTimer: ReturnType<typeof setTimeout> | null = null;
  private sending = false;
  private requiresSnapshot = true;
  private lastSentText = '';
  private lastSentSpeaker: string | null = null;
  private lastSentStreaming = false;

  publish(
    text: string,
    speaker: string | null,
    isStreaming: boolean,
    options: PublishOptions = {},
  ): void {
    if (!isTauri()) return;

    const update: SubtitleSnapshot = {
      text,
      speaker,
      isStreaming,
      forceSnapshot: options.forceSnapshot ?? false,
      final: options.final ?? false,
      boundary: options.boundary ?? false,
    };

    const lastQueued = this.queue.at(-1);
    if (lastQueued && !lastQueued.final && !update.boundary) {
      update.forceSnapshot = update.forceSnapshot || lastQueued.forceSnapshot;
      this.queue[this.queue.length - 1] = update;
    } else {
      this.queue.push(update);
    }

    this.scheduleFlush();
  }

  sync(text: string, speaker: string | null, isStreaming: boolean): void {
    this.publish(text, speaker, isStreaming, { forceSnapshot: true });
  }

  private scheduleFlush(): void {
    if (this.sending || this.frameTimer || this.queue.length === 0) return;

    this.frameTimer = setTimeout(() => {
      this.frameTimer = null;
      void this.flush();
    }, IPC_FRAME_INTERVAL_MS);
  }

  private async flush(): Promise<void> {
    if (this.sending) return;

    const pending = this.queue.shift();
    if (!pending) return;

    this.sending = true;

    const sendSnapshot = this.requiresSnapshot
      || pending.forceSnapshot
      || !pending.text.startsWith(this.lastSentText);
    const delta = sendSnapshot ? '' : pending.text.slice(this.lastSentText.length);
    const metadataChanged = pending.speaker !== this.lastSentSpeaker
      || pending.isStreaming !== this.lastSentStreaming;

    if (!sendSnapshot && !delta && !metadataChanged && !pending.final) {
      this.sending = false;
      this.scheduleFlush();
      return;
    }

    const payload: SubtitleContentUpdate = sendSnapshot
      ? {
          text: pending.text,
          reset: true,
          final: pending.final || undefined,
          speaker: pending.speaker ?? undefined,
          isStreaming: pending.isStreaming,
        }
      : {
          delta,
          final: pending.final || undefined,
          speaker: pending.speaker ?? undefined,
          isStreaming: pending.isStreaming,
        };

    try {
      await subtitleWindow.emitContentUpdate(payload);
      this.lastSentText = pending.text;
      this.lastSentSpeaker = pending.speaker;
      this.lastSentStreaming = pending.isStreaming;
      this.requiresSnapshot = false;
    } catch {
      // 字幕窗口可能尚未创建或刚被关闭；下次更新/就绪请求会发送完整快照。
      this.requiresSnapshot = true;
    } finally {
      this.sending = false;
      this.scheduleFlush();
    }
  }
}

export const subtitleStreamBridge = new SubtitleStreamBridge();
