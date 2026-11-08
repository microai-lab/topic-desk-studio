# Window-size layout regression checks

Open `/tests/fixtures/layout-preview-app.html` through the Vite development server.
It renders the real React application with deterministic, read-only Tauri fixtures;
it never accesses the user's SQLite databases, credentials or website sessions.
`layout-preview.html` additionally provides an iframe with three viewport presets.

Check widths/heights in this sequence: 1180×680 → 1920×1080 → 860×560 → 1180×680.

- Open all five settings tabs. The page must stay centered within the right panel,
  remain at or below its 840px maximum width and have no horizontal overflow.
- Open the local-data confirmation dialog, resize while it stays open, then Cancel.
  The dialog must remain centered and inside the viewport; no fixture deletion runs.
- Return to Discover, inspect the long-title fixture, then show the reading pane.
  Neither the document nor the topic list may overflow horizontally, and the
  reading viewport's right edge must stay inside the window after resizing.

On 2026-11-08 these viewport checks passed: all five settings pages had a measured
horizontal center offset of 0px, no horizontal overflow, and the dialog had 0px
horizontal/vertical center offsets at each tested size. The main list and reader
placeholder also remained within the viewport.

These checks exercise CSS and React resize behavior in a browser. They do not
replace testing macOS native fullscreen/Spaces transitions or the external native
reading WebView in a packaged desktop application.
