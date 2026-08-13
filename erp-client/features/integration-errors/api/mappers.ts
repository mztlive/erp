/**
 * W29 接口错误与对账中心 · 后端 DTO 映射与筛选。
 * 请求函数见 ./requests；本文件只含纯映射/筛选逻辑（无 React、无 HTTP 调用）。
 */

import type { InterfaceErrorClass } from "@/components/business"
import type {
    IntegrationResolutionItemView,
    IntegrationResolutionQuery,
} from "../types"
import { ERROR_CLASS_LABEL } from "../types"

// ---------------------------------------------------------------------------
// Backend wire types
// ---------------------------------------------------------------------------

export type BackendErrorTask = {
    id: string
    message_id?: string | null
    business_object_id?: string | null
    error_class: string
    status: string
    owner_role?: string | null
    owner_user_id?: string | null
    attempt_count: number
    last_attempt_at?: number | null
    last_attempt_summary?: string | null
    resolution_type?: string | null
    resolved_at?: number | null
    version: number
    created_at: number
    resolution?: string | null
}

export type BackendDifference = {
    id: string
    business_object_type: string
    business_object_id: string
    difference_type: string
    left_fact_reference?: string | null
    right_fact_reference?: string | null
    status?: string | null
    version: number
    created_at: number
    resolutions?: Array<{
        id: string
        resolution_no: number
        resolution_action: string
        resulting_status: string
        evidence_reference?: string | null
        handled_by: string
        handled_at: number
    }>
}

export type BackendReplayResult = {
    task_id: string
    original_action_idempotency_key_summary: string
    original_action_idempotency_key_locked: boolean
    replay_accepted: boolean
    task_status: string
    attempt_count: number
    task_version: number
}

// ---------------------------------------------------------------------------
// Mapping
// ---------------------------------------------------------------------------

function tsToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
        return ""
    return new Date(Number(secs) * 1000).toISOString()
}

function ageLabel(createdAtSecs: number): string {
    const ms = Date.now() - createdAtSecs * 1000
    if (ms < 0) return "—"
    const hours = Math.floor(ms / 3_600_000)
    if (hours < 24) return `${Math.max(1, hours)}h`
    return `${Math.floor(hours / 24)}d`
}

