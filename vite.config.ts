/** Vite 构建配置：开发服务器使用 Tauri 推荐的固定端口和严格端口占用检查。 */
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Avoid a Node type dependency in the browser project while still reading Tauri build flags.
const environment = (globalThis as typeof globalThis & {
  process?: { env?: Record<string, string | undefined> }
}).process?.env ?? {}

export default defineConfig({
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
})
