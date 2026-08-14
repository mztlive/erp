/**
 * W23 销售单执行投影 · 后端线格式与前端契约之间的映射（纯函数，无请求）。
 * 对外导出签名保持稳定；白名单字段以外不组装商业字段。
 */

import { apiGet, type Page } from "@/lib/api"
import type {
    DeliveryStatus,
    ExecutionProjectionListQuery,
    ExecutionProjectionMetric,
    ExecutionProjectionRow,
    ProjectionSource,
    ProjectionWhitelistContent,
} from "@/features/execution-projections/types"
import {
    DELIVERY_STATUS_LABEL,
    DELIVERY_STATUS_TONE,
    LATENCY_LABEL,
    RECONCILIATION_LABEL,
    SOURCE_LABEL,
} from "@/features/execution-projections/types"

// ─── Backend wire types ──────────────────────────────────────────────────────

export type BackendProjection = {
    id: string
    sales_order_id: string
    target_mall_id: string
    current_acked_revision_id?: string | null
    version: number
    created_at: number
}

export type BackendRevision = {
    id: string
    projection_id: string
    revision_no: number
    projection_source: string
    sales_order_revision_id: string
    customer_external_identity: string
    face_value: string
    card_count: number
    card_form: string
    effective_at: number
    version: number
    created_at: number
}

export type BackendDelivery = {
    id: string
    projection_revision_id: string
    target_mall_id: string
    status: string
    attempt_count: number
    last_attempt_at?: number | null
    next_attempt_at?: number | null
    mall_ack_at?: number | null
    mall_execution_baseline?: string | null
    error_class?: string | null
    error_code?: string | null
    error_summary?: string | null
    error_task_id?: string | null
    work_item_id?: string | null
    allowed_actions?: string[]
    action_blockers?: Array<{ action: string; code: string; message: string }>
    version: number
    created_at: number
}

export type BackendDeliveryActionResult = {
    operation_id: string
    delivery_id: string
    result:
        | "ACKED"
        | "FAILED"
        | "STILL_UNKNOWN"
        | "RETRY_SCHEDULED"
        | "ESCALATED"
    work_item_id?: string | null
    error_task_id?: string | null
    occurred_at: number
    next_action?: string | null
}

