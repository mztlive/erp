/**
 * Sidebar active-state helper: longest matching href wins so parent routes
 * (e.g. /workspace) do not stay active on child routes (/workspace/tasks).
 */
export function isNavItemActive(
  pathname: string,
  href: string,
  allHrefs: readonly string[]
): boolean {
  if (href.includes("/master-data/")) {
    return pathname === href || pathname.startsWith("/master-data/")
  }
  const exactOrChild =
    pathname === href || pathname.startsWith(`${href}/`)
  if (!exactOrChild) return false
  const longerMatch = allHrefs.some(
    (other) =>
      other !== href &&
      other.length > href.length &&
      (pathname === other || pathname.startsWith(`${other}/`)) &&
      other.startsWith(`${href}/`)
  )
  return !longerMatch
}
