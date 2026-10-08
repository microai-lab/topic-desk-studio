/** Shared, testable split-pane sizing for laptop windows and keyboard/pointer resizing. */
export const TOPIC_PANE_MIN_WIDTH = 320
export const READER_PANE_MIN_WIDTH = 420
export const DEFAULT_READER_PERCENT = 60
/** Match the workspace container breakpoint with 40px of layout headroom. */
export const SPLIT_WORKSPACE_MIN_WIDTH = 780

/** Invalid geometry must never produce NaN/Infinity CSS or hide the reader controls. */
export function clampReaderWidth(value: number): number {
  return Number.isFinite(value) ? Math.max(35, Math.min(70, value)) : DEFAULT_READER_PERCENT
}

/** Container CSS switches to a single reader before these two minima stop fitting. */
export function readerColumns(value: number): string {
  const percent = clampReaderWidth(value)
  return `minmax(${TOPIC_PANE_MIN_WIDTH}px, ${100 - percent}fr) minmax(${READER_PANE_MIN_WIDTH}px, ${percent}fr)`
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
