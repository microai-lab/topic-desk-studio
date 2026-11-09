/** Find dialog counts and navigation must agree, without privileged APIs or document layout mutations. */
import { readFileSync } from 'node:fs'
import { runInNewContext } from 'node:vm'
import { expect, it, vi } from 'vitest'

function fixture(texts: string[], locale = 'zh') {
  const controls = new Map<string, any>()
  const element = () => ({ setAttribute: vi.fn(), focus: vi.fn(), select: vi.fn(), remove: vi.fn(), value: '', textContent: '', disabled: false })
  for (const selector of ['input', 'form', 'output', '[data-prev]', '[data-next]', '[data-close]']) controls.set(selector, element())
  const root = { innerHTML: '', querySelector: (selector: string) => controls.get(selector) }
  const host = { ...element(), contains: () => false, style: { cssText: '' }, attachShadow: () => root, onkeydown: undefined as any }
  const block = {}
  const parent = { closest: (selector: string) => selector.startsWith('script') ? null : block, getClientRects: () => [1], scrollIntoView: vi.fn() }
  const nodes = texts.map(textContent => ({ textContent, parentElement: parent }))
  const selection = { removeAllRanges: vi.fn(), addRange: vi.fn() }
  let mounted = false
  host.remove = vi.fn(() => { mounted = false })
  const append = vi.fn(() => { mounted = true })
  const addEventListener = vi.fn()
  const document = {
    body: {}, addEventListener, getElementById: () => mounted ? host : null, createElement: () => host, documentElement: { append },
    createTreeWalker: () => { let index = 0; return { nextNode: () => nodes[index++] } },
    createRange: () => ({ startContainer: nodes[0], setStart(node: any, offset: number) { this.startContainer = node; this.start = offset }, setEnd: vi.fn(), start: 0 }),
  }
  const show = runInNewContext(`(${readFileSync(new URL('../src-tauri/src/find_panel.js', import.meta.url), 'utf8')})`, { document, window: { getSelection: () => selection }, setTimeout, clearTimeout })
  show(true, locale)
  const type = (value: string) => { controls.get('input').value = value; controls.get('input').oninput() }
  const query = (value: string) => { type(value); host.onkeydown({ key: 'Enter', preventDefault: vi.fn() }) }
  const next = () => controls.get('form').onsubmit({ preventDefault: vi.fn() })
  return { controls, host, root, nodes, parent, selection, append, query, type, next, show, addEventListener }
}

it('floats without resizing, counts inline-spanning matches and cycles both directions', () => {
  const f = fixture(['Ar', 'ticle article ARTICLE'])
  expect(f.host.style.cssText).toContain('position:fixed!important')
  expect(f.append).toHaveBeenCalledWith(f.host)
  expect(f.controls.get('output').textContent).toBe('0/0')
  expect(f.controls.get('[data-next]').disabled).toBe(true)
  f.query('article')
  expect(f.controls.get('output').textContent).toBe('1/3')
  f.next()
  expect(f.controls.get('output').textContent).toBe('2/3')
  f.controls.get('[data-prev]').onclick()
  expect(f.controls.get('output').textContent).toBe('1/3')
  f.controls.get('[data-prev]').onclick()
  expect(f.controls.get('output').textContent).toBe('3/3')
  expect(f.selection.addRange).toHaveBeenCalled()
  f.host.onkeydown({ key: 'Escape', preventDefault: vi.fn() })
  expect(f.host.remove).toHaveBeenCalledOnce()
})

it('handles literal regex punctuation, Chinese, empty input and missing results', () => {
  const f = fixture(['a+b a+b 比较比较'], 'en')
  expect(f.controls.get('input').placeholder).toBe('Find in page')
  for (const [query, count] of [['a+b', '1/2'], ['比较', '1/2'], ['missing', '0/0'], ['', '0/0']]) {
    f.query(query!)
    expect(f.controls.get('output').textContent).toBe(count)
  }
  expect(f.controls.get('[data-prev]').disabled).toBe(true)
  f.next()
  expect(f.controls.get('output').textContent).toBe('0/0')
})

it('does not join separate blocks or include hidden page content', () => {
  const f = fixture(['abc', 'def', 'abc'])
  f.nodes[1]!.parentElement = { ...f.parent, closest: (selector: string) => selector.startsWith('script') ? null : {} }
  f.nodes[2]!.parentElement = { ...f.parent, getClientRects: () => [] }
  f.query('abcdef')
  expect(f.controls.get('output').textContent).toBe('0/0')
  f.query('abc')
  expect(f.controls.get('output').textContent).toBe('1/1')
})

it('reuses an open dialog and closes it on demand', () => {
  const host = { focus: vi.fn(), remove: vi.fn() }
  const document = { getElementById: () => host, addEventListener: vi.fn() }
  const show = runInNewContext(`(${readFileSync(new URL('../src-tauri/src/find_panel.js', import.meta.url), 'utf8')})`, { document })
  show(true, 'en'); expect(host.focus).toHaveBeenCalledOnce()
  show(false, 'en'); expect(host.remove).toHaveBeenCalledOnce()
})