/** backend snake_case error_class → UI InterfaceErrorClass kebab */
function mapErrorClass(
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

function statusLabel(status: string): string {
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

function severityOf(
    errorClass: string,
): IntegrationResolutionItemView["classification"]["severity"] {
    if (errorClass === "auth_signature" || errorClass === "result_unknown")
        return "critical"
    if (errorClass === "business_rejected" || errorClass === "mapping_error")
        return "high"
    if (errorClass === "rate_limited") return "medium"
    return "medium"
}

function allowedForTask(task: BackendErrorTask): {
    allowed: IntegrationResolutionItemView["allowedActions"]
    blockers: IntegrationResolutionItemView["actionBlockers"]
} {
    const terminal = task.status === "resolved" || task.status === "closed"
    if (terminal) return { allowed: [], blockers: [] }

    const allowed: IntegrationResolutionItemView["allowedActions"] = [
        "CLAIM",
        "ADD_EVIDENCE",
        "SKIP",
        "DEFER",
        "TRANSFER",
    ]
    const blockers: IntegrationResolutionItemView["actionBlockers"] = []

    if (task.error_class === "result_unknown") {
        allowed.push("QUERY_ORIGINAL_RESULT")
        blockers.push({
            action: "REPLAY_ORIGINAL",
            code: "QUERY_REQUIRED",
            message:
                "结果未知：须先查询原结果；仅确认无结果且系统判定安全后才可重新提交",
        })
    } else if (
        task.error_class === "transient_failure" ||
        task.error_class === "rate_limited"
    ) {
        // auto-retry classes: query optional
        allowed.push("QUERY_ORIGINAL_RESULT")
    }

    if (
        task.error_class !== "mapping_error" &&
        task.error_class !== "business_rejected" &&
        task.error_class !== "auth_signature" &&
        task.error_class !== "capability_gap"
    ) {
        // REPLAY only after server allows; keep gated
    }

    allowed.push("RESOLVE", "CLOSE_DUPLICATE", "CLOSE_MISROUTED")
    return { allowed, blockers }
}

export function mapErrorTask(
    task: BackendErrorTask,
): IntegrationResolutionItemView {
    const errorClass = mapErrorClass(task.error_class)
    const { allowed, blockers } = allowedForTask(task)
    const label = ERROR_CLASS_LABEL[errorClass] ?? task.error_class
    const severity = severityOf(task.error_class)

    return {
        identity: {
            itemType: "ERROR_TASK",
            id: task.id,
            number: task.id,
            subjectHash: `v${task.version}`,
        },
        workItem: {
            workItemId: task.id,
            workItemType:
                task.error_class === "result_unknown"
                    ? "INTEGRATION_RESULT_UNKNOWN"
                    : "BUSINESS_EXCEPTION",
            workItemVersion: String(task.version),
            status:
                task.status === "resolved"
                    ? "COMPLETED"
                    : task.status === "closed"
                      ? "CLOSED"
                      : task.owner_user_id
                        ? "IN_PROGRESS"
                        : "PENDING",
            subjectVersion: String(task.version),
            subjectHash: `v${task.version}`,
            completionAction: "RESOLVE",
        },
        businessObject: {
            objectType: task.message_id ? "INBOX_MESSAGE" : "BUSINESS_OBJECT",
            objectId: task.business_object_id ?? task.message_id ?? task.id,
            title: task.business_object_id ?? task.message_id ?? task.id,
        },
        classification: {
            code: task.error_class,
            errorClass,
            label,
            severity,
            severityLabel:
                severity === "critical"
                    ? "阻断"
                    : severity === "high"
                      ? "高"
                      : severity === "low"
                        ? "低"
                        : "中",
        },
        environment: "production",
        environmentLabel: "生产",
        status: {
            code: task.status,
            label: statusLabel(task.status),
        },
        fundsImpact: "NONE",
        fundsImpactLabel: "无资金影响",
        compensationOpen: false,
        ageLabel: ageLabel(task.created_at),
        ownerRole: task.owner_role ?? "—",
        ownerUser: task.owner_user_id ?? undefined,
        createdAt: tsToIso(task.created_at),
        message: task.message_id
            ? {
                  eventIdSummary: task.message_id,
                  idempotencyKeySummary: "—",
                  businessFactKeySummary: "—",
                  schemaVersion: "—",
                  directionLabel: "入站",
                  maskedPayloadSummary: task.last_attempt_summary ?? "—",
              }
            : undefined,
        hasWorkItem: true,
        attempts: task.last_attempt_summary
            ? [
                  {
                      attemptNumber: task.attempt_count,
                      attemptedAt:
                          tsToIso(task.last_attempt_at) ||
                          tsToIso(task.created_at),
                      result: task.last_attempt_summary,
                  },
              ]
            : [],
        objectVersion: String(task.version),
        allowedActions: allowed,
        actionBlockers: blockers,
        repairLinks: [],
        auditTrail: [],
        evidenceTimeline: [],
        linkedEvidence: [],
        freshness: {
            updatedAt:
                tsToIso(task.last_attempt_at) || tsToIso(task.created_at),
        },
    }
}

export function mapDifference(
    diff: BackendDifference,
): IntegrationResolutionItemView {
    const terminal =
        diff.status === "confirmed_no_error" ||
        diff.status === "confirmed_valid_difference"

    return {
        identity: {
            itemType: "RECONCILIATION_DIFFERENCE",
            id: diff.id,
            number: diff.id,
            subjectHash: `v${diff.version}`,
        },
        businessObject: {
            objectType: diff.business_object_type,
            objectId: diff.business_object_id,
            title: `${diff.business_object_type} · ${diff.business_object_id}`,
        },
        classification: {
            code: diff.difference_type,
            errorClass: "reconciliation-difference",
            label: "对账差异",
            severity: "high",
            severityLabel: "高",
        },
        environment: "production",
        environmentLabel: "生产",
        status: {
            code: diff.status ?? "open",
            label: terminal
                ? diff.status === "confirmed_no_error"
                    ? "确认无误"
                    : "确认有效差异"
                : "待处理",
        },
        fundsImpact: "POTENTIAL",
        fundsImpactLabel: "潜在资金影响",
        compensationOpen: false,
        ageLabel: ageLabel(diff.created_at),
        ownerRole: "finance",
        createdAt: tsToIso(diff.created_at),
        difference: {
            leftLabel: "左侧证据",
            leftSummary: diff.left_fact_reference ?? "—",
            rightLabel: "右侧证据",
            rightSummary: diff.right_fact_reference ?? "—",
            boundary: diff.business_object_type,
            watermark: tsToIso(diff.created_at),
            differenceType: diff.difference_type,
            differenceSummary: diff.difference_type,
        },
        hasWorkItem: false,
        attempts: [],
        objectVersion: String(diff.version),
        allowedActions: terminal
            ? []
            : ["CONFIRM_NO_ERROR", "CONFIRM_VALID_DIFFERENCE", "ADD_EVIDENCE"],
        actionBlockers: [],
        repairLinks: [],
        auditTrail: (diff.resolutions ?? []).map((r) => ({
            id: r.id,
            at: tsToIso(r.handled_at),
            actor: r.handled_by,
            action: r.resolution_action,
            detail: r.evidence_reference ?? r.resulting_status,
        })),
        evidenceTimeline: [],
        linkedEvidence: [],
        freshness: { updatedAt: tsToIso(diff.created_at) },
    }
}

export function matchesQuery(
    item: IntegrationResolutionItemView,
    q: IntegrationResolutionQuery,
): boolean {
    if (q.mode === "errors" && item.identity.itemType !== "ERROR_TASK") {
        return false
    }
    if (q.view === "result_unknown") {
        if (item.classification.errorClass !== "result-unknown") return false
    }
    if (q.view === "security") {
        if (item.classification.errorClass !== "authentication-or-signature")
            return false
    }
    if (q.view === "reconciliation") {
        if (item.identity.itemType !== "RECONCILIATION_DIFFERENCE") return false
    }
    if (q.view === "auto_retry") {
        if (
            item.classification.errorClass !== "network-timeout" &&
            item.classification.errorClass !== "rate-limited"
        ) {
            return false
        }
    }
    if (q.view === "resolved") {
        // open queue excludes resolved — detail path still works
        if (
            item.status.code !== "resolved" &&
            item.status.code !== "closed" &&
            !item.status.code?.startsWith("confirm")
        ) {
            return false
        }
    }
    if (q.errorClass && item.classification.errorClass !== q.errorClass) {
        return false
    }
    if (q.q) {
        const needle = q.q.toLowerCase()
        const hay = [
            item.identity.number,
            item.identity.id,
            item.businessObject.title,
            item.businessObject.objectId,
            item.classification.label,
        ]
            .join(" ")
            .toLowerCase()
        if (!hay.includes(needle)) return false
    }
    return true
}
