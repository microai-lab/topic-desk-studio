# Topic Desk Studio

**Discover trends. Find your next topic.**

A desktop topic workspace for content creators, editors, and anyone following the news. Find stories across sources, build a shortlist, and read original articles with on-demand translation—all in one place.

[English](./README.md) · [简体中文](./README.zh-CN.md) · [Download](https://github.com/microai-lab/topic-desk-studio/releases/latest)

<p>
  <img alt="Platforms: macOS, Windows, Linux" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-59636E" />
  <a href="https://github.com/microai-lab/topic-desk-studio/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/microai-lab/topic-desk-studio" /></a>
  <a href="./LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-green" /></a>
</p>

<p>
  <a href="./assets/topic-desk-studio-hero.png"><img src="./assets/topic-desk-studio-hero.gif" alt="Topic Desk Studio demo: discover AI news across sources and switch from the original article to bilingual reading" width="100%" /></a>
</p>

<p align="center"><a href="./assets/topic-desk-studio-hero.png">View high-resolution screenshot</a></p>

## Download

Get the latest installer from [GitHub Releases](https://github.com/microai-lab/topic-desk-studio/releases/latest).

| Platform | Package |
| --- | --- |
| macOS | `.dmg` for Apple Silicon (`aarch64`) or Intel (`x64`) |
| Windows | x64 `.exe` or `.msi` |
| Linux | x64 `.AppImage`, or the package for your distribution |

<details>
<summary>macOS: first-launch warnings</summary>

macOS may block unsigned downloads. Confirm that the installer came from this repository’s release page. If macOS reports that the app is “damaged,” download it again first.

For a verified download installed in `/Applications`, remove the quarantine flag for this app:

```bash
xattr -d com.apple.quarantine /Applications/Topic\ Desk\ Studio.app
```

</details>

## From headlines to a topic worth keeping

| Your workflow | What Topic Desk does |
| --- | --- |
| **Find an angle** | Bring news, AI updates, social trends, and community discussions into one list. Filter by source, region, category, or keyword. |
| **Follow a story** | See rank changes, trend lines, first-seen time, and consecutive chart appearances. Use **Recent additions** to catch newly collected topics. |
| **Build your shortlist** | Add promising topics to **Queue** and keep them after they leave the live chart. Hide unwanted topics and restore them later. |
| **Read and understand** | Open original articles beside the list, switch between tabs, and read translations beneath the original text as you scroll. |

Built-in sources include Weibo, Zhihu, Bilibili, 36Kr, TechCrunch, The Verge, Google Trends, and Hacker News, alongside developer sources such as GitHub and Hugging Face. Choose the sources that fit your interests, add your own feeds, and use Chinese or English with light, dark, or system themes.

## Get started

1. **Collect topics.** Open **Discover** and click **Collect**. Collection also starts after launch and refreshes about every 10 minutes while the app is running; one failing source does not stop the others.
2. **Narrow your focus.** Search for a topic such as `OpenAI`, or filter by source and category.
3. **Save what matters.** Click **Add to queue** to keep a topic for later, then revisit it in **Queue**.
4. **Read the original.** Click a title to open it in the right-hand reader. Resize or expand the pane, open more tabs, or enter a URL in its address bar.

The reader also offers page search and download management. Open completed downloads or their folders; cancelling active downloads is supported on macOS.

## Read in two languages

Configure a model in **Settings → Translation**: choose a provider, then save the endpoint, model, and API key if required. Cloud providers, Anthropic, custom OpenAI-compatible endpoints, and local Ollama are supported. Supported local endpoints can work without an API key.

- **Article pages:** click the translation button in the reader toolbar. Translations appear beneath the original text, starting with visible content and continuing as you scroll. Click the same button again to return to the original.
- **English titles:** click **Translate** on an eligible title to translate it into Simplified Chinese.
- **Target language:** choose it in the reader’s **Browser settings**.

Translation starts only when requested. It sends the selected title or page text encountered during translation to your configured model service, which may apply usage limits and charges.

## Make the workspace yours

- **Sources — Settings → Sources:** enable, edit, and reorder sources; add RSS, JSON, or HTML sources; import or export individual sources or the full list. **Restore defaults** keeps custom sources and historical topics.
- **Proxy — Settings → Network proxy:** select a saved proxy or edit its address. Check a source’s latest status and error details when collection fails.
- **Storage — Settings → Local data:** inspect storage health, clean up records, and create or restore local backups. The app keeps the three most recent backups. Restoring replaces current local data and retains downloaded files.
- **Hidden topics — Settings → Local data → Hidden topics:** search and restore a single topic or all hidden topics.

Public sources can change or restrict access. When MIT Technology Review’s official feed is unavailable, the app can recover headlines and original links through Bing’s public news index; results may be delayed or incomplete. Some websites require login, human verification, or viewing certain content in their own app.

## Data and privacy

- **Your workspace stays on your device.** Topics, queue, settings, source configurations, and saved browsing/download records are stored locally. No Topic Desk account is required, and the app does not upload your queue.
- **Network access follows your actions.** Collection contacts enabled sources, reading contacts the article’s website, and optional translation contacts your configured model service.
- **API keys are encrypted locally** and are never exposed to pages.
- **Login cookies can survive tab closures and app restarts.** They are encrypted locally and excluded from backups. Cookies without a website expiry are remembered for up to 30 days; website expiry or revocation may require signing in again. Clearing cookies removes remembered logins from all reading tabs.
- **Other website storage is temporary.** On macOS, reading tabs share a temporary session during each app run. Other website data is discarded when you clear site data or exit. The app does not save or autofill passwords.
- **Browsing history is optional.** You can disable it and clear browsing/download records in the reader. Clearing download records does not delete downloaded files.

Topic Desk Studio is the standalone desktop edition of [dsh-topic-desk](https://github.com/microai-lab/dsh-topic-desk).

## License

[MIT](./LICENSE) © 2026 MicroAI Lab
