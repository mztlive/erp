import type {
  MasterDataListItem,
  MasterDataListQuery,
} from "@/features/master-data/types"

export function filterMasterDataRows(
  rows: readonly MasterDataListItem[],
  query: Pick<
    MasterDataListQuery,
    "q" | "lifecycleStatus" | "revisionTiming" | "metricKey"
  >
): MasterDataListItem[] {
  const q = query.q?.trim().toLowerCase() ?? ""
  const lifecycle = query.lifecycleStatus ?? "all"
  const timing = query.revisionTiming ?? "all"
  const metric = query.metricKey ?? "all"

  return rows.filter((row) => {
    if (lifecycle === "enabled" && row.lifecycleStatus !== "ENABLED") {
      return false
    }
    if (lifecycle === "disabled" && row.lifecycleStatus !== "DISABLED") {
      return false
    }
    // revisionTiming filter is independent of lifecycle — FUTURE is not a lifecycle state
    if (timing === "current" && row.revisionTiming !== "CURRENT") {
      return false
    }
    if (timing === "future" && row.revisionTiming !== "FUTURE") {
      return false
    }
    if (metric === "enabled" && row.lifecycleStatus !== "ENABLED") {
      return false
    }
    if (metric === "disabled" && row.lifecycleStatus !== "DISABLED") {
      return false
    }
    if (metric === "pending" && row.revisionTiming !== "FUTURE") {
      return false
    }
    if (metric === "expiring" && !row.metricTags.includes("expiring")) {
      return false
    }
    if (q) {
      const hay = [
        row.stableNo,
        row.name,
        row.ownerName ?? "",
        ...row.keyFacts.map((f) => f.value),
        row.primaryBlocker ?? "",
      ]
        .join(" ")
        .toLowerCase()
      if (!hay.includes(q)) return false
    }
    return true
  })
}

export function formatEffectiveRange(
  from: string,
  to?: string
): string {
  if (!to) return `${from} ~ 长期`
  return `${from} ~ ${to}`
}
