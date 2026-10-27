# Topic Desk Studio

**发现热点，留下值得创作的选题。**

为内容创作者、编辑和资讯读者打造的桌面话题工作台。从多来源发现热点、整理待创作选题，到对照原文与译文，在同一个工作区完成。

[English](./README.md) · [简体中文](./README.zh-CN.md) · [下载安装](https://github.com/microai-lab/topic-desk-studio/releases/latest)

<p>
  <img alt="支持 macOS、Windows 与 Linux" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-59636E" />
  <a href="https://github.com/microai-lab/topic-desk-studio/releases/latest"><img alt="最新版本" src="https://img.shields.io/github/v/release/microai-lab/topic-desk-studio" /></a>
  <a href="./LICENSE"><img alt="MIT 许可证" src="https://img.shields.io/badge/license-MIT-green" /></a>
</p>

<p>
  <a href="./assets/topic-desk-studio-hero-zh.png"><img src="./assets/topic-desk-studio-hero-zh.gif" alt="Topic Desk Studio 动态演示：多来源 AI 选题与原文、中文译文对照阅读" width="100%" /></a>
</p>

<p align="center"><a href="./assets/topic-desk-studio-hero-zh.png">查看高清静态截图</a></p>

## 下载安装

前往 [GitHub Releases](https://github.com/microai-lab/topic-desk-studio/releases/latest) 下载最新安装包。

| 平台 | 安装包 |
| --- | --- |
| macOS | Apple Silicon（`aarch64`）或 Intel（`x64`）对应的 `.dmg` |
| Windows | x64 `.exe` 或 `.msi` |
| Linux | x64 `.AppImage`，或对应发行版的安装包 |

<details>
<summary>macOS 首次启动提示</summary>

macOS 可能拦截未签名的下载文件。请确认安装包来自本仓库的发布页；如果提示“已损坏”，请先重新下载。

确认下载可信且应用已安装到 `/Applications` 后，可移除这个应用的隔离标记：

```bash
xattr -d com.apple.quarantine /Applications/Topic\ Desk\ Studio.app
```

</details>

## 从热点列表，到值得留下的选题

| 你的工作流 | Topic Desk 能做什么 |
| --- | --- |
| **寻找切入点** | 把新闻、AI 动态、社交热点与社区讨论汇集到一个列表，按来源、地区、分类或关键词筛选。 |
| **追踪话题** | 查看排名变化、趋势折线、首次发现时间与连续上榜轮次，通过 **最近新增** 找到刚采集的内容。 |
| **整理选题** | 把有价值的话题加入 **待创作**，离榜后仍可保留；隐藏不感兴趣的话题，需要时再恢复。 |
| **阅读与理解** | 在列表旁打开原文，多标签切换；译文显示在原文下方，随滚动继续翻译。 |

内置微博、知乎、B 站、36氪、TechCrunch、The Verge、Google Trends、Hacker News 等来源，也覆盖 GitHub、Hugging Face 等开发者社区。可以按兴趣选择来源、添加自己的订阅，并切换中英文界面与浅色、深色或随系统主题。

## 快速上手

1. **采集话题。** 打开 **发现选题**，点击 **采集数据**。启动后也会自动采集，运行期间约每 10 分钟刷新一次；单个来源失败不会影响其他来源更新。
2. **聚焦感兴趣的内容。** 搜索 `OpenAI` 等关键词，或按来源、分类筛选。
3. **保存值得跟进的选题。** 点击 **加入待创作**，稍后在 **待创作** 中集中回看。
4. **打开原文。** 点击标题，在右侧阅读区继续阅读。可调整宽度、放大阅读区、打开多个标签页，或在地址栏输入其他网址。

阅读区还支持页内查找与下载管理，可打开已下载文件及所在文件夹；macOS 支持取消进行中的下载。

## 双语阅读

先在 **设置 → 翻译模型** 中选择服务商，保存接口地址、模型名称和所需的 API Key。支持云端服务、Anthropic、自定义 OpenAI 兼容接口与本地 Ollama；受支持的本地接口可以不配置 API Key。

- **文章页面：** 点击阅读区工具栏中的翻译按钮，优先翻译当前可见内容，译文显示在原文下方，随滚动继续翻译。再次点击同一按钮即可恢复原文。
- **英文标题：** 在符合条件的标题上点击 **译为中文**，生成简体中文译文。
- **目标语言：** 在阅读区的 **浏览器设置** 中选择。

翻译仅在主动操作后启动。所选标题或翻译过程中阅读到的页面文本会发送给配置的模型服务；服务商可能有自己的用量限制与费用。

## 配置自己的工作台

- **来源 — 设置 → 数据来源：** 启停、编辑与排序来源，添加 RSS、JSON、HTML 来源，导入导出单个来源或全部配置。**恢复默认来源** 会保留自定义来源和历史话题。
- **代理 — 设置 → 网络代理：** 选择已保存的代理或编辑地址。采集失败时，可查看来源最近的状态与错误信息。
- **存储 — 设置 → 本地数据：** 查看存储状态、整理记录、创建或恢复本地备份。应用保留最近三份备份；恢复会覆盖当前本地数据，并保留已下载文件。
- **隐藏话题 — 设置 → 本地数据 → 已隐藏话题：** 搜索并恢复单条话题，或全部恢复。

公开来源可能因网站改版或访问限制而暂时不可用。MIT Technology Review 官方订阅不可用时，应用会尝试通过 Bing 公开新闻索引补充标题与原文链接；结果可能延迟或不完整。部分网站仍要求登录、人机验证，或在其 App 内查看内容。

## 数据与隐私

- **工作台数据保存在本机。** 话题、待创作列表、设置、来源配置，以及保存的浏览和下载记录均保留在本地。无需注册 Topic Desk 账户，应用不上传待创作列表。
- **联网行为对应具体功能。** 采集会访问启用的来源，阅读会访问原文网站，可选翻译会访问配置的模型服务。
- **API Key 在本地加密保存，** 不会暴露给页面。
- **关闭标签页或重启应用后可恢复登录 Cookie。** Cookie 在本地加密保存，不进入备份；没有网站过期时间的 Cookie 最多保留 30 天。网站过期或撤销会话后仍可能需要重新登录。清除 Cookie 会移除所有阅读标签页中已记住的登录。
- **其他网站存储为临时数据。** macOS 阅读标签页在单次运行中共享临时会话；清除网站数据或退出应用后，其他网站数据会被丢弃。应用不保存或自动填充密码。
- **浏览历史可以关闭。** 阅读区支持关闭历史保存、清除浏览或下载记录；清除下载记录不会删除已下载文件。

Topic Desk Studio 是 [dsh-topic-desk](https://github.com/microai-lab/dsh-topic-desk) 的独立桌面版。

## 许可证

[MIT](./LICENSE) © 2026 MicroAI Lab
