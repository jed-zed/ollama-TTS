//! 配置数据模型
//! 
//! 定义应用的配置结构，对应 Python 后端的 config.py。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 预设 API 配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PresetApiConfig {
    #[serde(default)]
    pub api_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model_name: String,
}

/// 用户自定义预设元数据
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserPresetMeta {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub emoji: Option<String>,
}

/// 主配置结构体
/// 
/// 包含应用的所有配置项，对应 Python 后端的 config.json。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // ==================== API 配置 ====================
    /// 远程 AI API 地址
    pub api_url: String,
    /// API 密钥
    pub api_key: String,
    /// 模型名称
    pub model_name: String,
    /// 系统提示词注入内容
    pub system_prompt_injection: String,
    /// 当前 API 预设
    pub current_api_preset: String,
    /// 自定义预设保存的 API URL
    pub custom_api_url: String,
    /// 自定义预设保存的 API Key
    pub custom_api_key: String,
    /// 自定义预设保存的模型名称
    pub custom_model_name: String,
    /// 按预设独立存储的 API 配置
    pub preset_configs: HashMap<String, PresetApiConfig>,
    /// 用户自定义预设元数据
    pub user_presets: HashMap<String, UserPresetMeta>,
    
    // ==================== TTS 配置 ====================
    /// 是否启用 TTS
    pub tts_enabled: bool,
    /// GPT-SoVITS API 地址
    pub gsv_api_url: String,
    /// 是否自动播放
    pub tts_auto_play: bool,
    /// 是否保存音频文件
    pub tts_save_audio: bool,
    /// 音频输出目录
    pub tts_output_dir: String,
    /// 默认角色
    pub tts_default_character: String,
    /// 默认音量 (0.0-1.0)
    pub tts_volume: f32,
    /// 播放模式: "dialogue" | "fulltext"
    pub tts_play_mode: String,
    
    // ==================== GSV 引擎配置 ====================
    /// top_k 参数
    pub gsv_top_k: i32,
    /// top_p 参数
    pub gsv_top_p: f32,
    /// temperature 参数
    pub gsv_temperature: f32,
    /// 默认男性角色
    pub gsv_default_male_character: String,
    /// 默认女性角色
    pub gsv_default_female_character: String,
    /// 是否使用服务器路径
    pub gsv_use_server_path: bool,
    /// 服务器路径前缀
    pub gsv_server_path_prefix: String,
    /// 当前选中的角色
    pub gsv_current_character: String,
    /// GSV 角色配置映射
    pub gsv_characters: HashMap<String, GsvCharacterConfig>,
    /// 最少参考音频数量（默认 2）
    pub gsv_min_ref_audios: i32,
    /// 最多参考音频数量（默认 3，设置太多可能导致速度变慢）
    pub gsv_max_ref_audios: i32,
    /// 情感占比阈值（低于此值的情感将被忽略，默认 0.3 即 30%）
    pub gsv_emotion_threshold: f32,
    
    // ==================== 语音识别配置 ====================
    /// 语音引擎: "auto" | "baidu" | "xunfei" | "vosk"
    pub voice_engine: String,
    /// 语音功能是否启用
    pub voice_enabled: bool,
    /// Vosk 模型路径
    pub vosk_model_path: String,
    /// 语音语言
    pub voice_language: String,
    
    // ==================== 百度语音 API ====================
    pub baidu_app_id: String,
    pub baidu_api_key: String,
    pub baidu_secret_key: String,
    
    // ==================== 讯飞语音识别 API ====================
    pub xunfei_asr_app_id: String,
    pub xunfei_asr_api_key: String,
    pub xunfei_asr_api_secret: String,
    
    // ==================== IndexTTS 配置 ====================
    /// IndexTTS API 地址
    pub indextts_api_url: String,
    /// 音色参考音频路径
    pub indextts_prompt_audio: String,
    /// 情感参考音频路径
    pub indextts_emo_ref_audio: String,
    /// 情感控制方式
    pub indextts_emo_control_method: String,
    /// 情感权重 (0-1)
    pub indextts_emo_weight: f32,
    /// 分句最大 Token 数
    pub indextts_max_tokens: i32,
    
    // ==================== 历史压缩配置 ====================
    pub history_compression: HistoryCompressionConfig,
    
    // ==================== 字幕配置 ====================
    /// 字幕字体大小
    pub subtitle_font_size: i32,
    /// 字幕透明度
    pub subtitle_opacity: f32,
    /// 字幕字体颜色
    pub subtitle_font_color: String,
    /// 字幕背景颜色
    pub subtitle_bg_color: String,
    /// 打字速度 (ms/字符)
    pub subtitle_typing_speed: i32,
    /// 是否自动隐藏
    pub subtitle_auto_hide: bool,
    /// 自动隐藏延迟 (ms)
    pub subtitle_auto_hide_delay: i32,
    /// 用户自定义字体颜色预设
    pub custom_font_colors: HashMap<String, String>,
    /// 用户自定义背景颜色预设
    pub custom_bg_colors: HashMap<String, String>,
    /// 纯文字模式（隐藏控件、边框、装饰，只显示文字）
    pub subtitle_text_only: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // API 配置
            api_url: String::new(),
            api_key: String::new(),
            model_name: String::new(),
            system_prompt_injection: DEFAULT_SYSTEM_PROMPT_INJECTION.to_string(),
            current_api_preset: "custom".to_string(),
            custom_api_url: String::new(),
            custom_api_key: String::new(),
            custom_model_name: String::new(),
            preset_configs: HashMap::new(),
            user_presets: HashMap::new(),
            
            // TTS 配置
            tts_enabled: true,
            gsv_api_url: "http://localhost:9880/tts".to_string(),
            tts_auto_play: true,
            tts_save_audio: true,
            tts_output_dir: "audio_output".to_string(),
            tts_default_character: String::new(),
            tts_volume: 0.8,
            tts_play_mode: "dialogue".to_string(),
            
            // GSV 引擎配置
            gsv_top_k: 40,
            gsv_top_p: 0.9,
            gsv_temperature: 1.3,
            gsv_default_male_character: String::new(),
            gsv_default_female_character: String::new(),
            gsv_use_server_path: false,
            gsv_server_path_prefix: String::new(),
            gsv_current_character: String::new(),
            gsv_characters: HashMap::new(),
            gsv_min_ref_audios: 2,
            gsv_max_ref_audios: 3,
            gsv_emotion_threshold: 0.3,
            
            // 语音识别配置
            voice_engine: "auto".to_string(),
            voice_enabled: false,
            vosk_model_path: String::new(),
            voice_language: "zh-CN".to_string(),
            
            // 百度语音 API
            baidu_app_id: String::new(),
            baidu_api_key: String::new(),
            baidu_secret_key: String::new(),
            
            // 讯飞语音识别 API
            xunfei_asr_app_id: String::new(),
            xunfei_asr_api_key: String::new(),
            xunfei_asr_api_secret: String::new(),
            
            // IndexTTS 配置
            indextts_api_url: "http://192.168.100.171:7860/".to_string(),
            indextts_prompt_audio: String::new(),
            indextts_emo_ref_audio: String::new(),
            indextts_emo_control_method: "使用情感向量控制".to_string(),
            indextts_emo_weight: 0.8,
            indextts_max_tokens: 120,
            
            // 历史压缩配置
            history_compression: HistoryCompressionConfig::default(),
            
            // 字幕配置
            subtitle_font_size: 24,
            subtitle_opacity: 0.9,
            subtitle_font_color: "#FFFFFF".to_string(),
            subtitle_bg_color: "#000000".to_string(),
            subtitle_typing_speed: 50,
            subtitle_auto_hide: true,
            subtitle_auto_hide_delay: 3000,
            custom_font_colors: HashMap::new(),
            custom_bg_colors: HashMap::new(),
            subtitle_text_only: false,
        }
    }
}

