/** Regression coverage for consistent release metadata and labels in every build mode. */
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { appVersionLabel } from './appVersion'

describe('appVersionLabel', () => {
  it('displays the configured release without incrementing or appending beta dates', () => {
    expect(appVersionLabel('0.3.0')).toBe('v0.3.0')
    expect(appVersionLabel('1.2.3')).toBe('v1.2.3')
  })
  it('normalizes whitespace and an optional v without duplicating the prefix', () => {
    for (const version of ['0.3.0', 'v0.3.0', '  v0.3.0\n']) {
      expect(appVersionLabel(version)).toBe('v0.3.0')
    }
  })
  it('rejects invalid or stale development labels rather than silently showing them', () => {
    for (const version of ['', 'broken', '0.3', 'vv0.3.0', '0.03.0', '-1.3.0', '0.3.0-beta.20261009']) {
      expect(() => appVersionLabel(version)).toThrow('Invalid application version')
    }
  })
  it('rejects unsafe numeric components and permits valid zero boundaries', () => {
    for (const version of ['9007199254740992.3.0', '0.9007199254740992.0', '0.3.9007199254740992']) {
      expect(() => appVersionLabel(version)).toThrow('Invalid application version')
    }
    expect(appVersionLabel('0.0.0')).toBe('v0.0.0')
  })
  it('keeps npm, Rust, lockfile and Tauri release versions identical', () => {
    const npm = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))
    const tauri = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'))
    const cargo = readFileSync(new URL('../src-tauri/Cargo.toml', import.meta.url), 'utf8')
    const lock = readFileSync(new URL('../src-tauri/Cargo.lock', import.meta.url), 'utf8')
    expect(npm.version).toBe('0.3.0')
    expect(tauri.version).toBe(npm.version)
    expect(cargo.match(/^version = "([^"]+)"/m)?.[1]).toBe(npm.version)
    expect(lock.match(/name = "topic-desk-studio"\nversion = "([^"]+)"/)?.[1]).toBe(npm.version)
    expect(appVersionLabel(tauri.version)).toBe('v0.3.0')
    // The tagged-release workflow requires a matching bilingual body_path file.
    const notes = readFileSync(new URL(`../.github/release-notes/v${npm.version}.md`, import.meta.url), 'utf8')
    expect(notes).toContain(`# Topic Desk Studio v${npm.version} — English`)
    expect(notes).toContain(`# Topic Desk Studio v${npm.version} — 中文`)
  })
  it('injects the same release label in development, packaged builds and tagless archives', () => {
    const config = readFileSync(new URL('../vite.config.ts', import.meta.url), 'utf8')
    expect(config).toContain('appVersionLabel(releaseVersion)')
    expect(config).not.toContain('git')
    expect(config).not.toContain('new Date(')
  })
})
