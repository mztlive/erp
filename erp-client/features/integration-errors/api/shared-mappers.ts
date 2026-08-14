/**
 * W29 两个资源映射器共用的纯辅助函数（无 React、无 HTTP 调用）。
 * 从 mappers.ts 拆出；mappers.ts 只再导出其原本公开的 errorClassToBackend。
 */

import type { InterfaceErrorClass } from "@/components/business"
import type { WorkItemProjection } from "@/features/work-items"
import type { IntegrationResolutionItemView } from "../types"

export function tsToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
        return ""
    return new Date(Number(secs) * 1000).toISOString()
}

export function ageLabel(createdAtSecs: number): string {
    const ms = Date.now() - createdAtSecs * 1000
    if (ms < 0) return "—"
    const hours = Math.floor(ms / 3_600_000)
    if (hours < 24) return `${Math.max(1, hours)}h`
    return `${Math.floor(hours / 24)}d`
}

/** backend snake_case error_class → UI InterfaceErrorClass kebab */
export function mapErrorClass(
    raw: string,
): InterfaceErrorClass | "reconciliation-difference" {
    switch (raw) {
        case "capability_gap":
            return "capability-unsupported"
        case "mapping_error":
            return "parameter-or-mapping"
        case "business_rejected":
            return "business-rejected"
        case "transient_failure":
            return "network-timeout"
        case "result_unknown":
            return "result-unknown"
        case "auth_signature":
            return "authentication-or-signature"
        case "rate_limited":
            return "rate-limited"
        case "out_of_order":
            return "out-of-order-callback"
        default:
            return "network-timeout"
    }
}

export function errorClassToBackend(ui?: string): string | undefined {
    if (!ui) return undefined
    switch (ui) {
        case "capability-unsupported":
            return "capability_gap"
        case "parameter-or-mapping":
            return "mapping_error"
        case "business-rejected":
            return "business_rejected"
        case "network-timeout":
            return "transient_failure"
        case "result-unknown":
            return "result_unknown"
        case "authentication-or-signature":
            return "auth_signature"
        case "rate-limited":
            return "rate_limited"
        case "out-of-order-callback":
            return "out_of_order"
        case "duplicate-callback":
            return undefined
        default:
            return ui
    }
}

export function statusLabel(status: string): string {
    switch (status) {
        case "pending":
            return "待处理"
        case "auto_retrying":
            return "自动重试中"
        case "manual_required":
            return "待人工"
        case "resolved":
            return "已解决"
        case "closed":
            return "已关闭"
        default:
            return status
    }
}

export function severityOf(
    errorClass: string,
): IntegrationResolutionItemView["classification"]["severity"] {
    if (errorClass === "auth_signature" || errorClass === "result_unknown")
        return "critical"
    if (errorClass === "business_rejected" || errorClass === "mapping_error")
        return "high"
    if (errorClass === "rate_limited") return "medium"
    return "medium"
}

export function mapFormalWorkItem(workItem: WorkItemProjection) {
    return {
        workItemId: workItem.workItemId,
        workItemType: workItem.workItemType as
            | "INTEGRATION_RESULT_UNKNOWN"
            | "BUSINESS_EXCEPTION",
        taskVersion: workItem.taskVersion,
        status: workItem.status,
        assignmentMode: workItem.assignmentMode,
        processingState: workItem.processingState,
        subjectVersion: workItem.subjectVersion,
        ownerUser: workItem.ownerUser,
        allowedActions: workItem.allowedActions,
    }
}
