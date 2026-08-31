/**
 * W06 客户验收 — 后端报文形状与前端类型之间的映射（纯函数）。
 * 从 api/acceptance.ts 拆出，供 workspace 读取与登记/冲正变更共用。
 */

import type {
    AcceptanceEligibleFact,
    AcceptanceHistoryItem,
    AcceptanceOverallResult,
    AcceptanceSalesLineGroup,
    AcceptanceStatus,
    FulfillmentFactType,
    SaveAcceptanceDraftInput,
} from "@/features/sales-orders/lib/acceptance-types"
import { FACT_ONLY_NOTICE } from "@/features/sales-orders/lib/acceptance-types"
import { compareDecimal } from "@/lib/fixed-decimal"

// ─── 后端形状 ────────────────────────────────────────────────────────────────

export type BackendEligibleFact = {
    fulfillment_line_id: string
    fulfillment_fact_type: string
    /** 发货类型（仓发/直发区分；仅发货事实携带） */
    delivery_type?: string | null
    fulfillment_no: string
    sales_order_line_id: string
    line_no: number
    item_snapshot: string
    unit_code?: string | null
    occurred_at: number
    net_successful_quantity: string
    net_accepted_allocated_quantity: string
    eligible_quantity: string
    carrier?: string | null
    tracking_no?: string | null
}

export type BackendSalesLineGroup = {
    sales_order_line_id: string
    line_no: number
    item_snapshot: string
    unit_code?: string | null
    required_quantity: string
    net_accepted_quantity: string
    fulfillment_facts: BackendEligibleFact[]
}

export type BackendAcceptanceHeader = {
    id: string
    acceptance_no: string
    sales_order_id: string
    accepted_at: number
    result: string
    status: string
    reversal_of_acceptance_id?: string | null
    version: number
    created_at: number
}

export type BackendEligibilityView = {
    sales_order_id: string
    sales_lines: BackendSalesLineGroup[]
    history: BackendAcceptanceHeader[]
}

export type BackendAcceptanceDetail = {
    acceptance: BackendAcceptanceHeader
    lines: Array<{
        id: string
        line_no: number
        sales_order_line_id: string
        accepted_quantity: string
        short_quantity: string
        rejected_quantity: string
        reason?: string | null
    }>
    allocations: Array<{
        id: string
        customer_acceptance_line_id: string
        fulfillment_fact_type: string
        fulfillment_line_id: string
        allocation_action: string
        allocated_quantity: string
        reverses_allocation_id?: string | null
    }>
}

export type PageView<T> = {
    items: T[]
    total: number
    page: number
    page_size: number
}

// ─── 映射 ────────────────────────────────────────────────────────────────────

export function formatInstant(secs?: number | null): string {
    if (secs == null || secs <= 0) return ""
    return new Date(secs * 1000).toISOString()
}

export function mapFactType(
    code: string,
    deliveryType?: string | null,
): FulfillmentFactType {
    switch (code) {
        case "ELECTRONIC_DELIVERY":
            return "ELECTRONIC"
        case "SERVICE_FULFILLMENT":
            return "SERVICE"
        case "DELIVERY":
            // 后端只有 DELIVERY / ELECTRONIC / SERVICE；发货事实按 delivery_type
            // 区分仓发与直发（W06 验收区标签分别显示「仓发/代发」）
            return deliveryType === "SUPPLIER_DIRECT"
                ? "SUPPLIER_DIRECT"
                : "WAREHOUSE_SHIP"
        default:
            return "WAREHOUSE_SHIP"
    }
}

export function mapFactTypeToBackend(type: FulfillmentFactType): string {
    switch (type) {
        case "ELECTRONIC":
            return "ELECTRONIC_DELIVERY"
        case "SERVICE":
            return "SERVICE_FULFILLMENT"
        case "WAREHOUSE_SHIP":
        case "SUPPLIER_DIRECT":
            return "DELIVERY"
    }
}

export function mapOverallResult(code: string): AcceptanceOverallResult {
    switch (code) {
        case "SHORTAGE":
            return "SHORT"
        case "REJECTED":
            return "REJECT"
        case "SERVICE_FAILED":
            return "SERVICE_FAIL"
        case "PASSED":
        default:
            return "PASS"
    }
}

