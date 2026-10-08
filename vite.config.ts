/** Vite 构建配置：开发服务器使用 Tauri 推荐的固定端口和严格端口占用检查。 */
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { appVersionLabel } from './src/appVersion'

// Tauri build flags remain confined to build-time configuration.
const environment = (globalThis as typeof globalThis & {
  process?: { env?: Record<string, string | undefined> }
}).process?.env ?? {}

export default defineConfig(({ command, mode }) => {
  const releaseVersion = (JSON.parse(readFileSync(new URL('./src-tauri/tauri.conf.json', import.meta.url), 'utf8')) as { version: string }).version
  let latestTag: string | undefined
  try {
    latestTag = execFileSync('git', ['describe', '--tags', '--abbrev=0'], { cwd: fileURLToPath(new URL('.', import.meta.url)), encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim()
  } catch {
    // Source archives and shallow CI checkouts may not include Git tags.
  }
  const development = command === 'serve' || mode === 'development' || environment.TAURI_ENV_DEBUG === 'true' || environment.TAURI_ENV_DEBUG === '1'
  return {
    define: { __APP_VERSION__: JSON.stringify(appVersionLabel(releaseVersion, latestTag, development, new Date())) },
    plugins: [react()],
    clearScreen: false,
    server: {
      host: '127.0.0.1',
      port: 1420,
      strictPort: true,
    },
    envPrefix: ['VITE_', 'TAURI_'],
    build: {
      target: environment.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
      minify: environment.TAURI_ENV_DEBUG ? false : 'esbuild',
      sourcemap: Boolean(environment.TAURI_ENV_DEBUG),
    },
  }
})
