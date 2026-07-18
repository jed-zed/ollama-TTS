import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import { execFileSync, spawn } from 'node:child_process';
import fsSync from 'node:fs';
import fs from 'node:fs/promises';
import http from 'node:http';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..');
const releaseExe = path.join(repoRoot, 'src-tauri', 'target', 'release', 'app.exe');
const releaseDll = path.join(repoRoot, 'src-tauri', 'target', 'release', 'app_lib.dll');
const mainBase = 'http://127.0.0.1:8765';
const proxyBase = 'http://127.0.0.1:11434';
const mockBase = 'http://127.0.0.1:19880';
const cdpBase = 'http://127.0.0.1:19222';
const burstContent = '字幕帧合并增量🙂'.repeat(8);
const burstWireContent = JSON.stringify({ tts_config: {}, response: burstContent });

const cases = [];
const appLogs = [];
const mockRequests = [];
let tempDir;
let appProcess;
let mockServer;
let cdp;
let cdpTarget;
let cdpTargets = [];
let overlayCdp;

function redact(value) {
  return String(value).replaceAll('functional-test-key', '[REDACTED]');
}

function withTimeout(promise, timeoutMs, label) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label} timed out after ${timeoutMs}ms`)), timeoutMs);
    }),
  ]).finally(() => clearTimeout(timer));
}

async function runCase(name, fn) {
  const started = Date.now();
  try {
    await fn();
    const elapsedMs = Date.now() - started;
    cases.push({ name, status: 'PASS', elapsedMs });
    console.log(`PASS  ${name} (${elapsedMs}ms)`);
  } catch (error) {
    const elapsedMs = Date.now() - started;
    const message = redact(error?.stack ?? error);
    cases.push({ name, status: 'FAIL', elapsedMs, error: message });
    console.log(`FAIL  ${name} (${elapsedMs}ms)`);
    console.log(`      ${message.split('\n').join('\n      ')}`);
  }
}

async function assertPortFree(port) {
  const server = net.createServer();
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', resolve);
  });
  await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
}

async function waitFor(predicate, { timeoutMs = 10_000, intervalMs = 100, label = 'condition' } = {}) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const value = await predicate();
      if (value) return value;
    } catch (error) {
      lastError = error;
    }
    await delay(intervalMs);
  }
  throw new Error(`${label} not reached${lastError ? `: ${lastError.message}` : ''}`);
}

async function request(base, pathname, { method = 'GET', body, headers = {} } = {}) {
  const requestHeaders = { ...headers };
  let payload;
  if (body !== undefined) {
    requestHeaders['content-type'] ??= 'application/json';
    payload = typeof body === 'string' ? body : JSON.stringify(body);
  }
  const response = await fetch(`${base}${pathname}`, { method, headers: requestHeaders, body: payload });
  const text = await response.text();
  let json;
  try {
    json = JSON.parse(text);
  } catch {
    json = undefined;
  }
  return { status: response.status, headers: response.headers, text, json };
}

function rawRequest({ port = 8765, pathname = '/health', method = 'GET', headers = {}, body }) {
  return new Promise((resolve, reject) => {
    const req = http.request({ hostname: '127.0.0.1', port, path: pathname, method, headers }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => resolve({
        status: res.statusCode,
        headers: res.headers,
        text: Buffer.concat(chunks).toString('utf8'),
      }));
    });
    req.once('error', reject);
    if (body) req.write(body);
    req.end();
  });
}

function listenerRows(port) {
  const command = [
    `$items = @(Get-NetTCPConnection -State Listen -LocalPort ${port} -ErrorAction SilentlyContinue |`,
    'Select-Object LocalAddress,LocalPort,OwningProcess);',
    'ConvertTo-Json -Compress -InputObject $items',
  ].join(' ');
  const output = execFileSync('powershell.exe', ['-NoProfile', '-Command', command], {
    encoding: 'utf8',
    windowsHide: true,
  }).trim();
  if (!output) return [];
  const value = JSON.parse(output);
  return Array.isArray(value) ? value : [value];
}

async function waitForPortClosed(port) {
  await waitFor(() => listenerRows(port).length === 0, {
    timeoutMs: 8_000,
    intervalMs: 150,
    label: `port ${port} to close`,
  });
}

function parseSseBlock(block) {
  const data = [];
  for (const line of block.split('\n')) {
    if (line.startsWith(':')) continue;
    if (line === 'data') data.push('');
    if (line.startsWith('data:')) data.push(line.slice(5).replace(/^ /, ''));
  }
  return data.length ? data.join('\n') : null;
}

async function collectSse(url, {
  method = 'GET',
  body,
  headers = {},
  timeoutMs = 5_000,
  until = (data) => data === '[DONE]',
} = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(new Error(`SSE timeout after ${timeoutMs}ms`)), timeoutMs);
  const requestHeaders = { ...headers };
  let payload;
  if (body !== undefined) {
    requestHeaders['content-type'] ??= 'application/json';
    payload = JSON.stringify(body);
  }

  const events = [];
  try {
    const response = await fetch(url, {
      method,
      headers: requestHeaders,
      body: payload,
      signal: controller.signal,
    });
    assert.equal(response.status, 200, `Unexpected SSE status: ${response.status}`);
    assert.ok(response.body, 'SSE response body is missing');
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    while (true) {
      const { value, done } = await reader.read();
      if (value) buffer += decoder.decode(value, { stream: !done });
      buffer = buffer.replace(/\r\n/g, '\n');

      let boundary;
      while ((boundary = buffer.indexOf('\n\n')) >= 0) {
        const block = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary + 2);
        const data = parseSseBlock(block);
        if (data === null) continue;
        events.push(data);
        if (until(data, events)) {
          await reader.cancel();
          return events;
        }
      }

      if (done) {
        const data = parseSseBlock(buffer);
        if (data !== null) events.push(data);
        if (events.some((entry, index) => until(entry, events.slice(0, index + 1)))) return events;
        throw new Error(`SSE ended before expected event; received ${events.length} events`);
      }
    }
  } finally {
    clearTimeout(timer);
    controller.abort();
  }
}

function jsonEvents(events) {
  return events
    .filter((entry) => entry !== '[DONE]')
    .map((entry) => JSON.parse(entry));
}

function reconstructSubtitlePayloads(payloads) {
  let text = '';
  for (const payload of payloads) {
    if (payload.reset || payload.text !== undefined) text = payload.text ?? '';
    else text += payload.delta ?? '';
  }
  return text;
}

function makeWav() {
  const samples = 800;
  const dataSize = samples * 2;
  const buffer = Buffer.alloc(44 + dataSize);
  buffer.write('RIFF', 0);
  buffer.writeUInt32LE(36 + dataSize, 4);
  buffer.write('WAVE', 8);
  buffer.write('fmt ', 12);
  buffer.writeUInt32LE(16, 16);
  buffer.writeUInt16LE(1, 20);
  buffer.writeUInt16LE(1, 22);
  buffer.writeUInt32LE(16_000, 24);
  buffer.writeUInt32LE(32_000, 28);
  buffer.writeUInt16LE(2, 32);
  buffer.writeUInt16LE(16, 34);
  buffer.write('data', 36);
  buffer.writeUInt32LE(dataSize, 40);
  return buffer;
}

async function readRequestBody(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const text = Buffer.concat(chunks).toString('utf8');
  if (!text) return { text, json: undefined };
  try {
    return { text, json: JSON.parse(text) };
  } catch {
    return { text, json: undefined };
  }
}

function sendJson(res, status, value) {
  const payload = Buffer.from(JSON.stringify(value));
  res.writeHead(status, { 'content-type': 'application/json; charset=utf-8', 'content-length': payload.length });
  res.end(payload);
}

async function sendFragmented(res, payload) {
  const buffer = Buffer.from(payload, 'utf8');
  const emoji = Buffer.from('🙂');
  const emojiAt = buffer.indexOf(emoji);
  const cuts = emojiAt >= 0
    ? [Math.max(1, emojiAt + 2), Math.max(2, emojiAt + 3), buffer.length - 1]
    : [Math.floor(buffer.length / 3), Math.floor(buffer.length * 2 / 3), buffer.length - 1];
  let offset = 0;
  for (const cut of cuts) {
    if (cut > offset) res.write(buffer.subarray(offset, cut));
    offset = cut;
    await delay(8);
  }
  if (offset < buffer.length) res.write(buffer.subarray(offset));
  res.end();
}

async function handleMockRequest(req, res) {
  const url = new URL(req.url, mockBase);
  if (req.method === 'GET' && ['/health', '/tts', '/set_gpt_weights', '/set_sovits_weights'].includes(url.pathname)) {
    if (url.pathname === '/tts') {
      const wav = makeWav();
      res.writeHead(200, { 'content-type': 'audio/wav', 'content-length': wav.length });
      res.end(wav);
    } else {
      sendJson(res, 200, { ok: true });
    }
    return;
  }

  const parsed = await readRequestBody(req);
  mockRequests.push({
    method: req.method,
    pathname: url.pathname,
    search: url.search,
    hasAuthorization: Boolean(req.headers.authorization),
    remotePort: req.socket.remotePort,
    json: parsed.json,
  });

  if (req.method === 'POST' && url.pathname === '/tts') {
    if (parsed.json?.text?.includes('慢速')) await delay(800);
    const wav = makeWav();
    res.writeHead(200, { 'content-type': 'audio/wav', 'content-length': wav.length });
    res.end(wav);
    return;
  }

  if (req.method === 'POST' && url.pathname === '/v1/chat/completions') {
    const model = parsed.json?.model ?? '';
    if (model === 'mock_error') {
      sendJson(res, 502, { error: { message: 'mock upstream failure' } });
      return;
    }

    if (parsed.json?.stream) {
      if (model === 'mock_burst') {
        res.writeHead(200, { 'content-type': 'text/event-stream; charset=utf-8', 'cache-control': 'no-cache' });
        const events = [...burstWireContent]
          .map((content) => `data: ${JSON.stringify({ choices: [{ delta: { content } }] })}\n\n`)
          .join('');
        res.end(`${events}data: [DONE]\n\n`);
      } else if (model === 'mock_ndjson') {
        res.writeHead(200, { 'content-type': 'application/x-ndjson; charset=utf-8' });
        await sendFragmented(res,
          '{"message":{"content":"ND"}}\r\n' +
          '{"response":"JSON🙂"}\n' +
          '{"done":true}');
      } else {
        res.writeHead(200, { 'content-type': 'text/event-stream; charset=utf-8', 'cache-control': 'no-cache' });
        await sendFragmented(res,
          'data: {"choices":[{"delta":{"content":"你"}}]}\r\n\r\n' +
          'data: {"choices":[\r\n' +
          'data: {"delta":{"content":"🙂"}}]}\r\n\r\n' +
          'data:{"choices":[{"delta":{"content":"好"}}]}\r\n\r\n' +
          'data: [DONE]');
      }
      return;
    }

    const prompt = parsed.json?.messages?.at(-1)?.content ?? parsed.json?.prompt ?? '';
    const content = /压缩|整理|摘要/.test(prompt) ? '测试摘要：已完整压缩历史。' : 'mock nonstream 你好';
    sendJson(res, 200, {
      id: 'mock-chatcmpl',
      object: 'chat.completion',
      choices: [{ index: 0, message: { role: 'assistant', content }, finish_reason: 'stop' }],
    });
    return;
  }

  sendJson(res, 404, { error: 'mock route not found' });
}

async function startMockServer() {
  mockServer = http.createServer((req, res) => {
    handleMockRequest(req, res).catch((error) => {
      if (!res.headersSent) sendJson(res, 500, { error: error.message });
      else res.destroy(error);
    });
  });
  await new Promise((resolve, reject) => {
    mockServer.once('error', reject);
    mockServer.listen(19880, '127.0.0.1', resolve);
  });
}

class CdpClient {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    this.exceptions = [];
    this.consoleErrors = [];
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(typeof event.data === 'string' ? event.data : Buffer.from(event.data).toString('utf8'));
      if (message.id) {
        const pending = this.pending.get(message.id);
        if (!pending) return;
        this.pending.delete(message.id);
        if (message.error) pending.reject(new Error(message.error.message));
        else pending.resolve(message.result);
      } else if (message.method === 'Runtime.exceptionThrown') {
        this.exceptions.push(message.params.exceptionDetails?.text ?? 'unknown exception');
      } else if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
        this.consoleErrors.push(message.params.args?.map((arg) => arg.value ?? arg.description).join(' ') ?? 'console error');
      }
    });
  }

  static async connect(url) {
    const socket = new WebSocket(url);
    await withTimeout(new Promise((resolve, reject) => {
      socket.addEventListener('open', resolve, { once: true });
      socket.addEventListener('error', () => reject(new Error('CDP WebSocket connection failed')), { once: true });
    }), 5_000, 'CDP WebSocket');
    return new CdpClient(socket);
  }

  send(method, params = {}) {
    const id = this.nextId++;
    return withTimeout(new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    }), 8_000, `CDP ${method}`);
  }

  async evaluate(expression) {
    const result = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.text ?? 'CDP evaluation failed');
    return result.result?.value;
  }

  close() {
    this.socket.close();
  }
}

async function connectWebViewCdp() {
  const selected = await waitFor(async () => {
    const response = await fetch(`${cdpBase}/json/list`);
    if (!response.ok) return null;
    cdpTargets = (await response.json()).filter((target) => target.type === 'page');

    const ordered = [...cdpTargets].sort((left, right) => {
      const leftOverlay = /#\/overlay/.test(left.url) ? 1 : 0;
      const rightOverlay = /#\/overlay/.test(right.url) ? 1 : 0;
      return leftOverlay - rightOverlay;
    });
    for (const target of ordered) {
      const candidate = await CdpClient.connect(target.webSocketDebuggerUrl);
      await candidate.send('Runtime.enable');
      const runtime = await candidate.evaluate(`({
        url: location.href,
        tauri: '__TAURI_INTERNALS__' in window || '__TAURI__' in window
      })`);
      if (runtime.tauri && !/#\/overlay/.test(runtime.url)) {
        return { target, candidate };
      }
      candidate.close();
    }
    return null;
  }, { timeoutMs: 15_000, intervalMs: 200, label: 'Tauri WebView CDP target' });
  cdpTarget = selected.target;
  cdp = selected.candidate;
  await cdp.send('Page.enable');
  await cdp.send('Log.enable');
  return cdp;
}

async function connectOverlayWebViewCdp() {
  return waitFor(async () => {
    const response = await fetch(`${cdpBase}/json/list`);
    if (!response.ok) return null;
    const targets = (await response.json()).filter((target) => target.type === 'page' && /#\/overlay/.test(target.url));
    for (const target of targets) {
      const candidate = await CdpClient.connect(target.webSocketDebuggerUrl);
      await candidate.send('Runtime.enable');
      const runtime = await candidate.evaluate(`({
        url: location.href,
        tauri: '__TAURI_INTERNALS__' in window || '__TAURI__' in window
      })`);
      if (runtime.tauri && /#\/overlay/.test(runtime.url)) return candidate;
      candidate.close();
    }
    return null;
  }, { timeoutMs: 10_000, intervalMs: 200, label: 'subtitle overlay CDP target' });
}

async function waitForUiText(text, { present = true, timeoutMs = 8_000 } = {}) {
  try {
    return await waitFor(async () => {
      const found = await cdp.evaluate(`document.body?.innerText.includes(${JSON.stringify(text)}) ?? false`);
      return found === present;
    }, { timeoutMs, intervalMs: 100, label: `UI text ${present ? 'present' : 'absent'}: ${text}` });
  } catch (error) {
    const snapshot = await cdp.evaluate(`({ title: document.title, url: location.href, text: (document.body?.innerText ?? '').slice(0, 1000) })`);
    throw new Error(`${error.message}; UI snapshot=${JSON.stringify(snapshot)}`);
  }
}

async function clickUiText(selector, text) {
  const result = await cdp.evaluate(`(() => {
    const node = [...document.querySelectorAll(${JSON.stringify(selector)})]
      .find((element) => element.textContent?.trim().includes(${JSON.stringify(text)}));
    if (!node) return false;
    node.click();
    return true;
  })()`);
  assert.equal(result, true, `UI element not found: ${selector} containing ${text}`);
}

async function createFixture() {
  assert.ok(fsSync.existsSync(releaseExe), `Release binary missing: ${releaseExe}`);
  tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'ollama-tts-e2e-'));
  const appPath = path.join(tempDir, 'app.exe');
  await fs.copyFile(releaseExe, appPath);
  if (fsSync.existsSync(releaseDll)) await fs.copyFile(releaseDll, path.join(tempDir, 'app_lib.dll'));

  const characterDir = path.join(tempDir, 'character_configs');
  const ttsRefDir = path.join(tempDir, 'ttsrefs');
  const saveDir = path.join(tempDir, 'save_data');
  const backupDir = path.join(tempDir, 'backups');
  const audioOutputDir = path.join(tempDir, 'audio_output');
  await Promise.all([characterDir, ttsRefDir, saveDir, backupDir, audioOutputDir].map((dir) => fs.mkdir(dir, { recursive: true })));

  const refAudio = path.join(ttsRefDir, 'reference.wav');
  await fs.writeFile(refAudio, makeWav());
  const emotionJson = path.join(tempDir, 'emotions.json');
  await fs.writeFile(emotionJson, JSON.stringify({
    emotions: {
      neutral: {
        emotion_name: '中性',
        total_samples: 1,
        primary: {
          audio_file: 'reference.wav',
          text: '这是测试参考音频',
          full_path: refAudio,
          confidence: 1,
        },
        secondary: [],
      },
    },
  }, null, 2));

  const saveFile = path.join(saveDir, 'test-character.json');
  const originalSave = {
    Name: '测试角色',
    PlayerInfo: { RealName: 'Player' },
    ConversationHistory: [
      'Player: 第一轮问题', '测试角色: 第一轮回答',
      'Player: 第二轮问题', '测试角色: 第二轮回答',
      'Player: 第三轮问题', '测试角色: 第三轮回答',
      'Player: 第四轮问题', '测试角色: 第四轮回答',
    ],
    future_save_marker: { keep: true },
  };
  await fs.writeFile(saveFile, JSON.stringify(originalSave, null, 2));

  const config = {
    main_api_port: 8765,
    api_url: `${mockBase}/v1/chat/completions`,
    api_target_confirmation_required: false,
    api_key: 'functional-test-key',
    model_name: 'mock_sse',
    system_prompt_injection: '',
    current_api_preset: 'custom',
    custom_api_url: `${mockBase}/v1/chat/completions`,
    custom_api_key: 'functional-test-key',
    custom_model_name: 'mock_sse',
    tts_enabled: true,
    gsv_api_url: `${mockBase}/tts`,
    tts_auto_play: true,
    tts_save_audio: false,
    tts_output_dir: audioOutputDir,
    tts_default_character: 'test_voice',
    tts_volume: 0.8,
    gsv_current_character: 'test_voice',
    gsv_default_female_character: 'test_voice',
    gsv_use_server_path: false,
    gsv_min_ref_audios: 1,
    gsv_max_ref_audios: 1,
    gsv_characters: {
      test_voice: {
        name: '测试音色',
        ref_audio_dir: ttsRefDir,
        emotion_audio_json: emotionJson,
        gpt_model_path: '',
        sovits_model_path: '',
        future_character_marker: 'keep',
      },
    },
    lan_access_enabled: false,
    history_compression: {
      enabled: true,
      save_data_path: saveDir,
      max_messages: 2,
      retain_rounds: 1,
      compress_count: 2,
      check_interval: 30,
      backup_before_compress: true,
      backup_dir: backupDir,
      custom_prompt: '',
      summary_ratio_threshold: 0.9,
      future_history_marker: 'keep',
    },
    antigravity_bridge_enabled: false,
    antigravity_timeout_ms: 999,
    antigravity_plugin_custom: 'preserve-extension',
    future_test_marker: { keep: true },
  };
  await fs.writeFile(path.join(tempDir, 'config.json'), JSON.stringify(config, null, 2));

  return { appPath, characterDir, saveDir, saveFile, backupDir, audioOutputDir, originalSave };
}

async function startApp(appPath) {
  appProcess = spawn(appPath, [], {
    cwd: tempDir,
    windowsHide: true,
    env: {
      ...process.env,
      RUST_BACKTRACE: '1',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=19222',
      WEBVIEW2_USER_DATA_FOLDER: path.join(tempDir, 'webview2'),
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  for (const stream of [appProcess.stdout, appProcess.stderr]) {
    stream?.on('data', (chunk) => {
      appLogs.push(redact(chunk.toString('utf8')));
      if (appLogs.length > 200) appLogs.shift();
    });
  }
  appProcess.once('error', (error) => appLogs.push(`process error: ${redact(error.message)}`));

  await waitFor(async () => {
    if (appProcess.exitCode !== null) throw new Error(`app.exe exited with ${appProcess.exitCode}`);
    const result = await request(mainBase, '/health');
    return result.status === 200 && result.json?.status === 'ok';
  }, { timeoutMs: 15_000, intervalMs: 150, label: 'release app health' });
  await delay(400);
}

async function cleanup() {
  try { overlayCdp?.close(); } catch {}
  try { cdp?.close(); } catch {}
  if (appProcess && appProcess.exitCode === null) {
    try {
      execFileSync('taskkill.exe', ['/PID', String(appProcess.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' });
    } catch {}
  }
  if (mockServer) {
    await new Promise((resolve) => mockServer.close(() => resolve()));
  }
  try { await waitForPortClosed(8765); } catch {}
  try { await waitForPortClosed(11434); } catch {}
  if (tempDir) {
    const resolved = path.resolve(tempDir);
    const tempRoot = `${path.resolve(os.tmpdir())}${path.sep}`;
    if (resolved.startsWith(tempRoot) && path.basename(resolved).startsWith('ollama-tts-e2e-')) {
      await fs.rm(resolved, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
    } else {
      throw new Error(`Refusing to remove unexpected path: ${resolved}`);
    }
  }
}

async function main() {
  for (const port of [8765, 11434, 19880, 19222]) await assertPortFree(port);
  const fixture = await createFixture();
  await startMockServer();
  await startApp(fixture.appPath);
  try {
    await connectWebViewCdp();
  } catch (error) {
    appLogs.push(`CDP setup failed: ${error.message}`);
  }

  await runCase('发行版进程、健康检查与真实 Tauri WebView2', async () => {
    assert.equal(appProcess.exitCode, null);
    const health = await request(mainBase, '/health');
    assert.equal(health.status, 200);
    assert.equal(health.json.status, 'ok');
    assert.match(health.json.version, /^\d+\.\d+\.\d+/);
    const listeners = listenerRows(8765);
    assert.ok(listeners.some((row) => row.LocalAddress === '127.0.0.1'));
    assert.ok(listeners.every((row) => row.LocalAddress !== '0.0.0.0'));
    assert.ok(cdp, `Tauri WebView CDP unavailable: ${appLogs.at(-1) ?? 'no details'}`);
    const runtime = await cdp.evaluate(`({
      title: document.title,
      url: location.href,
      tauri: '__TAURI_INTERNALS__' in window || '__TAURI__' in window,
      tauriInternalType: typeof window.__TAURI_INTERNALS__,
      text: document.body?.innerText ?? ''
    })`);
    assert.equal(runtime.tauri, true, `runtime=${JSON.stringify(runtime)} targets=${JSON.stringify(cdpTargets)}`);
    assert.match(runtime.title, /Pixso|Ollama TTS Adapter/i);
    assert.match(runtime.url, /tauri|localhost/i);
    assert.ok(runtime.text.includes('O2O Adapter'));
    const binaryHash = crypto.createHash('sha256').update(await fs.readFile(fixture.appPath)).digest('hex');
    assert.equal(binaryHash.length, 64);
  });

  await runCase('本地控制 API Host、Origin、CORS 与监听边界', async () => {
    const valid = await rawRequest({ headers: { Host: '127.0.0.1:8765' } });
    assert.equal(valid.status, 200);
    for (const host of ['example.com', 'localhost.example.com', '127.0.0.1.example.com']) {
      const rejected = await rawRequest({ headers: { Host: host } });
      assert.equal(rejected.status, 403, `Host should be rejected: ${host}`);
    }
    const evilOrigin = await rawRequest({ headers: { Host: '127.0.0.1:8765', Origin: 'https://example.com' } });
    assert.equal(evilOrigin.status, 403);
    const trustedOrigin = await rawRequest({ headers: { Host: '127.0.0.1:8765', Origin: 'http://127.0.0.1:1420' } });
    assert.equal(trustedOrigin.status, 200);
    assert.equal(trustedOrigin.headers['access-control-allow-origin'], 'http://127.0.0.1:1420');
    const preflight = await rawRequest({
      method: 'OPTIONS',
      pathname: '/api/config',
      headers: {
        Host: '127.0.0.1:8765',
        Origin: 'tauri://localhost',
        'Access-Control-Request-Method': 'PUT',
        'Access-Control-Request-Headers': 'content-type',
      },
    });
    assert.ok([200, 204].includes(preflight.status));
    assert.equal(preflight.headers['access-control-allow-origin'], 'tauri://localhost');
    assert.notEqual(preflight.headers['access-control-allow-origin'], '*');
  });

  await runCase('Antigravity 迁移清理与并发原子配置写入', async () => {
    const configPath = path.join(tempDir, 'config.json');
    const migrated = JSON.parse(await fs.readFile(configPath, 'utf8'));
    assert.equal(migrated.antigravity_bridge_enabled, undefined);
    assert.equal(migrated.antigravity_timeout_ms, undefined);
    assert.equal(migrated.antigravity_plugin_custom, 'preserve-extension');
    assert.deepEqual(migrated.future_test_marker, { keep: true });
    assert.equal(migrated.gsv_characters.test_voice.future_character_marker, 'keep');
    assert.equal(migrated.history_compression.future_history_marker, 'keep');

    const updates = [
      ['subtitle_font_size', 31],
      ['subtitle_opacity', 0.77],
      ['subtitle_typing_speed', 17],
      ['subtitle_auto_hide_delay', 4321],
      ['voice_language', 'zh-TW'],
      ['paste_mode', 'direct'],
      ['fallback_trigger_threshold', 123],
      ['fallback_pattern', '^TEST$'],
      ['voice_shortcut_enabled', false],
      ['custom_font_colors', { functional: '#123456' }],
    ];
    let reading = true;
    const parseErrors = [];
    const reader = (async () => {
      while (reading) {
        try { JSON.parse(await fs.readFile(configPath, 'utf8')); } catch (error) { parseErrors.push(error.message); }
        await delay(2);
      }
    })();
    const responses = await Promise.all(updates.map(([key, value]) => request(mainBase, '/api/config', {
      method: 'PUT', body: { [key]: value },
    })));
    reading = false;
    await reader;
    assert.ok(
      responses.every((response) => response.status === 200 && response.json?.success),
      JSON.stringify(responses.map((response, index) => ({
        key: updates[index][0], status: response.status, body: response.json ?? response.text,
      }))),
    );
    assert.deepEqual(parseErrors, []);
    const persisted = JSON.parse(await fs.readFile(configPath, 'utf8'));
    for (const [key, value] of updates) {
      if (key === 'subtitle_opacity') assert.ok(Math.abs(persisted[key] - value) < 1e-5, key);
      else assert.deepEqual(persisted[key], value, key);
    }
    assert.deepEqual(persisted.future_test_marker, { keep: true });
    assert.equal(persisted.gsv_characters.test_voice.future_character_marker, 'keep');
    assert.equal(persisted.history_compression.future_history_marker, 'keep');
    assert.equal(persisted.api_target_confirmation_required, false);
    const leftovers = (await fs.readdir(tempDir)).filter((name) => /^\.tmp/i.test(name));
    assert.deepEqual(leftovers, []);
  });

  await runCase('角色 CRUD、切换、重载、路径校验与并发无死锁', async () => {
    const invalid = await request(mainBase, '/api/characters', {
      method: 'POST',
      body: { character_id: '../escape', name: '非法', gender: 'female', ref_audio_dir: '', available_emotions: {} },
    });
    assert.equal(invalid.status, 400);

    const ids = Array.from({ length: 12 }, (_, index) => `e2e_role_${index}`);
    const additions = await withTimeout(Promise.all(ids.map((id, index) => request(mainBase, '/api/characters', {
      method: 'POST',
      body: {
        character_id: id,
        name: `测试角色${index}`,
        gender: index % 2 ? 'male' : 'female',
        ref_audio_dir: fixture.audioOutputDir,
        description: '并发角色测试',
        name_aliases: [`alias_${index}`],
        available_emotions: {},
      },
    }))), 8_000, 'concurrent character additions');
    assert.ok(additions.every((response) => response.status === 200 && response.json?.success));

    const list = await request(mainBase, '/api/characters');
    assert.equal(list.json.data.total, ids.length);
    const updated = await request(mainBase, `/api/characters/${ids[0]}`, {
      method: 'PUT', body: { description: '已更新', name_aliases: ['updated-alias'] },
    });
    assert.equal(updated.status, 200);
    const switched = await request(mainBase, '/api/characters/switch', {
      method: 'POST', body: { character_id: ids[0] },
    });
    assert.equal(switched.status, 200);
    const current = await request(mainBase, '/api/characters/current');
    assert.equal(current.json.data.character_id, ids[0]);
    const reloaded = await withTimeout(request(mainBase, '/api/characters/reload', { method: 'POST', body: {} }), 4_000, 'character reload');
    assert.equal(reloaded.status, 200);
    assert.equal((await request(mainBase, `/api/characters/${ids[0]}`)).json.data.description, '已更新');

    const persistedFiles = (await fs.readdir(fixture.characterDir)).filter((name) => name.endsWith('.json'));
    assert.ok(persistedFiles.length >= ids.length);
    for (const file of persistedFiles) JSON.parse(await fs.readFile(path.join(fixture.characterDir, file), 'utf8'));

    const deletions = await withTimeout(Promise.all(ids.map((id) => request(mainBase, `/api/characters/${id}`, { method: 'DELETE' }))), 8_000, 'concurrent character deletions');
    assert.ok(deletions.every((response) => response.status === 200 && response.json?.success));
    assert.equal((await request(mainBase, '/api/characters')).json.data.total, 0);
    assert.equal(fsSync.existsSync(path.join(tempDir, 'escape.json')), false);
  });

  await runCase('存档扫描、AI 压缩、备份与原子发布', async () => {
    const scan = await request(mainBase, '/api/memory/scan', { method: 'POST', body: {} });
    assert.equal(scan.status, 200);
    assert.equal(scan.json.data.count, 1);
    const statusBefore = await request(mainBase, '/api/memory/status');
    assert.equal(statusBefore.json.data.length, 1);
    assert.equal(statusBefore.json.data[0].needs_compression, true);

    const prompt = await request(mainBase, '/api/memory/prompt-template');
    assert.ok(prompt.json.data.template.includes('{conversation_content}'));
    const customPrompt = '请压缩 {character_name} 与 {player_name}: {conversation_content}';
    assert.equal((await request(mainBase, '/api/memory/prompt-template', {
      method: 'PUT', body: { template: customPrompt },
    })).json.data.template, customPrompt);
    assert.ok((await request(mainBase, '/api/memory/prompt-template/reset', { method: 'POST', body: {} })).json.data.template.length > 100);
    assert.equal((await request(mainBase, '/api/memory/config', { method: 'PUT', body: { max_messages: 0 } })).status, 400);

    const compressed = await request(mainBase, '/api/memory/compress', {
      method: 'POST', body: { file_path: fixture.saveFile, force: true },
    });
    assert.equal(compressed.status, 200, compressed.text);
    assert.equal(compressed.json.data.success, true);
    const saved = JSON.parse(await fs.readFile(fixture.saveFile, 'utf8'));
    assert.equal(saved.future_save_marker.keep, true);
    assert.ok(saved.ConversationHistory[0].startsWith('System: [历史摘要'));
    assert.ok(saved.ConversationHistory.length < fixture.originalSave.ConversationHistory.length);
    const backups = (await fs.readdir(fixture.backupDir)).filter((name) => name.endsWith('.json'));
    assert.equal(backups.length, 1);
    assert.deepEqual(JSON.parse(await fs.readFile(path.join(fixture.backupDir, backups[0]), 'utf8')), fixture.originalSave);

    const configUpdate = await request(mainBase, '/api/memory/config', {
      method: 'PUT', body: { max_messages: 100 },
    });
    assert.equal(configUpdate.status, 200);
    const batch = await request(mainBase, '/api/memory/compress-all', { method: 'POST', body: { force: false } });
    assert.equal(batch.status, 200);
    assert.equal(batch.json.data.compressed_count, 0);
    JSON.parse(await fs.readFile(fixture.saveFile, 'utf8'));
    const persistedConfig = JSON.parse(await fs.readFile(path.join(tempDir, 'config.json'), 'utf8'));
    assert.equal(persistedConfig.history_compression.future_history_marker, 'keep');
  });

  await runCase('日志列表、筛选、统计、导出与 SSE', async () => {
    assert.equal((await request(mainBase, '/api/logs', { method: 'DELETE' })).status, 200);
    const streamPromise = collectSse(`${mainBase}/api/logs/stream`, {
      timeoutMs: 5_000,
      until: (data) => {
        try { return JSON.parse(data).message?.includes('测试'); } catch { return false; }
      },
    });
    await delay(100);
    assert.equal((await request(mainBase, '/api/logs/test', { method: 'POST', body: {} })).status, 200);
    const streamEvents = await streamPromise;
    assert.ok(streamEvents.some((entry) => JSON.parse(entry).message.includes('测试')));
    const logs = await request(mainBase, '/api/logs?limit=200&page=1');
    assert.equal(logs.json.success, true);
    assert.ok(logs.json.total >= 5);
    const errors = await request(mainBase, '/api/logs?level=ERROR&limit=200&page=1');
    assert.ok(errors.json.data.every((entry) => entry.level === 'ERROR'));
    const stats = await request(mainBase, '/api/logs/stats');
    assert.equal(stats.json.success, true);
    assert.ok(stats.json.total > 0);
    const exportedJson = await request(mainBase, '/api/logs/export?format=json');
    assert.equal(exportedJson.status, 200);
    assert.ok(exportedJson.headers.get('content-disposition').includes('logs.json'));
    JSON.parse(exportedJson.text);
    const exportedCsv = await request(mainBase, '/api/logs/export?format=csv');
    assert.match(exportedCsv.text, /timestamp|level/i);
  });

  await runCase('TTS 校验、连接诊断、合成、请求级播放覆盖与锁状态', async () => {
    const status = await request(mainBase, '/api/tts/status');
    assert.equal(status.status, 200);
    assert.equal(status.json.enabled, true);
    assert.equal(status.json.engine_available, true);
    const engineTest = await request(mainBase, '/api/tts/test', {
      method: 'POST', body: { engine: 'gpt_sovits', url: `${mockBase}/tts` },
    });
    assert.equal(engineTest.status, 200);
    assert.equal(engineTest.json.available, true);
    const diagnose = await request(mainBase, '/api/tts/diagnose', {
      method: 'POST', body: { url: `${mockBase}/tts` },
    });
    assert.equal(diagnose.status, 200);
    assert.equal(diagnose.json.tcp_reachable, true);
    assert.equal((await request(mainBase, '/api/tts/test', {
      method: 'POST', body: { engine: 'unsupported', url: `${mockBase}/tts` },
    })).status, 400);
    assert.equal((await request(mainBase, '/api/tts/test', {
      method: 'POST', body: { engine: 'gpt_sovits', url: 'http://169.254.169.254/latest/meta-data' },
    })).status, 400);
    assert.equal((await request(mainBase, '/api/tts/synthesize', {
      method: 'POST', body: { text: '   ', speed: 1 },
    })).status, 400);
    assert.equal((await request(mainBase, '/api/tts/synthesize', {
      method: 'POST', body: { text: '测试', speed: 0 },
    })).status, 400);
    const volume = await request(mainBase, '/api/tts/volume', { method: 'POST', body: { volume: 1.7 } });
    assert.equal(volume.status, 200);
    assert.equal(volume.json.volume, 1);

    const synthesis = await request(mainBase, '/api/tts/synthesize', {
      method: 'POST',
      body: { text: '你好，功能测试。', speaker: 'test_voice', emotion: 'neutral', speed: 1.1, auto_play: false },
    });
    assert.equal(synthesis.status, 200, synthesis.text);
    assert.equal(synthesis.json.success, true);
    assert.equal(synthesis.json.speaker_used, 'test_voice');
    assert.equal(synthesis.json.segment_count, 1);
    const ttsRequest = mockRequests.filter((entry) => entry.pathname === '/tts' && entry.method === 'POST').at(-1);
    assert.ok(ttsRequest.json.ref_audio_path.endsWith('reference.wav'));
    assert.ok(Math.abs(ttsRequest.json.speed - 1.1) < 1e-5);
    assert.equal(ttsRequest.hasAuthorization, false, 'LLM Authorization header leaked into TTS request');
    assert.equal((await request(mainBase, '/api/tts/status')).json.queue_size, 0);

    const slowSynthesis = request(mainBase, '/api/tts/synthesize', {
      method: 'POST',
      body: { text: '慢速状态探测', speaker: 'test_voice', emotion: 'neutral', speed: 1, auto_play: false },
    });
    await delay(100);
    const statusStarted = Date.now();
    const during = await request(mainBase, '/api/tts/status');
    const statusElapsed = Date.now() - statusStarted;
    assert.equal(during.status, 200);
    assert.equal(during.json.is_synthesizing, true);
    assert.ok(statusElapsed < 600, `status probe blocked for ${statusElapsed}ms`);
    assert.equal((await slowSynthesis).status, 200);
    assert.equal((await request(mainBase, '/api/tts/stop', { method: 'POST', body: {} })).json.success, true);
    assert.equal((await request(mainBase, '/api/tts/stop-all', { method: 'POST', body: {} })).json.success, true);
    assert.deepEqual(await fs.readdir(fixture.audioOutputDir), []);
  });

  await runCase('前端 Chat SSE、上游 SSE/NDJSON 分片与应用层心跳', async () => {
    const sseChat = await collectSse(`${mainBase}/api/chat`, {
      method: 'POST',
      body: { model: 'mock_sse', stream: true, messages: [{ role: 'user', content: 'SSE 测试' }] },
      timeoutMs: 6_000,
    });
    const sseContent = jsonEvents(sseChat)
      .filter((event) => event.type === 'content')
      .map((event) => event.data.content)
      .join('');
    assert.equal(sseContent, '你🙂好');

    const ndjsonChat = await collectSse(`${mainBase}/api/chat`, {
      method: 'POST',
      body: { model: 'mock_ndjson', stream: true, messages: [{ role: 'user', content: 'NDJSON 测试' }] },
      timeoutMs: 6_000,
    });
    const ndjsonContent = jsonEvents(ndjsonChat)
      .filter((event) => event.type === 'content')
      .map((event) => event.data.content)
      .join('');
    assert.equal(ndjsonContent, 'NDJSON🙂');

    const nonStream = await collectSse(`${mainBase}/api/chat`, {
      method: 'POST',
      body: { model: 'mock_sse', stream: false, messages: [{ role: 'user', content: '非流式测试' }] },
      timeoutMs: 5_000,
    });
    assert.ok(jsonEvents(nonStream).some((event) => event.data?.content === 'mock nonstream 你好'));

    for (const marker of ['共享连接复用一', '共享连接复用二']) {
      await collectSse(`${mainBase}/api/chat`, {
        method: 'POST',
        body: { model: 'mock_sse', stream: false, messages: [{ role: 'user', content: marker }] },
        timeoutMs: 5_000,
      });
    }
    const reuseRequests = mockRequests.filter((entry) =>
      entry.pathname === '/v1/chat/completions'
      && ['共享连接复用一', '共享连接复用二'].includes(entry.json?.messages?.at(-1)?.content)
    );
    assert.equal(reuseRequests.length, 2);
    assert.equal(reuseRequests[0].hasAuthorization, true);
    assert.equal(reuseRequests[0].remotePort, reuseRequests[1].remotePort, 'sequential LLM calls did not reuse one TCP connection');

    const testStream = await request(mainBase, '/api/proxy/test-stream', {
      method: 'POST', body: { prompt: '分片测试' },
    });
    assert.equal(testStream.status, 200);
    assert.equal(testStream.json.data.response, '你🙂好');
    assert.equal((await request(mainBase, '/api/proxy/test', { method: 'POST', body: {} })).json.data.success, true);

    const streamEventsPromise = collectSse(`${mainBase}/api/stream/events`, {
      timeoutMs: 5_000,
      until: (data) => {
        try { return JSON.parse(data).type === 'stream_end'; } catch { return false; }
      },
    });
    await delay(150);
    const streamSend = await request(mainBase, '/api/stream/test', {
      method: 'POST', body: { content: '你🙂好' },
    });
    assert.equal(streamSend.status, 200);
    const streamEvents = jsonEvents(await streamEventsPromise);
    assert.ok(streamEvents[0].data.full_content !== undefined);
    assert.equal(streamEvents.filter((event) => event.type === 'content').map((event) => event.data.content ?? '').join(''), '你🙂好');
    assert.equal((await request(mainBase, '/api/stream/state')).json.data.content, '你🙂好');

    const heartbeat = jsonEvents(await collectSse(`${mainBase}/api/stream/events`, {
      timeoutMs: 18_000,
      until: (data) => {
        try { return JSON.parse(data).type === 'ping'; } catch { return false; }
      },
    }));
    assert.equal(heartbeat[0].type, 'content');
    assert.ok(heartbeat.some((event) => event.type === 'ping'));
    assert.equal((await request(mainBase, '/api/stream/clear', { method: 'POST', body: {} })).status, 200);
  });

  await runCase('Ollama/OpenAI 代理全路由、流式转码与 LAN 开关', async () => {
    const started = await request(mainBase, '/api/proxy/start', { method: 'POST', body: {} });
    assert.equal(started.status, 200);
    assert.equal(started.json.success, true, started.text);
    await waitFor(() => listenerRows(11434).length > 0, { label: 'proxy listener' });
    let listeners = listenerRows(11434);
    assert.ok(listeners.some((row) => row.LocalAddress === '127.0.0.1'));
    assert.equal((await request(proxyBase, '/')).text, 'Ollama is running');
    assert.match((await request(proxyBase, '/api/version')).json.version, /^\d+\.\d+\.\d+/);
    assert.ok((await request(proxyBase, '/api/tags')).json.models.length >= 4);
    assert.equal((await request(proxyBase, '/v1/models')).json.data[0].id, 'mock_sse');

    const ollamaChat = await request(proxyBase, '/api/chat', {
      method: 'POST',
      body: { model: 'mock_sse', stream: false, messages: [{ role: 'user', content: 'Ollama 非流式' }] },
    });
    assert.equal(ollamaChat.status, 200);
    assert.equal(ollamaChat.json.message.content, 'mock nonstream 你好');
    const ollamaStreamResponse = await fetch(`${proxyBase}/api/chat`, {
      method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ model: 'mock_sse', stream: true, messages: [{ role: 'user', content: 'Ollama 流式' }] }),
    });
    const ollamaLines = (await ollamaStreamResponse.text()).trim().split(/\r?\n/).map((line) => JSON.parse(line));
    assert.equal(ollamaLines.map((entry) => entry.message?.content ?? '').join(''), '你🙂好');
    assert.equal(ollamaLines.at(-1).done, true);

    const generated = await request(proxyBase, '/api/generate', {
      method: 'POST', body: { model: 'mock_sse', prompt: '生成测试', stream: false },
    });
    assert.equal(generated.status, 200);
    assert.equal(generated.json.response, 'mock nonstream 你好');
    const openAi = await request(proxyBase, '/v1/chat/completions', {
      method: 'POST',
      body: {
        model: 'mock_sse', stream: false, temperature: 0.2,
        messages: [{ role: 'user', content: 'OpenAI 非流式' }],
        functional_extra: { keep: true },
      },
    });
    assert.equal(openAi.status, 200);
    assert.equal(openAi.json.choices[0].message.content, 'mock nonstream 你好');
    const forwarded = mockRequests.filter((entry) => entry.pathname === '/v1/chat/completions').at(-1).json;
    assert.equal(forwarded.temperature, 0.2);
    assert.deepEqual(forwarded.functional_extra, { keep: true });
    const openAiStream = await collectSse(`${proxyBase}/v1/chat/completions`, {
      method: 'POST',
      body: { model: 'mock_sse', stream: true, messages: [{ role: 'user', content: 'OpenAI 流式' }] },
      timeoutMs: 6_000,
    });
    const rawContent = jsonEvents(openAiStream).map((event) => event.choices?.[0]?.delta?.content ?? '').join('');
    assert.equal(rawContent, '你🙂好');
    assert.ok([400, 500].includes((await request(proxyBase, '/v1/chat/completions', {
      method: 'POST', body: '{not json', headers: { 'content-type': 'application/json' },
    })).status));
    assert.equal((await request(proxyBase, '/unknown-route')).status, 404);

    assert.equal((await request(mainBase, '/api/proxy/stop', { method: 'POST', body: {} })).json.success, true);
    await waitForPortClosed(11434);
    assert.equal((await request(mainBase, '/api/config', { method: 'PUT', body: { lan_access_enabled: true } })).status, 200);
    assert.equal((await request(mainBase, '/api/proxy/start', { method: 'POST', body: {} })).json.success, true);
    await waitFor(() => listenerRows(11434).length > 0, { label: 'LAN proxy listener' });
    listeners = listenerRows(11434);
    assert.ok(listeners.some((row) => row.LocalAddress === '0.0.0.0'));
    assert.ok(listenerRows(8765).every((row) => row.LocalAddress === '127.0.0.1'));
    assert.equal((await request(mainBase, '/api/proxy/stop', { method: 'POST', body: {} })).json.success, true);
    await waitForPortClosed(11434);
    assert.equal((await request(mainBase, '/api/config', { method: 'PUT', body: { lan_access_enabled: false } })).status, 200);
  });

  await runCase('语音识别端点本地输入校验（不访问真实云服务）', async () => {
    const missingBaidu = await request(mainBase, '/api/speech/baidu/test', { method: 'POST', body: { api_key: '' } });
    assert.equal(missingBaidu.status, 422);
    const invalidXunfei = await request(mainBase, '/api/speech/xunfei/recognize', {
      method: 'POST', body: { audio_data: '%%%', app_id: 'test', api_key: 'test', api_secret: 'test' },
    });
    assert.equal(invalidXunfei.status, 200);
    assert.equal(invalidXunfei.json.success, false);
    assert.match(invalidXunfei.json.error, /decode/i);
  });

  await runCase('配置更新保持 SSE 长连接，字幕 IPC 增量与窗口就绪快照同步', async () => {
    assert.ok(cdp, 'Tauri WebView CDP was not connected');
    await cdp.send('Page.addScriptToEvaluateOnNewDocument', {
      source: `(() => {
        if (window.__functionalEventSourceWrapped || !window.EventSource) return;
        window.__functionalEventSourceWrapped = true;
        window.__functionalEventSourceCount = 0;
        const NativeEventSource = window.EventSource;
        window.EventSource = new Proxy(NativeEventSource, {
          construct(target, args) {
            window.__functionalEventSourceCount += 1;
            return Reflect.construct(target, args, target);
          }
        });
      })();`,
    });
    await cdp.send('Page.reload', { ignoreCache: true });
    await waitForUiText('核心服务已连接 :8765', { timeoutMs: 10_000 });
    const initialConnections = await waitFor(
      async () => {
        const count = await cdp.evaluate('window.__functionalEventSourceCount ?? 0');
        return count >= 1 ? count : null;
      },
      { timeoutMs: 8_000, intervalMs: 100, label: 'initial frontend EventSource' },
    );

    await clickUiText('a', '字幕视觉');
    await waitFor(async () => (await cdp.evaluate('location.hash')) === '#/subtitles', { label: 'subtitle settings route' });
    await waitForUiText('纯文字模式', { timeoutMs: 8_000 });
    const originalTextOnly = Boolean((await request(mainBase, '/api/config')).json.data.subtitle_text_only);
    await clickUiText('label', '纯文字模式');
    await waitFor(async () => Boolean((await request(mainBase, '/api/config')).json.data.subtitle_text_only) !== originalTextOnly, {
      timeoutMs: 5_000,
      label: 'subtitle config persistence',
    });
    await delay(300);
    assert.equal(await cdp.evaluate('window.__functionalEventSourceCount'), initialConnections);

    await clickUiText('label', '纯文字模式');
    await waitFor(async () => Boolean((await request(mainBase, '/api/config')).json.data.subtitle_text_only) === originalTextOnly, {
      timeoutMs: 5_000,
      label: 'subtitle config restore',
    });
    assert.equal(await cdp.evaluate('window.__functionalEventSourceCount'), initialConnections);

    await clickUiText('button', '显示字幕');
    overlayCdp = await connectOverlayWebViewCdp();
    const listener = await overlayCdp.evaluate(`(async () => {
      const internals = window.__TAURI_INTERNALS__;
      if (!internals?.invoke || !internals?.transformCallback) return null;
      window.__subtitleIpcPayloads = [];
      const handler = internals.transformCallback((event) => {
        window.__subtitleIpcPayloads.push(event.payload);
      });
      return internals.invoke('plugin:event|listen', {
        event: 'subtitle-content-update',
        target: { kind: 'Any' },
        handler,
      });
    })()`);
    assert.ok(listener !== null && listener !== undefined, 'Tauri event listener instrumentation failed');

    const expected = '字幕IPC增量🙂合并完成';
    const streamed = await request(mainBase, '/api/stream/test', {
      method: 'POST',
      body: { content: expected },
    });
    assert.equal(streamed.status, 200);
    await waitFor(async () => (await overlayCdp.evaluate('document.body?.innerText ?? ""')).includes(expected), {
      timeoutMs: 8_000,
      intervalMs: 100,
      label: 'incremental subtitle IPC result',
    });
    const incrementalPayloads = await overlayCdp.evaluate('window.__subtitleIpcPayloads');
    assert.ok(incrementalPayloads.some((payload) => typeof payload.delta === 'string' && payload.delta.length > 0));
    assert.equal(reconstructSubtitlePayloads(incrementalPayloads), expected);

    await overlayCdp.evaluate('window.__subtitleIpcPayloads = []');
    assert.equal((await request(mainBase, '/api/proxy/start', { method: 'POST', body: {} })).status, 200);
    try {
      const burstResponse = await request(proxyBase, '/api/chat', {
        method: 'POST',
        body: { model: 'mock_burst', stream: true, messages: [{ role: 'user', content: '突发字幕合并测试' }] },
      });
      assert.equal(burstResponse.status, 200);
    } finally {
      await request(mainBase, '/api/proxy/stop', { method: 'POST', body: {} });
      await waitForPortClosed(11434);
    }
    await waitFor(async () => (await overlayCdp.evaluate('document.body?.innerText ?? ""')).includes(burstContent), {
      timeoutMs: 8_000,
      intervalMs: 100,
      label: 'coalesced burst subtitle result',
    });
    const burstPayloads = await overlayCdp.evaluate('window.__subtitleIpcPayloads');
    const burstCharacterCount = [...burstContent].length;
    const transportedCharacters = burstPayloads.reduce(
      (total, payload) => total + [...(payload.text ?? payload.delta ?? '')].length,
      0,
    );
    assert.ok(burstPayloads.length < burstCharacterCount / 4, `burst IPC count=${burstPayloads.length}`);
    assert.ok(transportedCharacters <= burstCharacterCount * 2, `transported characters=${transportedCharacters}`);
    assert.equal(reconstructSubtitlePayloads(burstPayloads), burstContent);

    await overlayCdp.send('Page.reload', { ignoreCache: true });
    await waitFor(async () => (await overlayCdp.evaluate('document.body?.innerText ?? ""')).includes(burstContent), {
      timeoutMs: 8_000,
      intervalMs: 100,
      label: 'subtitle ready snapshot sync',
    });
    assert.deepEqual(overlayCdp.exceptions, []);
  });

  await runCase('真实 Tauri UI 页面导航、状态展示、代理启停与截图', async () => {
    assert.ok(cdp, 'Tauri WebView CDP was not connected');
    await waitForUiText('核心服务已连接 :8765', { timeoutMs: 10_000 });
    assert.equal(await cdp.evaluate(`document.body.innerText.includes('Something went wrong')`), false);
    const pages = [
      ['字幕视觉', '#/subtitles'],
      ['语音配置', '#/voice'],
      ['记忆管理', '#/memory'],
      ['运行日志', '#/logs'],
      ['服务连接', '#/'],
    ];
    for (const [label, hash] of pages) {
      await clickUiText('a', label);
      await waitFor(async () => (await cdp.evaluate('location.hash')) === hash, { label: `route ${hash}` });
      assert.ok((await cdp.evaluate('document.body.innerText')).includes(label));
    }
    await clickUiText('button', '启动服务');
    await waitForUiText('运行中 :11434', { timeoutMs: 8_000 });
    assert.equal((await request(mainBase, '/api/proxy/status')).json.data.running, true);
    await clickUiText('button', '停止服务');
    await waitForUiText('已停止', { timeoutMs: 8_000 });
    assert.equal((await request(mainBase, '/api/proxy/status')).json.data.running, false);
    const screenshot = await cdp.send('Page.captureScreenshot', { format: 'png' });
    assert.ok(Buffer.from(screenshot.data, 'base64').length > 20_000);
    assert.deepEqual(cdp.exceptions, []);
  });

  await runCase('API 目标变化 fail-closed、HTTP 不可绕过与 Tauri IPC 确认', async () => {
    const changedUrl = `${mockBase}/v1/chat/completions?confirmed-target=1`;
    const changed = await request(mainBase, '/api/config', { method: 'PUT', body: { api_url: changedUrl } });
    assert.equal(changed.status, 200);
    assert.equal(changed.json.data.api_target_confirmation_required, true);
    const bypass = await request(mainBase, '/api/config', {
      method: 'PUT', body: { api_target_confirmation_required: false },
    });
    assert.equal(bypass.json.data.api_target_confirmation_required, true);
    const blocked = await request(mainBase, '/api/chat', {
      method: 'POST', body: { stream: false, messages: [{ role: 'user', content: '不应发送' }] },
    });
    assert.equal(blocked.status, 500);
    assert.match(blocked.text, /确认|目标/);

    assert.ok(cdp, 'Tauri WebView CDP was not connected');
    await cdp.send('Page.reload', { ignoreCache: true });
    await waitForUiText('确认使用以上 API 目标', { timeoutMs: 10_000 });
    await waitForUiText(changedUrl, { timeoutMs: 10_000 });
    await clickUiText('button', '确认使用以上 API 目标');
    await waitForUiText('确认使用以上 API 目标', { present: false, timeoutMs: 10_000 });
    const confirmed = await request(mainBase, '/api/config');
    assert.equal(confirmed.json.data.api_target_confirmation_required, false);
    assert.equal(confirmed.json.data.api_url, changedUrl);
    const allowed = await collectSse(`${mainBase}/api/chat`, {
      method: 'POST',
      body: { model: 'mock_sse', stream: false, messages: [{ role: 'user', content: '确认后允许' }] },
      timeoutMs: 5_000,
    });
    assert.ok(jsonEvents(allowed).some((event) => event.data?.content === 'mock nonstream 你好'));
  });

  const failures = cases.filter((entry) => entry.status === 'FAIL');
  console.log('\nFUNCTIONAL TEST SUMMARY');
  console.log(`  passed: ${cases.length - failures.length}`);
  console.log(`  failed: ${failures.length}`);
  console.log(`  total:  ${cases.length}`);
  console.log(`  mock HTTP requests: ${mockRequests.length}`);
  if (failures.length) {
    console.log('  failed cases:');
    for (const failure of failures) console.log(`    - ${failure.name}`);
    const safeLogs = redact(appLogs.join('')).split('\n').filter(Boolean).slice(-30);
    if (safeLogs.length) {
      console.log('  app log tail:');
      for (const line of safeLogs) console.log(`    ${line}`);
    }
    process.exitCode = 1;
  }
}

try {
  await main();
} catch (error) {
  console.error(`FATAL ${redact(error?.stack ?? error)}`);
  process.exitCode = 1;
} finally {
  await cleanup();
}
