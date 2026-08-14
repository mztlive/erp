/**
 * W08 采购单 · 状态与显示映射（前后端枚举转换、进度文案、类型归一化）。
 */

import type {
    FulfillmentResponsibility,
    PurchaseOrderMetricFilter,
    PurchaseOrderStatus,
    PurchaseOrderStatusFilter,
    PurchaseReviewStatus,
    PurchaseType,
} from "@/features/purchase-orders/types"
import { PAYMENT_TERM_OPTIONS } from "@/features/purchase-orders/types"

export function secsToIso(secs?: number | null): string {
    if (secs == null || secs <= 0) return new Date(0).toISOString()
    return new Date(secs * 1000).toISOString()
}

/** 前端状态 → 后端状态 */
export function toBackendStatus(
    status?: PurchaseOrderStatusFilter,
): string | undefined {
    if (!status || status === "all") return undefined
    switch (status) {
        case "PENDING_REVIEW":
            return "PENDING_FINANCE_REVIEW"
        case "PARTIAL":
            return "PARTIALLY_EXECUTED"
        case "VOID":
            return "VOIDED"
        default:
            return status
    }
}

/** 后端状态 → 前端状态 */
export function fromBackendStatus(status: string): PurchaseOrderStatus {
    switch (status) {
        case "PENDING_FINANCE_REVIEW":
            return "PENDING_REVIEW"
        case "PARTIALLY_EXECUTED":
            return "PARTIAL"
        case "VOIDED":
            return "VOID"
        case "DRAFT":
        case "EFFECTIVE":
        case "COMPLETED":
            return status
        default:
            return "DRAFT"
    }
}

export function fromBackendReviewStatus(
    status: string,
    orderStatus: PurchaseOrderStatus,
): PurchaseReviewStatus {
    if (orderStatus === "DRAFT" && status !== "REJECTED") {
        // 草稿且未进入审核轨时前端展示 NONE
        if (status === "PENDING" || !status) return "NONE"
    }
    if (status === "PENDING") return "PENDING"
    if (status === "APPROVED") return "APPROVED"
    if (status === "REJECTED") return "REJECTED"
    return "NONE"
}

export function progressDisplay(
    code: string,
    kind: "payment" | "invoice" | "fulfillment",
): string {
    const normalized = (code ?? "NONE").toUpperCase()
    if (kind === "payment") {
        if (normalized === "NONE") return "未付"
        if (normalized === "PARTIAL") return "部分"
        if (normalized === "COMPLETED") return "已付"
    }
    if (kind === "invoice") {
        if (normalized === "NONE") return "未收"
        if (normalized === "PARTIAL") return "部分"
        if (normalized === "COMPLETED") return "完成"
    }
    if (kind === "fulfillment") {
        if (normalized === "NONE") return "未开始"
        if (normalized === "PARTIAL") return "部分"
        if (normalized === "COMPLETED") return "完成"
    }
    return code || "—"
}

export function paymentTermLabel(code: string): string {
    return (
        PAYMENT_TERM_OPTIONS.find((o) => o.value === code)?.label ??
        (code === "NET-30" ? "货到 30 天" : code || "—")
    )
}

export function mapPurchaseType(value: string): PurchaseType {
    if (value === "PHYSICAL" || value === "VIRTUAL" || value === "SERVICE") {
        return value
    }
    return "PHYSICAL"
}

export function mapFulfillment(value: string): FulfillmentResponsibility {
    if (
        value === "WAREHOUSE" ||
        value === "SUPPLIER_DIRECT" ||
        value === "ELECTRONIC" ||
        value === "SERVICE"
    ) {
        return value
    }
    return "WAREHOUSE"
}

export function deriveAllowedActions(status: PurchaseOrderStatus): string[] {
    const common = ["OPEN_CENTER", "PRINT"]
    if (status === "DRAFT") {
        return [...common, "EDIT", "SUBMIT", "VOID"]
    }
    if (status === "PENDING_REVIEW") {
        // 财务审核只能从服务端 review_work_item 责任投影进入。
        return common
    }
    if (status === "EFFECTIVE" || status === "PARTIAL") {
        return [...common, "FULFILL", "PAY", "START_CHANGE"]
    }
    return common
}

export function metricStatusParam(
    metric: PurchaseOrderMetricFilter | undefined,
): string | undefined {
    switch (metric) {
        case "draft":
            return "DRAFT"
        case "review":
            return "PENDING_FINANCE_REVIEW"
        case "fulfill":
            // 后端无「待履约」复合筛选；用 EFFECTIVE 近似
            return "EFFECTIVE"
        case "gate_blocked":
            // 缺口：无门禁筛选
            return undefined
        default:
            return undefined
    }
}
