//! 文本切分逻辑

use regex::Regex;

/// 智能切分长文本
pub fn split_text(text: &str, max_length: usize) -> Vec<String> {
    if text.chars().count() <= max_length {
        return vec![text.to_string()];
    }

    let primary_regex = Regex::new(r"[。！？!?]").unwrap();
    let secondary_regex = Regex::new(r"[，,；;：:]").unwrap();

    let mut segments = Vec::new();
    let mut remaining = text.trim();

    while !remaining.is_empty() {
        if remaining.chars().count() <= max_length {
            segments.push(remaining.to_string());
            break;
        }

        let search_end = remaining.char_indices()
            .take(max_length)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(remaining.len());

        let search_text = &remaining[..search_end];
        let mut split_pos = None;

        if let Some(m) = primary_regex.find_iter(search_text).last() {
            split_pos = Some(m.end());
        }

        if split_pos.is_none() {
            if let Some(m) = secondary_regex.find_iter(search_text).last() {
                split_pos = Some(m.end());
            }
        }

        let pos = split_pos.unwrap_or(search_end);
        segments.push(remaining[..pos].trim().to_string());
        remaining = remaining[pos..].trim();
    }

    segments.into_iter().filter(|s| !s.is_empty()).collect()
}
