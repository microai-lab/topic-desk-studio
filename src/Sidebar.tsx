/** Accessible desktop navigation that can collapse to an icon-only rail. */
import { Bookmark, Compass, PanelLeftClose, PanelLeftOpen, Settings, Sparkles } from 'lucide-react'
import { Brand } from './Brand'
import type { Messages } from './i18n'

/** Navigation destinations remain the same in expanded and compact layouts. */
export type SidebarView = 'discover' | 'queue' | 'new' | 'settings'

/** Controlled state keeps navigation, filters, and reader tabs intact when resizing. */
export function Sidebar({ collapsed, view, queuedTotal, recentTotal, version, m, onToggle, onNavigate }: {
  readonly collapsed: boolean
  readonly view: SidebarView
  readonly queuedTotal: number
  readonly recentTotal: number
  readonly version: string
  readonly m: Messages
  readonly onToggle: () => void
  readonly onNavigate: (view: SidebarView) => void
}) {
  const items = [
    { view: 'discover' as const, label: m.navDiscover, icon: Compass },
    { view: 'queue' as const, label: m.navQueue, icon: Bookmark, count: queuedTotal },
    { view: 'new' as const, label: m.navNew, icon: Sparkles, count: recentTotal },
  ]
  const toggleLabel = collapsed ? m.sidebarExpand : m.sidebarCollapse
  const ToggleIcon = collapsed ? PanelLeftOpen : PanelLeftClose
  return <aside className={`sidebar${collapsed ? ' sidebar-collapsed' : ''}`}>
    <Brand version={version} collapsed={collapsed} />
    <nav id="primary-navigation" aria-label={m.navMain}>
      {items.map(({ view: destination, label, icon: Icon, count }) => <button
        key={destination} type="button"
        className={`nav-item${view === destination ? ' active' : ''}`}
        aria-label={label} aria-current={view === destination ? 'page' : undefined}
        title={count === undefined ? label : `${label} (${count})`}
        onClick={() => onNavigate(destination)}
      >
        <Icon aria-hidden="true" />
        {!collapsed ? <><span className="nav-label">{label}</span>{count === undefined ? null : <small>{count}</small>}</> : null}
      </button>)}
    </nav>
    <div className="sidebar-footer">
      <button className="nav-item" type="button" title={m.navSettings} aria-label={m.navSettings} onClick={() => onNavigate('settings')}>
        <Settings aria-hidden="true" />{!collapsed ? <span className="nav-label">{m.navSettings}</span> : null}
      </button>
      <button className="nav-item sidebar-toggle" type="button" title={toggleLabel}
        aria-label={toggleLabel} aria-expanded={!collapsed} aria-controls="primary-navigation" onClick={onToggle}>
        <ToggleIcon aria-hidden="true" />{!collapsed ? <span className="nav-label">{toggleLabel}</span> : null}
      </button>
    </div>
  </aside>
}
