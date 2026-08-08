import type {
  CustomerDirectoryItem,
  CustomerDirectoryQuery,
  CustomerScope,
} from "@/features/customers/types"

export const SCOPE_LABELS: Record<CustomerScope, string> = {
  mine: "我的客户",
  collaborating: "协作客户",
  assigned: "我参与的客户",
  all_authorized: "全部有权客户",
}

export const SCOPE_ORDER: readonly CustomerScope[] = [
  "mine",
  "collaborating",
  "all_authorized",
]

export function parseCustomerScope(value: string | null | undefined): CustomerScope {
  if (
    value === "collaborating" ||
    value === "all_authorized" ||
    value === "mine"
  ) {
    return value
  }
  return "mine"
}

export function filterCustomerDirectory(
  items: readonly CustomerDirectoryItem[],
  query: CustomerDirectoryQuery
): CustomerDirectoryItem[] {
  const q = query.query?.trim().toLowerCase() ?? ""

  let rows = items.filter((item) => item.scopeTags.includes(query.scope))

  if (query.status === "active") {
    rows = rows.filter((item) => item.status === "active")
  } else if (query.status === "disabled") {
    rows = rows.filter((item) => item.status === "disabled")
  }

  if (q) {
    rows = rows.filter((item) => {
      const hay = [
        item.legalName,
        item.shortName ?? "",
        item.customerNo,
        item.ownerName,
      ]
        .join(" ")
        .toLowerCase()
      return hay.includes(q)
    })
  }

  const sorted = [...rows]
  sorted.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
  // 表头排序支持升序（默认更新时间为降序）。
  if (query.sortDir === "asc") sorted.reverse()

  return sorted
}
