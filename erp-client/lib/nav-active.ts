/**
 * Sidebar active-state helper: longest matching href wins so parent routes
 * (e.g. /workspace) do not stay active on child routes (/workspace/tasks).
 *
 * Href 可带查询串（如 `/fulfillment?lane=warehouse`）。带查询约束的入口必须与
 * 当前 URL 对应参数一致才算激活；同 path 下更具体的查询项优先于无查询项。
 */

function splitHref(href: string): {
  path: string
  params: URLSearchParams
} {
  const q = href.indexOf("?")
  if (q === -1) return { path: href, params: new URLSearchParams() }
  return {
    path: href.slice(0, q),
    params: new URLSearchParams(href.slice(q + 1)),
  }
}

function pathMatches(pathname: string, hrefPath: string): boolean {
  return pathname === hrefPath || pathname.startsWith(`${hrefPath}/`)
}

/** href 上声明的查询参数是否全部出现在当前 search 中且值相等 */
function queryConstraintsMatch(
  hrefParams: URLSearchParams,
  currentParams: URLSearchParams
): boolean {
  for (const [key, value] of hrefParams.entries()) {
    if (currentParams.get(key) !== value) return false
  }
  return true
}

export function isNavItemActive(
  pathname: string,
  href: string,
  allHrefs: readonly string[],
  search: string = ""
): boolean {
  const { path: hrefPath, params: hrefParams } = splitHref(href)
  if (!pathMatches(pathname, hrefPath)) return false

  const currentParams = new URLSearchParams(
    search.startsWith("?") ? search.slice(1) : search
  )
  if (!queryConstraintsMatch(hrefParams, currentParams)) return false

  const hrefConstraintCount = [...hrefParams.keys()].length

  const longerOrMoreSpecific = allHrefs.some((other) => {
    if (other === href) return false
    const { path: otherPath, params: otherParams } = splitHref(other)
    if (!pathMatches(pathname, otherPath)) return false
    if (!queryConstraintsMatch(otherParams, currentParams)) return false

    // 更长的子路径优先（原有规则）
    if (
      otherPath !== hrefPath &&
      otherPath.length > hrefPath.length &&
      otherPath.startsWith(`${hrefPath}/`)
    ) {
      return true
    }

    // 同 path：查询约束更多者优先（避免 /fulfillment 与 /fulfillment?lane= 双高亮）
    if (otherPath === hrefPath) {
      const otherCount = [...otherParams.keys()].length
      if (otherCount > hrefConstraintCount) return true
    }

    return false
  })

  return !longerOrMoreSpecific
}
