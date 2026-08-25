/**
 * W22 商品发布 · 后端接口 wire 类型（仅 HTTP 适配层内部使用）。
 */

export type BackendPublication = {
    id: string
    sku_id: string
    target_mall_id: string
    status: string
    current_revision_id?: string | null
    version: number
    created_at: number
}

export type BackendRevision = {
    id: string
    product_publication_id: string
    revision_no: number
    name: string
    sale_status: string
    sales_price_gross: string
    valid_from: number
    valid_to?: number | null
    version: number
    created_at: number
}

export type BackendRevisionCommit = {
    revision: BackendRevision
    delivery_id: string
    delivery_status: string
    operation_id: string
}

export type BackendDelivery = {
    id: string
    publication_revision_id: string
    target_mall_id: string
    delivery_status: string
    attempt_count: number
    mall_version?: string | null
    error_code?: string | null
    version: number
    created_at: number
}

export type BackendDeliveryResult = {
    delivery_id: string
    delivery_status: string
    inbox_message_id: string
    error_task_id?: string | null
    mall_version?: string | null
    publication_version: number
}

export type BackendRetryDeliveryResult = {
    delivery_id: string
    attempt_count: number
    delivery_status: string
    operation_id: string
}

export type BackendMedia = {
    id: string
    product_publication_revision_id: string
    file_asset_id: string
    media_role: string
    sort_no: number
    alt_text?: string | null
}

export type SourceSystem = {
    id: string
    code: string
    name: string
    system_type?: string
    status?: string
}
