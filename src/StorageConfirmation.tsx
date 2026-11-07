/** Shared modal confirmation for local maintenance and restoring default sources. */
import { useEffect, useId, useRef, useState } from 'react'
import type { Messages } from './i18n'
import type { ClearDataSelection } from './types'
import { clearDataOptions, EMPTY_CLEAR_SELECTION, hasClearSelection } from './storagePresentation'

export type StorageAction = 'optimize' | 'backup' | 'restore' | 'clear'
/** Source reset has its own consequences and never invokes a storage restore. */
export type ConfirmationAction = StorageAction | 'restoreSources'

/** Keep action-specific consequences and translated labels together. */
export function storageConfirmation(action: ConfirmationAction, m: Messages) {
  return {
    title: action === 'clear' ? m.storageClear : action === 'restoreSources' ? m.sourceRestoreDefaults : action === 'optimize' ? m.storageOptimize : action === 'backup' ? m.storageBackup : m.storageRestore,
    description: action === 'clear' ? m.storageClearConfirm : action === 'restoreSources' ? m.sourceRestoreConfirm : action === 'optimize' ? m.storageOptimizeConfirm : action === 'backup' ? m.storageBackupConfirm : m.storageRestoreConfirm,
    dangerous: action === 'clear' || action === 'restore' || action === 'restoreSources',
  }
}

/** Native HTML modality traps focus; Escape and Cancel dismiss without running an operation. */
export function StorageConfirmation({ action, m, onCancel, onConfirm }: {
  action: ConfirmationAction
  m: Messages
  onCancel: () => void
  onConfirm: (selection?: ClearDataSelection) => void
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  const cancel = useRef<HTMLButtonElement>(null)
  const id = useId()
  const content = storageConfirmation(action, m)
  const [selection, setSelection] = useState(EMPTY_CLEAR_SELECTION)
  useEffect(() => {
    dialog.current?.showModal()
    cancel.current?.focus()
  }, [])
  return (
    <dialog ref={dialog} className={`storage-confirmation${action === 'clear' ? ' storage-clear-dialog' : ''}`} aria-labelledby={`${id}-title`} aria-describedby={`${id}-description`}
      onCancel={(event) => { event.preventDefault(); onCancel() }}>
      <h2 id={`${id}-title`}>{content.title}</h2>
      <p id={`${id}-description`}>{content.description}</p>
      {action === 'clear' ? <fieldset className="storage-clear-options">
        <legend>{m.storageClearSelect}</legend>
        {clearDataOptions(m).map(({ key, label, hint }) => <label className="storage-clear-option" key={key}>
          <input type="checkbox" checked={selection[key]} aria-labelledby={`${id}-${key}-label`} aria-describedby={hint ? `${id}-${key}-hint` : undefined} onChange={event => setSelection(current => ({ ...current, [key]: event.target.checked }))} />
          <span className="storage-clear-option-copy"><span id={`${id}-${key}-label`}>{label}</span>{hint ? <small id={`${id}-${key}-hint`}>{hint}</small> : null}</span>
        </label>)}
      </fieldset> : null}
      <div className="storage-confirmation-actions">
        <button ref={cancel} type="button" onClick={onCancel}>{m.sourceCancel}</button>
        <button type="button" className={content.dangerous ? 'danger' : 'primary'} disabled={action === 'clear' && !hasClearSelection(selection)}
          onClick={() => { if (action !== 'clear' || hasClearSelection(selection)) onConfirm(action === 'clear' ? selection : undefined) }}>{action === 'clear' ? m.storageClearSelected : content.title}</button>
      </div>
    </dialog>
  )
}
