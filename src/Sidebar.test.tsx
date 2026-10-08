/** Sidebar rendering and interaction regressions for compact and full navigation. */
import { Children, isValidElement } from 'react'
import type { ReactNode } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { readFileSync } from 'node:fs'
import { describe, expect, it, vi } from 'vitest'
import { Sidebar } from './Sidebar'
import type { SidebarView } from './Sidebar'
import { messages } from './i18n'

/** Inspect authored button handlers without introducing a browser or native backend. */
function buttons(node: ReactNode): { 'aria-label'?: string; onClick: () => void; children?: ReactNode }[] {
  return Children.toArray(node).flatMap((child) => {
    if (!isValidElement<{ 'aria-label'?: string; onClick: () => void; children?: ReactNode }>(child)) return []
    return child.type === 'button' ? [child.props] : buttons(child.props.children)
  })
}

describe('Sidebar', () => {
  it('keeps the footer seamless without moving its controls', () => {
    const styles = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    const footer = styles.match(/\.sidebar-footer\s*\{([^}]+)\}/)?.[1]
    expect(footer).toContain('border-top: 0')
    expect(footer).toContain('padding: 8px')
    expect(footer).toContain('display: flex; flex-direction: column')
    expect(footer).toContain('gap: 1px')
  })
  it('places Settings above the collapse/expand control in both layouts', () => {
    for (const collapsed of [false, true]) {
      const m = messages.zh
      const html = renderToStaticMarkup(<Sidebar collapsed={collapsed} view="discover" queuedTotal={0} recentTotal={0}
        version="v0.3.0" m={m} onToggle={vi.fn()} onNavigate={vi.fn()} />)
      const footer = html.slice(html.indexOf('class="sidebar-footer"'))
      expect(footer.indexOf(`aria-label="${m.navSettings}"`)).toBeLessThan(footer.indexOf('class="nav-item sidebar-toggle"'))
    }
  })
  for (const locale of ['zh', 'en'] as const) {
    for (const collapsed of [false, true]) {
      it(`preserves localized navigation and counts with collapsed=${collapsed} in ${locale}`, () => {
        const m = messages[locale]
        const html = renderToStaticMarkup(<Sidebar collapsed={collapsed} view="new" queuedTotal={0} recentTotal={151}
          version="v0.3.0" m={m} onToggle={vi.fn()} onNavigate={vi.fn()} />)
        for (const label of [m.navDiscover, m.navQueue, m.navNew, m.navSettings]) expect(html).toContain(`aria-label="${label}"`)
        expect(html).toContain(`title="${m.navNew} (151)"`)
        expect(html).toContain(`title="${m.navQueue} (0)"`)
        expect(html).toContain('aria-current="page"')
        expect(html).toContain(`aria-expanded="${!collapsed}"`)
        expect(html).toContain(`aria-label="${collapsed ? m.sidebarExpand : m.sidebarCollapse}"`)
        expect(html).toContain('aria-controls="primary-navigation"')
        if (collapsed) {
          expect(html).not.toContain('class="nav-label"')
          expect(html).not.toContain('<small>')
          expect(html).not.toContain('class="brand-title"')
          expect(html).toContain('Topic Desk STUDIO · v0.3.0')
        } else {
          expect(html).toContain('<small>0</small>')
          expect(html).toContain('<small>151</small>')
          expect(html).toContain('class="brand-title"')
        }
      })
    }
  }
  it('toggles in either direction without navigating, and keeps every destination clickable', () => {
    let collapsed = false
    const navigate = vi.fn<(view: SidebarView) => void>()
    const toggle = vi.fn(() => { collapsed = !collapsed })
    for (const state of [false, true]) {
      expect(collapsed).toBe(state)
      const controls = buttons(Sidebar({ collapsed, view: 'discover', queuedTotal: 0, recentTotal: 0,
        version: 'v0.3.0', m: messages.zh, onToggle: toggle, onNavigate: navigate }))
      controls.find((button) => button['aria-label'] === (state ? messages.zh.sidebarExpand : messages.zh.sidebarCollapse))!.onClick()
      expect(navigate).not.toHaveBeenCalled()
      for (const label of [messages.zh.navDiscover, messages.zh.navQueue, messages.zh.navNew, messages.zh.navSettings]) {
        controls.find((button) => button['aria-label'] === label)!.onClick()
      }
      expect(navigate.mock.calls.map(([view]) => view)).toEqual(['discover', 'queue', 'new', 'settings'])
      navigate.mockClear()
    }
    expect(collapsed).toBe(false)
    expect(toggle).toHaveBeenCalledTimes(2)
  })
  it('escapes labels and retains only one current destination', () => {
    const html = renderToStaticMarkup(<Sidebar collapsed view="queue" queuedTotal={0} recentTotal={0} version="v0.3.0"
      m={{ ...messages.zh, navQueue: '<script>' }} onToggle={vi.fn()} onNavigate={vi.fn()} />)
    expect(html).toContain('&lt;script&gt;')
    expect(html).not.toContain('<script>')
    expect(html.match(/aria-current="page"/g)).toHaveLength(1)
  })
  it('resizes the whole workspace, centers compact icons, and respects reduced motion', () => {
    const styles = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    expect(styles).toContain('grid-template-columns: var(--sidebar-width, 220px) minmax(0, 1fr)')
    expect(styles).toContain('.app-shell.sidebar-is-collapsed { --sidebar-width: var(--sidebar-rail-width); }')
    expect(styles).toContain('.sidebar-collapsed .nav-item { justify-content: center;')
    expect(styles).toContain('@media (prefers-reduced-motion: reduce)')
    expect(styles).toContain('.app-shell { transition: none; }')
  })
  it('fits centered 32px pointer targets inside a 48px desktop rail', () => {
    const styles = readFileSync(new URL('./styles.css', import.meta.url), 'utf8')
    const rail = Number(styles.match(/--sidebar-rail-width:\s*(\d+)px/)?.[1])
    const target = Number(styles.match(/--shell-control-size:\s*(\d+)px/)?.[1])
    const icon = Number(styles.match(/\.sidebar-collapsed \.nav-item > svg\s*\{\s*width:\s*(\d+)px/)?.[1])
    expect(rail).toBe(48)
    expect(target).toBe(32)
    expect(icon).toBe(20)
    expect(rail - 1 - target).toBeGreaterThanOrEqual(14)
    expect(target - icon).toBeGreaterThanOrEqual(12)
    // Desktop-only sizing prevents vertical rail padding from altering mobile rows.
    expect(styles).toContain('@media (min-width: 761px) {\n  .sidebar-collapsed nav, .sidebar-collapsed .sidebar-footer { padding: 8px 0; align-items: center; }')
    expect(styles).toContain('.sidebar-collapsed .nav-item { width: var(--shell-control-size); height: var(--shell-control-size); padding: 0; flex: none; }')
  })
})
