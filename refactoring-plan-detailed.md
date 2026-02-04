# 项目重构详细计划

## 项目概述

**项目名称**: Ollama TTS Adapter
**技术栈**: Tauri 2.x + React 19 + Python FastAPI (Sidecar)
**工作目录**: `I:\BaiduNetdiskDownload\ai\react`

---

## 阶段划分

```
Phase 1: FastAPI 后端骨架 + 健康检查
Phase 2: 配置 API + 前端服务控制
Phase 3: Ollama 对话 + 流式响应
Phase 4: 角色管理 API
Phase 5: TTS 引擎集成
Phase 6: 记忆压缩系统
Phase 7: 清理与优化
```

---

# Phase 1: FastAPI 后端骨架 + 健康检查

## 目标
- 创建 FastAPI 后端基础结构
- 实现 `/health` 健康检查端点
- 确保 Tauri 能正确启动和检测后端

## 文件操作

### 创建目录结构
```
python-backend/
├── app/
│   ├── __init__.py
│   ├── config.py
│   ├── routers/
│   │   ├── __init__.py
│   │   └── health.py
│   ├── services/
│   │   └── __init__.py
│   └── models/
│       └── __init__.py
├── main.py (重写)
└── requirements.txt
```

### 需要重写的文件
- `python-backend/main.py` - 从 GUI 入口改为 FastAPI 入口

### 需要修改的文件
- `src-tauri/src/lib.rs` - 添加健康检查调用

## 验收标准
- [ ] `uvicorn main:app --port 8765` 能正常启动
- [ ] `GET http://localhost:8765/health` 返回 `{"status": "ok"}`
- [ ] `npm run tauri dev` 不再弹出旧 GUI 窗口

## Phase 1 提示词

```
# Phase 1: FastAPI 后端骨架 + 健康检查

## 项目背景
我正在重构一个 Tauri + React + Python 项目。当前 Python 后端是一个 GUI 程序，需要改造为 FastAPI API 服务。

## 工作目录
I:\BaiduNetdiskDownload\ai\react

## 当前问题
1. `python-backend/main.py` 调用 `create_gui()` 启动 GUI，需要改为 FastAPI
2. 前端期望后端运行在 `http://localhost:8765`
3. Tauri 使用 sidecar 模式启动 Python 后端

## 本阶段任务

### 1. 重写 main.py
将 `python-backend/main.py` 改为 FastAPI 入口：
- 使用 uvicorn 启动
- 支持 `--port` 参数（默认 8765）
- 添加 CORS 中间件（允许 localhost）

### 2. 创建目录结构
```
python-backend/
├── app/
│   ├── __init__.py
│   ├── config.py          # 应用配置
│   ├── routers/
│   │   ├── __init__.py
│   │   └── health.py      # 健康检查路由
│   ├── services/
│   │   └── __init__.py
│   └── models/
│       └── __init__.py
├── main.py                 # FastAPI 入口
└── requirements.txt        # 依赖清单
```

### 3. 实现健康检查端点
`GET /health` 返回：
```json
{
  "status": "ok",
  "version": "1.0.0",
  "timestamp": "2026-02-01T12:00:00Z"
}
```

### 4. 创建 requirements.txt
```
fastapi>=0.109.0
uvicorn[standard]>=0.27.0
pydantic>=2.5.0
python-multipart>=0.0.6
```

## 技术要求
- Python 3.10+
- FastAPI 最新版本
- 使用 Pydantic v2 模型

## 验收标准
完成后请测试：
1. `cd python-backend && python main.py --port 8765`
2. 访问 `http://localhost:8765/health`
3. 访问 `http://localhost:8765/docs` 查看 API 文档

## 注意事项
- 不要删除 `gui.py` 等旧文件（留待 Phase 7 清理）
- 保留旧的虚拟环境目录 `venv/` 和 `ollama_env/`
- 确保 Windows 兼容性（路径使用正斜杠或 pathlib）
```

---

# Phase 2: 配置 API + 前端服务控制

## 目标
- 实现配置读写 API
- 前端主页添加"启动服务"和"测试连接"控件
- 显示后端连接状态

## 文件操作

### 创建
- `python-backend/app/routers/config.py` - 配置 API 路由
- `python-backend/app/services/config_store.py` - 配置存储服务
- `python-backend/app/models/schemas.py` - Pydantic 模型

### 修改
- `src/pages/Service.tsx` - 添加服务控制 UI
- `src/services/api.ts` - 添加健康检查方法
- `src/stores/configStore.ts` - 添加后端状态

## 验收标准
- [ ] `GET /api/config` 返回完整配置
- [ ] `PUT /api/config` 能更新配置
- [ ] 前端能显示后端连接状态（已连接/未连接）
- [ ] 点击"测试连接"能检测后端状态

## Phase 2 提示词

```
# Phase 2: 配置 API + 前端服务控制

