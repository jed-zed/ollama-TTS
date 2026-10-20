# Ollama TTS Adapter

Ollama TTS Adapter 是一个基于 React、Vite、Rust 和 Tauri 2 的 Windows 桌面应用，提供 Ollama/OpenAI 兼容代理、字幕、语音输入和 GPT-SoVITS 语音合成能力。

## 环境要求

- Node.js 22（Vite 7 要求 Node.js `20.19+` 或 `22.12+`）
- Rust stable `1.88+`（当前锁文件中的依赖要求）
- Windows + Visual Studio C++ Build Tools
- WebView2 Runtime

## 克隆与安装

`tauri-plugin-mcp` 是构建所需的 Git 子模块，请递归克隆：

```powershell
git clone --recurse-submodules https://github.com/jed-zed/ollama-TTS.git
cd ollama-TTS
npm ci
```

如果已经普通克隆，补齐子模块：

```powershell
git submodule update --init --recursive
```

## 开发与验证

```powershell
npm run dev
npm run check
npm run test:rust
```

`npm run check` 会运行 ESLint、TypeScript 类型检查和带锁文件的 Cargo 检查。

## 生产构建

构建前端资源：

```powershell
npm run build
```

构建并打包 Tauri 桌面应用：

```powershell
npm run tauri -- build
```

可执行文件与安装包会生成在 `src-tauri/target/release/` 和 `src-tauri/target/release/bundle/` 下。

## 运行时配置

程序从可执行文件同目录读取 `config.json`。文件不存在时使用内置默认配置，在应用中保存设置后写入配置文件。

运行时配置可能包含 API 密钥和本机路径，应保留在本机。仓库已忽略根目录的 `config.json`、个人工具配置及参考资料；GitHub Actions 构建不需要个人 API 密钥。
