/** One editable combobox shares proxy presets with the existing explicit native save action. */
import { useId, useRef, useState } from 'react'
import { ChevronDown } from 'lucide-react'
import type { Messages } from './i18n'
import { DEFAULT_PROXY_URL, PROXY_PRESETS, proxyComboboxKey, proxyPresetSelection, selectProxyPreset } from './networkPresentation'

/** Choosing a preset and typing a custom address update the same unsaved draft. */
export function ProxyAddressField({ value, onChange, m }: {
  value: string; onChange: (value: string) => void; m: Messages
}) {
  const id = useId()
  const input = useRef<HTMLInputElement>(null)
  const [state, setState] = useState({ open: false, active: -1 })
  const options = [
    ...PROXY_PRESETS.map(address => ({ value: address, label: `${address}${address === DEFAULT_PROXY_URL ? ` ${m.proxyDefault}` : ''}` })),
    { value: 'direct', label: m.proxyDirect },
  ]
  const choose = (selection: string) => {
    onChange(selectProxyPreset(selection, value))
    setState({ open: false, active: -1 })
    input.current?.focus()
  }
  return <div className="proxy-address-field">
    <label htmlFor={id}>{m.labelProxy}</label>
    <div className="proxy-combobox" onBlur={event => {
      if (!event.currentTarget.contains(event.relatedTarget)) setState({ open: false, active: -1 })
    }}>
      <input id={id} ref={input} role="combobox" aria-autocomplete="list" aria-haspopup="listbox"
        aria-expanded={state.open} aria-controls={`${id}-options`}
        aria-activedescendant={state.open && state.active >= 0 ? `${id}-option-${state.active}` : undefined}
        value={value} placeholder={m.proxyPlaceholder} spellCheck={false} autoComplete="off"
        onChange={event => { onChange(event.target.value); setState({ open: false, active: -1 }) }}
        onKeyDown={event => {
          const next = proxyComboboxKey(state, event.key, options.length)
          if (!next) return
          if (event.key !== 'Tab') event.preventDefault()
          if (next.selected !== undefined) choose(options[next.selected]!.value)
          else setState({ open: next.open, active: next.active })
        }} />
      <button type="button" className="proxy-preset-toggle" aria-label={m.labelProxyPreset}
        aria-expanded={state.open} aria-controls={`${id}-options`}
        onMouseDown={event => event.preventDefault()}
        onClick={() => { input.current?.focus(); setState({ open: !state.open, active: -1 }) }}>
        <ChevronDown aria-hidden="true" />
      </button>
      {state.open ? <div id={`${id}-options`} className="proxy-preset-options" role="listbox" aria-label={m.labelProxyPreset}>
        {options.map((option, index) => <div key={option.value} id={`${id}-option-${index}`} role="option"
          aria-selected={proxyPresetSelection(value) === option.value}
          className={state.active === index ? 'highlighted' : ''}
          onMouseDown={event => event.preventDefault()}
          onMouseEnter={() => setState({ open: true, active: index })}
          onClick={() => choose(option.value)}>{option.label}</div>)}
      </div> : null}
    </div>
  </div>
}
