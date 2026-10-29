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
4. **Read the original.** Click a title to open it in the right-hand reader. Resize the pane, use **Hide topics** for focused reading, or open more tabs. **Show topics** restores the list and its previous width.

Collapsed topics remain accessible through a narrow rail: hover or focus a marker to preview its title and source, then click to read in a new tab.

Playing tabs show a speaker icon; click it to mute or unmute that tab, including background tabs. The reader also offers page search and download management. Open completed downloads or their folders; cancelling active downloads is supported on macOS.

## Read in two languages

Configure a model in **Settings → Translation**: choose a provider, then save the endpoint, model, and API key if required. Cloud providers, Anthropic, custom OpenAI-compatible endpoints, and local Ollama are supported. Supported local endpoints can work without an API key.

- **Article pages:** click the translation button in the reader toolbar. Translations appear beneath the original text, starting with visible content and continuing as you scroll. Click the same button again to return to the original.
- **English titles:** click **Translate** on an eligible title to translate it into Simplified Chinese.
- **Target language:** choose it in the reader’s **Browser settings**.

The official DeepSeek API uses Direct first. If the connection cannot be established, it retries once through the proxy saved in **Settings → Network proxy**. Local models stay on Direct.

Translation starts only when requested. It sends the selected title or page text encountered during translation to your configured model service, which may apply usage limits and charges.

## Make the workspace yours

- **Sources — Settings → Sources:** enable, edit, and reorder sources; add RSS, JSON, or HTML sources; import or export individual sources or the full list. Use **Test connectivity** to check every source, including disabled ones, or the connection icon to test one source. Each test chooses a route afresh using the saved proxy address, without adding topics or changing source settings; a dialog summarizes the results when the batch finishes. **Restore defaults** keeps custom sources and historical topics.
- **Proxy — Settings → Network proxy:** select a saved proxy or edit its address. Check a source’s latest status and error details when collection fails.
- **Storage — Settings → Local data:** inspect storage health, clean up records, and create or restore local backups. The app keeps the three most recent backups. Restoring replaces current local data and retains downloaded files.
- **Hidden topics — Settings → Local data → Hidden topics:** search and restore a single topic or all hidden topics.

Public sources can change or restrict access. When MIT Technology Review’s official feed is unavailable, the app can recover headlines and original links through Bing’s public news index; results may be delayed or incomplete. Some websites require login, human verification, or viewing certain content in their own app.

## Source connection Q&A

When a source fails, use the questions and steps below to troubleshoot. Each single-source or batch test chooses a route afresh for that test, without requiring you to edit the saved Network route first:

1. **Save the actual proxy address.** In **Settings → Network proxy**, save the HTTP proxy address used by your running proxy app, with the correct port. Ports 7897 and 1082 are only presets; the app does not scan or switch ports automatically.
2. **Start a test.** Use the connectivity icon on a source row, or **Test connectivity** for all sources. Check the actual **Direct or Proxy** route; a successful switch shows **Automatically switched**. Every test chooses afresh without changing source settings, topics, or collection history.
3. **Choose a preference if needed.** Manual and scheduled collection automatically try the selected route first, then the other available route after a connection failure. If both fail, collection records the error and continues to the next source. Source settings offer Automatic, Prefer Direct, and Prefer proxy; recovery needs no separate switch.

**When does recovery switch?** Collection starts with the saved preference; diagnostics start with the source default. TLS, connection reset, timeout, or DNS connection failures can trigger one attempt over the other route when a proxy is saved. HTTP 403, 429, and invalid data do not trigger switching. Local and private-network sources stay on Direct. Without a saved proxy, only Direct is attempted. Exhausted sources record an error without blocking other sources or changing saved settings.

<details>
<summary>View all 11 questions and next steps</summary>

### A source will not connect. What should I try first?

Every single-source or batch connectivity test chooses a route afresh for that test; you do not need to edit the saved Network route first. Testing does not add topics, change source settings, or replace collection history. The last collection and current test are different requests, so errors can differ.

1. In Settings → Network proxy, save the HTTP proxy address actually used by your proxy app. Check that the app is running and the port matches. Without a saved proxy address, only Direct can be tested.
2. Use the test icon beside the affected source, or Test connectivity for all sources. Check the actual Direct or Proxy route and any Automatically switched message.
3. Collection also recovers automatically without an extra switch. Testing and collection happen at different times; share the source name, error, and test result if it still fails.

### What should I do about “TLS failed” or “connection reset”?

TLS means a secure connection; a failure means it could not finish. A reset means the connection was cut off. The network, website, or proxy may be involved, so this message alone cannot identify the cause. Each test starts with the source’s default route; with a saved proxy address, a connection problem can trigger one attempt over the other route.

1. In Settings → Network proxy, save the HTTP proxy address actually used by your running proxy app. Its address and port must match. Ports 7897 and 1082 are only presets; the app does not search for an available port.
2. Use the connectivity icon beside QbitAI or the affected source directly; you do not need to select Automatic first. Check the actual Direct or Proxy route. A successful switch shows Automatically switched.
3. Each collection also recovers automatically. Use the source’s pencil Edit icon to choose Prefer Direct or Prefer proxy if you want to change the first attempt. Keep certificate verification enabled. If it still fails, compare in a browser, check the system clock, or share the error rather than bypassing security warnings.

### What should I do about a timeout or DNS failure?

A timeout means the result took too long. A DNS failure means the website’s network address could not be found. The network may be temporarily unavailable or the address may be misspelled; the source is not necessarily gone.

1. Check that other websites open, then test only this source once.
2. Check the saved proxy address and that its app is running. Each test chooses a route automatically; a timeout or DNS connection problem can trigger one attempt over the other route. The result shows the route actually used for this test without changing the saved route preference.
3. Open the same address in a browser and check for spelling mistakes. If a later retry still fails, share the source, error, and test result.

