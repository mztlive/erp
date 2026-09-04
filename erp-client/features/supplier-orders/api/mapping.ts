/**
 * W26 供应商订单 · Wire DTO → 客户端视图映射。
 * 后端视图较精简：缺失字段以安全默认值适配并登记 backend_gap。
 */

import type {
    CancelStatus,
    RefundStatus,
    SupplierFulfillmentStatus,
    SupplierOrderDetailView,
    SupplierOrderListQuery,
    SupplierOrderListRow,
    SupplierOrderMetric,
} from "@/features/supplier-orders/types"
import {
    CANCEL_STATUS_LABEL,
    CANCEL_STATUS_TONE,
    FULFILLMENT_STATUS_LABEL,
    FULFILLMENT_STATUS_TONE,
    REFUND_STATUS_LABEL,
    REFUND_STATUS_TONE,
} from "@/features/supplier-orders/types"
import { mapWorkItemDto } from "@/features/work-items/types"
import type { BackendDetail, BackendOrder } from "./wire-types"

export const PAYMENT_OCCURRED_NOTICE =
    "客户款项已收。供应商履约结果独立记录，不得用取消/退款折入履约主状态。"
export const PERMISSION_VERSION = "server"

export function tsToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
        return ""
    return new Date(Number(secs) * 1000).toISOString()
}

export function asFulfillment(raw: string): SupplierFulfillmentStatus {
    const u = raw.toUpperCase() as SupplierFulfillmentStatus
    const allowed: SupplierFulfillmentStatus[] = [
        "RECEIVED",
        "SUBMITTING",
        "ACCEPTED",
        "REJECTED",
        "RESULT_UNKNOWN",
        "FULFILLING",
        "SHIPPED",
        "COMPLETED",
        "EXCEPTION",
    ]
    return allowed.includes(u) ? u : "RECEIVED"
}

export function asCancel(raw: string): CancelStatus {
    const u = raw.toUpperCase() as CancelStatus
    const allowed: CancelStatus[] = [
        "NONE",
        "CANCEL_PENDING",
        "CANCELED",
        "FAILED",
        "MANUAL",
    ]
    return allowed.includes(u) ? u : "NONE"
}

export function asRefund(raw: string): RefundStatus {
    const u = raw.toUpperCase() as RefundStatus
    const allowed: RefundStatus[] = [
        "NONE",
        "REFUND_PENDING",
        "PARTIAL",
        "REFUNDED",
        "REFUND_FAILED",
        "MANUAL",
    ]
    return allowed.includes(u) ? u : "NONE"
}

function priorityOf(status: SupplierFulfillmentStatus): number {
    switch (status) {
        case "RESULT_UNKNOWN":
            return 100
        case "EXCEPTION":
        case "REJECTED":
            return 90
        case "SUBMITTING":
        case "RECEIVED":
            return 70
        default:
            return 10
    }
}

export function mapListRow(o: BackendOrder): SupplierOrderListRow {
    const fulfillment = asFulfillment(o.fulfillment_status)
    const cancel = asCancel(o.cancel_status)
    const refund = asRefund(o.refund_status)
    const lastBusinessAt =
        tsToIso(o.completed_at) ||
        tsToIso(o.accepted_at) ||
        tsToIso(o.submitted_at) ||
        tsToIso(o.created_at)

    return {
        orderId: o.id,
        orderNo: o.fulfillment_order_no,
        supplierId: o.supplier_id,
        supplierName: "",
        externalOrderNo: o.external_order_no ?? undefined,
        fulfillmentStatus: fulfillment,
        fulfillmentLabel: FULFILLMENT_STATUS_LABEL[fulfillment],
        fulfillmentTone: FULFILLMENT_STATUS_TONE[fulfillment],
        cancelStatus: cancel,
        cancelLabel: CANCEL_STATUS_LABEL[cancel],
        cancelTone: CANCEL_STATUS_TONE[cancel],
        refundStatus: refund,
        refundLabel: REFUND_STATUS_LABEL[refund],
        refundTone: REFUND_STATUS_TONE[refund],
        paidAt: tsToIso(o.created_at),
        updatedAt: lastBusinessAt,
        lastBusinessAt,
        itemCount: 0,
        allowedActions: ["OPEN_CENTER", "NOTE"],
        actionBlockers: [
            {
                action: "VIEW_SUPPLIER_NAME",
                code: "SUPPLIER_NAME_NOT_PROJECTED_IN_LIST",
                message: "列表接口未返回权威供应商名称，禁止以 ID 伪装名称",
            },
        ],
        priority: priorityOf(fulfillment),
    }
}

