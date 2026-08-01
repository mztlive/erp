import type {
  CustomerDirectoryItem,
  CustomerDirectoryQuery,
  CustomerScope,
} from "@/features/customers/types"

export const SCOPE_LABELS: Record<CustomerScope, string> = {
  mine: "我的客户",
  collaborating: "协作客户",
  team: "团队客户",
}

export const SCOPE_ORDER: readonly CustomerScope[] = [
  "mine",
  "collaborating",
  "team",
]

export function parseCustomerScope(value: string | null | undefined): CustomerScope {
  if (value === "collaborating" || value === "team" || value === "mine") {
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
  if (query.sort === "name") {
    sorted.sort((a, b) => a.legalName.localeCompare(b.legalName, "zh-CN"))
  } else if (query.sort === "overdue_desc") {
    sorted.sort(
      (a, b) =>
        Number.parseFloat(b.metrics.overdueAmount) -
        Number.parseFloat(a.metrics.overdueAmount)
    )
  } else {
    sorted.sort((a, b) =>
      (b.recentBusinessAt ?? "").localeCompare(a.recentBusinessAt ?? "")
    )
  }

  return sorted
}
