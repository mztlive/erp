import { apiGet, apiPost } from "@/lib/api"

export type CatalogImage = {
    url: string
    content_fingerprint: string | null
    file_asset_id?: string | null
}
export type CatalogAvailability =
    | "AVAILABLE"
    | "UNAVAILABLE"
    | "UNKNOWN"
    | "WITHDRAWN"
export type CatalogFilters = {
    keyword?: string
    category_id?: string
    brand_id?: string
    price_min?: string
    price_max?: string
    availability?: CatalogAvailability
    selection_status?: "ALL" | "UNSELECTED" | "SELECTED"
}
export type CatalogFilterOptions = {
    categories: Array<{ id: string; name: string }>
    brands: Array<{ id: string; name: string }>
}

export type CatalogItem = {
    id: string
    version: number
    product_id: string
    spec_id: string
    name: string
    specification: string
    unit: string | null
    purchase_price: string | null
    observed_at: number
    sku_id: string | null
    sku_no: string | null
    company_product_id: string | null
    offering_id: string | null
    category_id: string | null
    category_name: string | null
    brand_id: string | null
    brand_name: string | null
    description: string | null
    images: CatalogImage[] | null
    detail_images: CatalogImage[] | null
    availability: CatalogAvailability
    available_quantity: string | null
    quantity_unbounded: boolean
    requires_identity_review: boolean
    source_fingerprint: string
}
export type CatalogJob = {
    stalled: boolean
    job_id: string
    job_no: string
    status:
        | "pending"
        | "running"
        | "succeeded"
        | "partially_succeeded"
        | "failed"
        | "cancelled"
    observed_count: number
    skipped_count: number
    skipped_samples: string[]
    pages: number
    created_at: number
    finished_at: number | null
    error_summary: string | null
    sync_mode: "FULL" | "INCREMENTAL_PARTIAL" | "INCREMENTAL_COMPLETE"
}
export type CatalogResult = {
    items: CatalogItem[]
    total: number
    page: number
    page_size: number
    jobs: CatalogJob[]
    filters: CatalogFilterOptions
    sync_mode: "FULL" | "INCREMENTAL_PARTIAL" | "INCREMENTAL_COMPLETE"
    coverage: "PARTIAL" | "COMPLETE"
    incremental_supported: boolean
}
export const catalogRunning = (job?: CatalogJob) =>
    !job?.stalled &&
    (job?.status === "pending" ||
        job?.status === "running" ||
        job?.status === "partially_succeeded")
export const CATALOG_JOB_LABEL: Record<CatalogJob["status"], string> = {
    pending: "等待同步",
    running: "正在同步",
    succeeded: "同步完成",
    partially_succeeded: "正在同步",
    failed: "同步失败",
    cancelled: "已取消",
}
export function fetchCatalog(
    connectionId: string,
    page: number,
    filters: CatalogFilters = {},
) {
    return apiGet<CatalogResult>(
        `/admin/supplier-api-connections/${encodeURIComponent(connectionId)}/catalog`,
        { page, page_size: 20, ...filters },
    )
}
export function matchCatalog(input: {
    connectionId: string
    itemId: string
    skuId: string
    expectedVersion: number
}) {
    return apiPost<CatalogItem>(
        `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}/catalog/${encodeURIComponent(input.itemId)}/match`,
        {
            sku_id: input.skuId,
            expected_version: input.expectedVersion,
        },
    )
}
