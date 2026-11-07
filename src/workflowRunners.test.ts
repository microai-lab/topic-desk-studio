/** Guard desktop workflows against retired macOS runners and lost architecture coverage. */
import { readFileSync, readdirSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

const workflows = new URL('../.github/workflows/', import.meta.url)

describe('desktop workflow runners', () => {
  it('does not schedule jobs on retired macOS 13 or 14 images', () => {
    for (const name of readdirSync(workflows).filter(name => /\.ya?ml$/.test(name))) {
      const source = readFileSync(new URL(name, workflows), 'utf8')
      // Include larger-runner suffixes: those macOS 14 images retired together.
      expect(source, name).not.toMatch(/\bmacos-(?:13|14)(?:-(?:large|xlarge))?\b/)
    }
  })

  it('retains native Apple Silicon and Intel release builds', () => {
    const source = readFileSync(new URL('release.yml', workflows), 'utf8')
    const entries = [...source.matchAll(/- platform: (\S+)\s+args: '([^']*)'\s+rust_target: '([^']*)'/g)]
    const mac = entries.filter(entry => entry[1]?.startsWith('macos-'))
    expect(mac.map(entry => entry.slice(1))).toEqual([
      ['macos-15', '--target aarch64-apple-darwin', 'aarch64-apple-darwin'],
      ['macos-15-intel', '--target x86_64-apple-darwin', 'x86_64-apple-darwin'],
    ])
  })
})
