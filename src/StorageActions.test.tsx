/** Production builds hide developer storage entry points without hiding user maintenance actions. */
import { readFileSync } from 'node:fs'
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it, vi } from 'vitest'
import { messages } from './i18n'
import { StorageActions } from './StorageActions'
import type { StorageAction } from './StorageConfirmation'

const props = () => ({ development: false, currentAction: null as StorageAction | null, collecting: false, canRestore: true,
  directory: '/fixture/private-data', m: messages.zh, onOpenDirectory: vi.fn(), onRequest: vi.fn() })

it('hides directory access, path and backup creation in production for both languages', () => {
  for (const locale of ['zh', 'en'] as const) {
    const options = { ...props(), m: messages[locale] }
    const html = renderToStaticMarkup(<StorageActions {...options} />)
    for (const hidden of [options.m.storageOpenFolder, options.m.storageBackup, options.directory, 'storage-path']) expect(html).not.toContain(hidden)
    for (const shown of [options.m.storageOptimize, options.m.storageClear, options.m.storageRestore]) expect(html).toContain(shown)
    expect(options.onOpenDirectory).not.toHaveBeenCalled()
    expect(options.onRequest).not.toHaveBeenCalled()
  }
})

it('shows developer controls and path only in development and tolerates an unloaded path', () => {
  const options = { ...props(), development: true }
  const html = renderToStaticMarkup(<StorageActions {...options} />)
  for (const shown of [options.m.storageOpenFolder, options.m.storageBackup, options.directory]) expect(html).toContain(shown)
  expect(renderToStaticMarkup(<StorageActions {...options} directory={undefined} />)).not.toContain('storage-path')
})

it('keeps native operations behind clicks and blocks operations while busy', () => {
  const options = { ...props(), development: true }
  const buttons = StorageActions(options).props.children[0].props.children
  buttons[0].props.onClick()
  buttons[1].props.onClick()
  buttons[2].props.onClick()
  buttons[3].props.onClick()
  buttons[4].props.onClick()
  expect(options.onOpenDirectory).toHaveBeenCalledTimes(1)
  expect(options.onRequest.mock.calls.map(call => call[0])).toEqual(['optimize', 'backup', 'clear', 'restore'])
  options.onOpenDirectory.mockClear(); options.onRequest.mockClear()
  for (const button of StorageActions({ ...options, currentAction: 'backup' }).props.children[0].props.children) {
    expect(button.props.disabled).toBe(true)
    button.props.onClick()
  }
  expect(options.onOpenDirectory).not.toHaveBeenCalled()
  expect(options.onRequest).not.toHaveBeenCalled()
})

it('retains collection and missing-backup restrictions', () => {
  const options = { ...props(), development: true, collecting: true, canRestore: false }
  const buttons = StorageActions(options).props.children[0].props.children
  for (const index of [3, 4]) {
    expect(buttons[index].props.disabled).toBe(true)
    buttons[index].props.onClick()
  }
  expect(options.onRequest).not.toHaveBeenCalled()
})

it('uses the build-time development flag rather than a runtime setting', () => {
  const app = readFileSync(new URL('./App.tsx', import.meta.url), 'utf8')
  expect(app).toContain('<StorageActions development={import.meta.env.DEV}')
})