## 项目背景
继续重构 Tauri + React + Python 项目。Phase 1 已完成 FastAPI 骨架和健康检查。

## 工作目录
I:\BaiduNetdiskDownload\ai\react

## 当前状态
- FastAPI 后端骨架已创建
- `/health` 端点已实现
- 后端运行在 `http://localhost:8765`

## 本阶段任务

### 1. 实现配置 API

**创建 `python-backend/app/routers/config.py`:**

端点：
- `GET /api/config` - 获取完整配置
- `PUT /api/config` - 更新配置（部分更新）
- `PATCH /api/config/{key}` - 更新单个配置项

**创建 `python-backend/app/services/config_store.py`:**
- 配置文件路径：`python-backend/data/config.json`
- 支持默认值
- 线程安全读写

**创建 `python-backend/app/models/schemas.py`:**

参考前端类型定义 `src/services/types.ts` 中的 `Config` 接口：
```typescript
interface Config {
  api_url: string;
  api_key: string;
  model_name: string;
  current_api_preset: string;
  system_prompt_injection: string;

  tts_enabled: boolean;
  tts_engine: 'gpt_sovits' | 'xunfei';
  tts_auto_play: boolean;
  tts_save_audio: boolean;
  tts_volume: number;
  tts_play_mode: 'dialogue' | 'fulltext';

  gsv_api_url: string;
  gsv_current_character: string;
  gsv_characters: Record<string, GSVCharacter>;
  // ... 更多字段
}
```

### 2. 修改前端主页（Service.tsx）

**添加服务状态显示：**
- 顶部显示后端连接状态（绿色圆点=已连接，红色=未连接）
- 显示后端版本号

**添加控制按钮：**
- "测试连接" 按钮 - 调用 `/health` 检测
- 状态指示器（连接中/已连接/连接失败）

**修改 `src/pages/Service.tsx`:**
在 API 配置区域上方添加服务状态卡片：
```tsx
<Card>
  <CardHeader>
    <CardTitle>后端服务状态</CardTitle>
  </CardHeader>
  <CardContent>
    <div className="flex items-center gap-4">
      <span className="flex items-center gap-2">
        <span className={`w-3 h-3 rounded-full ${isConnected ? 'bg-green-500' : 'bg-red-500'}`} />
        {isConnected ? '已连接' : '未连接'}
      </span>
      <Button onClick={handleTestConnection}>测试连接</Button>
    </div>
  </CardContent>
</Card>
```

### 3. 添加前端健康检查

**修改 `src/services/api.ts`:**
添加 `checkHealth()` 方法

**修改 `src/stores/configStore.ts`:**
添加 `backendStatus` 状态：
```typescript
interface ConfigState {
  // ... 现有字段
  backendStatus: 'unknown' | 'connecting' | 'connected' | 'disconnected';
  checkBackendHealth: () => Promise<boolean>;
}
```

## 配置文件结构

`python-backend/data/config.json`:
```json
{
  "api_url": "http://localhost:11434",
  "api_key": "",
  "model_name": "qwen2.5:7b",
  "tts_enabled": true,
  "tts_engine": "gpt_sovits",
  "tts_volume": 0.8,
  // ... 完整配置
}
```

## 验收标准
1. `GET http://localhost:8765/api/config` 返回配置
2. `PUT http://localhost:8765/api/config` 能保存配置
3. 前端主页显示后端连接状态
4. 点击"测试连接"能正确检测

## 注意事项
- 配置文件不存在时自动创建默认配置
- 使用 Pydantic 验证配置格式
- 前端使用 react-query 或直接 fetch
```

---

# Phase 3: Ollama 对话 + 流式响应

## 目标
- 实现 Ollama API 集成
- 支持流式响应（SSE）
- 前端能发送消息并接收流式回复

## 文件操作

### 创建
- `python-backend/app/routers/chat.py` - 对话 API
- `python-backend/app/services/ollama.py` - Ollama 客户端
- `python-backend/app/routers/stream.py` - 流式事件 API

### 修改
- `src/services/stream.ts` - 确保与后端接口匹配
- `src/stores/streamStore.ts` - 连接后端

## 验收标准
- [ ] `POST /api/chat` 能调用 Ollama 并流式返回
- [ ] `GET /api/stream/history` 返回历史消息
- [ ] 前端能实时显示 AI 回复

## Phase 3 提示词

```
# Phase 3: Ollama 对话 + 流式响应

## 项目背景
继续重构项目。Phase 1-2 已完成 FastAPI 骨架、健康检查和配置 API。

