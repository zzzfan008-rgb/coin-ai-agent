import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import './index.css'

async function bootstrap() {
  // 开发环境默认启用 MSW mock；设置 VITE_USE_MOCK=false 可对接真实网关
  const useMock =
    import.meta.env.DEV && import.meta.env.VITE_USE_MOCK !== 'false'

  if (useMock) {
    const { worker } = await import('./mocks/browser')
    await worker.start({
      onUnhandledRequest: 'bypass',
      quiet: false,
    })
  }

  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
}

void bootstrap()
