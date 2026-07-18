//! 服务模块
//!
//! 提供业务逻辑服务。

pub mod atomic_file;
pub mod audio_player;
pub mod character_manager;
pub mod config_store;
pub mod history_compressor;
pub mod http_client;
pub mod llm_client;
pub mod log_layer;
pub mod proxy_manager;
pub mod sse_broadcaster;
pub mod stream_parser;
pub mod tts_gsv;
pub mod tts_parser;
pub mod wire_stream;

pub use audio_player::{PlayerStatus, TTSManager, TTSPlayer};
pub use character_manager::CharacterManager;
pub use config_store::ConfigStore;
pub use history_compressor::HistoryCompressor;
pub use llm_client::LlmClient;
pub use log_layer::BroadcasterLayer;
pub use proxy_manager::ProxyServerManager;
pub use sse_broadcaster::SseBroadcaster;
pub use stream_parser::StreamingResponseParser;
pub use tts_gsv::GPTSoVITSEngine;
pub use tts_parser::StreamingTTSParser;
