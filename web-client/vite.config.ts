import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Mock 模式下请求由 MSW（Service Worker）在浏览器内拦截；
// 对接真实网关时，Vite 代理把请求转发到 Go 网关（默认 :8080）。
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/auth': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
      '/v1': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
      '/ws': {
        target: 'ws://localhost:8080',
        ws: true,
      },
    },
  },
})
