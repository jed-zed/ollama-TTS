import { existsSync } from "node:fs"
import { spawnSync } from "node:child_process"
import path from "node:path"
import process from "node:process"

const root = process.cwd()
const args = new Set(process.argv.slice(2))
const fullMode = args.has("--full")

const requiredPaths = [
  "package.json",
  "vite.config.ts",
  "tsconfig.json",
  "src/main.tsx",
  "src/App.tsx",
  "src/lib/utils.ts",
  "src/components/ui/input.tsx",
  "src/services/tts.ts",
  "src/stores/ttsStore.ts",
  "src-tauri/Cargo.toml",
  "src-tauri/tauri.conf.json",
  "src-tauri/src/main.rs",
  "src-tauri/src/services/audio_player.rs",
  "src-tauri/src/services/tts_parser.rs"
]

const shouldNotExistPaths = [
  "src-tauri/target/debug",
  "src-tauri/target/release"
]

const missing = requiredPaths.filter((p) => !existsSync(path.join(root, p)))
const unexpected = shouldNotExistPaths.filter((p) => existsSync(path.join(root, p)))

if (missing.length > 0) {
  console.error("[post-reset-check] 缺失关键文件：")
  for (const file of missing) {
    console.error(`- ${file}`)
  }
  process.exit(1)
}

if (unexpected.length > 0) {
  console.warn("[post-reset-check] 发现可能应清理的构建产物目录：")
  for (const p of unexpected) {
    console.warn(`- ${p}`)
  }
}

console.log("[post-reset-check] 关键文件检查通过")

function run(command, commandArgs, cwd = root) {
  const result = spawnSync(command, commandArgs, {
    cwd,
    stdio: "inherit",
    shell: process.platform === "win32"
  })

  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}

if (fullMode) {
  console.log("[post-reset-check] 开始完整校验：build + tsc + cargo check")
  run("npm", ["run", "build"])
  run("npx", ["tsc", "--noEmit"])
  run("cargo", ["check", "-q"], path.join(root, "src-tauri"))
  console.log("[post-reset-check] 完整校验通过")
}
