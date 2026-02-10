# Rust 重写 Python 后端 - 完整计划书

## 目录

1. [项目概述](#1-项目概述)
2. [架构设计](#2-架构设计)
3. [模块划分](#3-模块划分)
4. [API 清单](#4-api-清单)
5. [核心解析器逻辑](#5-核心解析器逻辑)
6. [TTS 集成方案](#6-tts-集成方案)
7. [分阶段实施指南](#7-分阶段实施指南)
8. [数据结构定义](#8-数据结构定义)
9. [依赖配置](#9-依赖配置)

---

## 1. 项目概述

### 1.1 当前架构问题

当前项目使用 **Tauri + Python Sidecar** 架构：
- Tauri 主进程 (`ollama-tts-adapter.exe`)
- Python 后端 Sidecar (`python-backend.exe`)

**存在的问题**：
1. 启动两个 exe，用户体验差
2. Python 冷启动慢（1-3秒）
3. 配置同步复杂（前端 localStorage + 后端 config.json）
4. 进程管理问题（前端关闭后后端可能残留）

### 1.2 目标架构

用 **Rust** 重写 Python 后端，集成到 Tauri 中：
- 单个 exe 输出
- 启动更快（无 Python 冷启动）
- 配置即时生效
- 进程管理简化

### 1.3 功能保留清单

必须保留的功能：
- ✅ Ollama 兼容代理服务（localhost:11434）
- ✅ 流式响应处理
- ✅ 流式字幕显示（response 字段识别）
- ✅ TTS 对话识别和筛选逻辑
- ✅ GPT-SoVITS TTS 引擎
- ✅ 情感向量匹配
- ✅ 角色管理
- ✅ 记忆压缩
- ✅ 配置管理

---

## 2. 架构设计

### 2.1 最终架构图

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        单个 exe (Tauri + Rust)                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────────────┐                                                    │
│  │    前端 (React)   │                                                    │
│  └────────┬─────────┘                                                    │
│           │                                                              │
│           ▼                                                              │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                    Rust 后端 (Axum HTTP 服务器)                    │   │
│  │                                                                   │   │
│  │  localhost:8765 (主 API)          localhost:11434 (Ollama 代理)   │   │
│  │  • 配置 API                        • /api/chat                    │   │
│  │  • 流式/SSE API                    • /api/generate               │   │
│  │  • TTS API                        • /api/tags                    │   │
│  │  • 角色管理 API                                                    │   │
│  │  • 记忆压缩 API                                                    │   │
│  │  • 日志 API                                                        │   │
│  │                                                                   │   │
│  │  内置服务：                                                        │   │
│  │  • 配置管理 (config.json)                                          │   │
│  │  • GPT-SoVITS 客户端                                               │   │
│  │  • 音频播放 (rodio)                                                │   │
│  │  • SSE 推送                                                        │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 数据流

```
外部客户端 (SillyTavern)
        │
        ▼
┌─────────────────────────────────┐
│  Ollama 代理 (localhost:11434)  │
│  • 接收 /api/chat 请求           │
│  • 注入系统提示词                │
│  • 转发到远程 AI API             │
└───────────────┬─────────────────┘
                │
                ▼
┌─────────────────────────────────┐
│       远程 AI API               │
│  (api_url 配置的地址)            │
└───────────────┬─────────────────┘
                │
                ▼
┌─────────────────────────────────┐
│     流式响应处理                 │
│  1. StreamingResponseParser     │
│     - 提取 response 字段内容     │
│     - 过滤技术字段               │
│                                 │
│  2. StreamingTTSParser          │
│     - 识别 tts_config           │
│     - 检测对话边界 (**角色**："")│
│     - 匹配情感向量               │
│     - 触发 TTS 合成              │
└───────────────┬─────────────────┘
                │
        ┌───────┴───────┐
        ▼               ▼
┌─────────────┐  ┌─────────────┐
│  字幕显示    │  │  TTS 合成   │
│  (SSE 推送)  │  │  (GPT-SoVITS)│
└─────────────┘  └─────────────┘
```

---

## 3. 模块划分

### 3.1 目录结构

```
src-tauri/
├── src/
│   ├── main.rs                 # Tauri 入口（保持不变）
│   ├── lib.rs                  # Tauri 命令 + 启动 HTTP 服务器
│   │
│   ├── server/                 # HTTP 服务器模块
│   │   ├── mod.rs              # 服务器启动逻辑
│   │   ├── main_api.rs         # 主 API 路由 (8765)
│   │   └── proxy_api.rs        # 代理 API 路由 (11434)
│   │
│   ├── api/                    # API 处理器
│   │   ├── mod.rs
│   │   ├── config.rs           # GET/PUT /api/config
│   │   ├── proxy.rs            # 代理控制 API
│   │   ├── stream.rs           # SSE 事件流
│   │   ├── characters.rs       # 角色管理
│   │   ├── memory.rs           # 记忆压缩
│   │   ├── tts.rs              # TTS API
│   │   └── logs.rs             # 日志 SSE
│   │
│   ├── services/               # 业务逻辑
│   │   ├── mod.rs
│   │   ├── config_store.rs     # 配置读写
│   │   ├── llm_client.rs       # LLM API 客户端
│   │   ├── stream_parser.rs    # 流式响应解析（核心）
│   │   ├── tts_parser.rs       # TTS 对话解析（核心）
│   │   ├── tts_gsv.rs          # GPT-SoVITS 客户端
│   │   ├── audio_player.rs     # 音频播放器
│   │   ├── history_compressor.rs # 历史压缩
│   │   └── sse_broadcaster.rs  # SSE 广播
│   │
│   └── models/                 # 数据模型
│       ├── mod.rs
│       ├── config.rs           # Config 结构
│       ├── character.rs        # 角色配置
│       ├── tts_types.rs        # TTS 相关类型
│       └── api_types.rs        # API 请求/响应类型
│
├── Cargo.toml                  # 依赖配置
└── ...
```

### 3.2 模块职责

| 模块 | 职责 | 对应 Python 文件 |
|------|------|-----------------|
| `server/mod.rs` | 启动 Axum HTTP 服务器 | `api_server.py` |
| `server/main_api.rs` | 主 API 路由定义 | `api.py` 的路由部分 |
| `server/proxy_api.rs` | Ollama 代理路由 | `api.py` 的代理部分 |
| `services/stream_parser.rs` | 流式响应解析 | `core/parsers.py` 的 `StreamingResponseParser` |
| `services/tts_parser.rs` | TTS 对话解析 | `core/parsers.py` 的 `StreamingTTSParser` |
| `services/tts_gsv.rs` | GPT-SoVITS 客户端 | `tts/gsv.py` |
| `services/audio_player.rs` | 音频播放 | `tts/manager.py` 的 `TTSPlayer` |
| `services/config_store.rs` | 配置管理 | `config.py` |
| `services/history_compressor.rs` | 历史压缩 | `core/history_compressor.py` |

---

## 4. API 清单

### 4.1 主 API (localhost:8765)

| 方法 | 路径 | 功能 | 优先级 |
|------|------|------|--------|
| GET | `/health` | 健康检查 | P0 |
| GET | `/api/config` | 获取配置 | P0 |
| PUT | `/api/config` | 更新配置 | P0 |
| GET | `/api/stream/events` | SSE 字幕事件流 | P1 |
| GET | `/api/stream/state` | 获取流状态 | P1 |
| POST | `/api/stream/clear` | 清除内容 | P1 |
| GET | `/api/stream/history` | 获取历史 | P2 |
| POST | `/api/tts/synthesize` | 合成语音 | P1 |
| POST | `/api/tts/stop` | 停止播放 | P1 |
| POST | `/api/tts/stop-all` | 停止全部 | P1 |
| POST | `/api/tts/volume` | 设置音量 | P2 |
| GET | `/api/tts/status` | 获取状态 | P2 |
| POST | `/api/tts/test` | 测试引擎 | P2 |
| GET | `/api/characters` | 获取角色列表 | P2 |
| POST | `/api/characters` | 添加角色 | P2 |
| PUT | `/api/characters/{id}` | 更新角色 | P2 |
| DELETE | `/api/characters/{id}` | 删除角色 | P2 |
| POST | `/api/characters/switch` | 切换角色 | P2 |
| GET | `/api/memory/status` | 获取压缩状态 | P2 |
| POST | `/api/memory/compress` | 压缩历史 | P2 |
| POST | `/api/memory/compress-all` | 压缩全部 | P2 |
| GET | `/api/memory/config` | 获取压缩配置 | P2 |
| PUT | `/api/memory/config` | 更新压缩配置 | P2 |
| GET | `/api/logs/stream` | 日志 SSE | P2 |
| GET | `/api/proxy/status` | 代理状态 | P0 |
| POST | `/api/proxy/start` | 启动代理 | P0 |
| POST | `/api/proxy/stop` | 停止代理 | P0 |
| POST | `/api/proxy/test` | 测试代理 | P1 |
| POST | `/api/proxy/test-stream` | 流式测试 | P1 |

### 4.2 代理 API (localhost:11434)

| 方法 | 路径 | 功能 | 优先级 |
|------|------|------|--------|
| GET | `/api/tags` | 返回模型列表 | P0 |
| POST | `/api/chat` | 聊天请求 | P0 |
| POST | `/api/generate` | 生成请求 | P0 |

---

## 5. 核心解析器逻辑

### 5.1 StreamingResponseParser（流式响应解析器）

**Python 源码位置**: `core/parsers.py` 第 367-454 行

**功能**: 实时解析流式 JSON，只提取 `response` 字段内容，过滤技术字段

**核心逻辑**:

```rust
/// 流式响应解析器
/// 边接收数据边提取 response 字段内容
/// 只显示 response 字段内容，过滤掉 romance_intent、decision 等技术字段
pub struct StreamingResponseParser {
    buffer: String,
    in_response_field: bool,
    response_content: String,
    brace_count: i32,
    in_string: bool,
    escape_next: bool,
    field_name_buffer: String,
    looking_for_field: bool,
    after_colon: bool,
    displayed_length: usize,
}

impl StreamingResponseParser {
    pub fn new() -> Self { /* 初始化 */ }
    
    pub fn reset(&mut self) { /* 重置状态 */ }
    
    /// 喂入新的数据块，返回新增的 response 内容（用于实时显示）
    /// 只返回 response 字段的内容，忽略其他字段
    pub fn feed(&mut self, chunk: &str) -> String {
        let mut new_content = String::new();
        
        for char in chunk.chars() {
            self.buffer.push(char);
            
            // 1. 处理转义字符
            if self.escape_next {
                self.escape_next = false;
                if self.in_response_field && self.in_string {
                    match char {
                        'n' => new_content.push('\n'),
                        '"' => new_content.push(' '),  // 转义引号替换为空格
                        '\\' => {},  // 反斜杠替换为空
                        't' => new_content.push('\t'),
                        'r' => new_content.push('\r'),
                        _ => new_content.push(char),
                    }
                }
                continue;
            }
            
            if char == '\\' {
                self.escape_next = true;
                continue;
            }
            
            // 2. 检测字符串边界
            if char == '"' {
                if !self.in_string {
                    self.in_string = true;
                    if self.after_colon && self.looking_for_field {
                        if self.field_name_buffer.trim() == "response" {
                            self.in_response_field = true;
                            self.looking_for_field = false;
                        }
                        self.after_colon = false;
                        self.field_name_buffer.clear();
                    }
                } else {
                    self.in_string = false;
                    if self.in_response_field {
                        self.in_response_field = false;
                        self.looking_for_field = true;
                    }
                }
                continue;
            }
            
            // 3. 在字符串内部
            if self.in_string {
                if self.in_response_field {
                    new_content.push(char);
                } else if self.looking_for_field && !self.after_colon {
                    self.field_name_buffer.push(char);
                }
                continue;
            }
            
            // 4. 在字符串外部
            match char {
                ':' => self.after_colon = true,
                ',' | '}' => {
                    self.field_name_buffer.clear();
                    self.looking_for_field = true;
                    self.after_colon = false;
                }
                _ => {}
            }
        }
        
        new_content
    }
}
```

### 5.2 StreamingTTSParser（流式 TTS 解析器）

**Python 源码位置**: `core/parsers.py` 第 29-365 行

**功能**: 边接收数据边检测对话并触发 TTS

**工作流程**:
1. 先接收 `tts_config`（等到 `"response":` 出现）
2. 建立 `text_marker` → 情感的映射表
3. 流式接收 response，检测对话边界，立即触发回调

**核心数据结构**:

```rust
/// 检测到的对话段落
#[derive(Debug, Clone)]
pub struct DialogueSegment {
    pub text: String,           // 对话文本（引号内内容）
    pub speaker: String,        // 说话者
    pub gender: String,         // 性别标识 (male/female)
    pub emotion: String,        // 情感 (neutral, happy, angry, etc.)
    pub emotion_description: String,
    pub emotion_vector: Option<HashMap<String, f32>>,
    pub speed: f32,
    pub is_nsfw: bool,
    pub skip_tts: bool,
}

/// 流式 TTS 解析器
pub struct StreamingTTSParser {
    // 回调
    on_dialogue_detected: Option<Box<dyn Fn(DialogueSegment) + Send>>,
    
    // 阶段1: 收集 tts_config
    buffer: String,
    tts_config_complete: bool,
    tts_config: Option<TtsConfig>,
    segments_map: HashMap<String, TtsSegment>,  // text_marker -> segment info
    is_nsfw: bool,
    
    // 阶段2: 流式解析 response
    in_response: bool,
    response_buffer: String,
    dialogue_buffer: String,    // 当前正在收集的对话
    in_dialogue: bool,          // 是否在引号内
    current_speaker: String,
    detected_dialogues: Vec<DialogueSegment>,
    
    // 记录每个角色最近使用的 segment（用于回退匹配）
    last_used_segment_by_speaker: HashMap<String, TtsSegment>,
    
    // 引号匹配状态
    quote_char: Option<char>,
    
    // 等待 response 开始
    waiting_for_response_start: bool,
}
```

**核心解析逻辑**:

```rust
impl StreamingTTSParser {
    /// 喂入新的数据块
    pub fn feed(&mut self, chunk: &str) -> String {
        let mut display_content = String::new();
        
        for char in chunk.chars() {
            self.buffer.push(char);
            
            // 阶段1: 等待 tts_config 完成（检测 "response": 的出现）
            if !self.tts_config_complete {
                if self.buffer.ends_with("\"response\":") {
                    self.parse_tts_config();
                    self.tts_config_complete = true;
                    self.waiting_for_response_start = true;
                }
                continue;
            }
            
            // 等待 response 字符串的开始引号
            if self.waiting_for_response_start {
                if char == '"' {
                    self.in_response = true;
                    self.waiting_for_response_start = false;
                }
                continue;
            }
            
            // 阶段2: 流式解析 response 内容
            if self.in_response {
                // 处理转义字符...
                // 检测 response 字符串结束...
                
                self.response_buffer.push(char);
                display_content.push(char);
                self.process_char(char);
            }
        }
        
        display_content
    }
    
    /// 处理单个字符，检测对话边界
    fn process_char(&mut self, char: char) {
        // 引号对定义
        let quote_pairs: HashMap<char, char> = [
            ('"', '"'),
            ('「', '」'),
            ('『', '』'),
            ('"', '"'),
        ].iter().cloned().collect();
        
        let left_quotes: HashSet<char> = quote_pairs.keys().cloned().collect();
        
        // 检测引号开始 - 只有紧跟在 **角色**： 之后才识别为对话
        if left_quotes.contains(&char) && !self.in_dialogue {
            // 使用正则检查 response_buffer 末尾是否是 **角色名**：
            // 匹配模式：**角色名**：后面可能有空格、换行等，然后是当前引号
            let pattern = Regex::new(r"\*\*([^*]+)\*\*[：:]\s*$").unwrap();
            if let Some(captures) = pattern.captures(&self.response_buffer) {
                self.current_speaker = captures.get(1).unwrap().as_str().to_string();
                self.in_dialogue = true;
                self.quote_char = Some(char);
                self.dialogue_buffer.clear();
                return;
            }
            // 否则跳过这个引号（非角色对话）
            return;
        }
        
        // 检测引号结束
        if self.in_dialogue {
            if let Some(quote) = self.quote_char {
                let expected_right = quote_pairs.get(&quote).unwrap_or(&'"');
                if char == *expected_right || (char == '"' && quote == '"') {
                    // 对话结束，触发回调
                    self.emit_dialogue();
                    self.in_dialogue = false;
                    self.quote_char = None;
                    self.dialogue_buffer.clear();
                    return;
                }
            }
            
            // 累积对话内容
            self.dialogue_buffer.push(char);
        }
    }
    
    /// 发送检测到的对话
    fn emit_dialogue(&mut self) {
        if self.dialogue_buffer.trim().is_empty() {
            return;
        }
        
        let dialogue_text = self.dialogue_buffer.trim().to_string();
        
        // 1. 查找匹配的 segment（通过 text_marker）
        let mut matched_segment: Option<TtsSegment> = None;
        let mut matched_marker: Option<String> = None;
        
        for (marker, seg) in &self.segments_map {
            // text_marker 是对话的开头部分
            if dialogue_text.starts_with(marker) || dialogue_text.contains(marker) {
                matched_segment = Some(seg.clone());
                matched_marker = Some(marker.clone());
                break;
            }
        }
        
        // 2. 更宽松的匹配（前几个字）
        if matched_segment.is_none() {
            for (marker, seg) in &self.segments_map {
                let min_len = marker.len().min(dialogue_text.len()).min(5);
                if min_len >= 3 && marker[..min_len] == dialogue_text[..min_len] {
                    matched_segment = Some(seg.clone());
                    matched_marker = Some(marker.clone());
                    break;
                }
            }
        }
        
        // 3. 移除已使用的 segment
        if let Some(marker) = &matched_marker {
            self.segments_map.remove(marker);
        }
        
        // 4. 使用默认配置（单一 speaker 模式）
        // 5. 回退到同角色的下一个 segment
        // 6. 回退到同角色上一个使用过的 segment
        
        // 构建对话段落
        let segment = DialogueSegment {
            text: dialogue_text,
            speaker: self.current_speaker.clone(),
            gender: matched_segment.as_ref().map(|s| s.gender.clone()).unwrap_or_default(),
            emotion: matched_segment.as_ref().map(|s| s.emotion.clone()).unwrap_or("neutral".to_string()),
            emotion_description: matched_segment.as_ref().map(|s| s.emotion_description.clone()).unwrap_or_default(),
            emotion_vector: matched_segment.as_ref().and_then(|s| s.emotion_vector.clone()),
            speed: matched_segment.as_ref().map(|s| s.speed).unwrap_or(1.0),
            is_nsfw: self.is_nsfw,
            skip_tts: matched_segment.as_ref().map(|s| s.skip_tts).unwrap_or(false),
        };
        
        self.detected_dialogues.push(segment.clone());
        
        // 跳过旁白
        if segment.skip_tts {
            return;
        }
        
        // 触发回调
        if let Some(callback) = &self.on_dialogue_detected {
            callback(segment);
        }
    }
}
```

### 5.3 引号对定义

```rust
/// 引号配对常量
pub const QUOTE_PAIRS: &[(char, char)] = &[
    ('"', '"'),       // 英文双引号
    ('"', '"'),       // 中文左双引号 -> 中文右双引号
    ('„', '"'),       // 中文低双引号 -> 中文右双引号
    ('「', '」'),     // 日式引号
    ('『', '』'),     // 日式双引号
];

/// 所有开始引号
pub fn left_quotes() -> HashSet<char> {
    QUOTE_PAIRS.iter().map(|(l, _)| *l).collect()
}

/// 所有结束引号
pub fn right_quotes() -> HashSet<char> {
    QUOTE_PAIRS.iter().map(|(_, r)| *r).collect()
}
```

---

## 6. TTS 集成方案

### 6.1 GPT-SoVITS 客户端

**Python 源码位置**: `tts/gsv.py`

**功能**:
- 调用本地 GPT-SoVITS API (localhost:9880)
- 使用参考音频克隆语音和情感
- 支持多角色动态切换
- 智能情感匹配（情感向量 → 参考音频）
- 长文本切分

**核心结构**:

```rust
/// GPT-SoVITS TTS 引擎
pub struct GPTSoVITSEngine {
    // API 配置
    api_url: String,              // 默认: "http://localhost:9880/tts"
    timeout: Duration,            // 默认: 120秒
    
    // 默认角色
    default_character: String,
    default_male_character: String,
    default_female_character: String,
    
    // GSV 角色配置 {角色ID: 配置}
    gsv_characters: HashMap<String, CharacterConfig>,
    current_gsv_character: String,
    
    // 情感音频配置缓存 {角色ID: JSON数据}
    emotion_audio_cache: HashMap<String, EmotionAudioConfig>,
    
    // 是否使用服务器路径
    use_server_path: bool,
    server_path_prefix: String,
    
    // 推理参数
    top_k: i32,       // 默认: 40
    top_p: f32,       // 默认: 0.9
    temperature: f32, // 默认: 1.3
    
    // HTTP 客户端
    client: reqwest::Client,
}

/// 情感向量到 JSON 情感 ID 的映射
const EMOTION_VECTOR_TO_JSON: &[(&str, &str)] = &[
    ("joy", "happy"),
    ("anger", "angry"),
    ("sorrow", "sad"),
    ("fear", "fear"),
    ("disgust", "angry"),    // 厌恶合并到愤怒
    ("depression", "depressed"),
    ("surprise", "surprised"),
    ("calm", "calm"),
];
```

**核心方法**:

```rust
impl GPTSoVITSEngine {
    /// 合成语音
    pub async fn synthesize(&self, request: TTSRequest) -> TTSResult {
        // 1. 确定使用的角色
        let target_character = self.resolve_target_character(&request.speaker, &request.gender);
        
        // 2. 根据情感向量选择参考音频
        let ref_audios = if let Some(emotion_vector) = &request.emotion_vector {
            self.select_ref_audios_by_emotion_vector(&target_character, emotion_vector)
        } else {
            self.select_audio_by_emotion(&target_character, &request.emotion)
        };
        
        // 3. 长文本切分（超过40字符时切分）
        let text_segments = self.split_text(&request.text, 40);
        
        // 4. 分段合成
        let mut audio_segments = Vec::new();
        for segment_text in text_segments {
            let payload = TtsApiRequest {
                text: segment_text,
                text_lang: "zh".to_string(),
                ref_audio_path: ref_audios[0].audio_file.clone(),
                prompt_text: ref_audios[0].text.clone(),
                prompt_lang: "zh".to_string(),
                top_k: self.top_k,
                top_p: self.top_p,
                temperature: self.temperature,
                speed: request.speed,
            };
            
            let response = self.client.post(&self.api_url)
                .json(&payload)
                .timeout(self.timeout)
                .send()
                .await?;
            
            if response.status().is_success() {
                audio_segments.push(response.bytes().await?);
            }
        }
        
        // 5. 返回结果
        TTSResult {
            success: !audio_segments.is_empty(),
            audio_data: if audio_segments.len() == 1 { 
                Some(audio_segments[0].clone()) 
            } else { 
                None 
            },
            audio_segments: if audio_segments.len() > 1 { 
                Some(audio_segments) 
            } else { 
                None 
            },
            speaker_used: target_character,
            emotion_used: request.emotion,
            ..Default::default()
        }
    }
    
    /// 根据情感向量选择多个参考音频
    fn select_ref_audios_by_emotion_vector(
        &self,
        char_id: &str,
        emotion_vector: &HashMap<String, f32>
    ) -> Vec<RefAudioInfo> {
        // 1. 将向量字段映射到 JSON 情感 ID
        let mut emotion_weights: HashMap<String, f32> = HashMap::new();
        for (vec_key, json_emotion) in EMOTION_VECTOR_TO_JSON {
            let weight = emotion_vector.get(*vec_key).copied().unwrap_or(0.0);
            emotion_weights
                .entry(json_emotion.to_string())
                .and_modify(|w| *w = w.max(weight))
                .or_insert(weight);
        }
        
        // 2. 排序（权重从高到低）
        let mut sorted: Vec<_> = emotion_weights.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        let total_weight: f32 = sorted.iter().map(|(_, w)| *w).filter(|w| *w > 0.0).sum();
        let total_weight = if total_weight == 0.0 { 1.0 } else { total_weight };
        
        // 3. 选择音频
        let mut selected = Vec::new();
        for (emotion_id, weight) in sorted {
            if weight <= 0.0 { continue; }
            let ratio = weight / total_weight;
            
            if selected.is_empty() {
                // 主情感必选
                selected.push((emotion_id, ratio));
            } else if ratio >= 0.30 && selected.len() < 3 {
                // 次要情感，≥30% 且最多3个
                selected.push((emotion_id, ratio));
            } else {
                break;
            }
        }
        
        // 4. 转换为音频信息
        // ...
    }
    
    /// 智能切分长文本
    fn split_text(&self, text: &str, max_length: usize) -> Vec<String> {
        if text.len() <= max_length {
            return vec![text.to_string()];
        }
        
        let mut segments = Vec::new();
        let mut remaining = text.trim();
        
        // 主要分隔符（句子结束）
        let primary_regex = Regex::new(r"[。！？!?]").unwrap();
        // 次要分隔符（句内停顿）
        let secondary_regex = Regex::new(r"[，,；;：:]").unwrap();
        
        while !remaining.is_empty() {
            if remaining.len() <= max_length {
                segments.push(remaining.to_string());
                break;
            }
            
            let search_text = &remaining[..max_length.min(remaining.len())];
            let mut split_pos = None;
            
            // 1. 优先在主要分隔符处切分
            if let Some(m) = primary_regex.find_iter(search_text).last() {
                split_pos = Some(m.end());
            }
            
            // 2. 次要分隔符
            if split_pos.is_none() {
                if let Some(m) = secondary_regex.find_iter(search_text).last() {
                    split_pos = Some(m.end());
                }
            }
            
            // 3. 强制切分
            let pos = split_pos.unwrap_or(max_length);
            segments.push(remaining[..pos].trim().to_string());
            remaining = remaining[pos..].trim();
        }
        
        segments
    }
}
```

### 6.2 音频播放器

**Python 源码位置**: `tts/manager.py` 第 80-291 行

```rust
use rodio::{Decoder, OutputStream, Sink};
use std::sync::mpsc;

/// TTS 音频播放器 - 支持队列顺序播放
pub struct TTSPlayer {
    // rodio 相关
    stream_handle: Option<rodio::OutputStreamHandle>,
    sink: Option<rodio::Sink>,
    
    // 队列
    audio_queue: mpsc::Sender<Vec<u8>>,
    queue_receiver: mpsc::Receiver<Vec<u8>>,
    
    // 状态
    playing: bool,
    volume: f32,
    
    // 回调
    on_play_start: Option<Box<dyn Fn() + Send>>,
    on_play_end: Option<Box<dyn Fn() + Send>>,
}

impl TTSPlayer {
    pub fn new() -> Self { /* 初始化 */ }
    
    /// 将音频加入播放队列
    pub fn enqueue(&self, audio_data: Vec<u8>) {
        let _ = self.audio_queue.send(audio_data);
    }
    
    /// 播放音频数据
    pub fn play(&mut self, audio_data: Vec<u8>) {
        // 使用 rodio 播放
        if let (Some(handle), Some(sink)) = (&self.stream_handle, &self.sink) {
            let cursor = std::io::Cursor::new(audio_data);
            if let Ok(source) = Decoder::new(cursor) {
                sink.append(source);
            }
        }
    }
    
    /// 停止当前播放
    pub fn stop(&mut self) {
        if let Some(sink) = &self.sink {
            sink.stop();
        }
    }
    
    /// 设置音量 (0.0 - 1.0)
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
    }
}
```

---

## 7. 分阶段实施指南

### 阶段 1：基础框架（预计 2-3 小时）

#### 目标
搭建 Axum HTTP 服务器框架，实现配置 API

#### 提示词

```
请帮我实现 Rust 后端的基础框架：

1. 修改 src-tauri/Cargo.toml，添加以下依赖：
   - axum 0.7（HTTP 服务器）
   - tower-http 0.5（CORS 中间件）
   - tokio（异步运行时，Tauri 已有）
   - reqwest 0.12（HTTP 客户端）
   - serde、serde_json（JSON 序列化）
   - parking_lot（高性能锁）
   - once_cell（全局状态）

2. 创建 src-tauri/src/server/mod.rs：
   - 定义 start_server() 函数，启动 Axum HTTP 服务器
   - 监听 localhost:8765
   - 添加 CORS 中间件

3. 创建 src-tauri/src/models/config.rs：
   - 定义 Config 结构体，包含所有配置字段
   - 实现 JSON 序列化/反序列化
   - 参考 python-backend/config.py 的配置结构

4. 创建 src-tauri/src/services/config_store.rs：
   - 实现 load_config() 从 config.json 加载配置
   - 实现 save_config() 保存配置
   - 使用 parking_lot::RwLock 保护配置状态

5. 创建 src-tauri/src/api/config.rs：
   - GET /api/config - 返回当前配置
   - PUT /api/config - 更新配置

6. 修改 src-tauri/src/lib.rs：
   - 在 Tauri setup 阶段启动 HTTP 服务器
   - 移除 start_backend 命令（不再需要 Python sidecar）

完成后可用 API：
- GET /api/config
- PUT /api/config
- GET /health
```

#### 验证
```bash
# 启动应用后测试
curl http://localhost:8765/health
curl http://localhost:8765/api/config
```

---

### 阶段 2：代理服务（预计 3-4 小时）

#### 目标
实现 Ollama 兼容代理，支持流式响应

#### 提示词

```
请帮我实现 Ollama 代理服务：

1. 创建 src-tauri/src/server/proxy_api.rs：
   - 监听 localhost:11434
   - 实现 GET /api/tags - 返回模型列表
   - 实现 POST /api/chat - 转发聊天请求
   - 实现 POST /api/generate - 转发生成请求

2. 创建 src-tauri/src/services/llm_client.rs：
   - 使用 reqwest 调用远程 AI API
   - 支持流式响应 (stream: true)
   - 支持非流式响应 (stream: false)
   - 处理超时和错误

3. 实现系统提示词注入：
   - 从配置读取 system_prompt_injection
   - 如果 messages 中有 system 角色，追加内容
   - 如果没有，插入新的 system 消息

4. 实现 detect_fake_stream：
   - 检测模型名是否以 -preview、-stream、-fast 结尾
   - 如果是，移除后缀并强制使用流式

5. 创建 src-tauri/src/api/proxy.rs：
   - GET /api/proxy/status - 返回代理状态
   - POST /api/proxy/start - 启动代理
   - POST /api/proxy/stop - 停止代理

参考 Python 代码：
- python-backend/api.py 的 forward_request 函数
- python-backend/api.py 的 inject_prompt 函数
- python-backend/utils.py 的 detect_fake_stream 函数
```

---

### 阶段 3：流式响应解析（预计 3-4 小时）

#### 目标
实现 StreamingResponseParser 和 StreamingTTSParser

#### 提示词

```
请帮我实现流式响应解析器：

1. 创建 src-tauri/src/services/stream_parser.rs：
   - 实现 StreamingResponseParser 结构体
   - 核心方法 feed(chunk: &str) -> String
   - 只提取 response 字段内容
   - 过滤 romance_intent、decision 等技术字段
   - 处理转义字符 (\n, \", \\, \t)

2. 创建 src-tauri/src/services/tts_parser.rs：
   - 实现 StreamingTTSParser 结构体
   - 实现 DialogueSegment 结构体
   - 核心方法 feed(chunk: &str) -> String
   
   解析流程：
   a. 阶段1：收集 tts_config
      - 等待 "response": 出现
      - 解析 tts_config JSON
      - 建立 text_marker -> segment 映射
   
   b. 阶段2：流式解析 response
      - 检测对话边界：**角色**："对话内容"
      - 使用 text_marker 匹配情感配置
      - 触发 on_dialogue_detected 回调

   引号匹配规则：
   - 支持: "", "", 「」, 『』
   - 只有紧跟在 **角色名**： 之后的引号才识别为对话
   - 其他引号（如强调用法）跳过

3. 创建 src-tauri/src/models/tts_types.rs：
   - TtsConfig 结构体
   - TtsSegment 结构体
   - DialogueSegment 结构体
   - 情感向量类型定义

参考 Python 代码：
- core/parsers.py 的 StreamingResponseParser (第 367-454 行)
- core/parsers.py 的 StreamingTTSParser (第 29-365 行)
- constants.py 的 QUOTE_PAIRS
```

---

### 阶段 4：SSE 流式推送（预计 2 小时）

#### 目标
实现字幕和日志的 SSE 推送

#### 提示词

```
请帮我实现 SSE 流式推送：

1. 创建 src-tauri/src/services/sse_broadcaster.rs：
   - 使用 tokio::sync::broadcast 实现广播
   - 定义事件类型：
     - StreamStart - 流开始
     - StreamChunk { content: String } - 新内容
     - StreamEnd - 流结束
     - Clear - 清除内容
   - 支持多个客户端订阅

2. 创建 src-tauri/src/api/stream.rs：
   - GET /api/stream/events - SSE 事件流
     返回格式：
     ```
     event: chunk
     data: {"content": "新内容"}
     
     event: end
     data: {}
     ```
   - GET /api/stream/state - 获取当前状态
   - POST /api/stream/clear - 清除内容

3. 创建 src-tauri/src/api/logs.rs：
   - GET /api/logs/stream - 日志 SSE 流
   - 使用 tracing 收集日志并推送

4. 在流式响应处理中集成 SSE：
   - 收到新内容时调用 broadcaster.send()
   - 流结束时发送 StreamEnd 事件
```

---

### 阶段 5：TTS 功能（预计 4-5 小时）

#### 目标
实现 GPT-SoVITS 调用和音频播放

#### 提示词

```
请帮我实现 TTS 功能：

1. 创建 src-tauri/src/services/tts_gsv.rs：
   - 实现 GPTSoVITSEngine 结构体
   - 核心方法：
     - initialize(config) - 初始化引擎
     - synthesize(request) -> TTSResult - 合成语音
     - test_connection() -> bool - 测试连接
   
   功能实现：
   a. 角色解析 resolve_target_character：
      - 从 speaker 匹配已配置角色
      - 根据 gender 选择默认角色
      - 使用当前选中角色
      - 使用默认角色
   
   b. 情感向量匹配 select_ref_audios_by_emotion_vector：
      - 将情感向量映射到 JSON 情感 ID
      - 排序选择主/次情感
      - 返回参考音频列表
   
   c. 长文本切分 split_text：
      - 在句号、逗号等处切分
      - 每段不超过 max_length
   
   d. API 调用：
      - POST http://localhost:9880/tts
      - 参数：text, text_lang, ref_audio_path, prompt_text, prompt_lang, top_k, top_p, temperature, speed

2. 创建 src-tauri/src/services/audio_player.rs：
   - 使用 rodio 库播放音频
   - 支持播放队列
   - 支持停止、音量控制

3. 创建 src-tauri/src/api/tts.rs：
   - POST /api/tts/synthesize - 合成语音
   - POST /api/tts/stop - 停止播放
   - POST /api/tts/stop-all - 停止全部
   - POST /api/tts/volume - 设置音量
   - GET /api/tts/status - 获取状态
   - POST /api/tts/test - 测试引擎

4. 在流式响应处理中集成 TTS：
   - StreamingTTSParser 检测到对话时触发 TTS
   - 异步合成和播放，不阻塞响应

参考 Python 代码：
- tts/gsv.py 的 GPTSoVITSEngine 类
- tts/manager.py 的 TTSPlayer 类
- tts/manager.py 的 MultiEngineTTSManager 类
```

---

### 阶段 6：角色和记忆管理（预计 2-3 小时）

#### 目标
实现角色管理和历史压缩

#### 提示词

```
请帮我实现角色和记忆管理：

1. 创建 src-tauri/src/models/character.rs：
   - CharacterConfig 结构体
   - EmotionConfig 结构体
   - 从 character_configs/*.json 加载

2. 创建 src-tauri/src/api/characters.rs：
   - GET /api/characters - 获取角色列表
   - POST /api/characters - 添加角色
   - PUT /api/characters/{id} - 更新角色
   - DELETE /api/characters/{id} - 删除角色
   - POST /api/characters/switch - 切换当前角色

3. 创建 src-tauri/src/services/history_compressor.rs：
   - 实现 auto_compress_check 函数
   - 扫描 save_data_path 目录下的 JSON 文件
   - 检查 ConversationHistory 消息数
   - 超过阈值时调用 AI API 压缩

4. 创建 src-tauri/src/api/memory.rs：
   - GET /api/memory/status - 获取压缩状态
   - POST /api/memory/compress - 压缩指定文件
   - POST /api/memory/compress-all - 压缩所有
   - GET /api/memory/config - 获取压缩配置
   - PUT /api/memory/config - 更新压缩配置

参考 Python 代码：
- core/character_manager.py
- core/history_compressor.py
- config.py 的 DEFAULT_COMPRESSION_PROMPT
```

---

### 阶段 7：清理和测试（预计 2 小时）

#### 目标
清理旧代码，全面测试

#### 提示词

```
请帮我完成清理和测试：

1. 修改 src-tauri/tauri.conf.json：
   - 移除 sidecar 配置
   - 更新 bundle 配置

2. 修改 src-tauri/src/lib.rs：
   - 移除 start_backend、stop_backend 命令
   - 移除 BackendState 状态管理
   - 更新 on_window_event 处理

3. 备份并删除 Python 后端：
   - 备份 python-backend/ 目录
   - 删除 src-tauri/binaries/python-backend-*.exe

4. 更新前端（如需要）：
   - 检查 API 调用是否兼容
   - 移除后端启动相关代码

5. 全面测试：
   - 配置 API
   - 代理服务
   - 流式响应
   - TTS 功能
   - 字幕显示
```

---

## 8. 数据结构定义

### 8.1 配置结构

```rust
// src-tauri/src/models/config.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    // API 配置
    pub api_url: String,
    pub api_key: String,
    pub model_name: String,
    pub system_prompt_injection: String,
    
    // TTS 配置
    pub tts_enabled: bool,
    pub gsv_api_url: String,
    pub tts_auto_play: bool,
    pub tts_save_audio: bool,
    pub tts_output_dir: String,
    pub tts_default_character: String,
    pub tts_volume: f32,
    pub tts_play_mode: String,  // "dialogue" | "fulltext"
    
    // GSV 引擎配置
    pub gsv_top_k: i32,
    pub gsv_top_p: f32,
    pub gsv_temperature: f32,
    pub gsv_default_male_character: String,
    pub gsv_default_female_character: String,
    pub gsv_use_server_path: bool,
    pub gsv_server_path_prefix: String,
    pub gsv_current_character: String,
    pub gsv_characters: HashMap<String, CharacterConfig>,
    
    // 语音识别配置
    pub voice_engine: String,
    pub voice_enabled: bool,
    pub vosk_model_path: String,
    pub voice_language: String,
    
    // 百度语音 API
    pub baidu_app_id: String,
    pub baidu_api_key: String,
    pub baidu_secret_key: String,
    
    // 讯飞语音识别 API
    pub xunfei_asr_app_id: String,
    pub xunfei_asr_api_key: String,
    pub xunfei_asr_api_secret: String,
    
    // IndexTTS 配置
    pub indextts_api_url: String,
    pub indextts_prompt_audio: String,
    pub indextts_emo_ref_audio: String,
    pub indextts_emo_control_method: String,
    pub indextts_emo_weight: f32,
    pub indextts_max_tokens: i32,
    
    // 历史压缩配置
    pub history_compression: HistoryCompressionConfig,
    
    // 字幕配置
    pub subtitle_font_size: i32,
    pub subtitle_opacity: f32,
    pub subtitle_font_color: String,
    pub subtitle_bg_color: String,
    pub subtitle_typing_speed: i32,
    pub subtitle_auto_hide: bool,
    pub subtitle_auto_hide_delay: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterConfig {
    pub name: String,
    pub ref_audio_dir: String,
    pub emotion_audio_json: String,
    pub gpt_model_path: String,
    pub sovits_model_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryCompressionConfig {
    pub enabled: bool,
    pub save_data_path: String,
    pub max_messages: i32,
    pub compress_count: i32,
    pub backup_before_compress: bool,
    pub backup_dir: String,
    pub custom_prompt: String,
}
```

### 8.2 TTS 类型

```rust
// src-tauri/src/models/tts_types.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// TTS 配置（从 AI 响应中提取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    pub speaker: Option<String>,
    pub gender: Option<String>,
    pub emotion: Option<String>,
    pub emotion_description: Option<String>,
    pub emotion_vector: Option<HashMap<String, f32>>,
    pub speed: Option<f32>,
    pub skip_tts: Option<bool>,
    pub is_nsfw: Option<bool>,
    pub segments: Option<Vec<TtsSegment>>,
}

/// TTS 段落配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSegment {
    pub speaker: Option<String>,
    pub gender: Option<String>,
    pub emotion: Option<String>,
    pub emotion_description: Option<String>,
    pub emotion_vector: Option<HashMap<String, f32>>,
    pub speed: Option<f32>,
    pub skip_tts: Option<bool>,
    pub text_marker: Option<String>,
}

/// 检测到的对话段落
#[derive(Debug, Clone)]
pub struct DialogueSegment {
    pub text: String,
    pub speaker: String,
    pub gender: String,
    pub emotion: String,
    pub emotion_description: String,
    pub emotion_vector: Option<HashMap<String, f32>>,
    pub speed: f32,
    pub is_nsfw: bool,
    pub skip_tts: bool,
}

/// TTS 合成请求
#[derive(Debug, Clone)]
pub struct TTSRequest {
    pub text: String,
    pub speaker: String,
    pub gender: String,
    pub emotion: String,
    pub emotion_description: String,
    pub speed: f32,
    pub emotion_vector: Option<HashMap<String, f32>>,
    pub is_nsfw: bool,
}

/// TTS 合成结果
#[derive(Debug, Clone, Default)]
pub struct TTSResult {
    pub success: bool,
    pub error: Option<String>,
    pub audio_data: Option<Vec<u8>>,
    pub audio_segments: Option<Vec<Vec<u8>>>,
    pub speaker_used: String,
    pub emotion_used: String,
}

/// 情感维度（8维情感向量）
pub const EMOTION_DIMENSIONS: &[&str] = &[
    "joy",        // 喜
    "anger",      // 怒
    "sorrow",     // 哀
    "fear",       // 惧
    "disgust",    // 厌恶
    "depression", // 低落
    "surprise",   // 惊喜
    "calm",       // 平静
];
```

---

## 9. 依赖配置

### 9.1 Cargo.toml 新增依赖

```toml
[dependencies]
# 现有依赖保持不变...

# HTTP 服务器
axum = { version = "0.7", features = ["macros"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }

# 异步运行时（Tauri 已有 tokio）
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"

# HTTP 客户端
reqwest = { version = "0.12", features = ["json", "stream"] }

# JSON 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# SSE
async-stream = "0.3"
futures = "0.3"

# 音频播放
rodio = "0.19"

# 其他
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
parking_lot = "0.12"  # 高性能锁
once_cell = "1.19"    # 全局状态
tracing = "0.1"       # 日志
regex = "1"           # 正则表达式
```

---

## 附录 A：常量定义

```rust
// src-tauri/src/constants.rs

use std::collections::{HashMap, HashSet};
use once_cell::sync::Lazy;

/// 特殊说话者
pub static SPECIAL_SPEAKERS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "narrator", "player", "system", "user", "assistant",
        "旁白", "玩家", "系统", "用户", "助手"
    ].into_iter().collect()
});

pub static NARRATOR_SPEAKERS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    ["narrator", "旁白", "narration"].into_iter().collect()
});

/// 引号配对
pub static QUOTE_PAIRS: Lazy<HashMap<char, char>> = Lazy::new(|| {
    [
        ('"', '"'),
        ('"', '"'),
        ('「', '」'),
        ('『', '』'),
        ('\'', '\''),
        ('（', '）'),
        ('(', ')'),
    ].into_iter().collect()
});

/// 情感向量到 JSON 情感 ID 的映射
pub static EMOTION_VECTOR_TO_JSON: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    [
        ("joy", "happy"),
        ("anger", "angry"),
        ("sorrow", "sad"),
        ("fear", "fear"),
        ("disgust", "angry"),
        ("depression", "depressed"),
        ("surprise", "surprised"),
        ("calm", "calm"),
    ].into_iter().collect()
});

/// 默认超时时间
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub const API_TIMEOUT_SECS: u64 = 60;
pub const TTS_TIMEOUT_SECS: u64 = 120;
```

---

## 附录 B：错误处理

```rust
// src-tauri/src/error.rs

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("配置错误: {0}")]
    ConfigError(String),
    
    #[error("API 请求失败: {0}")]
    ApiError(String),
    
    #[error("TTS 合成失败: {0}")]
    TTSError(String),
    
    #[error("解析错误: {0}")]
    ParseError(String),
    
    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("JSON 错误: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("HTTP 错误: {0}")]
    HttpError(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, AppError>;
```

---

**文档版本**: 1.0
**创建日期**: 2026-02-03
**适用项目**: Ollama TTS Adapter
