# Reader translation performance

## Design references

The reader pipeline was informed by the paragraph/piece bookkeeping and dynamic-content handling in [TWP](https://github.com/FilipePS/Traduzir-paginas-web/blob/master/src/contentScript/pageTranslator.js), and the separate bilingual output/restoration in [old Immersive Translate](https://github.com/immersive-translate/old-immersive-translate/blob/main/src/contentScript/pageTranslator.js). These are design references, not copied extension code. Their browser-extension messaging and translation providers are not used by this application.

The application keeps network requests and credential decryption in Rust. The isolated article document only discovers text, tracks visible slots, and displays plain-text results. It receives neither credentials nor native IPC privileges.

## Shipped pipeline

- Index text ownership once; re-index only mutated subtrees. Ignore the translator's own DOM writes.
- Queue the current viewport first. Re-check visibility before dispatch so old offscreen work cannot consume all model slots.
- Keep completed page translations when scrolling and when virtualized content is recreated. Native session caching is scoped to endpoint, model, target language, and source text.
- Join identical in-flight text to one request. Apply cache hits before reserving model concurrency.
- For the configured DeepSeek Flash path, use four concurrent single-paragraph requests with streaming output. Other models retain bounded batches rather than assuming an unverified streaming protocol.
- Wake on model output instead of sleeping until the next polling interval. Coalesce cumulative stream updates per DOM slot. Only successful final answers enter the cache.
- Cancel old session work when restoring translation or navigating away. Do not retry authentication or connectivity failures as malformed-output failures.

## Native model measurement — 2026-10-08

Measurement used the application's saved `deepseek-flash` routing, proxy, and encrypted credential through the shipped native translator. The database was opened read-only. The key was decrypted only inside Rust into zeroizable memory and was never printed or passed to the page.

The fixture contained four generated, non-private English prose paragraphs, totaling 745 source characters, translated to Simplified Chinese. Every returned paragraph contained Chinese text and every completed result matched its cache entry. Strategies alternated between one four-item JSON request and four concurrent streaming single-item requests. Each process started with an empty translation cache; both strategies ran three times.

| Run | Batch first text / all complete (ms) | Parallel first text / all complete (ms) |
| --- | --- | --- |
| 1 | 4782.704 / 4782.721 | 2258.875 / 3129.952 |
| 2 | 4225.537 / 4225.568 | 3053.011 / 4405.190 |
| 3 | 3731.855 / 3731.877 | 1765.367 / 2809.992 |
| Median | 4225.537 / 4225.568 | 2258.875 / 3129.952 |

First text became available about **46.5% sooner**; all four paragraphs completed about **25.9% sooner** at the median. In the streaming strategy, first text means a provisional text callback, not a fully completed paragraph. Batch output becomes available only after the complete response is parsed. These numbers measure native translator output availability, not time to paint in the embedded WebView, and are not a latency guarantee for other models, pages, or network conditions.

All four native cache lookups hit in all six runs, taking 0.052–0.614 ms per four-entry lookup. Unit tests separately verify that cached translation results bypass the model request path. The temporary live-network benchmark executable was removed after measurement; it is not part of unit tests or CI.

## Real-browser extraction measurement

Open `/scripts/translation-benchmark.html` through the local Vite server. It compares a frozen full-tree collector with the shipped indexed runtime on the same document: 4,000 paragraphs, 20,000 text nodes, five runs, no model requests.

The 2026-10-08 run used a Chromium 154 browser on macOS. Median elapsed times were:

| Operation | Frozen full-tree collector | Indexed runtime |
| --- | --- | --- |
| Initial extraction | 53.6 ms | 33.3 ms |
| 100 unchanged drains | 3939.9 ms | 0.1 ms |

The near-zero idle result is bounded by browser timer precision; it does not mean literally zero CPU work. This fixture measures DOM work, not model latency or embedded WebKit paint time.

## Regression coverage

`src/pageTranslationRuntime.test.ts` executes the shipped injection against deterministic DOM fixtures. Coverage includes viewport-only discovery, scroll-back cache reuse, recreated/changed paragraphs, split long paragraphs, dynamic mutations, exclusion rules, cleanup, retryable errors, and provisional stream output that cannot overwrite a final answer.

`src-tauri/src/browser_control.rs` covers bounded visible batching, in-flight deduplication, cache-hit dispatch under a full pool, coalesced stream updates, and result-triggered scheduler wake-up. `src-tauri/src/translator.rs` covers provider response formats, malformed batches, bounded fallback/cancellation, language/model-scoped caching, streaming chunk boundaries, and credential/configuration preflight.

Deterministic validation commands:

```sh
pnpm typecheck
pnpm test
pnpm exec vitest run src/pageTranslationRuntime.test.ts
cd src-tauri
cargo fmt --check
cargo test
cargo clippy -- -D warnings
```

Unit tests must never depend on a live provider or a saved API key.

## Native reader smoke check

A local macOS debug application bundle was built for UI inspection, because the unbundled development executable could not be resolved by native automation. On the already-open Ars Technica article, the native reader displayed four Chinese translations beneath their English source text, with progress reporting four completed slots, zero active/queued slots, and zero failures. This proves native rendering, independently of the API benchmark above; no native paint-latency number was measured.

Moving the native scrollbar into later content produced additional Chinese translations with seven completed slots and zero active/queued/failed slots. Returning to the earlier position preserved rendered translations. Completion counts are slot counts, not HTTP-request counts, so this visual check is not used to claim a measured zero-request scroll-back. That invariant is covered by the deterministic cache/discovery tests and native cache checks above.

The public `/scripts/translation-smoke.html` fixture is available for repeatable manual native checks without private page text. API/extraction timings must not be described as measured native paint timings.

## Completion audit — 2026-10-08

- Both referenced implementations were inspected: TWP groups inline text into pieces and observes new DOM nodes; old Immersive Translate tracks untranslated visible pieces, separates bilingual output, and retains original nodes for restoration. The corresponding application mechanisms are the indexed ownership/dirty-root collector, viewport queue, and isolated output/restoration above. No claim is made that this reproduces every extension feature or matches the latency of today's proprietary Immersive Translate release.
- The stored six-run native measurements were checked against this report. They use the same saved model, routing, credentials, source fixture, and target language for the batch and parallel strategies; only the dispatch strategy changes. The medians demonstrate an improvement, with variation and scope stated explicitly above.
- The actual injected runtime is included by Rust with `include_str!("page_translation.js")`, and the browser benchmark imports that same file. Tests execute the actual runtime rather than a second implementation.
- Current validation passed: TypeScript typecheck, all 66 frontend tests, the focused 26 runtime tests, Rust formatting, all 90 Rust tests (none ignored), Clippy with warnings denied, and whitespace checks. Scroll-back/recreated-content tests emit no new translation work after successful completion; native cache tests return results without credentials or network access.
- Native Chinese rendering and newly visible translations were observed in the macOS reader, as described above. The temporary debug application process was stopped; the original development application remains running. The temporary live-network measurement source is absent.

This is a local development verification, not a published release or installer delivery. Windows/Linux CI and real-installer smoke tests remain release gates; this report does not claim those have run.
