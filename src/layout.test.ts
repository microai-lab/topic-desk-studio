/** Laptop-size regression rules: bounded split panes, responsive CSS, and native defaults. */
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { clampTopicWidth, DEFAULT_TOPIC_WIDTH, topicColumns, topicDragWidth, READER_PANE_MIN_WIDTH, sidebarNavigationPlan, SPLIT_WORKSPACE_MIN_WIDTH, TOPIC_PANE_MIN_WIDTH } from './layout'

describe('reader sizing', () => {
  it('ignores stale capture, released buttons and invalid geometry after idle', () => {
    expect(topicDragWidth(7, 7, 1, 500, 100, 1000)).toBe(400)
    expect(topicDragWidth(7, 7, 1, -100, 100, 1000)).toBe(320)
    expect(topicDragWidth(7, 7, 1, 900, 100, 1000)).toBe(580)
    for (const [active, pointer, buttons] of [[null, 7, 1], [7, 8, 1], [7, 7, 0], [7, 7, 2]]) {
      expect(topicDragWidth(active!, pointer!, buttons!, 400, 1000, 1000)).toBeUndefined()
    }
    for (const width of [0, -1, NaN, Infinity]) {
      expect(topicDragWidth(7, 7, 1, 400, 1000, width)).toBeUndefined()
    }
    expect(topicDragWidth(7, 7, 1, NaN, 1000, 1000)).toBeUndefined()
    expect(topicDragWidth(7, 7, 1, 400, Infinity, 1000)).toBeUndefined()
  })
  it('keeps a single pane toggle in the reader instead of duplicating it in the list header', () => {
    const app = readFileSync(new URL('./App.tsx', import.meta.url), 'utf8')
    const browser = readFileSync(new URL('./BrowserPane.tsx', import.meta.url), 'utf8')
    expect(app).not.toContain('<ReaderExpandButton')
    expect(browser.match(/<ReaderExpandButton/g)).toHaveLength(1)
    expect(app).toContain('.browser-tabbar [data-topic-toggle="collapse"]')
    expect(app).not.toContain('.topic-list-pane [data-topic-toggle="collapse"]')
  })
  it('shows source before category in the topic filters', () => {
    const app = readFileSync(new URL('./App.tsx', import.meta.url), 'utf8')
    const filters = app.slice(app.indexOf('<section className="filters"'), app.indexOf('<div className="results-heading"'))
    expect([...filters.matchAll(/<FilterDropdown label=\{m\.(filter\w+)\}/g)].map(match => match[1]))
      .toEqual(['filterRegion', 'filterSource', 'filterCategory', 'filterSort'])
  })
  it('starts with a fixed 420px topic pane as the workspace grows', () => {
    expect(DEFAULT_TOPIC_WIDTH).toBe(420)
    for (const width of [840, 1060, 1600, 2400]) {
      expect(clampTopicWidth(DEFAULT_TOPIC_WIDTH, width)).toBe(420)
      expect(topicColumns(DEFAULT_TOPIC_WIDTH, width)).toBe('minmax(0px, 420px) minmax(420px, 1fr)')
    }
  })
  it('permits free resizing between the two content minima', () => {
    expect(clampTopicWidth(100, 1400)).toBe(320)
    expect(clampTopicWidth(800, 1400)).toBe(800)
    expect(clampTopicWidth(1400, 1400)).toBe(980)
    expect(clampTopicWidth(420 - 24, 1000)).toBe(396)
    expect(clampTopicWidth(420 + 24, 1000)).toBe(444)
  })
  it('temporarily fits narrow workspaces without losing the requested width', () => {
    expect(clampTopicWidth(420, 780)).toBe(360)
    expect(clampTopicWidth(420, 840)).toBe(420)
    expect(clampTopicWidth(600, 800)).toBe(380)
    expect(clampTopicWidth(600, 1200)).toBe(600)
    expect(clampTopicWidth(420, 500)).toBe(TOPIC_PANE_MIN_WIDTH)
  })
  it('collapses only the topic track and restores the selected pixel width', () => {
    for (const width of [320, 420, 600, NaN, Infinity]) {
      expect(topicColumns(width, 1100, true)).toBe('minmax(0px, 0px) minmax(420px, 1fr)')
      expect(topicColumns(width, 1100, false)).toBe(topicColumns(width, 1100))
    }
  })
  it('animates matching tracks without removing the list and respects reduced motion', () => {
    const styles = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    expect(styles).not.toContain('.reader-expanded .topic-list-pane { display: none; }')
    expect(styles).toContain('transition: opacity 120ms ease, visibility 0s 120ms')
    const reduced = styles.slice(styles.indexOf('@media (prefers-reduced-motion: reduce)'))
    expect(reduced).toContain('.topic-list-pane')
    expect(reduced).toContain('transition: none')
  })
  it('recovers invalid sizing without NaN or Infinity CSS', () => {
    for (const input of [NaN, Infinity, -Infinity]) {
      expect(clampTopicWidth(input, 1200)).toBe(DEFAULT_TOPIC_WIDTH)
      expect(topicColumns(input, 1200)).toBe(topicColumns(DEFAULT_TOPIC_WIDTH, 1200))
      expect(clampTopicWidth(420, input)).toBe(DEFAULT_TOPIC_WIDTH)
    }
    expect(clampTopicWidth(420, 0)).toBe(DEFAULT_TOPIC_WIDTH)
  })
  it('fits a 1280-wide laptop and uses available workspace instead of the global viewport', () => {
    const styles = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    const minimum = TOPIC_PANE_MIN_WIDTH + READER_PANE_MIN_WIDTH
    expect(minimum).toBeLessThan(1280 - 220)
    expect(styles).toContain('container: workspace / inline-size')
    const breakpoint = Number(styles.match(/@container workspace \(max-width: (\d+)px\)/)?.[1])
    expect(breakpoint + 1).toBe(SPLIT_WORKSPACE_MIN_WIDTH)
    expect(breakpoint).toBeGreaterThanOrEqual(minimum)
    const collapsedWidth = Number(styles.match(/--sidebar-rail-width:\s*(\d+)px/)?.[1])
    expect(breakpoint).toBeLessThan(1024 - collapsedWidth)
    expect(styles).not.toContain('@media (max-width: 1100px)')
  })
})

