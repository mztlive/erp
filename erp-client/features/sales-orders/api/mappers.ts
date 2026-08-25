import type { SalesOrdersListQuery } from "@/features/sales-orders/api/contracts"

export {
    formatEpochDate,
    formatInstant,
    formatIsoNow,
    mapFulfillmentModeFromBackend,
    mapListItemFromBackend,
    mapNature,
    throwValidation,
} from "@/features/sales-orders/lib/sales-order-detail-mappers"
export {
    mapChangeOrder,
    mapDetailToListItem,
    pickSalesOrderCommercialSource,
} from "@/features/sales-orders/lib/sales-order-detail-projection-mappers"

export function mapCommercialStatusFilterToBackend(
    status?: string,
): string | undefined {
    switch (status) {
        case "draft":
            return "DRAFT"
        case "pending_review":
            return "PENDING_REVIEW"
        case "effective":
            return "EFFECTIVE"
        case "voided":
            return "VOIDED"
        default:
            return undefined
    }
}

export function mapReviewStatusFilterToBackend(
    status?: string,
): string | undefined {
    switch (status) {
        case "not_submitted":
            return "NOT_SUBMITTED"
        case "pending_procurement_confirmation":
            return "PENDING_PROCUREMENT_CONFIRMATION"
        case "pending_low_margin_superior":
            return "PENDING_LOW_MARGIN_SUPERIOR"
        case "pending_sales_leader":
            return "PENDING_SALES_LEADER"
        case "pending_operations":
            return "PENDING_OPERATIONS"
        case "approved":
            return "APPROVED"
        case "rejected":
            return "REJECTED"
        default:
            return undefined
    }
}

export function mapFulfillmentFilterToBackend(
    progress?: string,
): string | undefined {
    switch (progress) {
        case "not_started":
            return "NOT_STARTED"
        case "partially_fulfilled":
            return "PARTIALLY_FULFILLED"
        case "completed":
            return "COMPLETED"
        default:
            return undefined
    }
}

export function mapCollectionFilterToBackend(
    progress?: string,
): string | undefined {
    switch (progress) {
        case "not_collected":
            return "NOT_COLLECTED"
        case "partially_collected":
            return "PARTIALLY_COLLECTED"
        case "settled":
            return "SETTLED"
        default:
            return undefined
    }
}

export function mapInvoiceFilterToBackend(
    progress?: string,
): string | undefined {
    switch (progress) {
        case "not_invoiced":
            return "NOT_INVOICED"
        case "partially_invoiced":
            return "PARTIALLY_INVOICED"
        case "completed":
            return "COMPLETED"
        default:
            return undefined
    }
}

export function mapCloseFilterToBackend(status?: string): string | undefined {
    switch (status) {
        case "not_satisfied":
            return "NOT_SATISFIED"
        case "closeable":
            return "CLOSEABLE"
        case "closed":
            return "CLOSED"
        default:
            return undefined
    }
}

export function mapSortBy(
    sortBy?: SalesOrdersListQuery["sortBy"],
): string | undefined {
    if (!sortBy) return undefined
    if (sortBy === "documentNumber") return "order_no"
    if (sortBy === "submittedAt") return "created_at"
    // amountGross / contractNumber / ownerName 不在后端白名单
    return "created_at"
}

export function mapFulfillmentMode(label: string): string {
    const t = label.trim()
    if (t.includes("直发")) return "SUPPLIER_DIRECT"
    if (t.includes("电子")) return "ELECTRONIC_DELIVERY"
    if (t.includes("服务") || t.includes("线下")) return "OFFLINE_SERVICE"
    return "COMPANY_WAREHOUSE"
}

export function mapCardForm(label: string): string {
    return label.includes("实体") ? "PHYSICAL" : "ELECTRONIC"
}

/** 福利场景：表单码 / 中文 → 后端 SCREAMING_SNAKE_CASE；无法识别则 null。 */
export function mapWelfareScenarioCode(raw: string): string | null {
    const value = raw.trim()
    if (!value) return null
    switch (value) {
        case "ANNUAL_GIFT_BAG":
        case "年节礼包":
            return "ANNUAL_GIFT_BAG"
        case "MEAL_SUBSIDY":
        case "餐补":
            return "MEAL_SUBSIDY"
        case "CONDOLENCE_GIFT":
        case "慰问品":
            return "CONDOLENCE_GIFT"
        case "CONSUMPTION_FUND":
        case "消费金":
            return "CONSUMPTION_FUND"
        case "OTHER":
        case "其他":
        case "其它":
            return "OTHER"
        default:
            return null
    }
}

export function percentToRate(percent: string): string {
    const n = Number(percent)
    if (!Number.isFinite(n)) return "0.000000"
    return (n / 100).toFixed(6)
}

export function rateToPercent(rate: string | undefined): string {
    const n = Number(rate)
    if (!Number.isFinite(n)) return "13.00"
    return (n * 100).toFixed(2)
}

export function mapCardFormFromBackend(
    code: string | null | undefined,
): string {
    return code === "PHYSICAL" ? "实体卡" : "电子卡"
}

export function dateToUnixSecs(dateStr: string): number {
    if (!dateStr) return Math.floor(Date.now() / 1000)
    // YYYY-MM-DD or datetime
    const normalized =
        dateStr.length === 10 ? `${dateStr}T00:00:00+08:00` : dateStr
    const ms = Date.parse(normalized)
    if (Number.isNaN(ms)) return Math.floor(Date.now() / 1000)
    return Math.floor(ms / 1000)
}

export function localOrderNo(): string {
    const d = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    const stamp = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`
    return `XS${stamp}`
}