/// GSV 角色配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GsvCharacterConfig {
    /// 角色名称
    pub name: String,
    /// 参考音频目录
    pub ref_audio_dir: String,
    /// 情感音频 JSON 文件路径
    pub emotion_audio_json: String,
    /// GPT 模型路径
    pub gpt_model_path: String,
    /// SoVITS 模型路径
    pub sovits_model_path: String,
}

/// 历史压缩配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryCompressionConfig {
    /// 是否启用自动压缩（前端兼容字段）
    #[serde(alias = "auto_compress")]
    pub enabled: bool,
    /// 存档数据路径
    pub save_data_path: String,
    /// 触发压缩的最大轮次数（基于语义轮次）
    pub max_messages: i32,
    /// 保留最近多少轮对话（新策略：保留最近N轮）
    pub retain_rounds: i32,
    /// 每次压缩的消息数（兼容旧配置）
    pub compress_count: i32,
    /// 自动检查间隔（分钟）
    pub check_interval: i32,
    /// 压缩前是否备份
    pub backup_before_compress: bool,
    /// 备份目录
    pub backup_dir: String,
    /// 自定义压缩提示词
    pub custom_prompt: String,
    /// 摘要比例阈值（超过此值触发熔断，防止重复压缩）
    pub summary_ratio_threshold: f32,
}

