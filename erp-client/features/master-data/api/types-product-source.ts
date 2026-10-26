/** 公司展示资料的供应商来源差异；不包含供应商成本。 */
export type ProductSourceField =
    | "NAME"
    | "DESCRIPTION"
    | "SKU_NAME"
    | "MAIN_IMAGE"
    | "CAROUSEL_MEDIA"
    | "DETAIL_MEDIA"

export type ProductSourceImage = Readonly<{
    url: string | null
    file_asset_id: string | null
    content_fingerprint: string | null
}>

export type ProductSourceValue =
    | Readonly<{ type: "TEXT"; value: string | null }>
    | Readonly<{ type: "IMAGES"; value: readonly ProductSourceImage[] }>

export type ProductSourceDifference = Readonly<{
    field: ProductSourceField
    label: string
    sku_id: string | null
    sku_no: string | null
    company: ProductSourceValue
    supplier: ProductSourceValue
    baseline: ProductSourceValue
    manually_maintained: boolean
    verification_required: boolean
    application_blocker?: string | null
}>

export type ProductSourceDiffView = Readonly<{
    product_id: string
    product_version: number
    source_version: number
    sku_versions: Readonly<Record<string, number>>
    source: Readonly<{
        supplier_id: string
        supplier_name: string | null
        connection_id: string
        catalog_item_id: string
    }>
    observed_at: number
    fields: readonly ProductSourceDifference[]
    pending_count: number
}>

export type ProductSourceDecision = Readonly<{
    field: ProductSourceField
    sku_id?: string
    action: "ADOPT" | "KEEP" | "EDIT"
    value?: ProductSourceValue
}>

export type ResolveProductSourceDiff = Readonly<{
    idempotency_key: string
    expected_product_version: number
    expected_source_version: number
    expected_sku_versions: Readonly<Record<string, number>>
    decisions: readonly ProductSourceDecision[]
}>

export const PRODUCT_SOURCE_FIELD_LABELS: Record<ProductSourceField, string> = {
    NAME: "商品名称",
    DESCRIPTION: "商品介绍",
    SKU_NAME: "规格商品名称",
    MAIN_IMAGE: "SKU 主图",
    CAROUSEL_MEDIA: "商品轮播图",
    DETAIL_MEDIA: "商品详情图",
}
