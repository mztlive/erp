/** 公司 SKU 的供应商供给合同。 */

import type { WorkItemProjection } from "@/features/work-items/types"

export type OfferingSourceType = "MANUAL" | "EXCEL" | "API"
export type OfferingStatus = "ACTIVE" | "PAUSED" | "STOPPED"
export type AvailabilityStatus =
    | "AVAILABLE"
    | "UNAVAILABLE"
    | "STOPPED"
    | "STALE"

export type SupplierOfferingView = Readonly<{
    id: string
    sku_id: string
    sku_no?: string | null
    product_no?: string | null
    sku_name?: string | null
    specification?: string | null
    supplier_id: string
    supplier_no?: string | null
    supplier_name?: string | null
    supplier_product_code?: string | null
    supplier_sku_code: string
    source_type: OfferingSourceType
    source_connection_id?: string | null
    status: OfferingStatus
    current_revision_id?: string | null
    current_revision_no?: number | null
    dropship_supply_price_gross?: string | null
    dropship_supply_price_net?: string | null
    bulk_supply_price_gross?: string | null
    bulk_supply_price_net?: string | null
    input_tax_rate?: string | null
    bulk_minimum_order_quantity?: string | null
    supply_region: readonly string[]
    product_capabilities: readonly string[]
    dropship_express?: string | null
    freight_amount?: string | null
    service_fee_amount?: string | null
    valid_from?: string | null
    valid_to?: string | null
    availability_status?: AvailabilityStatus | null
    available_quantity?: string | null
    availability_source_updated_at?: number | null
    availability_version?: number | null
    version: number
    created_at: number
}>

export type SupplierOfferingPage = Readonly<{
    items: readonly SupplierOfferingView[]
    total: number
    page: number
    page_size: number
}>

export type SupplierOfferingListQuery = Readonly<{
    q?: string
    skuId?: string
    skuNo?: string
    productNo?: string
    supplierId?: string
    status?: OfferingStatus
    sourceType?: OfferingSourceType
    availabilityStatus?: AvailabilityStatus
    page?: number
    pageSize?: number
}>

/** W22 安全暂停在 W21 的唯一已注册任务投影。 */
export type SupplierSupplyExceptionWorkItem = WorkItemProjection &
    Readonly<{
        workItemType: "BUSINESS_EXCEPTION"
        handlerKey: "supplier_supply_exception"
        destinationWorkspaceId: "W21"
        status: "OPEN"
        businessObjectType: "SUPPLIER_OFFERING"
    }>

export type CompleteSupplierSupplyExceptionTaskInput = Readonly<{
    offeringId: string
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    evidenceReference: string
    comment: string
    idempotencyKey: string
}>

export type CompleteSupplierSupplyExceptionTaskResult = Readonly<{
    work_item_id: string
    safety_pause_operation_id: string
    evidence_reference: string
    message: string
}>

type SupplierOfferingTermsInput = Readonly<{
    dropship_supply_price_gross: string
    bulk_supply_price_gross: string
    input_tax_rate: string
    bulk_minimum_order_quantity: string
    supply_region: readonly string[]
    product_capabilities: readonly string[]
    valid_from: string
    valid_to?: string | null
    dropship_express?: string | null
    freight_amount?: string | null
    service_fee_amount?: string | null
}>

export type CreateSupplierOfferingInput = Readonly<{
    sku_id: string
    supplier_id: string
    supplier_product_code?: string | null
    supplier_sku_code: string
    source_type: OfferingSourceType
    source_connection_id?: string | null
    terms: SupplierOfferingTermsInput
    availability_status: AvailabilityStatus
    available_quantity?: string | null
    change_reason: string
    idempotency_key: string
}>

export type ReviseSupplierOfferingInput = Readonly<{
    offeringId: string
    expected_revision_no: number
    terms: SupplierOfferingTermsInput
    status?: OfferingStatus
    change_reason: string
    idempotency_key: string
}>

export type UpdateOfferingAvailabilityInput = Readonly<{
    offeringId: string
    expected_version?: number
    availability_status: AvailabilityStatus
    available_quantity?: string | null
    change_reason: string
    idempotency_key: string
}>

/** 公司商品详情向新增供给弹窗提供的固定 SKU。 */
export type FixedSku = Readonly<{
    skuId: string
    skuCode: string
    skuName: string
    specification: string
    baseUnit: string
    productKind?: string
    category?: string
    brand?: string
    barcode?: string
    description?: string
    carouselImages?: readonly string[]
    detailImages?: readonly string[]
    mainImage?: string
    carouselFileAssetIds?: Readonly<Record<string, string>>
    detailFileAssetIds?: Readonly<Record<string, string>>
    carouselPreviewUrls?: Readonly<Record<string, string>>
    detailPreviewUrls?: Readonly<Record<string, string>>
    mainImageAssetId?: string
    mainImagePreviewUrl?: string
}>

export const OFFERING_STATUS_LABELS: Readonly<Record<OfferingStatus, string>> =
    {
        ACTIVE: "启用",
        PAUSED: "暂停",
        STOPPED: "停止",
    }

export const AVAILABILITY_STATUS_LABELS: Readonly<
    Record<AvailabilityStatus, string>
> = {
    AVAILABLE: "可供",
    UNAVAILABLE: "不可供",
    STOPPED: "停止供应",
    STALE: "数据已过期",
}

export const SOURCE_TYPE_LABELS: Readonly<Record<OfferingSourceType, string>> =
    {
        MANUAL: "手工",
        EXCEL: "Excel",
        API: "API",
    }
