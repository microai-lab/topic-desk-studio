# Topic Desk Studio

<p align="center">
  <img src="./assets/topic-desk-studio-hero.png" alt="Topic Desk Studio promotional banner" width="100%" />
</p>

<p align="center">
  A private, local-first desktop workspace for discovering, tracking, and curating trending topics.
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="./README.zh-CN.md">简体中文</a>
</p>

<p align="center">
  <img alt="Platforms" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-59636E" />
  <a href="./LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-green" /></a>
</p>

Topic Desk Studio is a standalone desktop edition of [`dsh-topic-desk`](https://github.com/microai-lab/dsh-topic-desk). It collects public trending topics and keeps your workspace on the device without requiring an account or remote storage.

## Highlights

- **49 built-in sources** across Chinese and international platforms.
- **Resilient collection:** one failing source never blocks the others.
- **Automatic refresh** at startup and every 10 minutes, plus manual refresh.
- **Fast local discovery** with source, region, category, keyword, and sort filters.
- **Trend context** including rank movement, sparklines, first-seen time, and consecutive appearances.
- **Persistent creation queue** that keeps saved topics even after they leave the live charts.
- **Source controls and health** with individual enable/disable switches and recent error details.
- **Optional title translation** through any OpenAI-compatible chat-completions endpoint.
- **Private by design:** topics, queues, and model settings stay in the local application database.
- **Local data controls:** inspect storage health, compact expired history, create local backups, and restore the latest snapshot.
- **Safe link handling:** original articles open as tabs in an isolated reading panel that accepts only web links.

## Screens and workflows

| Discover | Curate | Operate |
| --- | --- | --- |
| Browse current topics and newly collected items. | Save promising topics to a durable creation queue. | Enable sources, inspect health, and configure optional translation. |
| Filter by source, region, category, or keyword. | Keep items even after they disappear from the rankings. | Review per-source failures without interrupting healthy collectors. |

## Supported desktop platforms

| Platform | Availability |
| --- | --- |
| macOS | Apple Silicon and Intel |
| Windows | x64 |
| Linux | x64 |

### macOS: "Topic Desk Studio.app is damaged" on first launch

The published macOS builds are not yet signed or notarized, so Gatekeeper blocks downloaded copies with a misleading "damaged" message. The app is intact; clear the quarantine attribute once after installing:

```bash
xattr -d com.apple.quarantine "/Applications/Topic\ Desk\ Studio.app"
```

## Translation and credential storage

Translation is optional and disabled until configured. Model settings stay on the device, and the API key is encrypted locally without being exposed to the page. Only a title explicitly selected for translation is sent to the configured model service.

The default compatible endpoint is `https://api.deepseek.com` with model `deepseek-flash`. Built-in provider and model presets cover several mainstream cloud platforms and local Ollama, while any OpenAI-compatible service can still be configured manually. A trusted local endpoint may be used without an API key.

## Data and privacy

- Topic history, source health, model settings, and the creation queue stay on the device.
- The app keeps high-frequency trend history bounded and retains up to three user-created local backups.
- Topic Desk Studio does not require an account and does not upload the creation queue.
- Only a title explicitly selected for translation is sent to the configured model endpoint.
- Availability of public sources may change as websites update their feeds, markup, or access policies.
## License

[MIT](./LICENSE) © 2026 MicroAI Lab
