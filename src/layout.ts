/** Shared, testable split-pane sizing for laptop windows and keyboard/pointer resizing. */
export const TOPIC_PANE_MIN_WIDTH = 320
export const READER_PANE_MIN_WIDTH = 420
export const DEFAULT_TOPIC_WIDTH = 420
/** Match the workspace container breakpoint with 40px of layout headroom. */
export const SPLIT_WORKSPACE_MIN_WIDTH = 780

/** Keep the requested pixel width across window sizes, shrinking only when
 * necessary to reserve the reader minimum. The caller retains the requested
 * value so growing the window restores it instead of overwriting user intent. */
export function clampTopicWidth(value: number, workspaceWidth: number): number {
  const requested = Number.isFinite(value) ? value : DEFAULT_TOPIC_WIDTH
  const maximum = Number.isFinite(workspaceWidth) && workspaceWidth > 0
    ? Math.max(TOPIC_PANE_MIN_WIDTH, workspaceWidth - READER_PANE_MIN_WIDTH)
    : DEFAULT_TOPIC_WIDTH
  return Math.max(TOPIC_PANE_MIN_WIDTH, Math.min(maximum, requested))
}

/** Matching pixel tracks interpolate smoothly when the mounted list collapses. */
export function topicColumns(value: number, workspaceWidth: number, collapsed = false): string {
  const pixels = collapsed ? 0 : clampTopicWidth(value, workspaceWidth)
  return `minmax(0px, ${pixels}px) minmax(${READER_PANE_MIN_WIDTH}px, 1fr)`
}

/** Sidebar navigation must reveal its destination even in reader-only layouts.
 * Wide windows retain the article alongside the list; narrow windows hide the
 * pane without deleting tabs. Settings uses its existing separate screen. */
export function sidebarNavigationPlan(settings: boolean, workspaceWidth: number, viewportWidth: number, containerQueries: boolean) {
  const canSplit = containerQueries
    ? Number.isFinite(workspaceWidth) && workspaceWidth >= SPLIT_WORKSPACE_MIN_WIDTH
    : Number.isFinite(viewportWidth) && viewportWidth > 1000
  return { expanded: false, hideBrowser: !settings && !canSplit } as const
}

/** A captured pointer can survive sleep or a lost mouse-up. Resize only while
 * the initiating pointer still holds the primary button and geometry is valid. */
export function topicDragWidth(activePointer: number | null, pointer: number, buttons: number, clientX: number, left: number, width: number): number | undefined {
  if (activePointer === null || activePointer !== pointer || (buttons & 1) === 0
    || ![clientX, left, width].every(Number.isFinite) || width <= 0) return undefined
  return clampTopicWidth(clientX - left, width)
}