export function emptyMetrics(): SupplierOrderMetric[] {
    return [
        {
            key: "pending_submit",
            label: "待提交",
            value: 0,
            fulfillmentStatuses: ["RECEIVED", "SUBMITTING"],
        },
        {
            key: "result_unknown",
            label: "结果未知",
            value: 0,
            fulfillmentStatus: "RESULT_UNKNOWN",
        },
        {
            key: "exception",
            label: "履约异常",
            value: 0,
            fulfillmentStatuses: ["EXCEPTION", "REJECTED"],
        },
        {
            key: "aftersale",
            label: "售后待处理",
            value: 0,
            aftersalePending: true,
        },
        { key: "all", label: "全部订单", value: 0, view: "all" },
    ]
}

export function filterSummary(
    query: SupplierOrderListQuery,
    total: number,
): string {
    const parts: string[] = []
    if (query.view === "actionable") parts.push("可操作")
    else if (query.view === "recent_completed") parts.push("最近完成")
    else parts.push("全部")
    if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
    if (query.supplierId) parts.push(query.supplierId)
    if (query.fulfillmentStatuses?.length) {
        parts.push(
            query.fulfillmentStatuses
                .map((s) => FULFILLMENT_STATUS_LABEL[s])
                .join("/"),
        )
    }
    parts.push(`${total} 条`)
    return parts.join(" · ")
}

function mapFormalWorkItem(item: ReturnType<typeof mapWorkItemDto>) {
    return {
        workItemId: item.workItemId,
        taskVersion: item.taskVersion,
        workItemType: item.workItemType as
            | "INTEGRATION_RESULT_UNKNOWN"
            | "BUSINESS_EXCEPTION",
        businessObjectType: "SUPPLIER_FULFILLMENT_ORDER" as const,
        businessObjectId: item.businessObjectId,
        subjectVersion: item.subjectVersion,
        processingState: item.processingState,
        ownerUser: item.ownerUser,
        allowedTaskActions: item.allowedActions,
        actionBlockers: item.actionBlockers,
        workItemStatus: item.status,
    }
}

