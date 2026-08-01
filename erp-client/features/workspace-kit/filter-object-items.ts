import type { ObjectListItem } from "@/features/workspace-kit/types"

/**
 * Object selector filtering used by ObjectWorkspacePage.
 * Default (first) scope shows all items.
 * Other scopes require item.scopeTags to include the scope label.
 * Search matches title, code, and subtitle.
 */
export function filterObjectItems(
  items: readonly ObjectListItem[],
  options: {
    search?: string
    scope?: string
    scopeLabels?: readonly string[]
  }
): ObjectListItem[] {
  const { search = "", scope, scopeLabels = [] } = options
  const defaultScope = scopeLabels[0]
  const q = search.trim().toLowerCase()

  return items.filter((item) => {
    if (scope && defaultScope && scope !== defaultScope) {
      if (!(item.scopeTags ?? []).includes(scope)) return false
    }
    if (!q) return true
    return (
      item.title.toLowerCase().includes(q) ||
      item.code.toLowerCase().includes(q) ||
      item.subtitle.toLowerCase().includes(q)
    )
  })
}
