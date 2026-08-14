import type {
    PurchaseOrderListItem,
    PurchaseOrderMetricFilter,
    PurchaseOrderStatusFilter,
} from "@/features/purchase-orders/types"

export type PurchaseOrderListQuery = {
    q?: string
    status?: PurchaseOrderStatusFilter
    metric?: PurchaseOrderMetricFilter
    page?: number
    pageSize?: number
    sortBy?: string
    sortDir?: "asc" | "desc"
}

export type PurchaseOrderListResult = {
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
