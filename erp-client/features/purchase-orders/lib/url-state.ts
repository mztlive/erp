import { createUrlStateCodec } from "@/lib/url-state"
import type {
    PurchaseOrderMetricFilter,
    PurchaseOrderStatusFilter,
} from "@/features/purchase-orders/types"

export type PurchaseOrdersUrlState = {
    ownerUserIds?: string
    q?: string
    status: PurchaseOrderStatusFilter
    metric: PurchaseOrderMetricFilter
    page: number
    pageSize: number
    sort?: string
    basisId?: string
    salesOrderId?: string
    workItemId?: string
    action?: string
    mode?: string
}

const STATUS_VALUES: readonly PurchaseOrderStatusFilter[] = [
    "all",
    "DRAFT",
    "PENDING_REVIEW",
    "EFFECTIVE",
    "PARTIAL",
    "COMPLETED",
    "VOID",
]

const METRIC_VALUES: readonly PurchaseOrderMetricFilter[] = [
    "all",
    "pending_create",
    "draft",
    "review",
    "fulfill",
    "gate_blocked",
]

const codec = createUrlStateCodec<PurchaseOrdersUrlState>([
    { key: "ownerUserIds", type: "string" },
    { key: "q", type: "string", trim: true },
    { key: "status", type: "enum", values: STATUS_VALUES, defaultValue: "all" },
    { key: "metric", type: "enum", values: METRIC_VALUES, defaultValue: "all" },
    { key: "page", type: "number", defaultValue: 1, min: 1 },
    { key: "pageSize", type: "number", defaultValue: 20, min: 1, max: 100 },
    { key: "sort", type: "string" },
    { key: "basisId", type: "string" },
    { key: "salesOrderId", type: "string" },
    { key: "workItemId", type: "string" },
    { key: "action", type: "string" },
    { key: "mode", type: "string" },
])

/** 旧指标链接归一到主状态，避免指标覆盖 Tab 查询；无后端筛选的指标回到全部。 */
export const parsePurchaseOrdersSearchParams: typeof codec.parse = (params) => {
    const state = codec.parse(params)
    const legacyStatus: Partial<
        Record<PurchaseOrderMetricFilter, PurchaseOrderStatusFilter>
    > = {
        draft: "DRAFT",
        review: "PENDING_REVIEW",
        fulfill: "EFFECTIVE",
    }
    return {
        ...state,
        status: legacyStatus[state.metric] ?? state.status,
        metric: "all",
    }
}
export const buildPurchaseOrdersSearchParams = codec.build
