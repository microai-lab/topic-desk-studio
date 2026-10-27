/** Proxy draft selection never performs networking or discards a saved custom route. */
import { describe, expect, it } from 'vitest'
import { DEFAULT_PROXY_URL, PROXY_PRESETS, proxyPresetSelection, selectProxyPreset, proxyComboboxKey } from './networkPresentation'

describe('proxy presets', () => {
  it('keeps both addresses available with 7897 as the default', () => {
    expect(DEFAULT_PROXY_URL).toBe('http://127.0.0.1:7897')
    expect(PROXY_PRESETS).toEqual(['http://127.0.0.1:7897', 'http://127.0.0.1:1082'])
    expect(proxyPresetSelection(DEFAULT_PROXY_URL)).toBe(DEFAULT_PROXY_URL)
  })
  it('switches back and forth and recognizes a reloaded saved choice', () => {
    let draft = DEFAULT_PROXY_URL
    for (const preset of [PROXY_PRESETS[1], PROXY_PRESETS[0]]) {
      draft = selectProxyPreset(preset, draft)
      expect(proxyPresetSelection(draft)).toBe(preset)
      expect(PROXY_PRESETS).toHaveLength(2)
    }
    expect(proxyPresetSelection('  http://127.0.0.1:1082/  ')).toBe(PROXY_PRESETS[1])
  })
  it('preserves custom addresses and supports explicit direct access', () => {
    expect(proxyPresetSelection('https://proxy.example.org:9443')).toBe('custom')
    expect(proxyPresetSelection('http://127.0.0.1:10820')).toBe('custom')
    expect(proxyPresetSelection('http://user:pass@127.0.0.1:1082')).toBe('custom')
    expect(proxyPresetSelection('')).toBe('direct')
    expect(proxyPresetSelection('  ')).toBe('direct')
    expect(selectProxyPreset('direct', DEFAULT_PROXY_URL)).toBe('')
    for (const selection of ['custom', 'unknown', 'javascript:alert(1)']) {
      expect(selectProxyPreset(selection, 'https://proxy.example.org:9443')).toBe('https://proxy.example.org:9443')
    }
  })
})

describe('editable proxy combobox keyboard', () => {
  it('opens either end of the list and wraps without altering the draft', () => {
    const closed = { open: false, active: -1 }
    expect(proxyComboboxKey(closed, 'ArrowDown', 3)).toEqual({ open: true, active: 0 })
    expect(proxyComboboxKey(closed, 'ArrowUp', 3)).toEqual({ open: true, active: 2 })
    expect(proxyComboboxKey({ open: true, active: 2 }, 'ArrowDown', 3)).toEqual({ open: true, active: 0 })
    expect(proxyComboboxKey({ open: true, active: 0 }, 'ArrowUp', 3)).toEqual({ open: true, active: 2 })
  })
  it('commits only a highlighted choice on Enter and permits editing normally', () => {
    expect(proxyComboboxKey({ open: true, active: 1 }, 'Enter', 3)).toEqual({ open: false, active: -1, selected: 1 })
    for (const active of [-1, 3]) expect(proxyComboboxKey({ open: true, active }, 'Enter', 3)).toBeUndefined()
    expect(proxyComboboxKey({ open: false, active: 0 }, 'Enter', 3)).toBeUndefined()
    for (const key of ['a', 'Backspace', 'ArrowLeft', 'Home']) {
      expect(proxyComboboxKey({ open: true, active: 1 }, key, 3)).toBeUndefined()
    }
    for (const count of [0, -1, NaN, 1.5]) expect(proxyComboboxKey({ open: false, active: -1 }, 'ArrowDown', count)).toBeUndefined()
  })
  it('dismisses without choosing on Escape or Tab', () => {
    for (const key of ['Escape', 'Tab']) {
      expect(proxyComboboxKey({ open: true, active: 1 }, key, 3)).toEqual({ open: false, active: -1 })
      expect(proxyComboboxKey({ open: false, active: -1 }, key, 3)).toBeUndefined()
    }
  })
})