## 工作目录
I:\BaiduNetdiskDownload\ai\react

## 当前状态
- FastAPI 后端运行正常
- 配置读写 API 已实现
- 前端能检测后端状态

## 本阶段任务

### 1. 实现 Ollama 客户端

**创建 `python-backend/app/services/ollama.py`:**

```python
import httpx
from typing import AsyncGenerator

class OllamaClient:
    def __init__(self, base_url: str = "http://localhost:11434"):
        self.base_url = base_url

    async def chat_stream(
        self,
        messages: list[dict],
        model: str = "qwen2.5:7b"
    ) -> AsyncGenerator[str, None]:
        """流式对话"""
        async with httpx.AsyncClient() as client:
            async with client.stream(
                "POST",
                f"{self.base_url}/api/chat",
                json={"model": model, "messages": messages, "stream": True},
                timeout=60.0
            ) as response:
                async for line in response.aiter_lines():
                    if line:
                        data = json.loads(line)
                        if "message" in data:
                            yield data["message"]["content"]

    async def test_connection(self) -> bool:
        """测试连接"""
        try:
            async with httpx.AsyncClient() as client:
                response = await client.get(f"{self.base_url}/api/tags", timeout=5.0)
                return response.status_code == 200
        except:
            return False
```

### 2. 实现对话 API

**创建 `python-backend/app/routers/chat.py`:**

端点：
- `POST /api/chat` - 发送消息，流式响应
- `GET /api/chat/test` - 测试 Ollama 连接

请求体：
```json
{
  "message": "你好",
  "system_prompt": "你是一个助手",
  "history": [
    {"role": "user", "content": "之前的消息"},
    {"role": "assistant", "content": "之前的回复"}
  ]
}
```

流式响应（SSE）：
```
data: {"type": "content", "content": "你"}
data: {"type": "content", "content": "好"}
data: {"type": "content", "content": "！"}
data: {"type": "done", "total_tokens": 10}
```

### 3. 实现历史记录 API

**创建 `python-backend/app/routers/stream.py`:**

端点：
- `GET /api/stream/history` - 获取历史消息
- `POST /api/stream/clear` - 清空历史
- `GET /api/stream/events` - SSE 事件流

历史消息存储在内存中（简化实现）。

### 4. 前端集成

**检查 `src/services/stream.ts`:**
确保 API 调用与后端匹配。

**检查 `src/stores/streamStore.ts`:**
确保状态管理正确。

## 技术要点

### SSE 流式响应
```python
from fastapi import FastAPI
from fastapi.responses import StreamingResponse

@app.post("/api/chat")
async def chat(request: ChatRequest):
    async def generate():
        async for chunk in ollama_client.chat_stream(messages):
            yield f"data: {json.dumps({'type': 'content', 'content': chunk})}\n\n"
        yield f"data: {json.dumps({'type': 'done'})}\n\n"

    return StreamingResponse(generate(), media_type="text/event-stream")
```

### 前端 SSE 处理
```typescript
const response = await fetch('/api/chat', {
  method: 'POST',
  body: JSON.stringify({ message }),
});

const reader = response.body?.getReader();
const decoder = new TextDecoder();

while (reader) {
  const { done, value } = await reader.read();
  if (done) break;

  const text = decoder.decode(value);
  // 解析 SSE 数据...
}
```

## 依赖更新

`requirements.txt` 添加：
```
httpx>=0.26.0
```

## 验收标准
1. `POST /api/chat` 能与 Ollama 对话
2. 流式响应正常工作
3. 前端能实时显示 AI 回复
4. 历史记录能保存和清除

## 注意事项
- 处理 Ollama 不可用的情况
- 超时设置合理（60秒）
- 错误信息友好
```

---

# Phase 4: 角色管理 API

## 目标
- 实现角色 CRUD API
- 支持角色配置持久化
- 前端角色管理功能连接后端

## Phase 4 提示词

```
# Phase 4: 角色管理 API

## 项目背景
继续重构项目。Phase 1-3 已完成基础架构、配置和对话功能。

## 工作目录
I:\BaiduNetdiskDownload\ai\react

## 本阶段任务

### 1. 实现角色 API

**创建 `python-backend/app/routers/characters.py`:**

端点：
- `GET /api/characters` - 获取所有角色
- `POST /api/characters` - 添加角色
- `GET /api/characters/{id}` - 获取单个角色
- `PUT /api/characters/{id}` - 更新角色
- `DELETE /api/characters/{id}` - 删除角色
- `POST /api/characters/switch` - 切换当前角色

### 2. 角色数据模型

```python
class GSVCharacter(BaseModel):
    id: str
    name: str
    ref_audio_dir: str = ""
    ref_audio_json: str = ""
    emotion_audio_json: str = ""
    gpt_model_path: str = ""
    sovits_model_path: str = ""
```

