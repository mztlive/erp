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
import { paymentTermLabel as sharedPaymentTermLabel } from "@/lib/business-options"

export function secsToIso(secs?: number | null): string {
    if (secs == null || secs <= 0) return new Date(0).toISOString()
    return new Date(secs * 1000).toISOString()
}

/**
 * 把前端状态筛选映射为后端状态码。
 *
 * `PENDING_REVIEW` 对应统一审批中的 `IN_APPROVAL`。
 */
export function toBackendStatus(
    status?: PurchaseOrderStatusFilter,
): string | undefined {
    if (!status || status === "all") return undefined
    switch (status) {
        case "PENDING_REVIEW":
            return "IN_APPROVAL"
        case "PARTIAL":
            return "PARTIALLY_EXECUTED"
        case "VOID":
            return "VOIDED"
        default:
            return status
    }
}

/**
 * 把后端状态码映射为前端生命周期。
 *
 * `IN_APPROVAL` 与旧的待财务审核码都视为审批中，不上屏枚举原值。
 */
export function fromBackendStatus(status: string): PurchaseOrderStatus {
    switch (status) {
        case "IN_APPROVAL":
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
    const term =
        code
            .trim()
            .split(/\s*[|｜]\s*经营类目：/)[0]
            ?.trim() ?? ""
    return term ? sharedPaymentTermLabel(term) : "—"
}

/**
 * 历史供应商商务快照把结算方式和经营类目编进同一字符串。
 * 新数据已拆字段；解析仍用于已落单的付款条件代码。
 */
export function parsePaymentTermSnapshot(raw: string): {
    paymentTerm: string
    businessCategory: string
} {
    const trimmed = raw.trim()
    if (!trimmed) {
        return { paymentTerm: "—", businessCategory: "" }
    }
    const [termPart, ...categoryParts] = trimmed.split(/\s*[|｜]\s*经营类目：/)
    const term = (termPart ?? "").trim()
    return {
        paymentTerm: paymentTermLabel(term),
        businessCategory: categoryParts
            .map((part) => part.trim())
            .filter(Boolean)
            .join("、"),
    }
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

/**
 * 由生命周期给出业务动作。审批决定不在此推导，只读服务端白名单。
 */
export function deriveAllowedActions(status: PurchaseOrderStatus): string[] {
    const common = ["OPEN_CENTER", "PRINT"]
    if (status === "DRAFT") {
        return [...common, "EDIT", "SUBMIT", "VOID"]
    }
    if (status === "PENDING_REVIEW") {
        // 审批决定只能从服务端 allowed_actions 进入。
        return common
    }
    if (status === "EFFECTIVE" || status === "PARTIAL") {
        return [...common, "FULFILL", "PAY", "START_CHANGE"]
    }
    return common
}

/**
 * 把列表指标筛选项映射为后端状态。审批中对应 `IN_APPROVAL`。
 */
export function metricStatusParam(
    metric: PurchaseOrderMetricFilter | undefined,
): string | undefined {
    switch (metric) {
        case "draft":
            return "DRAFT"
        case "review":
            return "IN_APPROVAL"
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
