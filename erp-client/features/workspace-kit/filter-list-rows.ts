import type { ListRow, MetricDef } from "@/features/workspace-kit/types"

/**
 * Default-view list filtering used by ListWorkspacePage.
 *
 * - Search: case-insensitive match across all cell values.
 * - Metric (non-default): row.metricTags includes key, or status.label equals
 *   that metric's Chinese label (so status-aligned mocks work without tags).
 * - Filter label (non-default first): row.filterTags includes label, or
 *   status.label equals the filter label.
 */
export function filterListRows(
  rows: readonly ListRow[],
  options: {
    search?: string
    metricKey?: string
    filterLabel?: string
    metrics?: readonly MetricDef[]
    filterLabels?: readonly string[]
  }
): ListRow[] {
  const {
    search = "",
    metricKey,
    filterLabel,
    metrics = [],
    filterLabels = [],
  } = options
  const defaultMetricKey = metrics[0]?.key
  const defaultFilterLabel = filterLabels[0]
  const q = search.trim().toLowerCase()
  const activeMetric = metrics.find((metric) => metric.key === metricKey)

  return rows.filter((row) => {
    if (
      metricKey &&
      defaultMetricKey &&
      metricKey !== defaultMetricKey &&
      metricKey !== "all"
    ) {
      const byTag = (row.metricTags ?? []).includes(metricKey)
      const byStatus =
        activeMetric != null && row.status?.label === activeMetric.label
      if (!byTag && !byStatus) return false
    }

    if (
      filterLabel &&
      defaultFilterLabel &&
      filterLabel !== defaultFilterLabel
    ) {
      const byTag = (row.filterTags ?? []).includes(filterLabel)
      const byStatus = row.status?.label === filterLabel
      if (!byTag && !byStatus) return false
    }

    if (!q) return true
    return Object.values(row.cells).some((value) =>
      value.toLowerCase().includes(q)
    )
  })
}
