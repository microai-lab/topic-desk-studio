/** Regression tests for persisted switch state across panel reloads and stale editor drafts. */
import { describe, expect, it, vi } from 'vitest'
import { createSourceSettingsReader, enabledSources, sourceDraftForSave, sourceSettingsGroups, sourceSettingsOverview } from './sourceSettingsState'
import type { SourceConfiguration } from './types'

const source: SourceConfiguration = {
  code: 'test', displayName: 'Test', homeUrl: 'https://example.org', endpointUrl: 'https://example.org/feed',
  region: 'international', category: 'technology', parserType: 'rss', proxyMode: 'auto',
  enabled: true, builtIn: false, parserConfig: {},
}

describe('source settings state', () => {
  it('counts only enabled regional sources for the overview, including the all-disabled boundary', () => {
    const sources = [source, { ...source, code: 'domestic', region: 'domestic' as const }, { ...source, code: 'off', enabled: false }]
    expect(sourceSettingsOverview(sources)).toEqual({ enabled: 2, domestic: 1, international: 1 })
    expect(sourceSettingsOverview(sources.map(item => ({ ...item, enabled: false })))).toEqual({ enabled: 0, domestic: 0, international: 0 })
    expect(sourceSettingsOverview([])).toEqual({ enabled: 0, domestic: 0, international: 0 })
  })
  it('puts enabled sources first without changing manual order or persisted switches', () => {
    const sources = [{ ...source, code: 'disabled-a', enabled: false }, { ...source, code: 'enabled-a' }, { ...source, code: 'disabled-b', enabled: false }, { ...source, code: 'enabled-b' }]
    const groups = sourceSettingsGroups(sources)
    expect(groups.enabled.map(item => item.code)).toEqual(['enabled-a', 'enabled-b'])
    expect(groups.disabled.map(item => item.code)).toEqual(['disabled-a', 'disabled-b'])
    expect(enabledSources(sources).map(item => item.code)).toEqual(['enabled-a', 'enabled-b'])
    expect(sources[0]?.code).toBe('disabled-a')
    expect(sourceSettingsGroups([])).toEqual({ enabled: [], disabled: [] })
    const toggled = sources.map(item => item.code === 'enabled-a' ? { ...item, enabled: false } : item)
    expect(enabledSources(toggled).map(item => item.code)).toEqual(['enabled-b'])
    expect(sourceSettingsGroups(toggled).disabled.map(item => item.code)).toContain('enabled-a')
  })
  it('reopening settings publishes the disabled state returned by storage', async () => {
    const reader = createSourceSettingsReader()
    const publish = vi.fn()
    await reader.refresh(async () => [source], publish)
    await reader.refresh(async () => [{ ...source, enabled: false }], publish)
    expect(publish.mock.lastCall?.[0][0].enabled).toBe(false)
  })

  it('does not let a read started before a toggle restore its old state', async () => {
    const reader = createSourceSettingsReader()
    const publish = vi.fn()
    let resolve!: (sources: SourceConfiguration[]) => void
    const stale = reader.refresh(() => new Promise(done => { resolve = done }), publish)
    reader.invalidate()
    await reader.refresh(async () => [{ ...source, enabled: false }], publish)
    resolve([source])
    await stale
    expect(publish).toHaveBeenCalledTimes(1)
    expect(publish.mock.lastCall?.[0][0].enabled).toBe(false)
  })

  it('discards reads after leaving the panel and propagates storage failures', async () => {
    const reader = createSourceSettingsReader()
    const publish = vi.fn()
    let resolve!: (sources: SourceConfiguration[]) => void
    const pending = reader.refresh(() => new Promise(done => { resolve = done }), publish)
    reader.invalidate()
    resolve([source])
    await pending
    expect(publish).not.toHaveBeenCalled()
    await expect(reader.refresh(async () => { throw new Error('fixture storage failure') }, publish)).rejects.toThrow('fixture storage failure')
    expect(publish).not.toHaveBeenCalled()
  })

  it('saves editor changes using the current switch rather than its stale draft', () => {
    const draft = { ...source, displayName: 'Edited' }
    expect(sourceDraftForSave(draft, [{ ...source, enabled: false }])).toMatchObject({ displayName: 'Edited', enabled: false })
    expect(sourceDraftForSave({ ...draft, enabled: false }, [source]).enabled).toBe(true)
    expect(draft.enabled).toBe(true)
  })

  it('preserves explicit defaults when creating a new source', () => {
    expect(sourceDraftForSave(source, []).enabled).toBe(true)
    expect(sourceDraftForSave({ ...source, enabled: false }, []).enabled).toBe(false)
  })
})