export function mapOverallResultToBackend(
    lines: SaveAcceptanceDraftInput["lines"],
): string {
    if (lines.some((l) => l.serviceFail)) return "SERVICE_FAILED"
    if (
        lines.some((line) => compareDecimal(line.rejectedQuantity, "0", 6) > 0)
    ) {
        return "REJECTED"
    }
    if (lines.some((line) => compareDecimal(line.shortQuantity, "0", 6) > 0)) {
        return "SHORTAGE"
    }
    return "PASSED"
}

export function mapEligibleFact(
    f: BackendEligibleFact,
): AcceptanceEligibleFact {
    return {
        fulfillmentLineId: f.fulfillment_line_id,
        fulfillmentFactType: mapFactType(
            f.fulfillment_fact_type,
            f.delivery_type,
        ),
        fulfillmentNo: f.fulfillment_no,
        salesOrderLineId: f.sales_order_line_id,
        lineNo: f.line_no,
        itemSnapshot: f.item_snapshot,
        unitCode: f.unit_code ?? "",
        occurredAt: formatInstant(f.occurred_at),
        netSuccessfulQuantity: f.net_successful_quantity,
        netAcceptedAllocatedQuantity: f.net_accepted_allocated_quantity,
        eligibleQuantity: f.eligible_quantity,
        carrier: f.carrier ?? undefined,
        trackingNo: f.tracking_no ?? undefined,
    }
}

/** 是否还有尚未验收完的履约事实。无交付记录时不可当作当前验收待办。 */
export function hasRemainingEligibleAcceptance(
    eligibility: BackendEligibilityView,
): boolean {
    return (eligibility.sales_lines ?? []).some((line) =>
        (line.fulfillment_facts ?? []).some(
            (fact) => compareDecimal(fact.eligible_quantity, "0", 6) > 0,
        ),
    )
}

export function mapSalesLine(
    g: BackendSalesLineGroup,
): AcceptanceSalesLineGroup {
    return {
        salesOrderLineId: g.sales_order_line_id,
        lineNo: g.line_no,
        itemSnapshot: g.item_snapshot,
        unitCode: g.unit_code ?? "",
        requiredQuantity: g.required_quantity,
        netAcceptedQuantity: g.net_accepted_quantity,
        fulfillmentFacts: (g.fulfillment_facts ?? []).map(mapEligibleFact),
    }
}

export function mapHistoryItem(
    h: BackendAcceptanceHeader,
): AcceptanceHistoryItem | null {
    const status = h.status as AcceptanceStatus
    if (status !== "POSTED" && status !== "REVERSED") return null
    return {
        acceptanceId: h.id,
        acceptanceNo: h.acceptance_no,
        status,
        acceptedAt: formatInstant(h.accepted_at),
        postedAt: formatInstant(h.created_at),
        overallResult: mapOverallResult(h.result),
        lines: [],
        recordedBy: "",
        version: h.version,
        reversedByAcceptanceId: h.reversal_of_acceptance_id ?? undefined,
        factOnlyNotice: FACT_ONLY_NOTICE,
    }
}

/**
 * 映射完整验收历史，并从原记录的反向引用推导“冲正记录 → 原记录”。
 * 后端字段保存在被冲正的原记录上，前端必须二次关联后才能判定反向记录。
 */
export function mapAcceptanceHistory(
    headers: BackendAcceptanceHeader[],
): AcceptanceHistoryItem[] {
    const items = headers
        .map(mapHistoryItem)
        .filter((item): item is AcceptanceHistoryItem => item != null)
    const originalByReverseId = new Map<string, AcceptanceHistoryItem>()
    for (const item of items) {
        if (item.reversedByAcceptanceId) {
            originalByReverseId.set(item.reversedByAcceptanceId, item)
        }
    }
    return items.map((item) => {
        const original = originalByReverseId.get(item.acceptanceId)
        return original
            ? { ...item, reversalOfAcceptanceId: original.acceptanceId }
            : item
    })
}
