---
name: Tauri + FastAPI 桌面应用开发
description: 构建 Tauri 跨平台桌面应用，集成 Python FastAPI 后端（Sidecar 模式），适用于 AI 对话、TTS、流式响应等场景
---

# Tauri + FastAPI 桌面应用开发指南

你是一个专注于 Tauri + FastAPI 桌面应用开发的专家助手。

## 技术栈

### 前端
- **框架**: React 18+ with TypeScript
- **样式**: Tailwind CSS
- **组件库**: Shadcn UI
- **状态管理**: Zustand 或 React Context
- **构建工具**: Vite

### 后端 (Sidecar)
- **框架**: FastAPI (Python 3.10+)
- **TTS 引擎**: 多引擎支持 (edge-tts, pyttsx3, Azure TTS 等)
- **AI 集成**: OpenAI API, Claude API, 本地模型
- **打包**: PyInstaller

### 桌面框架
- **Tauri 2.x**: Rust 核心 + WebView 前端
- **Sidecar 模式**: 嵌入 Python 可执行文件

## 项目结构

```
project/
├── src/                      # React 前端
│   ├── components/           # UI 组件
│   ├── hooks/                # 自定义 Hooks
│   ├── lib/                  # 工具函数
│   ├── stores/               # 状态管理
│   └── App.tsx
├── src-tauri/                # Tauri Rust 后端
│   ├── src/
│   │   ├── main.rs
│   │   └── lib.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── python/                   # FastAPI Sidecar
│   ├── main.py               # FastAPI 入口
│   ├── api/                  # API 路由
│   ├── services/             # 业务逻辑
│   │   ├── tts.py            # TTS 服务
│   │   └── ai.py             # AI 对话服务
│   └── requirements.txt
└── package.json
```

## 核心开发模式

### 1. Sidecar 集成

**tauri.conf.json 配置:**
```json
{
  "bundle": {
    "externalBin": ["binaries/python-backend"]
  }
}
```

**Rust 端启动 Sidecar:**
```rust
use tauri::api::process::{Command, CommandEvent};

#[tauri::command]
async fn start_backend() -> Result<u16, String> {
    let (mut rx, _child) = Command::new_sidecar("python-backend")
        .expect("failed to create sidecar command")
        .spawn()
        .expect("failed to spawn sidecar");

    // 返回端口号
    Ok(8765)
}
```

### 2. 流式响应处理

**FastAPI 流式端点:**
```python
from fastapi import FastAPI
from fastapi.responses import StreamingResponse
import asyncio

app = FastAPI()

@app.post("/api/chat/stream")
async def chat_stream(request: ChatRequest):
    async def generate():
        async for chunk in ai_service.stream_response(request.message):
            yield f"data: {json.dumps({'content': chunk})}\n\n"

    return StreamingResponse(generate(), media_type="text/event-stream")
```

**前端 SSE 处理:**
```typescript
const streamChat = async (message: string, onChunk: (text: string) => void) => {
  const response = await fetch('http://localhost:8765/api/chat/stream', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ message }),
  });

  const reader = response.body?.getReader();
  const decoder = new TextDecoder();

  while (reader) {
    const { done, value } = await reader.read();
    if (done) break;

    const chunk = decoder.decode(value);
    const lines = chunk.split('\n');
    for (const line of lines) {
      if (line.startsWith('data: ')) {
        const data = JSON.parse(line.slice(6));
        onChunk(data.content);
      }
    }
  }
};
```

### 3. TTS 多引擎支持

```python
from abc import ABC, abstractmethod

class TTSEngine(ABC):
    @abstractmethod
    async def synthesize(self, text: str) -> bytes:
        pass

class EdgeTTSEngine(TTSEngine):
    async def synthesize(self, text: str) -> bytes:
        import edge_tts
        communicate = edge_tts.Communicate(text, voice="zh-CN-XiaoxiaoNeural")
        audio_data = b""
        async for chunk in communicate.stream():
            if chunk["type"] == "audio":
                audio_data += chunk["data"]
        return audio_data

class TTSService:
    def __init__(self):
        self.engines = {
            "edge": EdgeTTSEngine(),
            # 添加更多引擎...
        }

    async def speak(self, text: str, engine: str = "edge") -> bytes:
        return await self.engines[engine].synthesize(text)
```

### 4. 实时字幕显示

```tsx
import { useState, useEffect } from 'react';

function SubtitleDisplay({ text }: { text: string }) {
  const [displayText, setDisplayText] = useState('');

  useEffect(() => {
    let index = 0;
    const timer = setInterval(() => {
      if (index < text.length) {
        setDisplayText(prev => prev + text[index]);
        index++;
      } else {
        clearInterval(timer);
      }
    }, 50);

    return () => clearInterval(timer);
  }, [text]);

  return (
    <div className="fixed bottom-8 left-1/2 -translate-x-1/2
                    bg-black/80 text-white px-6 py-3 rounded-lg
                    text-xl font-medium max-w-[80vw] text-center">
      {displayText}
    </div>
  );
}
```

## 最佳实践

### 错误处理
- 前端使用 try-catch 包装所有 API 调用
- 后端使用 FastAPI 异常处理器统一返回错误格式
- Sidecar 启动失败时提供友好提示

### 性能优化
- 使用 `useMemo` 和 `useCallback` 避免不必要的重渲染
- TTS 音频使用流式播放，不等待完整下载
- AI 响应使用流式输出，提升用户体验

### 跨平台兼容
- 使用 Tauri API 处理文件路径 (`@tauri-apps/api/path`)
- Sidecar 需要为每个平台单独编译
- 测试 Windows/macOS/Linux 的差异

### 打包发布
```bash
# Python Sidecar 打包
cd python
pyinstaller --onefile --name python-backend main.py

# 复制到 Tauri binaries 目录
cp dist/python-backend ../src-tauri/binaries/python-backend-x86_64-pc-windows-msvc.exe

# Tauri 构建
npm run tauri build
```

## 常用命令

```bash
# 开发模式
npm run tauri dev

# Python 后端独立运行 (调试用)
cd python && uvicorn main:app --reload --port 8765

# 构建发布版本
npm run tauri build
```

## 调试技巧

1. **Sidecar 日志**: 在 Rust 端捕获并输出 Python 进程的 stdout/stderr
2. **网络请求**: 使用浏览器开发者工具检查 API 调用
3. **Tauri 日志**: 设置 `RUST_LOG=debug` 环境变量

## 注意事项

- Sidecar 端口冲突：检测并自动选择可用端口
- 进程清理：确保应用退出时正确终止 Python 进程
- 安全性：Sidecar API 仅绑定 localhost，避免外部访问