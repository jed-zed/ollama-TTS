//! TTS 音频播放器模块
//!
//! 功能：
//! - 使用 rodio 库播放音频
//! - 支持播放队列
//! - 支持停止、音量控制
//! - 支持回调通知
//! - 支持缓冲播放模式（边合成边播放）

use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Sender, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::collections::BTreeMap;
use tracing::{info, error, debug};
use rodio::{Decoder, OutputStream, Sink};

/// 播放器命令
enum PlayerCommand {
    /// 播放音频数据
    Play(Vec<u8>),
    /// 缓冲播放模式：带序号的音频段落
    PlayBuffered {
        /// 段落序号（从 0 开始）
        seq: usize,
        /// 总段落数
        total: usize,
        /// 音频数据
        data: Vec<u8>,
    },
    /// 停止当前播放
    Stop,
    /// 停止所有并清空队列
    StopAll,
    /// 设置音量
    SetVolume(f32),
    /// 关闭播放器
    Shutdown,
}

/// 播放器状态
#[derive(Debug, Clone)]
pub struct PlayerStatus {
    /// 是否正在播放
    pub playing: bool,
    /// 队列中等待播放的数量
    pub queue_size: usize,
    /// 当前音量
    pub volume: f32,
}

/// TTS 音频播放器
/// 
/// 支持队列顺序播放，使用 rodio 作为音频后端。
pub struct TTSPlayer {
    /// 命令发送通道
    command_tx: Option<Sender<PlayerCommand>>,
    /// 播放线程句柄
    play_thread: Option<JoinHandle<()>>,
    /// 播放状态
    status: Arc<Mutex<PlayerStatus>>,
    /// 是否已初始化
    initialized: bool,
}

impl TTSPlayer {
    /// 创建新的播放器
    pub fn new() -> Self {
        let status = Arc::new(Mutex::new(PlayerStatus {
            playing: false,
            queue_size: 0,
            volume: 0.8,
        }));

        Self {
            command_tx: None,
            play_thread: None,
            status,
            initialized: false,
        }
    }

    /// 初始化播放器
    pub fn initialize(&mut self) -> bool {
        if self.initialized {
            return true;
        }

        let (tx, rx): (Sender<PlayerCommand>, Receiver<PlayerCommand>) = mpsc::channel();
        let status = self.status.clone();

        let handle = thread::spawn(move || {
            Self::player_thread(rx, status);
        });

        self.command_tx = Some(tx);
        self.play_thread = Some(handle);
        self.initialized = true;
        
        info!("TTS 播放器初始化成功");
        true
    }

    /// 播放器线程
    fn player_thread(rx: Receiver<PlayerCommand>, status: Arc<Mutex<PlayerStatus>>) {
        let (_stream, stream_handle) = match OutputStream::try_default() {
            Ok(s) => s,
            Err(e) => {
                error!("无法创建音频输出流: {}", e);
                return;
            }
        };

        let sink = match Sink::try_new(&stream_handle) {
            Ok(s) => s,
            Err(e) => {
                error!("无法创建音频 Sink: {}", e);
                return;
            }
        };

        let mut current_volume = 0.8_f32;
        sink.set_volume(current_volume);

        let mut audio_queue: Vec<Vec<u8>> = Vec::new();
        let mut playing = false;
        
        let mut buffered_segments: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
        let mut buffered_total: usize = 0;
        let mut buffered_next_seq: usize = 0;
        let mut buffered_started: bool = false;
        const BUFFER_THRESHOLD: usize = 2;

        loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(cmd) => match cmd {
                    PlayerCommand::Play(data) => {
                        info!("收到播放命令: {} 字节, 当前队列大小: {}", data.len(), audio_queue.len());
                        audio_queue.push(data);
                        if let Ok(mut s) = status.lock() {
                            s.queue_size = audio_queue.len();
                        }
                    }
                    PlayerCommand::PlayBuffered { seq, total, data } => {
                        debug!("收到缓冲播放命令: seq={}/{}, {} 字节", seq, total, data.len());
                        
                        if seq == 0 && (buffered_total != total || buffered_next_seq != 0) {
                            if !buffered_segments.is_empty() {
                                info!("新对话开始，清空旧缓冲区 (旧: {} 段)", buffered_segments.len());
                            }
                            buffered_segments.clear();
                            buffered_next_seq = 0;
                            buffered_started = false;
                        }
                        
                        buffered_segments.insert(seq, data);
                        buffered_total = total;
                        
                        if !buffered_started {
                            let ready_count = buffered_segments.len();
                            let is_last_segment = seq == total - 1;
                            
                            if ready_count >= BUFFER_THRESHOLD || is_last_segment || total == 1 {
                                buffered_started = true;
                                info!("缓冲播放启动: 已缓冲 {} 段 (阈值={})", ready_count, BUFFER_THRESHOLD);
                            }
                        }
                        
                        if let Ok(mut s) = status.lock() {
                            s.queue_size = audio_queue.len() + buffered_segments.len();
                        }
                    }
                    PlayerCommand::Stop => {
                        sink.stop();
                        playing = false;
                        if let Ok(mut s) = status.lock() {
                            s.playing = false;
                        }
                    }
                    PlayerCommand::StopAll => {
                        sink.stop();
                        audio_queue.clear();
                        buffered_segments.clear();
                        buffered_total = 0;
                        buffered_next_seq = 0;
                        buffered_started = false;
                        playing = false;
                        if let Ok(mut s) = status.lock() {
                            s.playing = false;
                            s.queue_size = 0;
                        }
                        info!("停止所有播放，队列已清空");
                    }
                    PlayerCommand::SetVolume(vol) => {
                        current_volume = vol.clamp(0.0, 1.0);
                        sink.set_volume(current_volume);
                        if let Ok(mut s) = status.lock() {
                            s.volume = current_volume;
                        }
                        info!("音量设置为: {:.0}%", current_volume * 100.0);
                    }
                    PlayerCommand::Shutdown => {
                        sink.stop();
                        info!("播放器关闭");
                        break;
                    }
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    info!("播放器通道断开");
                    break;
                }
            }

