//! 情感音频选择逻辑

use std::collections::HashMap;
use tracing::{info, warn};

use super::types::{EmotionAudioConfig, RefAudioInfo};
use crate::models::config::GsvCharacterConfig;
use crate::models::tts_types::emotion_vector_to_json;

/// 根据情感向量选择参考音频
///
/// 逻辑：
/// 1. 按照情感向量百分比匹配
/// 2. 占比低于阈值的忽略
/// 3. 最高的当做主参考音频，其他的作为辅助参考音频
/// 4. 如果主要情感占比超过70%则再加一段这个情感的音频作为辅助参考音频
/// 5. 至少要 min_ref_audios 条音频，最多 max_ref_audios 条
pub fn select_ref_audios_by_emotion_vector(
    char_id: &str,
    emotion_vector: &HashMap<String, f32>,
    emotion_audio_cache: &HashMap<String, EmotionAudioConfig>,
    gsv_characters: &HashMap<String, GsvCharacterConfig>,
    min_ref_audios: i32,
    max_ref_audios: i32,
    emotion_threshold: f32,
    build_audio_path: &dyn Fn(&RefAudioInfo, &str) -> String,
) -> Vec<RefAudioInfo> {
    info!("🎵 情感向量匹配: 角色={}", char_id);
    info!("   📦 已缓存的角色: {:?}", emotion_audio_cache.keys().collect::<Vec<_>>());

    let emotion_config = match emotion_audio_cache.get(char_id) {
        Some(config) => {
            info!("   ✅ 找到角色 {} 的情感配置，包含 {} 个情感", char_id, config.emotions.len());
            config
        },
        None => {
            warn!("   ❌ 未找到角色 {} 的情感配置，可用角色: {:?}", char_id, emotion_audio_cache.keys().collect::<Vec<_>>());
            return Vec::new();
        }
    };

    if emotion_config.emotions.is_empty() {
        warn!("   ⚠️ 角色 {} 的情感配置为空", char_id);
        return Vec::new();
    }

    info!("   🎭 可用情感: {:?}", emotion_config.emotions.keys().collect::<Vec<_>>());

    let emotion_map = emotion_vector_to_json();
    let mut emotion_weights: HashMap<String, f32> = HashMap::new();

    for (vec_key, json_emotion) in &emotion_map {
        let weight = emotion_vector.get(*vec_key).copied().unwrap_or(0.0);
        emotion_weights
            .entry(json_emotion.to_string())
            .and_modify(|w| *w = w.max(weight))
            .or_insert(weight);
    }

    let mut sorted: Vec<_> = emotion_weights.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let total_weight: f32 = sorted.iter().map(|(_, w)| *w).filter(|w| *w > 0.0).sum();
    let total_weight = if total_weight == 0.0 { 1.0 } else { total_weight };

    info!("🎵 GSV 情感分析 (阈值={:.0}%, 最少{}条, 最多{}条):",
          emotion_threshold * 100.0, min_ref_audios, max_ref_audios);
    let mapping_parts: Vec<String> = emotion_map.iter()
        .filter_map(|(k, v)| {
            let w = emotion_vector.get(*k).copied().unwrap_or(0.0);
            if w > 0.0 { Some(format!("{}→{}({:.2})", k, v, w)) } else { None }
        })
        .collect();
    info!("   📊 向量映射: {}", if mapping_parts.is_empty() { "无有效情感".to_string() } else { mapping_parts.join(", ") });

    let char_config = gsv_characters.get(char_id);
    let ref_audio_dir = char_config.map(|c| c.ref_audio_dir.as_str()).unwrap_or("");

    let mut result = Vec::new();
    let mut primary_emotion: Option<String> = None;
    let mut primary_ratio: f32 = 0.0;

    for (emotion_id, weight) in &sorted {
        if *weight <= 0.0 { continue; }
        let ratio = *weight / total_weight;

        if result.is_empty() {
            primary_emotion = Some(emotion_id.clone());
            primary_ratio = ratio;

            if let Some(entry) = emotion_config.emotions.get(emotion_id) {
                if let Some(primary) = &entry.primary {
                    let full_path = build_audio_path(primary, ref_audio_dir);
                    result.push(RefAudioInfo::from_path(full_path, primary.text.clone()));
                    info!("   ✅ 主情感: {} ({:.0}%) - {}", emotion_id, ratio * 100.0, primary.text.chars().take(20).collect::<String>());
                }
            }
        } else if ratio >= emotion_threshold && (result.len() as i32) < max_ref_audios {
            if let Some(entry) = emotion_config.emotions.get(emotion_id) {
                if let Some(primary) = &entry.primary {
                    let full_path = build_audio_path(primary, ref_audio_dir);
                    result.push(RefAudioInfo::from_path(full_path, primary.text.clone()));
                    info!("   ✅ 次情感: {} ({:.0}%) - {}", emotion_id, ratio * 100.0, primary.text.chars().take(20).collect::<String>());
                }
            }
        } else if ratio < emotion_threshold {
            info!("   ⏭️ 跳过: {} ({:.0}%) - 低于{:.0}%阈值", emotion_id, ratio * 100.0, emotion_threshold * 100.0);
        }
    }

    append_supplementary_audios(
        &mut result, &primary_emotion, primary_ratio,
        emotion_config, ref_audio_dir, min_ref_audios, max_ref_audios,
        build_audio_path,
    );

    info!("   📢 最终选择: {} 条参考音频", result.len());
    result
}