### 3. 角色存储

角色配置存储在 `python-backend/data/characters/` 目录：
```
data/
├── config.json
└── characters/
    ├── char_001.json
    ├── char_002.json
    └── ...
```

### 4. 前端集成

确保 `src/pages/Voice.tsx` 中的角色管理功能调用正确的 API。

## 验收标准
1. 能创建、编辑、删除角色
2. 角色配置持久化到文件
3. 前端角色管理正常工作
```

---

# Phase 5: TTS 引擎集成

## 目标
- 实现 TTS 引擎抽象层
- 集成 GPT-SoVITS
- 集成讯飞 TTS（可选）

## Phase 5 提示词

```
# Phase 5: TTS 引擎集成

## 项目背景
继续重构项目。Phase 1-4 已完成基础功能。

## 工作目录
I:\BaiduNetdiskDownload\ai\react

## 本阶段任务

### 1. TTS 引擎抽象层

**创建 `python-backend/app/services/tts/base.py`:**

```python
from abc import ABC, abstractmethod

class TTSEngine(ABC):
    @abstractmethod
    async def synthesize(self, text: str, **kwargs) -> bytes:
        """合成语音，返回音频数据"""
        pass

    @abstractmethod
    async def test_connection(self) -> bool:
        """测试引擎连接"""
        pass
```

### 2. GPT-SoVITS 引擎

**创建 `python-backend/app/services/tts/gpt_sovits.py`:**

- 调用 GPT-SoVITS API
- 支持角色切换
- 支持情感参数

### 3. TTS API

**创建 `python-backend/app/routers/tts.py`:**

端点：
- `POST /api/tts/test` - 测试引擎
- `POST /api/tts/synthesize` - 合成语音
- `POST /api/tts/stop` - 停止播放
- `GET /api/tts/status` - 播放状态

### 4. 音频播放

- 合成的音频通过 HTTP 返回
- 前端使用 Web Audio API 播放

## 验收标准
1. 能测试 GPT-SoVITS 连接
2. 能合成语音并返回音频
3. 前端能播放合成的语音
```

---

# Phase 6: 记忆压缩系统

## Phase 6 提示词

```
# Phase 6: 记忆压缩系统

## 项目背景
继续重构项目。Phase 1-5 已完成核心功能。

## 工作目录
I:\BaiduNetdiskDownload\ai\react

## 本阶段任务

### 1. 记忆压缩 API

**创建 `python-backend/app/routers/memory.py`:**

端点：
- `GET /api/memory/status` - 获取压缩状态
- `POST /api/memory/compress` - 执行压缩
- `GET /api/memory/config` - 获取配置
- `PUT /api/memory/config` - 更新配置

### 2. 压缩服务

**创建 `python-backend/app/services/memory_compressor.py`:**

- 调用 LLM 压缩历史对话
- 保留关键信息
- 支持自定义压缩提示词

## 验收标准
1. 能查看记忆压缩状态
2. 能执行压缩操作
3. 压缩配置可调整
```

---

# Phase 7: 清理与优化

## Phase 7 提示词

```
# Phase 7: 清理与优化

## 项目背景
重构基本完成。现在需要清理废弃代码和优化。

## 工作目录
I:\BaiduNetdiskDownload\ai\react

## 本阶段任务

### 1. 删除废弃文件
- `python-backend/gui.py`（如存在）
- 其他未使用的 Python 模块

### 2. 清理前端
- 移除 `/overlay` 路由（如不需要）
- 移除未使用的组件
- 移除未使用的 store selector

### 3. 优化
- 添加错误边界
- 添加加载状态
- 优化性能

### 4. 文档
- 更新 README
- 添加 API 文档

## 验收标准
1. 无废弃代码
2. 无 TypeScript 警告
3. 应用运行稳定
```

---

## 总体时间估算

| 阶段 | 预计时间 | 依赖 |
|------|----------|------|
| Phase 1 | 2-3 小时 | 无 |
| Phase 2 | 3-4 小时 | Phase 1 |
| Phase 3 | 4-6 小时 | Phase 2 |
| Phase 4 | 2-3 小时 | Phase 2 |
| Phase 5 | 4-6 小时 | Phase 4 |
| Phase 6 | 3-4 小时 | Phase 3 |
| Phase 7 | 2-3 小时 | Phase 1-6 |
| **总计** | **20-29 小时** | |

---

## 使用说明

1. 每个阶段开始新对话
2. 复制对应的提示词
3. 完成后验证验收标准
4. 确认通过后进入下一阶段

## 注意事项

- 每阶段完成后进行 git commit
- 遇到问题及时记录
- 保持代码风格一致
