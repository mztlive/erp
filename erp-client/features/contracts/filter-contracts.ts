import type { ContractListRow, ContractStatus } from "@/features/contracts/types"

export type ContractMetricFilter =
  | "all"
  | "effective"
  | "expiring_30d"
  | "expired"
  | "terminated"

export type ContractStatusFilter = "all" | ContractStatus

export function filterContracts(
  rows: readonly ContractListRow[],
  options: {
    search: string
    metricKey: ContractMetricFilter
    statusFilter: ContractStatusFilter
  }
): ContractListRow[] {
  const q = options.search.trim().toLowerCase()
  return rows.filter((row) => {
    if (options.statusFilter !== "all" && row.status !== options.statusFilter) {
      return false
    }
    switch (options.metricKey) {
      case "effective":
        if (row.status !== "EFFECTIVE") return false
        break
      case "expiring_30d":
        if (!row.expiringWithin30Days || row.status !== "EFFECTIVE") return false
        break
      case "expired":
        if (row.status !== "EXPIRED") return false
        break
      case "terminated":
        if (row.status !== "TERMINATED") return false
        break
      default:
        break
    }
    if (!q) return true
    const haystack = [
      row.contractNo,
      row.customer.displayName,
      row.customer.customerNo,
      row.settlementParty.displayName,
      row.ownerLabel,
      row.statusLabel,
    ]
      .join(" ")
      .toLowerCase()
    return haystack.includes(q)
  })
}

export function contractMetricLabel(key: ContractMetricFilter): string {
  switch (key) {
    case "effective":
      return "有效"
    case "expiring_30d":
      return "30 天内到期"
    case "expired":
      return "已到期"
    case "terminated":
      return "已终止"
    default:
      return "全部"
  }
}

export function computeContractMetrics(rows: readonly ContractListRow[]) {
  return {
    all: rows.length,
    effective: rows.filter((r) => r.status === "EFFECTIVE").length,
    expiring_30d: rows.filter(
      (r) => r.status === "EFFECTIVE" && r.expiringWithin30Days
    ).length,
    expired: rows.filter((r) => r.status === "EXPIRED").length,
    terminated: rows.filter((r) => r.status === "TERMINATED").length,
  }
}