            if sink.empty() && !audio_queue.is_empty() {
                let audio_data = audio_queue.remove(0);
                let data_len = audio_data.len();
                
                if let Ok(mut s) = status.lock() {
                    s.queue_size = audio_queue.len() + buffered_segments.len();
                }

                match Decoder::new(Cursor::new(audio_data)) {
                    Ok(source) => {
                        sink.append(source);
                        playing = true;
                        if let Ok(mut s) = status.lock() {
                            s.playing = true;
                        }
                        info!("开始播放音频: {} 字节 (队列剩余: {})", data_len, audio_queue.len());
                    }
                    Err(e) => {
                        error!("音频解码失败: {} (数据大小: {} 字节)", e, data_len);
                    }
                }
            }
            
            if sink.empty() && audio_queue.is_empty() && buffered_started {
                if let Some(audio_data) = buffered_segments.remove(&buffered_next_seq) {
                    let data_len = audio_data.len();
                    let current_seq = buffered_next_seq;
                    buffered_next_seq += 1;
                    
                    if let Ok(mut s) = status.lock() {
                        s.queue_size = buffered_segments.len();
                    }

                    match Decoder::new(Cursor::new(audio_data)) {
                        Ok(source) => {
                            sink.append(source);
                            playing = true;
                            if let Ok(mut s) = status.lock() {
                                s.playing = true;
                            }
                            info!("播放缓冲段落 {}/{}: {} 字节 (剩余: {})", 
                                  current_seq + 1, buffered_total, data_len, buffered_segments.len());
                        }
                        Err(e) => {
                            error!("缓冲音频解码失败: {} (seq={}, 数据大小: {} 字节)", e, current_seq, data_len);
                        }
                    }
                    
                    if buffered_next_seq >= buffered_total && buffered_segments.is_empty() {
                        info!("缓冲播放完成，重置状态");
                        buffered_total = 0;
                        buffered_next_seq = 0;
                        buffered_started = false;
                    }
                }
            }
            
            if sink.empty() && playing && audio_queue.is_empty() && buffered_segments.is_empty() {
                debug!("当前音频播放完成，队列已空");
                playing = false;
                if let Ok(mut s) = status.lock() {
                    s.playing = false;
                }
            }
        }
    }

    /// 将音频加入播放队列
    pub fn enqueue(&mut self, audio_data: Vec<u8>) {
        if !self.initialized {
            info!("播放器未初始化，正在初始化...");
            self.initialize();
        }

        if let Some(tx) = &self.command_tx {
            let data_len = audio_data.len();
            if tx.send(PlayerCommand::Play(audio_data)).is_err() {
                error!("发送播放命令失败");
            } else {
                info!("播放命令已发送: {} 字节", data_len);
            }
        } else {
            error!("播放器通道未创建，无法发送播放命令");
        }
    }

    /// 将音频加入缓冲播放队列（边合成边播放模式）
    pub fn enqueue_buffered(&mut self, seq: usize, total: usize, audio_data: Vec<u8>) {
        if !self.initialized {
            info!("播放器未初始化，正在初始化...");
            self.initialize();
        }

        if let Some(tx) = &self.command_tx {
            let data_len = audio_data.len();
            if tx.send(PlayerCommand::PlayBuffered { seq, total, data: audio_data }).is_err() {
                error!("发送缓冲播放命令失败");
            } else {
                debug!("缓冲播放命令已发送: seq={}/{}, {} 字节", seq, total, data_len);
            }
        } else {
            error!("播放器通道未创建，无法发送缓冲播放命令");
        }
    }

    /// 播放音频数据
    pub fn play(&mut self, audio_data: Vec<u8>) {
        self.enqueue(audio_data);
    }

    /// 停止当前播放
    pub fn stop(&self) {
        if let Some(tx) = &self.command_tx {
            let _ = tx.send(PlayerCommand::Stop);
        }
    }

    /// 停止所有播放并清空队列
    pub fn stop_all(&self) {
        if let Some(tx) = &self.command_tx {
            let _ = tx.send(PlayerCommand::StopAll);
        }
    }

    /// 设置音量 (0.0 - 1.0)
    pub fn set_volume(&self, volume: f32) {
        if let Some(tx) = &self.command_tx {
            let _ = tx.send(PlayerCommand::SetVolume(volume));
        }
    }

    /// 获取播放状态
    pub fn get_status(&self) -> PlayerStatus {
        self.status.lock()
            .map(|s| s.clone())
            .unwrap_or(PlayerStatus {
                playing: false,
                queue_size: 0,
                volume: 0.8,
            })
    }

    /// 是否正在播放
    pub fn is_playing(&self) -> bool {
        self.status.lock().map(|s| s.playing).unwrap_or(false)
    }

    /// 获取队列大小
    pub fn queue_size(&self) -> usize {
        self.status.lock().map(|s| s.queue_size).unwrap_or(0)
    }

    /// 清理资源
    pub fn cleanup(&mut self) {
        if let Some(tx) = self.command_tx.take() {
            let _ = tx.send(PlayerCommand::Shutdown);
        }
        if let Some(handle) = self.play_thread.take() {
            let _ = handle.join();
        }
        self.initialized = false;
        info!("播放器资源已清理");
    }
}