describe('sidebar navigation from an expanded reader', () => {
  it('restores split content in wide windows, including clicking the current destination', () => {
    expect(sidebarNavigationPlan(false, 1060, 1280, true)).toEqual({ expanded: false, hideBrowser: false })
    expect(sidebarNavigationPlan(false, 780, 1000, true)).toEqual({ expanded: false, hideBrowser: false })
  })
  it('reveals lists in narrow windows instead of leaving them CSS-hidden', () => {
    expect(sidebarNavigationPlan(false, 779, 1280, true)).toEqual({ expanded: false, hideBrowser: true })
    expect(sidebarNavigationPlan(false, 640, 860, true)).toEqual({ expanded: false, hideBrowser: true })
  })
  it('opens Settings without discarding the reader and handles missing geometry safely', () => {
    expect(sidebarNavigationPlan(true, 0, 860, true)).toEqual({ expanded: false, hideBrowser: false })
    for (const width of [0, -1, NaN, Infinity]) expect(sidebarNavigationPlan(false, width, 1280, true).hideBrowser).toBe(true)
  })
  it('matches the legacy single-column fallback on older WebViews', () => {
    expect(sidebarNavigationPlan(false, 1100, 1000, false).hideBrowser).toBe(true)
    expect(sidebarNavigationPlan(false, 1100, 1001, false).hideBrowser).toBe(false)
  })
})