type SourceSystem = {
    id: string
    code: string
    name: string
    system_type?: string
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

export function secsToIso(secs?: number | null): string {
    if (secs == null || secs <= 0) return new Date(0).toISOString()
    return new Date(secs * 1000).toISOString()
}

export function mapDeliveryStatus(raw: string): DeliveryStatus {
    switch (raw) {
        case "pending_send":
            return "PENDING"
        case "sending":
            return "SENDING"
        case "retrying":
            return "RETRYING"
        case "result_unknown":
            return "UNKNOWN"
        case "confirmed":
            return "ACKED"
        case "failed":
            return "FAILED"
        case "manual":
            return "ESCALATED_MANUAL"
        default:
            return "PENDING"
    }
}

export function mapSource(raw: string): ProjectionSource {
    if (raw === "cutover_snapshot") return "MIGRATION_BASELINE"
    return "ERP_SALES_REVISION"
}

export function mapCardForm(raw: string): string {
    if (raw === "electronic") return "电子卡"
    if (raw === "physical") return "实体卡"
    return raw
}

export function recomputeActions(status: DeliveryStatus): {
    allowedActions: string[]
    actionBlockers: Array<{ action: string; code: string; message: string }>
} {
    const blockers: Array<{ action: string; code: string; message: string }> =
        []
    const allowed: string[] = []

    if (status === "UNKNOWN" || status === "SENDING" || status === "RETRYING") {
        allowed.push("QUERY_RESULT")
    }
    if (status === "FAILED") {
        allowed.push("RETRY", "QUERY_RESULT", "ESCALATE")
    }
    if (status === "ESCALATED_MANUAL") {
        allowed.push("ESCALATE")
        blockers.push(
            {
                action: "RETRY",
                code: "ESCALATED",
                message:
                    "已转人工处理，按单据重试请到接口错误中心按原任务号处理。",
            },
            {
                action: "QUERY_RESULT",
                code: "ESCALATED",
                message: "已有错误记录，请在接口错误中心处理。",
            },
        )
    }
    if (status === "ACKED") {
        blockers.push(
            {
                action: "RETRY",
                code: "ALREADY_ACKED",
                message: "商城已确认，无需重试。",
            },
            {
                action: "QUERY_RESULT",
                code: "ALREADY_ACKED",
                message: "已有明确确认结果。",
            },
        )
    }
    if (status === "PENDING") {
        blockers.push(
            {
                action: "RETRY",
                code: "NOT_YET_SENT",
                message: "尚未首次发送，将由后台按计划执行。",
            },
            {
                action: "QUERY_RESULT",
                code: "NO_REQUEST",
                message: "尚无可查询的原请求。",
            },
        )
    }
    if (status === "SENDING" || status === "RETRYING") {
        blockers.push({
            action: "RETRY",
            code: "IN_FLIGHT",
            message: "正在发送中，请勿重复操作。",
        })
    }
    if (status === "UNKNOWN") {
        blockers.push({
            action: "RETRY",
            code: "RESULT_UNKNOWN",
            message: "结果未知须先查询最终结果，未明确前不得重试或标为成功。",
        })
    }

    return { allowedActions: [...new Set(allowed)], actionBlockers: blockers }
}

export function whitelistFromRevision(
    rev: BackendRevision,
): ProjectionWhitelistContent {
    return {
        customerExternalIdentity: rev.customer_external_identity,
        customerExternalIdentityCopyable: false,
        voucherCategoryExternalIdentity: "—",
        voucherCategoryErpName: "—",
        voucherExpiryAt: "—",
        faceValue: String(rev.face_value),
        cardCount: String(rev.card_count),
        cardForm: mapCardForm(rev.card_form),
        effectiveAt: secsToIso(rev.effective_at),
        contentHash: rev.id,
    }
}

export async function loadMalls(): Promise<
    Array<{ id: string; name: string }>
> {
    try {
        const page = await apiGet<Page<SourceSystem>>("/admin/source-systems", {
            page: 1,
            page_size: 100,
            system_type: "MALL",
        })
        return page.items.map((s) => ({ id: s.id, name: s.name }))
    } catch {
        return []
    }
}

export function mallName(
    malls: Array<{ id: string; name: string }>,
    id: string,
): string {
    return malls.find((m) => m.id === id)?.name ?? id
}

export function computeMetrics(
    rows: ExecutionProjectionRow[],
): ExecutionProjectionMetric[] {
    return [
        {
            key: "pending_send",
            label: "待发送",
            value: rows.filter((r) => r.delivery.status === "PENDING").length,
        },
        {
            key: "inflight",
            label: "发送中",
            value: rows.filter((r) =>
                ["SENDING", "RETRYING", "UNKNOWN"].includes(r.delivery.status),
            ).length,
        },
        {
            key: "timeout",
            label: "已超时",
            value: rows.filter((r) => r.latencyBand === "over_sla").length,
        },
        {
            key: "fail_manual",
            label: "失败/转人工",
            value: rows.filter((r) =>
                ["FAILED", "ESCALATED_MANUAL"].includes(r.delivery.status),
            ).length,
        },
        {
            key: "acked",
            label: "已确认",
            value: rows.filter((r) => r.delivery.status === "ACKED").length,
        },
    ]
}

export function filterSummary(query: ExecutionProjectionListQuery): string {
    const parts: string[] = []
    if (query.mallId) parts.push(`商城=${query.mallId}`)
    if (query.deliveryStatus) parts.push(`状态=${query.deliveryStatus}`)
    if (query.source && query.source !== "all") {
        parts.push(
            `来源=${SOURCE_LABEL[query.source as ProjectionSource] ?? query.source}`,
        )
    }
    if (query.latency && query.latency !== "all") {
        parts.push(`等待时长=${LATENCY_LABEL[query.latency]}`)
    }
    if (query.reconciliation && query.reconciliation !== "all") {
        parts.push(
            `对账=${RECONCILIATION_LABEL[query.reconciliation] ?? query.reconciliation}`,
        )
    }
    if (query.q?.trim()) parts.push(`搜索=${query.q.trim()}`)
    return parts.length ? parts.join(" · ") : "默认：风险优先 · 全状态"
}

export function toRow(
    proj: BackendProjection,
    rev: BackendRevision | undefined,
    delivery: BackendDelivery | undefined,
    malls: Array<{ id: string; name: string }>,
): ExecutionProjectionRow {
    const status = delivery
        ? mapDeliveryStatus(delivery.status)
        : ("PENDING" as DeliveryStatus)
    const actions = delivery?.allowed_actions
        ? {
              allowedActions: delivery.allowed_actions,
              actionBlockers: delivery.action_blockers ?? [],
          }
        : recomputeActions(status)
    const content = rev
        ? whitelistFromRevision(rev)
        : {
              customerExternalIdentity: "—",
              customerExternalIdentityCopyable: false,
              voucherCategoryExternalIdentity: "—",
              voucherCategoryErpName: "—",
              voucherExpiryAt: "—",
              faceValue: "0",
              cardCount: "0",
              cardForm: "—",
              effectiveAt: secsToIso(proj.created_at),
              contentHash: proj.id,
          }

    return {
        projectionId: proj.id,
        projectionNo: proj.id.slice(0, 12).toUpperCase(),
        projectionRevisionId: rev?.id ?? "",
        projectionRevisionNo: rev?.revision_no ?? 0,
        projectionSource: rev
            ? mapSource(rev.projection_source)
            : "ERP_SALES_REVISION",
        salesOrderId: proj.sales_order_id,
        salesOrderNo: proj.sales_order_id,
        salesOrderRevisionId: rev?.sales_order_revision_id ?? "",
        salesOrderRevisionNo: rev?.revision_no ?? 0,
        salesOrderStatus: "—",
        salesOrderStatusTone: "neutral",
        customerLabel: content.customerExternalIdentity,
        targetMallId: proj.target_mall_id,
        targetMallName: mallName(malls, proj.target_mall_id),
        currentAckedRevisionNo: undefined,
        delivery: {
            deliveryId: delivery?.id ?? `dlv_${proj.id}`,
            status,
            statusLabel: DELIVERY_STATUS_LABEL[status],
            statusTone: DELIVERY_STATUS_TONE[status],
            attemptCount: delivery?.attempt_count ?? 0,
            lastAttemptAt: delivery
                ? secsToIso(delivery.last_attempt_at ?? delivery.created_at)
                : undefined,
            nextAttemptAt: delivery?.next_attempt_at
                ? secsToIso(delivery.next_attempt_at)
                : undefined,
            mallAckAt: delivery?.mall_ack_at
                ? secsToIso(delivery.mall_ack_at)
                : undefined,
            errorCode: delivery?.error_code ?? undefined,
            errorSummary: delivery?.error_summary ?? undefined,
            errorTaskId: delivery?.error_task_id ?? undefined,
            workItemId: delivery?.work_item_id ?? undefined,
        },
        latencyBand: "normal",
        reconciliationStatus: "NONE",
        pendingDurationLabel: "—",
        ownerLabel: "—",
        allowedActions: actions.allowedActions,
        actionBlockers: actions.actionBlockers,
        objectVersion: String(delivery?.version ?? proj.version),
        whitelistPreview: {
            voucherCategoryErpName: content.voucherCategoryErpName,
            faceValue: content.faceValue,
            cardCount: content.cardCount,
            cardForm: content.cardForm,
            voucherExpiryAt: content.voucherExpiryAt,
        },
    }
}
