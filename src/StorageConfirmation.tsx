/** Confirmation dialog for local maintenance, backup, and data replacement. */
import { useEffect, useId, useRef } from 'react'
import type { Messages } from './i18n'

export type StorageAction = 'optimize' | 'backup' | 'restore'

/** Keep action-specific consequences and translated labels together. */
export function storageConfirmation(action: StorageAction, m: Messages) {
  return {
    title: action === 'optimize' ? m.storageOptimize : action === 'backup' ? m.storageBackup : m.storageRestore,
    description: action === 'optimize' ? m.storageOptimizeConfirm : action === 'backup' ? m.storageBackupConfirm : m.storageRestoreConfirm,
    dangerous: action === 'restore',
  }
}

/** Native HTML modality traps focus; Escape and Cancel dismiss without running an operation. */
export function StorageConfirmation({ action, m, onCancel, onConfirm }: {
  action: StorageAction
  m: Messages
  onCancel: () => void
  onConfirm: () => void
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  const cancel = useRef<HTMLButtonElement>(null)
  const id = useId()
  const content = storageConfirmation(action, m)
  useEffect(() => {
    dialog.current?.showModal()
    cancel.current?.focus()
  }, [])
  return (
    <dialog ref={dialog} className="storage-confirmation" aria-labelledby={`${id}-title`} aria-describedby={`${id}-description`}
      onCancel={(event) => { event.preventDefault(); onCancel() }}>
      <h2 id={`${id}-title`}>{content.title}</h2>
      <p id={`${id}-description`}>{content.description}</p>
      <div className="storage-confirmation-actions">
        <button ref={cancel} type="button" onClick={onCancel}>{m.sourceCancel}</button>
        <button type="button" className={content.dangerous ? 'danger' : 'primary'} onClick={onConfirm}>{content.title}</button>
      </div>
    </dialog>
  )
}
