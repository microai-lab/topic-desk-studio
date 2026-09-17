/** React 应用入口：启用严格模式以尽早发现副作用和生命周期错误。 */
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import './styles.css'

const container = document.getElementById('root')
if (container === null) throw new Error('缺少 React 根节点')

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
