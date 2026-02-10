//! 服务模块
//! 
//! 提供业务逻辑服务。

pub mod config_store;
pub mod llm_client;
pub mod stream_parser;
pub mod tts_parser;
pub mod sse_broadcaster;
pub mod proxy_manager;
pub mod tts_gsv;
pub mod audio_player;
pub mod character_manager;
pub mod history_compressor;
pub mod log_layer;

pub use config_store::ConfigStore;
pub use llm_client::LlmClient;
pub use stream_parser::StreamingResponseParser;
pub use tts_parser::StreamingTTSParser;
pub use sse_broadcaster::SseBroadcaster;
pub use proxy_manager::ProxyServerManager;
pub use tts_gsv::GPTSoVITSEngine;
pub use audio_player::{TTSPlayer, TTSManager, PlayerStatus};
pub use character_manager::CharacterManager;
pub use history_compressor::HistoryCompressor;
pub use log_layer::BroadcasterLayer;
