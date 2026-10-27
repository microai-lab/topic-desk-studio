/** Deterministic DOM extraction must preserve publisher note-access links without native credentials. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import runtime from '../src-tauri/src/xiaohongshu_cards.js?raw'

type Link = { href: string; closest: () => { querySelector: () => { textContent: string } }; getAttribute: () => null }
const link = (href: string, title = 'Original title'): Link => ({ href, closest: () => ({ querySelector: () => ({ textContent: title }) }), getAttribute: () => null })
function extract(links: Link[]): { id: string; title: string; url: string }[] {
  vi.stubGlobal('document', { querySelectorAll: () => links })
  vi.stubGlobal('location', { href: 'https://www.xiaohongshu.com/explore' })
  return new Function(`return (${runtime})`)()
}
afterEach(() => vi.unstubAllGlobals())
describe('Xiaohongshu card extraction', () => {
  const id = '0123456789abcdef01234567'
  it('keeps access parameters and upgrades a duplicate bare link without changing its first title', () => {
    const bare = `https://www.xiaohongshu.com/explore/${id}`
    const signed = `${bare}?xsec_token=fake-note-access&xsec_source=pc_feed`
    expect(extract([link(bare), link(signed, 'Later title')])).toEqual([{ id, title: 'Original title', url: signed }])
  })
  it('rejects unrelated hosts, executable URLs, missing titles and malformed note identifiers', () => {
    expect(extract([link(`https://www.xiaohongshu.com.evil.org/explore/${id}`), link(`http://www.xiaohongshu.com/explore/${id}`), link('javascript:alert(1)'), link('https://www.xiaohongshu.com/explore/invalid'), link(`https://www.xiaohongshu.com/explore/${id}`, '')])).toEqual([])
  })
  it('handles discovery links and bounds collection at fifty notes', () => {
    expect(extract([link(`https://www.xiaohongshu.com/discovery/item/${id}?xsec_token=fake`)])[0]?.url).toContain('xsec_token=fake')
    const links = Array.from({ length: 60 }, (_, i) => link(`https://www.xiaohongshu.com/explore/${i.toString(16).padStart(24, '0')}`))
    expect(extract(links)).toHaveLength(50)
  })
})
