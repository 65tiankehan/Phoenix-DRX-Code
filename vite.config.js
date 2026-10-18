import { defineConfig } from 'vite'
import { sveltekit } from '@sveltejs/kit/vite'

// Tauri 前端开发约定：固定端口 1420，清除屏幕关闭，监听忽略 Rust 侧
export default defineConfig({
  plugins: [sveltekit()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: process.env.TAURI_DEV_HOST || '127.0.0.1',
    watch: {
      ignored: ['**/src-tauri/**']
    }
  }
})