### Why does testing pass while collection fails? Does collection recover automatically?

Each manual, startup, or scheduled collection reassesses its connection routes automatically. Prefer Direct and Prefer proxy choose the first route; Automatic uses the source default. A connection failure triggers at most one attempt over the other available route. If both fail, collection records the error and continues to the next source without rewriting settings. Local and private-network sources stay on Direct; without a saved proxy, only Direct is attempted.

1. Save the address of your running HTTP proxy in Settings → Network proxy, then start a single-source or batch test. Ports 7897 and 1082 are only presets and must match the running app. The app does not scan ports or switch presets automatically.
2. In Settings → Sources, use the pencil Edit icon to adjust the preferred first route if needed. All source modes support connection recovery without switching each source to Automatic.
3. Tests show the actual Direct or Proxy route and an Automatically switched message when applicable. Collection records exhausted-route failures and continues with other sources, without retrying indefinitely.

### What should I do about HTTP 401/403 or human verification?

HTTP 401/403 means the website refused access. It may require login, permission, or verification. Automatic does not switch routes for these access restrictions or verification pages; switching proxies is not a guaranteed fix either. Signing in through a browser does not guarantee access to the collection address.

1. Open the source in a browser and check its login or verification prompt. Do not repeatedly test a verification page.
2. If the test uses Proxy, check that the proxy is running and its settings are saved. Repeatedly switching nodes is not a reliable solution.
3. Prefer public RSS/API addresses: the website’s subscription or data addresses. For sources such as Xiaohongshu, use Login and collect. Report persistent errors from other built-in sources.

### Why does HTTP 429 appear after repeated tests?

HTTP 429 means too many requests: the website has temporarily limited access. Repeated tests or collection add requests, and other people on the same network may share the limit. Automatic does not switch routes because of a 429 response.

1. Stop repeated tests and collection. Follow any wait time given by the website.
2. After waiting, test only this source once, without collecting it at the same time.
3. If the limit remains, check the website in a browser, try later, or share the source and error. Do not keep switching proxies to retry.

### Does HTTP 5xx mean the app is broken?

HTTP 5xx means the website or an intermediate service temporarily failed. The website, a service along the connection, or a proxy may be involved; the number alone does not show that the app is at fault. Automatic does not switch routes for these website error responses.

1. Wait a little, then test only this source once instead of refreshing repeatedly.
2. For Proxy results, check that the proxy app is running and its correct address and port have been saved.
3. Open the same address in a browser. If it still fails, try later and share the source, full error, and Direct or Proxy label.

### What should I do about HTTP 404/410 or an invalid address?

This usually means the collection address could not be found. It may have moved, stopped working, or redirected to an unavailable page. Some websites also use these responses to restrict access.

1. Retest this source once. If you edited it, check that the changes are saved and the address is spelled correctly.
2. For custom sources, check the publisher’s website for its current RSS/API subscription or data address. Save the new address before retesting; do not replace it with the homepage.
3. You do not need to edit built-in source rules. Report the source name and error if it remains unavailable.

### The website opens, but the app says the data is invalid or no valid topics were found. What now?

A reachable website does not always return a topic list. It may have changed, or it may return a login or verification page, leaving the app unable to read its headlines. Automatic does not switch routes for invalid data.

1. Retest only this source and check the website in a browser for login or verification prompts.
2. Built-in sources do not require you to understand technical settings. Share the source and full error if it keeps failing so support can be updated.
3. For custom sources only, check the RSS/API subscription or data address first. If familiar with advanced settings, then check the content type and headline/link reading rules; save before retesting.

### Why are there no new topics when the result says “Connected, no items”?

The request finished, but it found no topics to display. The source may have no content at the moment, or the website may have changed. This result does not promise new data on the next collection.

1. Check the source in a browser for recent content, then test it once.
2. For a custom source, check its collection address first. Leave advanced reading rules unchanged if you are unsure what to edit.
3. Try later. If the website has content but the result stays empty, share the source name and test result.

### Why does the browser work while collection fails? Must I sign into Xiaohongshu again?

Browsers can use login sessions, load full pages, and complete verification; ordinary source collection does not use those browser sessions. Xiaohongshu needs Login and collect. “Browser check required” does not mean you have been logged out.

1. Open the source in the built-in browser and check for login or human verification.
2. For Xiaohongshu, use Login and collect beside the source. If its homepage still shows you as signed in, this test message alone is not a reason to sign in again.
3. For other sources, check the publisher’s public RSS/API subscription or data address. Share the source and error if the webpage works but collection still fails.

</details>

## Data and privacy

- **Your workspace stays on your device.** Topics, queue, settings, source configurations, and saved browsing/download records are stored locally. No Topic Desk account is required, and the app does not upload your queue.
- **Network access follows your actions.** Collection contacts enabled sources, connection tests contact the selected source, reading contacts the article’s website, and optional translation contacts your configured model service.
- **API keys are encrypted locally** and are never exposed to pages.
- **Login cookies can survive tab closures and app restarts.** They are encrypted locally and excluded from backups. Cookies without a website expiry are remembered for up to 30 days; website expiry or revocation may require signing in again. Clearing cookies removes remembered logins from all reading tabs.
- **Other website storage is temporary.** On macOS, reading tabs share a temporary session during each app run. Other website data is discarded when you clear site data or exit. The app does not save or autofill passwords.
- **Browsing history is optional.** You can disable it and clear browsing/download records in the reader. Clearing download records does not delete downloaded files.

Topic Desk Studio is the standalone desktop edition of [dsh-topic-desk](https://github.com/microai-lab/dsh-topic-desk).

## License

[MIT](./LICENSE) © 2026 MicroAI Lab
