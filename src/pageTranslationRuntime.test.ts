// Deterministic reader DOM fixture tests the shipped injection, without model/network access.
import { afterEach, describe, expect, it, vi } from 'vitest'
import runtime from '../src-tauri/src/page_translation.js?raw'

class Element {
  nodeType = 1
  isConnected = true
  parentElement: Element | null = null
  children: Element[] = []
  textContent = ''
  className = ''
  top = 0
  display = 'block'
  excluded = false
  get parentNode() { return this.parentElement }
  attributes = new Map<string, string>()
  getBoundingClientRect() { return { top: this.top, bottom: this.top + 20, left: 0, right: 200, width: 200, height: 20 } }
  closest(selector: string): Element | null { return this.className === 'topic-desk-translation' || (selector !== '.topic-desk-translation' && this.excluded) ? this : this.parentElement?.closest(selector) ?? null }
  appendChild(node: Element) { this.children.push(node); node.parentElement = this }
  remove() { this.isConnected = false }
  setAttribute(key: string, value: string) { this.attributes.set(key, value) }
  removeAttribute(key: string) { this.attributes.delete(key) }
  hasAttribute(key: string) { return this.attributes.has(key) }
}
type TextNode = { parentElement: Element; textContent: string }
type State = { token: string | null; slots: Element[]; texts: string[]; split(text: string): string[]; prioritize(ids: number[]): { valid: number[]; ready: number[] }; retry(index: number): void; collect(): { texts: string[] }; stop(): void; apply(start: number, values: string[], language: string, error: string | null): number; applyEntries(entries: [number, string][], language: string, error: string | null): number; applyPartial(entries: [number, string][], language: string): void }
function fixture(nodes: TextNode[]) {
  const order = new Map<TextNode, number>()
  const prepare = (node: TextNode) => {
    if (order.has(node)) return
    order.set(node, order.size)
    Object.defineProperties(node, {
      nodeType: { value: 3 },
      parentNode: { get: () => node.parentElement },
      isConnected: { get: () => node.parentElement.isConnected },
      compareDocumentPosition: { value: (other: TextNode) => (order.get(node) ?? 0) < (order.get(other) ?? 0) ? 4 : 2 },
    })
  }
  nodes.forEach(prepare)
  type Change = { target: TextNode | Element; type: string; addedNodes?: TextNode[]; removedNodes?: TextNode[] }
  let records: Change[] = []
  let intersectionCallback: (entries: { target: Element; isIntersecting: boolean }[]) => void
  const disconnect = vi.fn()
  vi.stubGlobal('IntersectionObserver', class {
    constructor(callback: typeof intersectionCallback) { intersectionCallback = callback }
    observe() {}
    unobserve() {}
    disconnect = disconnect
  })
  vi.stubGlobal('MutationObserver', class {
    observe() {}
    takeRecords() { const result = records; records = []; return result }
    disconnect = disconnect
  })
  vi.stubGlobal('addEventListener', vi.fn())
  vi.stubGlobal('removeEventListener', vi.fn())
  const doc = {
    hidden: false, body: new Element(), head: new Element(), documentElement: { lang: 'en' },
    addEventListener: vi.fn(), removeEventListener: vi.fn(),
    createElement: () => new Element(),
    createRange: () => { let selected: TextNode; return { selectNodeContents: (node: TextNode) => { selected = node }, getClientRects: () => [selected.parentElement.getBoundingClientRect()] } },
    createTreeWalker: vi.fn((root: Element) => {
      let index = 0
      const selected = root === doc.body ? nodes : nodes.filter(n => {
        for (let element: Element | null = n.parentElement; element; element = element.parentElement) if (element === root) return true
        return false
      })
      return { nextNode: () => selected[index++] ?? null }
    }),
  }
  vi.stubGlobal('document', doc)
  vi.stubGlobal('innerHeight', 600); vi.stubGlobal('innerWidth', 800)
  vi.stubGlobal('NodeFilter', { SHOW_TEXT: 4 })
  const styles = vi.fn((e: Element) => ({ display: e.display, visibility: 'visible', opacity: '1' }))
  vi.stubGlobal('getComputedStyle', styles)
  vi.stubGlobal('__topicDeskPageTranslation', undefined)
  const run = new Function(`return (${runtime})('test-session')`)
  const started = performance.now()
  const initial = run() as { texts: string[] }
  const initialMs = performance.now() - started
  const state = (globalThis as unknown as { __topicDeskPageTranslation: State }).__topicDeskPageTranslation
  return { initial, state, doc, styles, disconnect, initialMs,
    intersect: (element: Element, value = true) => intersectionCallback([{ target: element, isIntersecting: value }]),
    mutate: (record: Change) => { record.addedNodes?.forEach(node => { if (!(node instanceof Element)) prepare(node) }); records.push(record) },
  }
}
afterEach(() => vi.unstubAllGlobals())
describe('on-demand reader translation runtime', () => {
  it('shows stream text immediately but caches only the final answer', () => {
    const first = new Element()
    const { state } = fixture([{ parentElement: first, textContent: 'English paragraph' }])
    const slot = first.children[0]!
    state.applyPartial([[0, '英文']], 'zh-CN')
    expect(slot.textContent).toBe('英文')
    expect(slot.hasAttribute('data-loading')).toBe(true)
    state.applyEntries([[0, '英文段落']], 'zh-CN', null)
    expect(slot.textContent).toBe('英文段落')
    expect(slot.hasAttribute('data-loading')).toBe(false)
    state.applyPartial([[0, '晚到的片段']], 'zh-CN')
    expect(slot.textContent).toBe('英文段落')
  })
  it('replaces an interrupted stream with a retryable error', () => {
    const first = new Element()
    const { state } = fixture([{ parentElement: first, textContent: 'English paragraph' }])
    const slot = first.children[0]!
    state.applyPartial([[0, '未完成']], 'zh-CN')
    state.applyEntries([[0, '']], 'zh-CN', 'stream interrupted')
    expect(slot.textContent).toContain('点击重试')
    expect(slot.hasAttribute('data-error')).toBe(true)
    state.retry(0)
    expect(slot.textContent).toBe('')
    expect(slot.hasAttribute('data-loading')).toBe(true)
    expect(state.collect().texts).toEqual(['English paragraph'])
  })
  it('writes a whole model batch to non-contiguous page slots in one call', () => {
    const first = new Element(), second = new Element()
    const { state } = fixture([
      { parentElement: first, textContent: 'First paragraph' },
      { parentElement: second, textContent: 'Second paragraph' },
    ])
    expect(state.applyEntries([[1, '第二段'], [0, '第一段']], 'zh-CN', null)).toBe(2)
    expect(first.children[0]?.textContent).toBe('第一段')
    expect(second.children[0]?.textContent).toBe('第二段')
  })
  it('benchmarks a 20,000-text-node article fixture without model requests', () => {
    const nodes: TextNode[] = []
    for (let i = 0; i < 4000; i++) {
      const element = new Element(); element.top = i * 40
      for (let j = 0; j < 5; j++) nodes.push({ parentElement: element, textContent: `Paragraph ${i} fragment ${j}. ` })
    }
    const { state, initialMs } = fixture(nodes)
    const idleStart = performance.now()
    for (let i = 0; i < 100; i++) state.collect()
    console.info(JSON.stringify({ fixtureTextNodes: nodes.length, initialMs, idle100Ms: performance.now() - idleStart }))
    state.stop()
  })
  it('queues only the viewport, then newly visible text once', () => {
    const first = new Element(), below = new Element(); below.top = 900
    const { initial, state, intersect } = fixture([{ parentElement: first, textContent: 'First' }, { parentElement: below, textContent: 'Below' }])
    expect(initial.texts).toEqual(['First'])
    expect(state.collect().texts).toEqual([])
    first.top = -50; below.top = 100
    intersect(first, false); intersect(below)
    expect(state.collect().texts).toEqual(['Below'])
    expect(state.collect().texts).toEqual([])
  })
  it('keeps completed paragraphs translated while scrolling down and back up', () => {
    const first = new Element(), second = new Element(); second.top = 900
    const { initial, state, intersect } = fixture([
      { parentElement: first, textContent: 'First paragraph' },
      { parentElement: second, textContent: 'Second paragraph' },
    ])
    expect(initial.texts).toEqual(['First paragraph'])
    state.apply(0, ['第一段'], 'zh', null)
    const firstTranslation = first.children[0]!
    first.top = -100; second.top = 100
    intersect(first, false); intersect(second)
    expect(state.collect().texts).toEqual(['Second paragraph'])
    state.apply(1, ['第二段'], 'zh', null)
    first.top = 100; second.top = 900
    intersect(second, false); intersect(first)
    expect(state.collect().texts).toEqual([])
    expect(firstTranslation.textContent).toBe('第一段')
    expect(firstTranslation.isConnected).toBe(true)
    expect(state.collect().texts).toEqual([])
  })
  it('reuses completed translation when a virtualized page recreates the paragraph', () => {
    const oldElement = new Element()
    const oldNode = { parentElement: oldElement, textContent: 'Repeated content' }
    const nodes = [oldNode]
    const { state, mutate, doc } = fixture(nodes)
    state.apply(0, ['缓存译文'], 'zh', null)
    const newElement = new Element()
    const newNode = { parentElement: newElement, textContent: 'Repeated content' }
    nodes.push(newNode)
    oldElement.isConnected = false
    mutate({ target: doc.body, type: 'childList', removedNodes: [oldNode], addedNodes: [newNode] })
    expect(state.collect().texts).toEqual([])
    expect(newElement.children.map(slot => slot.textContent)).toEqual(['缓存译文'])
    expect(state.collect().texts).toEqual([])
  })
  it('reuses every completed part of a long paragraph without another request', () => {
    const paragraph = 'Long sentence. '.repeat(110)
    const oldElement = new Element(), oldNode = { parentElement: oldElement, textContent: paragraph }
    const nodes = [oldNode]
    const { initial, state, mutate, doc } = fixture(nodes)
    expect(initial.texts.length).toBeGreaterThan(1)
    initial.texts.forEach((_, index) => state.apply(index, [`part-${index}`], 'zh', null))
    const newElement = new Element(), newNode = { parentElement: newElement, textContent: paragraph }
    nodes.push(newNode); oldElement.isConnected = false
    mutate({ target: doc.body, type: 'childList', removedNodes: [oldNode], addedNodes: [newNode] })
    expect(state.collect().texts).toEqual([])
    expect(newElement.children.map(slot => slot.textContent)).toEqual(initial.texts.map((_, index) => `part-${index}`))
  })
  it('queues only the changed part when a long paragraph is rebuilt', () => {
    const firstPart = 'A'.repeat(900)
    const oldElement = new Element(), oldNode = { parentElement: oldElement, textContent: firstPart + 'B'.repeat(50) }
    const nodes = [oldNode]
    const { initial, state, mutate, doc } = fixture(nodes)
    expect(initial.texts).toEqual([firstPart, 'B'.repeat(50)])
    state.applyEntries([[0, '已译第一段'], [1, '已译第二段']], 'zh-CN', null)
    const newElement = new Element(), newNode = { parentElement: newElement, textContent: firstPart + 'C'.repeat(50) }
    nodes.push(newNode); oldElement.isConnected = false
    mutate({ target: doc.body, type: 'childList', removedNodes: [oldNode], addedNodes: [newNode] })
    expect(state.collect().texts).toEqual(['C'.repeat(50)])
    expect(newElement.children[0]?.textContent).toBe('已译第一段')
    expect(newElement.children[1]?.hasAttribute('data-loading')).toBe(true)
  })
  it('performs no traversal or style reads on idle drains or translation writes', () => {
    const { state, doc, styles, mutate } = fixture([{ parentElement: new Element(), textContent: 'First' }])
    doc.createTreeWalker.mockClear(); styles.mockClear()
    const slot = state.slots[0]!
    state.apply(0, ['译文'], 'zh', null)
    mutate({ target: slot, type: 'childList', addedNodes: [], removedNodes: [] })
    for (let i = 0; i < 100; i++) expect(state.collect().texts).toEqual([])
    expect(doc.createTreeWalker).not.toHaveBeenCalled()
    expect(styles).not.toHaveBeenCalled()
  })
  it('reindexes only edited text, not the rest of the article', () => {
    const nodes = Array.from({ length: 100 }, (_, i) => ({ parentElement: new Element(), textContent: `Text ${i}` }))
    const { state, doc, styles, mutate } = fixture(nodes)
    while (state.collect().texts.length) { /* Drain initial visible overflow. */ }
    doc.createTreeWalker.mockClear(); styles.mockClear()
    nodes[0]!.textContent = 'Changed'
    mutate({ target: nodes[0]!, type: 'characterData' })
    expect(state.collect().texts).toEqual(['Changed'])
    expect(doc.createTreeWalker).not.toHaveBeenCalled()
    expect(styles.mock.calls.length).toBeLessThanOrEqual(2)
  })
  it('disconnects observers and does not restart after restoration', () => {
    const { state, disconnect, doc } = fixture([{ parentElement: new Element(), textContent: 'First' }])
    state.stop(); doc.createTreeWalker.mockClear()
    expect(disconnect).toHaveBeenCalledTimes(2)
    expect(state.collect().texts).toEqual([])
    expect(doc.createTreeWalker).not.toHaveBeenCalled()
    expect((globalThis as unknown as { __topicDeskPageTranslation: State | null }).__topicDeskPageTranslation).toBeNull()
  })
  it('releases translated text and pending work when content is replaced', () => {
    const node = { parentElement: new Element(), textContent: 'Original' }
    const { state, mutate } = fixture([node])
    const oldSlot = state.slots[0]!
    state.apply(0, ['译文'], 'zh', null)
    expect(state.slots[0]).toBeUndefined()
    expect(state.texts[0]).toBeUndefined()
    node.textContent = 'Updated'
    mutate({ target: node, type: 'characterData' })
    expect(state.collect().texts).toEqual(['Updated'])
    expect(oldSlot.isConnected).toBe(false)
    expect(state.prioritize([0, 1])).toEqual({ valid: [1], ready: [1] })
  })
  it('removes stale output when source text is removed', () => {
    const node = { parentElement: new Element(), textContent: 'First' }
    const { state, mutate } = fixture([node])
    const slot = state.slots[0]!
    state.apply(0, ['译文'], 'zh', null)
    mutate({ target: node.parentElement, type: 'childList', addedNodes: [], removedNodes: [node] })
    expect(state.collect().texts).toEqual([])
    expect(slot.isConnected).toBe(false)
  })
  it('does not retranslate surrounding text when spinners are inserted', () => {
    const owner = new Element()
    const { state, mutate, doc } = fixture([{ parentElement: owner, textContent: 'First' }])
    doc.createTreeWalker.mockClear()
    // Browser mutation records for owned output must never invalidate the index.
    mutate({ target: owner, type: 'childList', addedNodes: [state.slots[0] as unknown as TextNode], removedNodes: [] })
    expect(state.collect().texts).toEqual([])
    expect(doc.createTreeWalker).not.toHaveBeenCalled()
  })
  it('retains inline words and parent text around a nested block', () => {
    const parent = new Element(), inline = new Element(), child = new Element()
    inline.parentElement = parent; inline.display = 'inline'; child.parentElement = parent
    const { initial } = fixture([{ parentElement: parent, textContent: 'Read ' }, { parentElement: inline, textContent: 'this link' }, { parentElement: child, textContent: 'Nested paragraph' }])
    expect(initial.texts).toEqual(['Read this link', 'Nested paragraph'])
  })
  it('preserves all long text including surrogate pairs without page limits', () => {
    const text = 'Long 🐱 text '.repeat(1000)
    const { initial } = fixture([{ parentElement: new Element(), textContent: text }])
    expect(initial.texts.join('')).toBe(text.trim())
    expect(initial.texts.every(p => Array.from(p).length <= 900)).toBe(true)
  })
  it('detects inserted and updated text without retaining stale translations', () => {
    const node = { parentElement: new Element(), textContent: 'Before' }, nodes = [node]
    const { state, mutate } = fixture(nodes)
    node.textContent = 'After'
    mutate({ target: node, type: 'characterData' })
    expect(state.collect().texts).toEqual(['After'])
    expect(state.slots[0]).toBeUndefined()
    expect(state.apply(0, ['stale'], 'zh', null)).toBe(0)
    nodes.push({ parentElement: new Element(), textContent: 'Inserted' })
    mutate({ target: node.parentElement, type: 'childList', addedNodes: [nodes[1]!], removedNodes: [] })
    expect(state.collect().texts).toEqual(['Inserted'])
  })
  it('excludes protected text, hidden pages and disconnected blocks', () => {
    const protectedElement = new Element(); protectedElement.excluded = true
    const detached = new Element(); detached.isConnected = false
    const nodes = [{ parentElement: protectedElement, textContent: 'Code' }, { parentElement: detached, textContent: 'Detached' }]
    const { initial, state, doc } = fixture(nodes)
    expect(initial.texts).toEqual([])
    nodes.push({ parentElement: new Element(), textContent: 'New' }); doc.hidden = true
    expect(state.collect().texts).toEqual([])
  })
  it('shows batch failures and removes indicators on restore', () => {
    const { state } = fixture([{ parentElement: new Element(), textContent: 'Test' }])
    const slot = state.slots[0]!
    state.apply(0, [''], 'zh', 'Request failed')
    expect(slot.textContent).toBe('⚠ 翻译未完成 · 点击重试')
    expect(slot.attributes.get('title')).toBe('Request failed')
    expect(slot.attributes.has('data-error')).toBe(true)
    expect(slot.attributes.has('data-loading')).toBe(false)
    state.stop()
    expect(state.token).toBe(null)
    expect(slot.isConnected).toBe(false)
  })
  it('adds translated text without replacing original text or markup', () => {
    const original = { parentElement: new Element(), textContent: 'Original text' }
    const { state } = fixture([original])
    const slot = state.slots[0]!
    expect(state.apply(0, ['译文'], 'zh', null)).toBe(1)
    expect(original.textContent).toBe('Original text')
    expect(slot.textContent).toBe('译文')
    state.stop()
    expect(original.parentElement.isConnected).toBe(true)
  })
  it('suppresses unchanged names and whitespace-only differences', () => {
    const { state } = fixture([{ parentElement: new Element(), textContent: 'Kevin Feng' }])
    const slot = state.slots[0]!
    state.apply(0, ['  Kevin   Feng  '], 'zh', null)
    expect(slot.isConnected).toBe(false)
    expect(state.collect().texts).toEqual([])
  })
  it('retries only failed segments once per click cycle and retains successes', () => {
    const { state } = fixture([{ parentElement: new Element(), textContent: 'First' }, { parentElement: new Element(), textContent: 'Second' }])
    const firstSlot = state.slots[0]!, secondSlot = state.slots[1]!
    state.apply(0, ['成功'], 'zh', null); state.apply(1, [''], 'zh', 'Failed')
    state.retry(0); state.retry(1); state.retry(1)
    expect(state.collect().texts).toEqual(['Second'])
    state.apply(2, ['重试成功'], 'zh', null)
    expect(firstSlot.textContent).toBe('成功')
    expect(secondSlot.textContent).toBe('重试成功')
    expect(state.collect().texts).toEqual([])
  })
  it('prioritizes the current viewport and excludes detached work', () => {
    const first = new Element(), second = new Element()
    second.top = 100
    const { state } = fixture([{ parentElement: first, textContent: 'First' }, { parentElement: second, textContent: 'Second' }])
    expect(state.prioritize([1, 0])).toEqual({ valid: [1, 0], ready: [0, 1] })
    first.top = -30
    expect(state.prioritize([0, 1])).toEqual({ valid: [0, 1], ready: [1] })
    state.slots[1]!.remove()
    expect(state.prioritize([0, 1])).toEqual({ valid: [0], ready: [] })
  })
  it('splits at sentence boundaries before falling back to whitespace or hard limits', () => {
    const { state } = fixture([])
    const sentence = 'a'.repeat(600) + '. '
    const text = sentence + 'word '.repeat(200)
    const parts = state.split(text)
    expect(parts[0]).toBe(sentence.trimEnd())
    expect(parts.join('')).toBe(text)
    expect(state.split('🐱'.repeat(1801)).map(p => Array.from(p).length)).toEqual([900, 900, 1])
  })
  it('defers overflow to subsequent dispatches rather than discarding it', () => {
    const { initial, state } = fixture(Array.from({ length: 81 }, (_, i) => ({ parentElement: new Element(), textContent: String(i) })))
    const all = [...initial.texts, ...state.collect().texts, ...state.collect().texts, ...state.collect().texts]
    expect(all).toHaveLength(81)
    expect(new Set(all).size).toBe(81)
  })
})
