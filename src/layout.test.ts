/** Laptop-size regression rules: bounded split panes, responsive CSS, and native defaults. */
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { clampReaderWidth, DEFAULT_READER_PERCENT, readerColumns, READER_PANE_MIN_WIDTH, sidebarNavigationPlan, SPLIT_WORKSPACE_MIN_WIDTH, TOPIC_PANE_MIN_WIDTH } from './layout'

describe('reader sizing', () => {
  it('preserves valid proportions and clamps pointer/keyboard extremes', () => {
    for (const [input, expected] of [[0, 35], [34, 35], [35, 35], [60, 60], [70, 70], [100, 70], [-1, 35]]) {
      expect(clampReaderWidth(input!)).toBe(expected)
    }
    expect(readerColumns(60)).toBe('minmax(320px, 40fr) minmax(420px, 60fr)')
    expect(readerColumns(100)).toBe('minmax(320px, 30fr) minmax(420px, 70fr)')
  })
  it('recovers invalid geometry without NaN or Infinity styles', () => {
    for (const input of [NaN, Infinity, -Infinity]) {
      expect(clampReaderWidth(input)).toBe(DEFAULT_READER_PERCENT)
      expect(readerColumns(input)).toBe(readerColumns(DEFAULT_READER_PERCENT))
    }
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
