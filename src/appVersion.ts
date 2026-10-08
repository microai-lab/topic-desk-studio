/** Version label shared by build configuration and the desktop brand. */

/** Development targets the next patch after the nearest tag, dated in local time.
 * Build time is injected so labels and date boundaries are deterministic in tests.
 */
export function appVersionLabel(releaseVersion: string, latestTag: string | undefined, development: boolean, buildDate: Date): string {
  const release = `v${releaseVersion.replace(/^v/, '')}`
  if (!development) return release
  const versionPattern = /^v?(\d+)\.(\d+)\.(\d+)(?:[-+].+)?$/
  const version = latestTag?.trim().match(versionPattern) ?? release.match(versionPattern)
  if (!version || !Number.isSafeInteger(Number(version[3]) + 1) || !Number.isFinite(buildDate.getTime())) {
    throw new Error('Invalid development version or build date')
  }
  const date = `${String(buildDate.getFullYear()).padStart(4, '0')}${String(buildDate.getMonth() + 1).padStart(2, '0')}${String(buildDate.getDate()).padStart(2, '0')}`
  return `v${version[1]}.${version[2]}.${Number(version[3]) + 1}-beta.${date}`
}
