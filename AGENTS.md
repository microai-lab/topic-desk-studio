# AGENTS.md

## 项目目标

- 本仓库构建独立的 Topic Desk 桌面应用，不依赖 DeepSeek Harness 运行时。
- 正式桌面目标为 Windows、macOS 与 Linux；公共核心不得阻碍未来的 iOS 适配。
- 桌面容器使用 Tauri 2，界面使用 React/Vite，采集与 SQLite 业务核心使用 Rust。

## 源码注释

- 每个源码文件必须包含模块或职责说明。
- 公共类型、公共函数、Tauri command、数据库事务与不直观的业务规则必须写注释。
- 注释应解释约束和原因，不要逐行复述代码。

## 仓库卫生

- `.DS_Store` 不得存在于仓库中；发现后立即删除。
- 不提交 API Key、凭据、SQLite 运行数据、日志、构建产物或签名材料。
- 保留用户已有的未提交修改，不重置或覆盖无关文件。

## 架构边界

- React 页面只能通过类型化的 Tauri commands/events 访问本地能力。
- 网络采集、SQLite、代理、定时器和凭据访问只能位于 Rust 后端。
- 来源返回内容一律视为不可信输入；校验、标准化后才能持久化或展示。
- 外部链接必须交给系统浏览器，不能在应用 WebView 内导航。
- 平台专属能力放入 `desktop` 或未来的 `mobile` adapter，不能污染公共核心。

## 数据不变量

- 话题身份优先级为：平台稳定 ID、规范化 URL、规范化标题。
- SHA-256 输入保持 `v1\0<platform_code>\0<identity_kind>\0<normalized_identity>`。
- 排名、热度与采集时间不参与身份计算。
- 重复话题保留首次标题、URL、发布时间和创建时间。
- 单个来源失败不得阻止其他来源提交。

## 完成检查

- TypeScript 修改至少运行 `pnpm typecheck` 与相关测试。
- Rust 修改至少运行 `cargo fmt --check`、`cargo test` 与 `cargo clippy -- -D warnings`。
- 交付桌面构建前运行对应系统的真实安装包冒烟测试。