it('reopens with repeated Command+F after close and refocuses an existing input without duplicate listeners', () => {
  const f = fixture(['article'])
  const shortcut = f.addEventListener.mock.calls[0]![1]
  const key = (metaKey = true, ctrlKey = false, value = 'f') => ({ key: value, metaKey, ctrlKey, preventDefault: vi.fn(), stopPropagation: vi.fn() })
  for (let i = 0; i < 3; i++) {
    f.controls.get('[data-close]').onclick()
    shortcut(key())
    shortcut(key())
    expect(f.controls.get('input').select).toHaveBeenCalledTimes(i + 1)
  }
  expect(f.append).toHaveBeenCalledTimes(4)
  expect(f.addEventListener).toHaveBeenCalledExactlyOnceWith('keydown', expect.any(Function), true)
  f.controls.get('[data-close]').onclick()
  shortcut(key(false, true, 'F'))
  expect(f.append).toHaveBeenCalledTimes(5)
  shortcut(key(false, false))
  shortcut({ ...key(), isComposing: true })
  shortcut({ ...key(), altKey: true })
  expect(f.append).toHaveBeenCalledTimes(5)
})

it('can install the article shortcut before opening the dialog', () => {
  const addEventListener = vi.fn()
  const document = { getElementById: () => null, addEventListener }
  const show = runInNewContext(`(${readFileSync(new URL('../src-tauri/src/find_panel.js', import.meta.url), 'utf8')})`, { document })
  expect(show(false, 'en')).toBe(true)
  expect(addEventListener).toHaveBeenCalledExactlyOnceWith('keydown', expect.any(Function), true)
})

it('preserves the native IME composition caret and waits for Enter after committing text', async () => {
  vi.useFakeTimers()
  try {
    const f = fixture(['中文 中文'])
    const input = f.controls.get('input')
    f.selection.removeAllRanges.mockClear(); input.focus.mockClear()
    input.oncompositionstart()
    input.value = 'zhong'
    input.oninput({ isComposing: true })
    f.host.onkeydown({ key: 'Enter', shiftKey: true, preventDefault: vi.fn() })
    f.host.onkeydown({ key: 'Escape', preventDefault: vi.fn() })
    f.next()
    expect(f.selection.removeAllRanges).not.toHaveBeenCalled()
    expect(f.selection.addRange).not.toHaveBeenCalled()
    expect(input.focus).not.toHaveBeenCalled()
    expect(f.host.remove).not.toHaveBeenCalled()
    input.value = '中文'
    input.oncompositionend()
    await vi.runAllTimersAsync()
    expect(f.selection.addRange).not.toHaveBeenCalled()
    f.host.onkeydown({ key: 'Enter', preventDefault: vi.fn() })
    expect(f.controls.get('output').textContent).toBe('1/2')
    expect(f.selection.addRange).toHaveBeenCalledOnce()
  } finally { vi.useRealTimers() }
})

it('does not search on composition-end or final input and ignores legacy composing key events', async () => {
  vi.useFakeTimers()
  try {
    const f = fixture(['中文'])
    const input = f.controls.get('input')
    input.oncompositionstart(); input.value = '中文'; input.oncompositionend()
    input.oninput({ isComposing: false })
    await vi.runAllTimersAsync()
    expect(f.selection.addRange).not.toHaveBeenCalled()
    f.host.onkeydown({ key: 'Enter', preventDefault: vi.fn() })
    expect(f.selection.addRange).toHaveBeenCalledOnce()
    f.host.onkeydown({ key: 'Escape', keyCode: 229, preventDefault: vi.fn() })
    expect(f.host.remove).not.toHaveBeenCalled()
  } finally { vi.useRealTimers() }
})

it('does not select or scroll while typing, starts on Enter, and navigates only the submitted query', () => {
  const f = fixture(['first first second'])
  f.type('first')
  expect(f.controls.get('output').textContent).toBe('0/0')
  expect(f.selection.addRange).not.toHaveBeenCalled()
  expect(f.parent.scrollIntoView).not.toHaveBeenCalled()
  f.host.onkeydown({ key: 'Enter', preventDefault: vi.fn() })
  expect(f.controls.get('output').textContent).toBe('1/2')
  f.host.onkeydown({ key: 'Enter', preventDefault: vi.fn() })
  expect(f.controls.get('output').textContent).toBe('2/2')
  f.host.onkeydown({ key: 'Enter', shiftKey: true, preventDefault: vi.fn() })
  expect(f.controls.get('output').textContent).toBe('1/2')
  f.selection.addRange.mockClear(); f.parent.scrollIntoView.mockClear()
  f.type('second')
  expect(f.controls.get('output').textContent).toBe('0/0')
  expect(f.selection.addRange).not.toHaveBeenCalled()
  expect(f.parent.scrollIntoView).not.toHaveBeenCalled()
  f.host.onkeydown({ key: 'Enter', preventDefault: vi.fn() })
  expect(f.controls.get('output').textContent).toBe('1/1')
})
