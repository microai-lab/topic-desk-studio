/** Compact sidebar branding with a single-line product name and build label. */

/** Keep the product suffix on the title line while the version stays beneath it. */
export function Brand({ version, collapsed = false }: { version: string; collapsed?: boolean }) {
  return (
    <div className="brand">
      <span className="brand-mark" title={`Topic Desk STUDIO · ${version}`}>TD</span>
      {!collapsed ? <div>
        <span className="brand-title"><strong>Topic Desk</strong><small>STUDIO</small></span>
        <span className="brand-version">{version}</span>
      </div> : null}
    </div>
  )
}
