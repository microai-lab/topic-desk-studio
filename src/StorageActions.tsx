/** Local-data controls keep directory access, paths and manual backup creation out of production UI. */
import { Archive, FolderOpen, RotateCcw, Trash2, Wrench } from 'lucide-react'
import type { Messages } from './i18n'
import type { StorageAction } from './StorageConfirmation'

/** Build mode comes from Vite in App; visibility never depends on a user preference or URL. */
export function StorageActions({ development, currentAction, collecting, canRestore, directory, m, onOpenDirectory, onRequest }: {
  development: boolean; currentAction: StorageAction | null; collecting: boolean; canRestore: boolean
  directory: string | undefined; m: Messages; onOpenDirectory: () => void; onRequest: (action: StorageAction) => void
}) {
  const busy = currentAction !== null
  return <>
    <div className="settings-card-actions storage-actions">
      {development ? <button type="button" disabled={busy} onClick={() => { if (!busy) onOpenDirectory() }}><FolderOpen aria-hidden="true" />{m.storageOpenFolder}</button> : null}
      <button type="button" disabled={busy} onClick={() => { if (!busy) onRequest('optimize') }}><Wrench aria-hidden="true" />{currentAction === 'optimize' ? m.storageWorking : m.storageOptimize}</button>
      {development ? <button type="button" disabled={busy} onClick={() => { if (!busy) onRequest('backup') }}><Archive aria-hidden="true" />{currentAction === 'backup' ? m.storageWorking : m.storageBackup}</button> : null}
      <button className="storage-restore" type="button" disabled={busy || collecting} onClick={() => { if (!busy && !collecting) onRequest('clear') }}><Trash2 aria-hidden="true" />{currentAction === 'clear' ? m.storageWorking : m.storageClear}</button>
      <button className="storage-restore" type="button" disabled={busy || !canRestore} onClick={() => { if (!busy && canRestore) onRequest('restore') }}><RotateCcw aria-hidden="true" />{currentAction === 'restore' ? m.storageWorking : m.storageRestore}</button>
    </div>
    {development && directory !== undefined ? <code className="storage-path">{directory}</code> : null}
  </>
}
