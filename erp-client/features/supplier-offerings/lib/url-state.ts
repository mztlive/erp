import { createUrlStateCodec } from "@/lib/url-state"

import type {
    AvailabilityStatus,
    OfferingSourceType,
    OfferingStatus,
} from "@/features/supplier-offerings/types"

export type SupplierOfferingsUrlState = Readonly<{
    q?: string
    skuId?: string
    skuNo?: string
    productNo?: string
    supplierId?: string
    status?: OfferingStatus
    sourceType?: OfferingSourceType
    availabilityStatus?: AvailabilityStatus
    page: number
    returnTo?: string
    workItemId?: string
    queueContextId?: string
    from?: "W02"
}>

const codec = createUrlStateCodec<SupplierOfferingsUrlState>([
    { key: "q", type: "string", trim: true },
    { key: "skuId", type: "string", trim: true },
    { key: "sku_no", name: "skuNo", type: "string", trim: true },
    { key: "product_no", name: "productNo", type: "string", trim: true },
    { key: "supplierId", type: "string", trim: true },
    {
        key: "status",
        type: "enum",
        values: ["ACTIVE", "PAUSED", "STOPPED"],
    },
    {
        key: "sourceType",
        type: "enum",
        values: ["MANUAL", "EXCEL", "API"],
    },
    {
        key: "availabilityStatus",
        type: "enum",
        values: ["AVAILABLE", "UNAVAILABLE", "STOPPED", "STALE"],
    },
    { key: "page", type: "number", defaultValue: 1, min: 1 },
    { key: "returnTo", type: "string" },
    { key: "workItemId", type: "string", trim: true },
    { key: "queueContextId", type: "string", trim: true },
    { key: "from", type: "enum", values: ["W02"] },
])

/** 从只读查询参数解析供应商供给列表状态。 */
export const parseSupplierOfferingsSearchParams = (
    searchParams: URLSearchParams | { get(name: string): string | null },
): SupplierOfferingsUrlState => codec.parse(searchParams)

/** 构建最小化的供应商供给列表查询字符串。 */
export const buildSupplierOfferingsSearchParams = (
    state: SupplierOfferingsUrlState,
): string => codec.build(state)
