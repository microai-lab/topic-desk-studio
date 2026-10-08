/** Version label shared by build configuration and the desktop brand. */

/** All build modes display the configured stable version, independent of tags or dates. */
export function appVersionLabel(releaseVersion: string): string {
  const version = releaseVersion.trim().match(/^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/)
  if (!version || !version.slice(1).every((part) => Number.isSafeInteger(Number(part)))) {
    throw new Error('Invalid application version')
  }
  return `v${version[1]}.${version[2]}.${version[3]}`
}