impl Default for TTSPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TTSPlayer {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// TTS 管理器
/// 
/// 管理 GPT-SoVITS 引擎和音频播放器。
pub struct TTSManager {
    /// 是否启用 TTS
    pub enabled: bool,
    /// 是否自动播放
    pub auto_play: bool,
    /// 是否保存音频
    pub save_audio: bool,
    /// 输出目录
    pub output_dir: String,
    /// 音量
    pub volume: f32,
    /// 播放器
    player: TTSPlayer,
}

impl TTSManager {
    /// 创建新的 TTS 管理器
    pub fn new() -> Self {
        Self {
            enabled: true,
            auto_play: true,
            save_audio: true,
            output_dir: "audio_output".to_string(),
            volume: 0.8,
            player: TTSPlayer::new(),
        }
    }

    /// 从配置初始化
    pub fn configure(&mut self, config: &crate::models::config::Config) {
        self.enabled = config.tts_enabled;
        self.auto_play = config.tts_auto_play;
        self.save_audio = config.tts_save_audio;
        self.output_dir = config.tts_output_dir.clone();
        self.volume = config.tts_volume;
        
        self.player.set_volume(self.volume);
        
        info!("TTS 管理器配置: 启用={}, 自动播放={}, 音量={:.0}%", 
              self.enabled, self.auto_play, self.volume * 100.0);
    }

    /// 播放音频数据
    pub fn play_audio(&mut self, audio_data: Vec<u8>) {
        if !self.enabled {
            info!("TTS 管理器未启用，跳过播放");
            return;
        }
        
        if self.auto_play {
            info!("TTS 管理器: 加入播放队列, 数据大小={} 字节", audio_data.len());
            self.player.play(audio_data);
        } else {
            info!("TTS 管理器: 自动播放已禁用，跳过播放");
        }
    }

    /// 将音频加入队列
    pub fn enqueue_audio(&mut self, audio_data: Vec<u8>) {
        if !self.enabled {
            return;
        }
        
        self.player.enqueue(audio_data);
    }

    /// 将音频加入缓冲播放队列（边合成边播放模式）
    pub fn enqueue_audio_buffered(&mut self, seq: usize, total: usize, audio_data: Vec<u8>) {
        if !self.enabled {
            return;
        }
        
        if self.auto_play {
            self.player.enqueue_buffered(seq, total, audio_data);
        }
    }

    /// 停止播放
    pub fn stop(&self) {
        self.player.stop();
    }

    /// 停止所有播放
    pub fn stop_all(&self) {
        self.player.stop_all();
    }

    /// 设置音量
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.player.set_volume(self.volume);
    }

    /// 获取播放状态
    pub fn get_status(&self) -> PlayerStatus {
        self.player.get_status()
    }

    /// 是否正在播放
    pub fn is_playing(&self) -> bool {
        self.player.is_playing()
    }

    /// 保存音频到文件
    pub fn save_audio_to_file(&self, audio_data: &[u8], filename: Option<&str>) -> Option<String> {
        if !self.save_audio {
            return None;
        }

        let output_dir = std::path::Path::new(&self.output_dir);
        if !output_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(output_dir) {
                error!("创建输出目录失败: {}", e);
                return None;
            }
        }

        let filename = filename.map(|s| s.to_string()).unwrap_or_else(|| {
            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S_%3f");
            format!("tts_{}.wav", timestamp)
        });

        let filepath = output_dir.join(&filename);
        
        match std::fs::write(&filepath, audio_data) {
            Ok(_) => {
                let path_str = filepath.to_string_lossy().to_string();
                info!("音频已保存: {}", path_str);
                Some(path_str)
            }
            Err(e) => {
                error!("保存音频失败: {}", e);
                None
            }
        }
    }

    /// 清理资源
    pub fn cleanup(&mut self) {
        self.player.cleanup();
    }
}

impl Default for TTSManager {
    fn default() -> Self {
        Self::new()
    }
}
