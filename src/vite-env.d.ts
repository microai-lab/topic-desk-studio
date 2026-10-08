/// <reference types="vite/client" />

/** Build-time brand label; Git metadata is resolved only by Vite, never in the WebView. */
declare const __APP_VERSION__: string

// Vite injects these ambient client types and constants at build time.

/** Vite 环境类型入口；业务环境变量必须在这里显式声明后才能使用。 */
