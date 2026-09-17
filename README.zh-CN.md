# Topic Desk Studio

<p align="center">
  <img src="./assets/topic-desk-studio-hero.png" alt="Topic Desk Studio 宣传横幅" width="100%" />
</p>

<p align="center">
  一款注重隐私、本地优先的热点发现、趋势追踪与创作整理桌面工具。
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="./README.zh-CN.md">简体中文</a>
</p>

<p align="center">
  <a href="https://github.com/microai-lab/topic-desk-studio/actions/workflows/release.yml"><img alt="桌面构建" src="https://github.com/microai-lab/topic-desk-studio/actions/workflows/release.yml/badge.svg" /></a>
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" />
  <img alt="支持平台" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-59636E" />
  <a href="./LICENSE"><img alt="MIT 许可证" src="https://img.shields.io/badge/license-MIT-green" /></a>
</p>

Topic Desk Studio 是 [`dsh-topic-desk`](https://github.com/microai-lab/dsh-topic-desk) 的独立桌面版。它将公开热点采集到本机 SQLite，无需 DeepSeek Harness、Node.js sidecar、用户账户或远程数据库。

## 核心亮点

- **内置 49 个来源**，覆盖国内外多个公开平台。
- **故障隔离采集：**单个来源失败不会阻塞其他来源。
- **自动刷新：**启动后采集、每 10 分钟后台刷新，同时支持手动刷新。
- **本地快速检索：**支持来源、地区、分类、关键词筛选与排序。
- **趋势上下文：**展示排名变化、趋势折线、首次发现时间与连续上榜轮次。
- **持久创作队列：**话题退出当前榜单后，已收藏内容仍会保留。
- **来源管理与健康状态：**来源可独立启停，并展示最近错误信息。
- **可选标题翻译：**兼容 OpenAI Chat Completions 接口。
- **隐私优先：**话题和创作队列保存在本机，API Key 进入操作系统凭据库。
- **安全打开链接：**原文始终交给系统浏览器，不在应用 WebView 中加载第三方页面。

## 使用场景

| 发现 | 整理 | 运维 |
| --- | --- | --- |
| 浏览当前热点与本轮新采集内容。 | 将有价值的话题保存到持久创作队列。 | 启停来源、查看健康状态、配置可选翻译。 |
| 按来源、地区、分类或关键词筛选。 | 即使话题退出榜单，收藏仍然保留。 | 单独检查来源故障，不影响正常采集任务。 |

## 技术架构

| 层级 | 技术 | 职责 |
| --- | --- | --- |
| 桌面容器 | Tauri 2 | 原生窗口、命令、事件、权限与打包 |
| 用户界面 | React 19、TypeScript、Vite | 搜索、筛选、话题视图、创作队列与设置 |
| 业务核心 | Rust | HTTP 采集、解析、校验、标准化、去重与调度 |
| 数据存储 | `rusqlite` + bundled SQLite | 话题、观测记录、采集运行、队列与非敏感设置 |
| 密钥存储 | 操作系统凭据库 | macOS Keychain、Windows Credential Manager 或 Linux Secret Service |

React 界面只能通过类型化的 Tauri commands 和 events 访问本地能力。网络请求、SQLite、调度、代理与凭据访问全部位于 Rust 后端。外部来源返回的数据一律按不可信输入处理，经过标准化后才会存储或展示。

## 桌面平台支持

| 平台 | 目标架构 | 打包说明 |
| --- | --- | --- |
| macOS | Apple Silicon 与 Intel | Tauri 原生安装包；公开分发前需要签名和公证 |
| Windows | x64 | Tauri 原生安装程序；公开分发时建议代码签名 |
| Linux | x64 | 在 Ubuntu 22.04 上生成 Tauri 原生安装包 |

GitHub Actions 会在对应操作系统上完成原生构建。Release 配置启用了体积优化、LTO、符号剥离以及 `panic = "abort"`，以尽量缩小安装包。

## 开始开发

### 环境要求

- Node.js 20 或更高版本
- pnpm 11.19 或更高版本
- 当前稳定版 Rust 工具链
- 当前操作系统对应的 [Tauri 系统依赖](https://v2.tauri.app/start/prerequisites/)

### 启动开发环境

```bash
pnpm install
pnpm dev:desktop
```

### 构建原生安装包

```bash
pnpm build:desktop
```

安装包生成在 `src-tauri/target/release/bundle/`。各平台应在对应操作系统上构建；Tauri 无法通过交叉编译生成所有格式的安装包。

## 质量检查

```bash
pnpm typecheck
pnpm test
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

发布前应在每个目标操作系统上安装并冒烟测试真实安装包。

## 翻译与凭据安全

翻译功能默认不启用，需要用户主动配置。接口地址与模型名称属于非敏感设置，保存在 SQLite；API Key 则通过操作系统凭据库保存：

- **macOS：**钥匙串（Keychain）
- **Windows：**凭据管理器（Credential Manager）
- **Linux：**Secret Service，通常由 GNOME Keyring 或 KDE Wallet 提供

完整 API Key 不会返回 WebView。发起翻译时，Rust 后端会根据话题 ID 从 SQLite 重新读取标题，再从系统凭据库获取 Key，并直接请求用户配置的模型接口。

默认兼容端点为 `https://api.deepseek.com`，默认模型为 `deepseek-chat`，也可以替换为其他 OpenAI-compatible 服务。受信任的本地模型端点可以不配置 API Key。

## 数据与隐私

- 本地数据库名为 `topic-desk.sqlite`，存放在操作系统分配的应用数据目录中。
- 话题历史、采集运行、来源健康状态和创作队列均保存在本机。
- Topic Desk Studio 不要求注册账户，也不会上传创作队列。
- 只有用户明确选择翻译的标题会发送到所配置的模型接口。
- 公开来源可能因为站点 Feed、页面结构或访问策略变化而暂时不可用。

请勿提交 API Key、凭据、运行时数据库、日志、构建产物或签名材料。

## 自动发布

推送符合 `v*` 的 Git Tag，或手动运行 `build-desktop` 工作流，即可分别构建：

- macOS Apple Silicon
- macOS Intel
- Windows
- Linux

对于 `v*` Tag，工作流会等待全部原生构建成功，创建或更新对应的 GitHub Release，自动生成版本说明并附加各平台安装包。手动运行工作流时只保留 Actions Artifacts，不会创建 Release。

仓库不会包含签名凭据；正式公开分发时需要单独配置各平台的签名材料。

## 许可证

[MIT](./LICENSE) © 2026 MicroAI Lab