describe('laptop presentation', () => {
  it('aligns brand, topic header and reader tabs without allowing flex shrink', () => {
    const styles = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    expect(styles).toContain('--shell-header-height: 48px;')
    for (const selector of ['brand', 'topbar', 'browser-tabbar', 'settings-fs-header']) {
      const rule = styles.match(new RegExp(`\\.${selector}\\s*\\{([^}]+)\\}`))?.[1]
      expect(rule).toContain('height: var(--shell-header-height)')
      expect(rule).toContain('min-height: var(--shell-header-height)')
      expect(rule).toContain('flex: none')
      // Responsive spacing overrides must not introduce a second header height.
      const rules = [...styles.matchAll(new RegExp(`\\.${selector}\\s*\\{([^}]+)\\}`, 'g'))]
      for (const match of rules) {
        for (const height of match[1]!.matchAll(/(?:^|;)\s*(?:min-)?height:\s*([^;]+)/g)) {
          expect(height[1]).toBe('var(--shell-header-height)')
        }
      }
    }
    expect(styles).toContain('position: sticky; top: var(--shell-header-height)')
    expect(styles).toContain('.workspace-frame { height: calc(100dvh - var(--shell-header-height)); }')
    expect(styles).toContain('.desk-workspace { height: calc(100vh - var(--shell-header-height)); }')
    expect(styles).not.toContain('height: calc(100vh - 57px)')
  })
  it('centers search input text without scaling glyphs or overlapping the icon', () => {
    const styles = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    const rule = styles.match(/\.topbar-search input\s*\{([^}]+)\}/)?.[1]
    expect(rule).toContain('appearance: none')
    expect(rule).toContain('line-height: calc(var(--shell-control-size) - 2px)')
    expect(rule).toContain('padding: 0 11px 0 calc(var(--shell-control-size) + 2px)')
    expect(rule).not.toContain('scaleX')
    expect(styles).toContain('.topbar-search input::placeholder')
  })
  it('uses compact, equal-size controls without crowding the header content', () => {
    const styles = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    const header = Number(styles.match(/--shell-header-height:\s*(\d+)px/)?.[1])
    const control = Number(styles.match(/--shell-control-size:\s*(\d+)px/)?.[1])
    expect(control).toBe(32)
    expect(header - control).toBe(16)
    for (const selector of ['topbar-search', 'topbar-search-toggle', 'topbar-icon-button', 'browser-tab']) {
      const rule = styles.match(new RegExp(`\\.${selector}\\s*\\{([^}]+)\\}`))?.[1]
      expect(rule).toContain('height: var(--shell-control-size)')
    }
    expect(styles.match(/\.topbar-search input\s*\{([^}]+)\}/)?.[1]).toContain('height: var(--shell-control-size)')
    const brand = styles.match(/\.brand\s*\{([^}]+)\}/)?.[1]
    expect(brand).toContain('padding: 8px 14px')
    // The 30px mark and two-line label must fit inside the header's padded area.
    const brandMarkHeight = Number(styles.match(/\.brand-mark\s*\{[^}]*height:\s*(\d+)px/)?.[1])
    expect(brandMarkHeight + 16 + 1).toBeLessThanOrEqual(header)
  })
  it('keeps default/minimum native window sizes inside a 13-inch logical work area', () => {
    const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'))
    const window = config.app.windows[0]
    expect(window.width).toBeLessThanOrEqual(1280 - 80)
    expect(window.height).toBeLessThanOrEqual(800 - 100)
    expect(window.minWidth).toBeLessThanOrEqual(window.width)
    expect(window.minHeight).toBeLessThanOrEqual(600)
    expect(window.minHeight).toBeLessThanOrEqual(window.height)
    expect(window.resizable).toBe(true)
  })
  it('provides equal filter widths and usable short-height settings and dialogs', () => {
    const styles = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    expect(styles).toContain('@container topic-list (max-width: 1050px)')
    expect(styles).toContain('grid-template-columns: repeat(4, minmax(0, 1fr))')
    expect(styles).toContain('@container topic-list (max-width: 640px)')
    expect(styles).toContain('grid-template-columns: repeat(2, minmax(0, 1fr))')
    expect(styles).toContain('@container reader (max-width: 520px)')
    expect(styles).toContain('@supports not (container-type: inline-size)')
    expect(styles).toContain('@media (max-height: 760px)')
    expect(styles).toContain('max-height: calc(100dvh - 40px); overflow-y: auto')
    expect(styles).toContain('grid-template-columns: 184px minmax(0, 1fr)')
  })
})
