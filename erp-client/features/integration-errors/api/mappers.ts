/**
 * W29 接口错误与对账中心 · 后端 DTO 映射与筛选。
 * 请求函数见 ./requests；本文件只含纯映射/筛选逻辑（无 React、无 HTTP 调用）。
 */

import type { InterfaceErrorClass } from "@/components/business"
import type { WorkItemProjection } from "@/features/work-items"
import type {
    IntegrationResolutionItemView,
    IntegrationResolutionQuery,
} from "../types"
import { ERROR_CLASS_LABEL, FUNDS_LABEL } from "../types"
import {
    mapBackendEvidenceRefs,
    mapBackendReconciliationReasonRegistry,
    mapBackendResolutionEvidencePolicy,
    mapAllowedIntegrationActions,
    type BackendControlledEvidenceRef,
    type BackendReconciliationReasonRegistry,
    type BackendResolutionEvidencePolicy,
} from "./wire"

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
    allowed_actions?: string[] | null
    action_blockers?: IntegrationResolutionItemView["actionBlockers"] | null
    linked_evidence?: BackendControlledEvidenceRef[] | null
    resolution_evidence_policy?: BackendResolutionEvidencePolicy | null
    reconciliation_reason_registry?: BackendReconciliationReasonRegistry | null
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
    allowed_actions?: string[] | null
    action_blockers?: IntegrationResolutionItemView["actionBlockers"] | null
    linked_evidence?: BackendControlledEvidenceRef[] | null
    resolution_evidence_policy?: BackendResolutionEvidencePolicy | null
    reconciliation_reason_registry?: BackendReconciliationReasonRegistry | null
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

function mapFormalWorkItem(workItem: WorkItemProjection) {
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

export function mapErrorTask(
    task: BackendErrorTask,
    formalWorkItem?: WorkItemProjection,
): IntegrationResolutionItemView {
    const errorClass = mapErrorClass(task.error_class)
    const label = ERROR_CLASS_LABEL[errorClass] ?? task.error_class
    const severity = severityOf(task.error_class)
    const workItem = formalWorkItem
        ? mapFormalWorkItem(formalWorkItem)
        : undefined
    const resolutionEvidencePolicy = workItem
        ? mapBackendResolutionEvidencePolicy(task.resolution_evidence_policy)
        : undefined
    const linkedEvidence = mapBackendEvidenceRefs(task.linked_evidence)
    const fundsImpact = resolutionEvidencePolicy?.key.fundsImpact ?? "NONE"
    const allowedActions = workItem
        ? mapAllowedIntegrationActions(task.allowed_actions, {
              hasWorkItem: true,
              hasResolutionPolicy: resolutionEvidencePolicy !== undefined,
              directConclusions: [],
          })
        : []

    return {
        identity: {
            itemType: "ERROR_TASK",
            id: task.id,
            number: task.id,
            subjectHash: `v${task.version}`,
        },
        workItem,
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
        fundsImpact,
        fundsImpactLabel: FUNDS_LABEL[fundsImpact],
        compensationOpen: false,
        ageLabel: ageLabel(task.created_at),
        ownerRole: formalWorkItem?.ownerRoleLabel ?? task.owner_role ?? "—",
        ownerUser:
            formalWorkItem?.ownerUser?.displayName ??
            task.owner_user_id ??
            undefined,
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
        hasWorkItem: workItem !== undefined,
        resolutionEvidencePolicy,
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
        allowedActions,
        actionBlockers: [
            ...(task.action_blockers ?? []),
            ...(workItem
                ? []
                : [
                      {
                          action: "PROCESS",
                          code: "FORMAL_WORK_ITEM_MISSING",
                          message:
                              "尚未建立 W29 正式处理责任，当前错误只能查看。",
                      },
                  ]),
        ],
        repairLinks: [],
        auditTrail: [],
        evidenceTimeline: [],
        linkedEvidence,
        freshness: {
            updatedAt:
                tsToIso(task.last_attempt_at) || tsToIso(task.created_at),
        },
    }
}

export function mapDifference(
    diff: BackendDifference,
    formalWorkItem?: WorkItemProjection,
): IntegrationResolutionItemView {
    const terminal =
        diff.status === "confirmed_no_error" ||
        diff.status === "confirmed_valid_difference"

    const workItem = formalWorkItem
        ? mapFormalWorkItem(formalWorkItem)
        : undefined
    const resolutionEvidencePolicy = workItem
        ? mapBackendResolutionEvidencePolicy(diff.resolution_evidence_policy)
        : undefined
    const reconciliationReasonRegistry = workItem
        ? undefined
        : mapBackendReconciliationReasonRegistry(
              diff.reconciliation_reason_registry,
          )
    const directConclusions =
        reconciliationReasonRegistry?.registeredReasons.map(
            (reason) => reason.conclusion,
        ) ?? []
    const linkedEvidence = mapBackendEvidenceRefs(diff.linked_evidence)
    const fundsImpact = resolutionEvidencePolicy?.key.fundsImpact ?? "POTENTIAL"
    const allowedActions = terminal
        ? []
        : mapAllowedIntegrationActions(diff.allowed_actions, {
              hasWorkItem: workItem !== undefined,
              hasResolutionPolicy: resolutionEvidencePolicy !== undefined,
              directConclusions,
          })
    return {
        identity: {
            itemType: "RECONCILIATION_DIFFERENCE",
            id: diff.id,
            number: diff.id,
            subjectHash: `v${diff.version}`,
        },
        workItem,
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
        fundsImpact,
        fundsImpactLabel: FUNDS_LABEL[fundsImpact],
        compensationOpen: false,
        ageLabel: ageLabel(diff.created_at),
        ownerRole: formalWorkItem?.ownerRoleLabel ?? "财务",
        ownerUser: formalWorkItem?.ownerUser?.displayName,
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
        hasWorkItem: workItem !== undefined,
        resolutionEvidencePolicy,
        reconciliationReasonRegistry,
        attempts: [],
        objectVersion: String(diff.version),
        allowedActions,
        actionBlockers: diff.action_blockers ?? [],
        repairLinks: [],
        auditTrail: (diff.resolutions ?? []).map((r) => ({
            id: r.id,
            at: tsToIso(r.handled_at),
            actor: r.handled_by,
            action: r.resolution_action,
            detail: r.evidence_reference ?? r.resulting_status,
        })),
        evidenceTimeline: [],
        linkedEvidence,
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
