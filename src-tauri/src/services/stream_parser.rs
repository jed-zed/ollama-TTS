//! 流式响应解析器
//! 
//! 实时解析流式 JSON，只提取 response 字段内容，过滤技术字段。

/// 流式响应解析器
/// 
/// 边接收数据边提取 response 字段内容。
/// 只显示 response 字段内容，过滤掉 romance_intent、decision 等技术字段。
pub struct StreamingResponseParser {
    /// 完整缓冲区
    buffer: String,
    /// 是否在 response 字段内
    in_response_field: bool,
    /// response 内容累积
    response_content: String,
    /// 是否在字符串内
    in_string: bool,
    /// 下一个字符是否是转义字符
    escape_next: bool,
    /// 字段名缓冲区
    field_name_buffer: String,
    /// 是否在查找字段名
    looking_for_field: bool,
    /// 是否在冒号之后
    after_colon: bool,
}

impl StreamingResponseParser {
    /// 创建新的解析器实例
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            in_response_field: false,
            response_content: String::new(),
            in_string: false,
            escape_next: false,
            field_name_buffer: String::new(),
            looking_for_field: true,
            after_colon: false,
        }
    }
    
    /// 重置解析器状态
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.in_response_field = false;
        self.response_content.clear();
        self.in_string = false;
        self.escape_next = false;
        self.field_name_buffer.clear();
        self.looking_for_field = true;
        self.after_colon = false;
    }
    
    /// 喂入新的数据块，返回新增的 response 内容
    /// 
    /// # Arguments
    /// * `chunk` - 新的数据块
    /// 
    /// # Returns
    /// 新增的 response 内容（用于实时显示）
    pub fn feed(&mut self, chunk: &str) -> String {
        let mut new_content = String::new();
        
        for char in chunk.chars() {
            self.buffer.push(char);
            
            // 处理转义字符
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
            
            // 检测字符串边界
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
            
            // 在字符串内部
            if self.in_string {
                if self.in_response_field {
                    new_content.push(char);
                    self.response_content.push(char);
                } else if self.looking_for_field && !self.after_colon {
                    self.field_name_buffer.push(char);
                }
                continue;
            }
            
            // 在字符串外部
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
    
    /// 获取累积的 response 内容
    pub fn get_response_content(&self) -> &str {
        &self.response_content
    }
    
    /// 获取完整缓冲区
    pub fn get_buffer(&self) -> &str {
        &self.buffer
    }
}

impl Default for StreamingResponseParser {
    fn default() -> Self {
        Self::new()
    }
}

/// 从完整内容中只提取 response 字段用于显示
/// 
/// 过滤掉 romance_intent, decision 等其他字段。
pub fn extract_response_only(full_content: &str) -> String {
    if full_content.is_empty() {
        return String::new();
    }
    
    // 尝试解析为 JSON
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(full_content) {
        if let Some(response) = data.get("response").and_then(|v| v.as_str()) {
            return response.to_string();
        }
    }
    
    // 如果不是 JSON 或没有 response 字段，使用流式解析器
    let mut parser = StreamingResponseParser::new();
    parser.feed(full_content)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_response() {
        let mut parser = StreamingResponseParser::new();
        let result = parser.feed(r#"{"response": "Hello, world!"}"#);
        assert_eq!(result, "Hello, world!");
    }
    
    #[test]
    fn test_with_other_fields() {
        let mut parser = StreamingResponseParser::new();
        let result = parser.feed(r#"{"romance_intent": 0.5, "response": "Hello!", "decision": "none"}"#);
        assert_eq!(result, "Hello!");
    }
    
    #[test]
    fn test_streaming() {
        let mut parser = StreamingResponseParser::new();
        let mut result = String::new();
        result.push_str(&parser.feed(r#"{"response": "Hel"#));
        result.push_str(&parser.feed(r#"lo, "#));
        result.push_str(&parser.feed(r#"world!"}"#));
        assert_eq!(result, "Hello, world!");
    }
    
    #[test]
    fn test_escape_sequences() {
        let mut parser = StreamingResponseParser::new();
        let result = parser.feed(r#"{"response": "Line1\nLine2"}"#);
        assert_eq!(result, "Line1\nLine2");
    }
}
