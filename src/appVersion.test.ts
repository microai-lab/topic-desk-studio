/** Regression coverage for tagged development builds and packaged release labels. */
import { describe, expect, it } from 'vitest'
import { appVersionLabel } from './appVersion'

describe('appVersionLabel', () => {
  const buildDate = new Date(2026, 9, 8, 12)
  it('increments the last tag patch and appends the local build date', () => {
    expect(appVersionLabel('0.3.0', 'v0.2.3', true, buildDate)).toBe('v0.2.4-beta.20261008')
  })
  it('uses the application version for a packaged release', () => {
    expect(appVersionLabel('0.2.4', 'v0.2.3', false, buildDate)).toBe('v0.2.4')
  })
  it('falls back when a checkout has no tags or is a source archive', () => {
    for (const tag of [undefined, '', 'unrelated-tag']) {
      expect(appVersionLabel('0.2.3', tag, true, buildDate)).toBe('v0.2.4-beta.20261008')
    }
  })
  it('normalizes tag whitespace, optional v and existing prerelease suffixes', () => {
    for (const tag of ['  v0.2.3\n', '0.2.3', 'v0.2.3-beta.20261007']) {
      expect(appVersionLabel('0.2.3', tag, true, buildDate)).toBe('v0.2.4-beta.20261008')
    }
    expect(appVersionLabel('0.2.3', 'v0.2.9', true, buildDate)).toBe('v0.2.10-beta.20261008')
  })
  it('pads local dates and follows the local calendar at midnight and year boundaries', () => {
    expect(appVersionLabel('0.2.3', 'v0.2.3', true, new Date(2026, 0, 2, 0, 1))).toBe('v0.2.4-beta.20260102')
    expect(appVersionLabel('0.2.3', 'v0.2.3', true, new Date(2027, 0, 1, 0, 0))).toBe('v0.2.4-beta.20270101')
  })
  it('rejects invalid build metadata instead of showing NaN in the brand', () => {
    expect(() => appVersionLabel('broken', undefined, true, buildDate)).toThrow('Invalid development version or build date')
    expect(() => appVersionLabel('0.2.3', undefined, true, new Date(NaN))).toThrow('Invalid development version or build date')
    expect(() => appVersionLabel('0.2.3', 'v0.2.9007199254740991', true, buildDate)).toThrow('Invalid development version or build date')
  })
})