export function mapDetail(d: BackendDetail): SupplierOrderDetailView {
    const o = d.order
    const fulfillment = asFulfillment(o.fulfillment_status)
    const cancel = asCancel(o.cancel_status)
    const refund = asRefund(o.refund_status)
    const formalTask = d.work_item ? mapWorkItemDto(d.work_item) : undefined
    const investigation = d.last_investigation

    return {
        order: {
            id: o.id,
            orderNo: o.fulfillment_order_no,
            paidAt: tsToIso(o.created_at),
            paymentFactKey: "",
            fulfillmentChain: "ERP_AUTOMATED",
            supplierId: o.supplier_id,
            supplierName: d.supplier_name ?? "",
            connectionCode: o.connection_id,
            connectionEnvironment: "production",
            supplyVersion: "",
            externalOrderNo: o.external_order_no ?? undefined,
            fulfillmentStatus: fulfillment,
            fulfillmentLabel: FULFILLMENT_STATUS_LABEL[fulfillment],
            fulfillmentTone: FULFILLMENT_STATUS_TONE[fulfillment],
            cancelStatus: cancel,
            cancelLabel: CANCEL_STATUS_LABEL[cancel],
            cancelTone: CANCEL_STATUS_TONE[cancel],
            refundStatus: refund,
            refundLabel: REFUND_STATUS_LABEL[refund],
            refundTone: REFUND_STATUS_TONE[refund],
            lockVersion: o.version,
            paymentOccurredNotice: PAYMENT_OCCURRED_NOTICE,
        },
        items: (d.items ?? []).map((it) => ({
            itemId: it.id,
            productName:
                it.supplier_product_code_snapshot ??
                it.supplier_sku_code_snapshot,
            skuCode: it.supplier_sku_code_snapshot,
            quantity: String(it.quantity),
            unit: "件",
            supplierProductId:
                it.supplier_product_code_snapshot ??
                it.supplier_sku_code_snapshot,
            supplierProductName:
                it.supplier_product_code_snapshot ??
                it.supplier_sku_code_snapshot,
            supplyVersion: it.supplier_offering_revision_id,
            unitCostGross: String(it.unit_cost_snapshot_gross),
            unitCostNet: null,
            inputTaxRate: String(it.input_tax_rate),
            snapshotImmutable: true as const,
        })),
        logistics: {
            acceptedAt: tsToIso(o.accepted_at) || undefined,
            shippedAt: undefined,
            completedAt: tsToIso(o.completed_at) || undefined,
        },
        statusHistory: (d.status_history ?? []).map((h) => ({
            id: h.id,
            at: tsToIso(h.occurred_at),
            track: "fulfillment" as const,
            fromLabel:
                FULFILLMENT_STATUS_LABEL[asFulfillment(h.previous_status)] ??
                h.previous_status,
            toLabel:
                FULFILLMENT_STATUS_LABEL[asFulfillment(h.new_status)] ??
                h.new_status,
            source: h.source_type,
        })),
        afterSales: [],
        costs: {
            cumulativeCostGross: String(
                d.items?.[0]?.cost_snapshot_total_gross ?? null,
            ),
            cumulativeCostNet: null,
            costSource: "下单成本快照",
            costVariance: null,
        },
        actions: (d.actions ?? []).map((a) => ({
            actionId: a.id,
            actionType:
                (a.action_type as SupplierOrderDetailView["actions"][number]["actionType"]) ||
                "PLACE",
            actionLabel: a.action_type,
            at: tsToIso(a.created_at),
            actor: "系统",
            outcomeLabel: a.status,
            outcomeTone: "neutral" as const,
            idempotencyKeyTail: a.external_request_id
                ? `…${a.external_request_id.slice(-6)}`
                : "—",
            attemptCount: a.attempt_count,
            operationId: a.id,
        })),
        address: {
            masked: d.address.masked ?? "—",
            phoneMasked: "—",
            recipientMasked: "—",
            canReveal: d.address.can_reveal,
        },
        workItem: formalTask ? mapFormalWorkItem(formalTask) : undefined,
        workItemBlocker: undefined,
        lastInvestigation: investigation
            ? {
                  evidenceId: investigation.evidence_id,
                  targetSupplierActionId:
                      investigation.target_supplier_action_id,
                  outcome: investigation.outcome,
                  outcomeLabel: investigation.outcome,
                  recordedAt: tsToIso(investigation.recorded_at),
                  canSafeRetry: investigation.can_safe_retry,
                  externalOrderNo: investigation.external_order_no ?? undefined,
                  summary: investigation.summary,
                  verifiedSupplierActionResultId:
                      investigation.verified_supplier_action_result_id ??
                      undefined,
                  verifiedResolution:
                      investigation.verified_resolution ?? undefined,
              }
            : undefined,
        placeActionId: d.target_supplier_action_id ?? "",
        allowedActions: ["OPEN_CENTER", "NOTE", ...(d.allowed_actions ?? [])],
        actionBlockers: (d.action_blockers ?? []).map((blocker) => ({
            action: blocker.action,
            code: blocker.code,
            message: blocker.message,
        })),
        freshness: {
            updatedAt: tsToIso(o.created_at),
            state: "fresh",
        },
    }
}
