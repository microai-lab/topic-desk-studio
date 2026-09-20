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
  <img alt="支持平台" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-59636E" />
  <a href="./LICENSE"><img alt="MIT 许可证" src="https://img.shields.io/badge/license-MIT-green" /></a>
</p>

Topic Desk Studio 是 [`dsh-topic-desk`](https://github.com/microai-lab/dsh-topic-desk) 的独立桌面版。它采集公开热点，并将个人工作区保留在本机，无需注册账户或使用远程存储。

## 核心亮点

- **内置 49 个来源**，覆盖国内外多个公开平台。
- **故障隔离采集：**单个来源失败不会阻塞其他来源。
- **自动刷新：**启动后采集、每 10 分钟后台刷新，同时支持手动刷新。
- **本地快速检索：**支持来源、地区、分类、关键词筛选与排序。
- **趋势上下文：**展示排名变化、趋势折线、首次发现时间与连续上榜轮次。
- **持久创作队列：**话题退出当前榜单后，已收藏内容仍会保留。
- **来源管理与健康状态：**来源可独立启停，并展示最近错误信息。
- **可选标题翻译：**兼容 OpenAI Chat Completions 接口。
- **隐私优先：**话题、创作队列与模型设置均保存在本地应用数据库。
- **安全打开链接：**原文以标签页形式在隔离的阅读区打开，并且只接受网页链接。

## 使用场景

| 发现 | 整理 | 运维 |
| --- | --- | --- |
| 浏览当前热点与本轮新采集内容。 | 将有价值的话题保存到持久创作队列。 | 启停来源、查看健康状态、配置可选翻译。 |
| 按来源、地区、分类或关键词筛选。 | 即使话题退出榜单，收藏仍然保留。 | 单独检查来源故障，不影响正常采集任务。 |

## 桌面平台支持

| 平台 | 支持范围 |
| --- | --- |
| macOS | Apple Silicon 与 Intel |
| Windows | x64 |
| Linux | x64 |

### macOS 提示"Topic Desk Studio.app 已损坏"

当前发布的 macOS 安装包尚未签名和公证，Gatekeeper 会拦截从网络下载的副本并误报"已损坏"。应用本身是完好的，安装后执行一次以下命令清除隔离属性即可：

```bash
xattr -d com.apple.quarantine "/Applications/Topic Desk Studio.app"
```

## 翻译与凭据存储

翻译功能默认不启用，需要用户主动配置。模型设置只保存在本机，API Key 会在本地加密保存且不会暴露给页面。只有用户明确选择翻译的标题才会发送到所配置的模型服务。

默认兼容端点为 `https://api.deepseek.com`，默认模型为 `deepseek-chat`，也可以替换为其他 OpenAI-compatible 服务。受信任的本地模型端点可以不配置 API Key。

## 数据与隐私

- 话题历史、来源健康状态、模型设置和创作队列均保存在本机。
- Topic Desk Studio 不要求注册账户，也不会上传创作队列。
- 只有用户明确选择翻译的标题会发送到所配置的模型接口。
- 公开来源可能因为站点 Feed、页面结构或访问策略变化而暂时不可用。
## 许可证

[MIT](./LICENSE) © 2026 MicroAI Lab
