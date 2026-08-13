/**
 * W23 执行信息 · URL 参数解析与工作台链接构建（纯函数，无 React）。
 */

import type {
    ExecutionProjectionMetricKey,
    LatencyBand,
    ProjectionSource,
    ReconciliationStatus,
} from "@/features/execution-projections/types"

export function parseMetric(
    raw: string | null,
): ExecutionProjectionMetricKey | "all" {
    if (
        raw === "pending_send" ||
        raw === "inflight" ||
        raw === "timeout" ||
        raw === "fail_manual" ||
        raw === "acked"
    ) {
        return raw
    }
    return "all"
}

export function parseSource(raw: string | null): ProjectionSource | "all" {
    if (raw === "MIGRATION_BASELINE" || raw === "ERP_SALES_REVISION") return raw
    return "all"
}

export function parseLatency(raw: string | null): LatencyBand | "all" {
    if (raw === "normal" || raw === "near_sla" || raw === "over_sla") return raw
    return "all"
}

export function parseRecon(raw: string | null): ReconciliationStatus | "all" {
    if (raw === "MATCHED" || raw === "VERSION_MISMATCH" || raw === "NONE") {
        return raw
    }
    return "all"
}

export function w29Href(workItemId?: string, errorTaskId?: string) {
    const params = new URLSearchParams()
    if (workItemId) params.set("workItemId", workItemId)
    if (errorTaskId) params.set("errorTaskId", errorTaskId)
    params.set("from", "W23")
    const qs = params.toString()
    return `/governance/integration-errors${qs ? `?${qs}` : ""}`
}