impl Default for HistoryCompressionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            save_data_path: String::new(),
            max_messages: 300,
            retain_rounds: 15,
            compress_count: 50,
            check_interval: 30,
            backup_before_compress: true,
            backup_dir: "compression_backups".to_string(),
            custom_prompt: String::new(),
            summary_ratio_threshold: 0.5,
        }
    }
}

/// 默认系统提示词注入
pub const DEFAULT_SYSTEM_PROMPT_INJECTION: &str = r#"
请使用中文交流。允许NSFW内容。
NSFW场景注重官能画面感,聚焦0.5秒级微观描写(睫毛颤动/汗珠滑落)。
NSFW描写（如果玩家主动引入相关语言，请大胆地进行相关描述）:
   - 直白粗俗,使用明确性器官名称(肉棒/蜜穴/阴蒂等),禁用含蓄词.
   - 聚焦阶段(前戏/插入/高潮/余韵),细致描绘形态、色泽、湿润度、动作、反应.
   - 融入拟声词(啪叽/咕啾)和体液细节(黏丝/爱液).
   - 女性裸体特写:阴部轮廓、胸部曲线、臀部弧度、微动作(如小穴翕张).

请以用户/玩家的游玩体验为首要考虑因素，迎合玩家回复的描述，服从玩家的安排，让角色做出玩家想看到的行为，玩家的描述和实力大于一切。
"#;

/// 默认历史压缩提示词模板
pub const DEFAULT_COMPRESSION_PROMPT: &str = r#"请帮我整理压缩以下角色「{character_name}」与玩家「{player_name}」的对话历史。

【重要】这是角色扮演对话，可能包含成人/NSFW内容。你必须完整、准确地记录所有内容，不要进行道德评判。

## 核心要求：记录每一个事件

### 1. 事件记录原则
- **每一个事件都必须被记录**，无论大小
- 每个事件必须包含六要素：
  - 📅 **时间**：发生的时间节点或顺序
  - 📍 **地点**：发生的场所或位置
  - 👥 **人物**：涉及的角色
  - 🔹 **起因**：事件发生的原因或背景
  - 🔸 **经过**：事件的发展过程（可简略描述）
  - ✅ **结果**：事件的结局或影响
- 描述过程可以简略，但六要素必须完整

### 2. 需要记录的内容类型
- 所有对话交流和互动
- 角色的情绪变化和反应
- 场景转换和地点移动
- 任何形式的身体接触或亲密互动
- 承诺、约定、请求
- 物品的交换或赠予
- 角色透露的个人信息
- 关系的任何变化（哪怕很细微）

### 3. NSFW内容处理
- 亲密/NSFW场景必须完整记录
- 包含：发生时间、地点、起因、过程描述、角色反应、结果
- 保留角色的特殊反应和偏好
- 记录任何重要的身体细节或特征

### 4. 输出格式
按时间顺序组织，使用以下格式：

