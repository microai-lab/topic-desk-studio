/** Custom popup rendering preserves accessible controls, groups, and safe source/download text. */
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it, vi } from 'vitest'
import { BrowserPopoverItems } from './BrowserPopover'
import type { PopoverItem } from './nativeBrowserMenu'

it('renders groups and disabled rows while escaping download text instead of executing markup', () => {
  const action = vi.fn()
  const base: PopoverItem = { id: 'open', text: '<script>download</script>', enabled: true, separator: false, children: [] }
  const html = renderToStaticMarkup(<BrowserPopoverItems items={[base, { ...base, id: 'disabled', text: 'Unavailable', enabled: false }, { ...base, id: 'group', text: 'Downloads', children: [base] }, { ...base, separator: true }]} onAction={action} />)
  expect(html).not.toContain('<script>')
  expect(html).toContain('&lt;script&gt;download&lt;/script&gt;')
  expect(html).toContain('<details')
  expect(html).toContain('<summary>')
  expect(html).toContain('disabled=""')
  expect(html).toContain('<hr')
  expect(action).not.toHaveBeenCalled()
})

it('renders zoom inline with minus, reset and plus instead of an expandable list', () => {
  const control = (id: string, text: string, enabled = true): PopoverItem => ({ id, text, enabled, separator: false, children: [], keepOpen: true })
  const html = renderToStaticMarkup(<BrowserPopoverItems items={[{ ...control('zoom', '缩放'), layout: 'zoom', children: [control('out', '缩小', false), control('reset', '100%'), control('in', '放大')] }]} onAction={() => {}} />)
  expect(html).toContain('popover-zoom-row')
  expect(html).not.toContain('<details')
  expect(html).toContain('aria-label="缩小" disabled=""')
  expect(html).toContain('aria-label="放大"')
  expect(html).toContain('aria-label="重置缩放为 100%"')
  expect(html).toContain('100%</button>')
})
