/** Deterministic tests for title-language detection and rank-history rendering. */

import { describe, expect, it } from 'vitest'
import { isEnglishTitle, rankTrendPoints } from './presentation'

describe('isEnglishTitle', () => {
  it('accepts Latin-only titles and rejects titles containing CJK text', () => {
    expect(isEnglishTitle('Rust gets a smaller runtime')).toBe(true)
    expect(isEnglishTitle('Rust 发布更小的运行时')).toBe(false)
    expect(isEnglishTitle('2026 年热点')).toBe(false)
  })
})

describe('rankTrendPoints', () => {
  it('draws an improving rank upward and remains finite for a flat trend', () => {
    expect(rankTrendPoints([10, 5, 1])).toBe('0,20 36,11.11111111111111 72,4')
    expect(rankTrendPoints([3, 3])).toBe('0,20 72,20')
  })
})
