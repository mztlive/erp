import type {
    PurchaseOrderListItem,
    PurchaseOrderMetricFilter,
    PurchaseOrderStatusFilter,
} from "@/features/purchase-orders/types"

export type PurchaseOrderListQuery = {
    scopeVersion?: string
    ownerUserIds?: string
    q?: string
    salesOrderId?: string
    status?: PurchaseOrderStatusFilter
    metric?: PurchaseOrderMetricFilter
    page?: number
    pageSize?: number
    sortBy?: string
    sortDir?: "asc" | "desc"
}

export type PurchaseOrderListResult = {
    emptyReason?: string | null
    scopeVersion?: string
    policyVersion?: number
    organizationVersion?: number
    scopeSummary?: string
    asOf?: string
    ownershipBasis?: string
    rows: PurchaseOrderListItem[]
    total: number
    page: number
    pageSize: number
    metrics: Array<{
        key: string
        label: string
        count: number
        detail: string
    }>
    freshness: { updatedAt: string; state: "fresh" }
}