/// 补充辅助参考音频（确保数量达标）
fn append_supplementary_audios(
    result: &mut Vec<RefAudioInfo>,
    primary_emotion: &Option<String>,
    primary_ratio: f32,
    emotion_config: &EmotionAudioConfig,
    ref_audio_dir: &str,
    min_ref_audios: i32,
    max_ref_audios: i32,
    build_audio_path: &dyn Fn(&RefAudioInfo, &str) -> String,
) {
    if let Some(primary_emo) = primary_emotion {
        if primary_ratio >= 0.70 && (result.len() as i32) < max_ref_audios {
            if let Some(entry) = emotion_config.emotions.get(primary_emo) {
                if let Some(secondary) = entry.secondary.first() {
                    let full_path = build_audio_path(secondary, ref_audio_dir);
                    result.push(RefAudioInfo::from_path(full_path, secondary.text.clone()));
                    info!("   ✅ 主情感辅助: {} (占比{:.0}%>70%) - {}", primary_emo, primary_ratio * 100.0, secondary.text.chars().take(20).collect::<String>());
                }
            }
        }
    }

    while (result.len() as i32) < min_ref_audios && !result.is_empty() {
        if let Some(primary_emo) = primary_emotion {
            if let Some(entry) = emotion_config.emotions.get(primary_emo) {
                if let Some(secondary) = entry.secondary.first() {
                    let full_path = build_audio_path(secondary, ref_audio_dir);
                    let already_has = result.iter().any(|r| r.audio_file == full_path);
                    if !already_has {
                        result.push(RefAudioInfo::from_path(full_path, secondary.text.clone()));
                        info!("   ✅ 补充辅助: {} (确保至少{}条) - {}", primary_emo, min_ref_audios, secondary.text.chars().take(20).collect::<String>());
                        continue;
                    }
                }
            }
        }

        if let Some(calm_entry) = emotion_config.emotions.get("calm") {
            if let Some(primary) = &calm_entry.primary {
                let full_path = build_audio_path(primary, ref_audio_dir);
                let already_has = result.iter().any(|r| r.audio_file == full_path);
                if !already_has {
                    result.push(RefAudioInfo::from_path(full_path, primary.text.clone()));
                    info!("   ✅ 补充 calm: (确保至少{}条) - {}", min_ref_audios, primary.text.chars().take(20).collect::<String>());
                    continue;
                }
            }
        }
        break;
    }

    if result.is_empty() {
        if let Some(calm_entry) = emotion_config.emotions.get("calm") {
            if let Some(primary) = &calm_entry.primary {
                warn!("   无有效情感，回退到 calm");
                let full_path = build_audio_path(primary, ref_audio_dir);
                result.push(RefAudioInfo::from_path(full_path, primary.text.clone()));
            }
            if let Some(secondary) = calm_entry.secondary.first() {
                let full_path = build_audio_path(secondary, ref_audio_dir);
                result.push(RefAudioInfo::from_path(full_path, secondary.text.clone()));
            }
        }
    }
}

/// 根据情感字符串选择参考音频
pub fn select_audio_by_emotion(
    char_id: &str,
    emotion: &str,
    emotion_audio_cache: &HashMap<String, EmotionAudioConfig>,
    gsv_characters: &HashMap<String, GsvCharacterConfig>,
    build_audio_path: &dyn Fn(&RefAudioInfo, &str) -> String,
) -> Option<RefAudioInfo> {
    let emotion_config = emotion_audio_cache.get(char_id)?;

    let emotion_lower = emotion.to_lowercase();
    let emotion_map: HashMap<&str, &str> = [
        ("happy", "happy"), ("joy", "happy"), ("joyful", "happy"),
        ("angry", "angry"), ("anger", "angry"), ("mad", "angry"),
        ("sad", "sad"), ("sorrow", "sad"), ("sorrowful", "sad"),
        ("fear", "fear"), ("afraid", "fear"), ("scared", "fear"),
        ("surprised", "surprised"), ("surprise", "surprised"),
        ("depressed", "depressed"), ("depression", "depressed"),
        ("calm", "calm"), ("neutral", "calm"), ("normal", "calm"),
        ("喜", "happy"), ("开心", "happy"), ("高兴", "happy"),
        ("怒", "angry"), ("愤怒", "angry"), ("生气", "angry"),
        ("哀", "sad"), ("悲伤", "sad"), ("难过", "sad"),
        ("惧", "fear"), ("恐惧", "fear"), ("害怕", "fear"),
        ("惊", "surprised"), ("惊讶", "surprised"),
        ("平静", "calm"), ("平和", "calm"),
    ].into_iter().collect();

    let json_emotion = emotion_map.get(emotion_lower.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            if emotion_config.emotions.contains_key(&emotion_lower) {
                Some(emotion_lower.clone())
            } else if emotion_config.emotions.contains_key("calm") {
                info!("🎵 情感 '{}' 未找到，回退到 calm", emotion);
                Some("calm".to_string())
            } else {
                emotion_config.emotions.keys().next().cloned()
            }
        })?;

    let entry = emotion_config.emotions.get(&json_emotion)?;
    let primary = entry.primary.as_ref()?;

    let char_config = gsv_characters.get(char_id);
    let ref_audio_dir = char_config.map(|c| c.ref_audio_dir.as_str()).unwrap_or("");
    let full_path = build_audio_path(primary, ref_audio_dir);

    Some(RefAudioInfo::from_path(full_path, primary.text.clone()))
}
