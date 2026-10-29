/** Deterministic unprivileged overlay tests: no layout reservation, safe text, lifecycle and popup routing. */
import { readFileSync } from 'node:fs'
import vm from 'node:vm'
import { describe, expect, it, vi } from 'vitest'
const script = readFileSync(new URL('../src-tauri/src/page_topic_rail.js', import.meta.url), 'utf8')

function fixture() {
  class Element {
    children: Element[] = []; parent: Element | null = null; shadow: Element | null = null
    id = ''; className = ''; textContent = ''; dataset: Record<string, string> = {}
    style = { cssText: '', top: '' }; attributes: Record<string, string> = {}
    onclick?: () => void; onpointerenter?: () => void; onfocus?: () => void
    onpointerleave?: () => void; onscroll?: () => void
    onkeydown?: (event: { key: string; stopPropagation(): void }) => void
    onfocusout?: (event: { relatedTarget: Element | null }) => void
    constructor(readonly tag: string) {}
    append(...nodes: Element[]) { nodes.forEach(node => { node.parent = this; this.children.push(node) }) }
    remove() { if (this.parent) this.parent.children = this.parent.children.filter(node => node !== this) }
    setAttribute(key: string, value: string) { this.attributes[key] = value }
    attachShadow() { this.shadow = new Element('shadow'); return this.shadow }
    getBoundingClientRect() { return { top: 300 } }
    contains(node: Element | null): boolean { return this === node || this.children.some(child => child.contains(node)) }
  }
  const documentElement = new Element('html'), open = vi.fn()
  const context = vm.createContext({
    document: { documentElement, createElement: (tag: string) => new Element(tag),
      getElementById: (id: string) => documentElement.children.find(child => child.id === id) },
    window: { open }, URL, innerHeight: 800, matchMedia: () => ({ matches: false }),
  })
  const data = { visible: true, active: 1, locale: 'zh', theme: 'light',
    topics: [{ id: 1, title: '<img src=x onerror=alert(1)>', platformName: 'AI News', url: 'https://example.com/story#original' }] }
  const run = (updates = {}) => vm.runInContext(`(${script})(${JSON.stringify({ ...data, ...updates })})`, context)
  const nodes = () => {
    const host = documentElement.children[0]!, root = host.shadow!, nav = root.children[1]!, markers = nav.children[0]!
    return { host, root, nav, markers, marker: markers.children[0]! }
  }
  return { run, nodes, documentElement, data, open }
}

describe('page-local topic rail', () => {
  it('keeps opaque markers legible on light and dark pages independently of app theme', () => {
    const luminance = (hex: string) => {
      const rgb = hex.match(/[a-f0-9]{2}/gi)!.map(value => {
        const channel = parseInt(value, 16) / 255
        return channel <= .04045 ? channel / 12.92 : ((channel + .055) / 1.055) ** 2.4
      })
      return rgb[0]! * .2126 + rgb[1]! * .7152 + rgb[2]! * .0722
    }
    const contrast = (a: string, b: string) => {
      const values = [luminance(a), luminance(b)].sort((x, y) => y - x)
      return (values[0]! + .05) / (values[1]! + .05)
    }
    for (const theme of ['light', 'dark', 'system']) {
      const f = fixture(); f.run({ theme })
      const css = f.nodes().root.children[0]!.textContent
      const marker = css.match(/\.marker i\{([^}]+)\}/)![1]!
      expect(marker).toContain('opacity:1')
      const fill = marker.match(/background:(#[a-f0-9]{6})/)![1]!
      const outline = marker.match(/box-shadow:0 0 0 1px (#[a-f0-9]{6})/)![1]!
      for (const background of ['#18191d', '#000000', '#ffffff', '#f4f4f8']) {
        expect(Math.max(contrast(fill, background), contrast(outline, background))).toBeGreaterThan(3)
      }
      const active = css.match(/\.marker\[aria-current\] i\{([^}]+)\}/)![1]!
      expect(contrast(active.match(/background:(#[a-f0-9]{6})/)![1]!, '#18191d')).toBeGreaterThan(7)
    }
  })

  it('floats over the page with transparent hit testing and centers scrollable markers without modifying page geometry', () => {
    const f = fixture(); f.run()
    const { host, root } = f.nodes()
    expect(host.style.cssText).toContain('position:fixed!important')
    expect(host.style.cssText).toContain('pointer-events:none!important')
    expect(host.style.cssText).not.toMatch(/margin|padding/)
    expect(f.documentElement.style.cssText).toBe('')
    expect(root.children[0]!.textContent).toContain('align-items:center')
    expect(root.children[0]!.textContent).toContain('overflow-y:auto')
    expect(host.attributes.translate).toBe('no')
    expect(root.children[0]!.textContent).toContain('prefers-reduced-motion')
  })

  it('renders untrusted text as text and routes a new-tab request without changing the original URL', () => {
    const f = fixture(); f.run()
    const { nav, marker } = f.nodes()
    marker.onpointerenter!()
    const card = nav.children[1]!.children[0]!
    expect(card.children[1]!.textContent).toBe(f.data.topics[0]!.title)
    expect(card.children[1]!.children).toHaveLength(0)
    expect(card.children[2]!.textContent).toContain('在新页签')
    card.onclick!()
    const [address, target] = f.open.mock.calls[0]!
    const request = new URL(address)
    expect(request.protocol).toBe('topic-desk-topic:')
    expect(JSON.parse(request.searchParams.get('topic')!)).toEqual(f.data.topics[0])
    expect(target).toBe('_blank')
    expect(nav.children).toHaveLength(1)
  })

  it('supports keyboard focus, Escape, leaving the rail and scrolling', () => {
    const f = fixture(); f.run()
    const { nav, markers, marker } = f.nodes()
    marker.onfocus!()
    expect(nav.children).toHaveLength(2)
    nav.onfocusout!({ relatedTarget: nav.children[1]!.children[0]! })
    expect(nav.children).toHaveLength(2)
    const stopPropagation = vi.fn()
    nav.onkeydown!({ key: 'Escape', stopPropagation })
    expect(nav.children).toHaveLength(1)
    expect(stopPropagation).toHaveBeenCalledOnce()
    marker.onpointerenter!(); nav.onpointerleave!()
    expect(nav.children).toHaveLength(1)
    marker.onpointerenter!(); markers.onscroll!()
    expect(nav.children).toHaveLength(1)
    marker.onfocus!(); nav.onfocusout!({ relatedTarget: null })
    expect(nav.children).toHaveLength(1)
    marker.onclick!()
    expect(f.open).toHaveBeenCalledOnce()
  })

  it('deduplicates status refreshes and removes overlays when expanded or empty', () => {
    const f = fixture(); f.run()
    const original = f.nodes().host
    f.run()
    expect(f.nodes().host).toBe(original)
    f.run({ active: 2 })
    expect(f.nodes().host).not.toBe(original)
    expect(f.nodes().marker.attributes['aria-current']).toBeUndefined()
    f.run({ visible: false })
    expect(f.documentElement.children).toHaveLength(0)
    f.run({ topics: [] })
    expect(f.documentElement.children).toHaveLength(0)
  })

  it('localizes the preview and follows dark mode', () => {
    const f = fixture(); f.run({ locale: 'en', theme: 'dark' })
    const { root, nav, marker } = f.nodes()
    marker.onfocus!()
    expect(nav.children[1]!.children[0]!.children[2]!.textContent).toContain('Read in a new tab')
    expect(root.children[0]!.textContent).toContain('color-scheme:dark')
  })
})
