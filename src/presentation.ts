/** Pure presentation helpers kept outside React so filtering and trend math stay testable. */

/** Match the native translation gate so non-English titles never show an unusable action. */
export function isEnglishTitle(title: string): boolean {
  return /[A-Za-z]/.test(title) && !/[\u3400-\u9fff\uf900-\ufaff\u3040-\u30ff\uac00-\ud7af]/u.test(title)
}

/** Convert rank history into SVG points where a smaller rank appears visually higher. */
export function rankTrendPoints(values: readonly number[]): string {
  const minimum = Math.min(...values)
  const maximum = Math.max(...values)
  const range = Math.max(1, maximum - minimum)
  return values.map((value, index) => {
    const x = values.length === 1 ? 0 : index * 72 / (values.length - 1)
    const y = 20 - (maximum - value) * 16 / range
    return `${x},${y}`
  }).join(' ')
}

/** Parse timezone-less SQLite text as device-local time; explicit source offsets remain authoritative. */
export function parseStoredTimestamp(value: string): Date {
  const trimmed = value.trim()
  const sqliteLocal = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(?:\.\d+)?$/.test(trimmed)
  return new Date(sqliteLocal ? trimmed.replace(' ', 'T') : trimmed)
}