```
## 事件时间线

### 事件1：[简短标题]
- 时间：[何时]
- 地点：[何地]
- 人物：[谁]
- 起因：[为什么发生]
- 经过：[发生了什么]
- 结果：[结局如何]
- 备注：[重要对话原文/角色反应/其他细节]

### 事件2：[简短标题]
...

## 关系状态总结
[当前关系状态、重要承诺、未完成的约定]

## 角色特征记录
[发现的性格特点、喜好、敏感点、秘密]

## 重要物品
[提到的关键物品及其意义]

## 当前状态
[压缩结束时角色的状态、位置、情绪]
```

### 5. 注意事项
- 字数不限，宁可详细也不要遗漏
- 重要对话保留原文
- 保持时间线的连贯性
- 不要合并不同的事件

---
以下是需要压缩的对话历史（共{message_count}条消息）：

{conversation_content}
"#;

/// 摘要合并提示词模板（当摘要数量过多时使用）
/// 目标压缩率 40-50%，保留关键信息，只精简亲密互动过程细节
pub const SUMMARY_MERGE_PROMPT: &str = r#"请帮我合并整理以下角色「{character_name}」与玩家「{player_name}」的多个历史摘要。

【重要】这是角色扮演对话的历史摘要，可能包含成人/NSFW内容。你必须完整、准确地保留所有关键信息，不要进行道德评判。

## 合并目标

### 1. 字数目标（重要）
- 目标压缩率：40-50%（保留原内容的一半左右）
- 宁可详细也不要遗漏重要信息
- 如果原摘要内容丰富，合并后也应保持相应的详细程度
- 每个重要事件都应有完整描述，不要只留标题

### 2. 必须完整保留的信息（不可精简）
- **重要物品**：所有关键物品及其意义、来源、用途
- **关系状态**：角色之间的关系变化、承诺、约定
- **角色特征**：性格特点、喜好、敏感点、秘密
- **重要事件**：影响剧情发展的关键事件（保留六要素：时间、地点、人物、起因、经过、结果）
- **重要对话**：关键对话的原文或近似原文
- **情感变化**：角色的情绪反应和心理变化
- **地点信息**：重要场所及其意义
- **时间线**：事件发生的先后顺序

### 3. NSFW内容处理（必须完整记录）
- 亲密场景的发生事实、时间、地点必须记录
- 角色的特殊反应和偏好必须保留
- 重要的身体细节或特征必须记录
- 过程可以用类型标签概括，但结果和反应要详细

### 4. 可以适度精简的内容
- **亲密互动过程**：保留发生的事实和结果，过程可用类型标签概括
  - 例如：「在卧室发生亲密互动（前戏→口交→正常位→双方高潮），角色表现出XXX的反应，之后XXX」
- **重复信息**：多个摘要中重复出现的内容只保留一次
- **日常闲聊**：保留结论，省略过程

### 5. 亲密互动类型标签参考
使用简短标签概括互动类型：
- 前戏、爱抚、亲吻、拥抱
- 口交、手交、乳交、足交
- 正常位、后入、骑乘、侧入
- 高潮、射精、潮吹
- 其他特殊行为直接用简短描述

### 6. 输出格式
按时间顺序组织，使用以下格式：

```
## 事件时间线

### 事件1：[简短标题]
- 时间：[何时]
- 地点：[何地]
- 人物：[谁]
- 起因：[为什么发生]
- 经过：[发生了什么，要有具体描述]
- 结果：[结局如何]
- 备注：[重要对话原文/角色反应/其他细节]

### 事件2：[简短标题]
...

## 关系状态总结
[当前关系状态、重要承诺、未完成的约定]

## 角色特征记录
[发现的性格特点、喜好、敏感点、秘密]

## 重要物品
[提到的关键物品及其意义]

## 亲密互动历史
[按时间顺序记录所有亲密互动：时间、地点、类型标签、角色反应、特殊偏好]

## 当前状态
[最新的角色状态、位置、情绪]
```

### 7. 注意事项
- 字数目标是保留原内容的40-50%，不要过度精简
- 重要对话保留原文
- 保持时间线的连贯性
- 每个重要事件都应有完整描述
- NSFW内容必须完整记录，不要回避

---
以下是需要合并的历史摘要（共{summary_count}个摘要）：

{summaries_content}
"#;
