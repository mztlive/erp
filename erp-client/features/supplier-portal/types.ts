/** 供应商门户只允许外部展示的字段；共享商品资料不含公司销售价格。 */
export type PortalRole = "maintainer" | "read_only"
export type PortalProfile = {
    account_id: string
    account: string
    name: string
    supplier_id: string
    supplier_name?: string
    role: PortalRole
    account_version?: number
    binding_version?: number
}
export type PortalPage<T> = {
    items: T[]
    total: number
    page: number
    page_size: number
}
export type PortalTerms = {
    dropship_supply_price_gross: string
    bulk_supply_price_gross: string
    input_tax_rate: string
    bulk_minimum_order_quantity: string
    supply_region: string[]
    product_capabilities: string[]
    valid_from: string
    valid_to: string | null
    dropship_express: string | null
    freight_amount: string | null
    service_fee_amount: string | null
}
export type PortalOffering = {
    id: string
    sku_id: string
    sku_no?: string
    name?: string
    sku_name?: string
    specification?: string
    base_unit?: string
    unit_name?: string
    writable?: boolean
    supplier_sku_code: string
    source_type: "MANUAL" | "EXCEL" | "API"
    status: "ACTIVE" | "PAUSED" | "STOPPED"
    version: number
    current_revision_no: number
    terms?: PortalTerms
    dropship_supply_price_gross?: string
    bulk_supply_price_gross?: string
    input_tax_rate?: string
    bulk_minimum_order_quantity?: string
    supply_region?: string[]
    product_capabilities?: string[]
    valid_from?: string
    valid_to?: string | null
    freight_amount?: string | null
    service_fee_amount?: string | null
    dropship_express?: string | null
    availability_status: string
    available_quantity: string | null
    availability_version: number
    availability_source_updated_at?: number | string
    image_asset_id?: string | null
    pending_applications?: PortalApplication[]
}
export type PortalQuoteTargetVersion = {
    sku_version: number
    sku_revision_id: string
    sku_revision_version: number
    product_id: string
    product_version: number
    product_revision_id: string
    product_revision_version: number
    unit_id: string
    unit_version: number
}
export type PortalCatalogSku = {
    target_version: PortalQuoteTargetVersion
    id: string
    sku_no: string
    name: string
    specification: string
    unit_id: string
    unit_name: string
    unit_precision: number
    version: number
    product_id: string
    listing_status: string
    product_kind?: string
    image_asset_id?: string
    offering_id?: string | null
    own_offering_id?: string | null
    own_offering?: PortalOffering | null
}
export type PortalCategoryPathNode = {
    id: string
    version: number
    name: string
    parent_id: string | null
    product_kind: string
}
export type PortalDictionary = {
    hierarchy?: PortalCategoryPathNode[]
    id: string
    name: string
    version: number
    path?: string
    product_kind?: string
    code?: string
    parent_id?: string | null
    quantity_scale?: number
}
export type RawDictionary = {
    raw_name: string
    selected_id?: string | null
    expected_version?: number | null
}
export type NewProductInput = {
    name: string
    product_kind: string
    brand: RawDictionary
    category: RawDictionary
    model?: string
    description?: string
    image_asset_ids: string[]
    file_asset_ids: string[]
    skus: {
        row_id: string
        name: string
        spec_entries: { attribute_code: string; attribute_value_code: string }[]
        unit: RawDictionary
        barcode?: string
        image_asset_id?: string
        ordering_code: string
        supply_terms: PortalTerms
        available_quantity: string | null
        reported_at: number
        packaging?: {
            original_unit: string
            base_unit: string
            units_per_package: string
            original_unit_price: string
            conversion_confirmed_by_supplier: boolean
        }
        quote_basis?: string
    }[]
}
export type ApplicationKind =
    | "quote"
    | "terms"
    | "stop"
    | "cooperation"
    | "new_product"
export type ApplicationStatus =
    | "draft"
    | "pending"
    | "returned"
    | "withdrawn"
    | "effective"
export type PortalApplication = {
    id: string
    application_no?: string
    kind: ApplicationKind
    status: ApplicationStatus
    version: number
    supplier_id?: string
    supplier_name?: string
    title?: string
    reason?: string
    input: Record<string, unknown>
    submitted_snapshot?: Record<string, unknown>
    submissions?: {
        id?: string
        submission_no?: number
        submitted_at?: number | string
        snapshot?: Record<string, unknown>
        input?: Record<string, unknown>
    }[]
    current?: Record<string, unknown>
    decisions?: {
        decision: string
        comment?: string
        actor_name?: string
        at?: number | string
    }[]
    result?: {
        product_id?: string
        product_no?: string
        product_created?: boolean
        sku_ids?: string[]
        offering_ids?: string[]
        offering_id?: string
        revision_no?: number
        operation?: string
        summary?: string
        new_skus_unlisted?: boolean
        skus?: {
            row_id: string
            sku_id: string
            sku_created: boolean
            listing_status: string
            offering_id?: string
        }[]
    }
    work_item_id?: string
    work_item_version?: number
    maintainer_user_id?: string
    existing_offerings?: {
        row_id: string
        offering_id: string
        expected_offering_version: number
        expected_revision_no: number
        sku_id?: string
        supplier_sku_code?: string
    }[]
    created_at?: number | string
    updated_at?: number | string
}
export type PortalCooperation = {
    supplier_name: string
    supplier_no?: string
    version: number
    supplier_version?: number
    profile_id?: string
    settlement_mode?: string
    reconciliation_cycle?: string
    payment_term_label?: string
    payment_term?: string
    contact_name?: string
    contact_phone?: string
}
export type PortalBatchMode = "quote" | "terms" | "new_product" | "availability"
export type PortalBatchPhase = "prepare" | "submit"
export type PortalBatchRow = {
    row_id: string
    idempotency_key: string
    input: Record<string, unknown>
}
export type PortalBatchResult = {
    valid?: boolean
    rows: {
        row_id: string
        status: string
        message?: string
        error?: string
        result?: unknown
        field_errors?: Record<string, string>
    }[]
}
export type PortalUpload = {
    id: string
    version: number
    file_name: string
    content_type: string
    byte_size: number
    request_version: number
    asset_kind: "IMAGE" | "DOCUMENT"
}

export type PortalCategoryMappingSuggestion = {
    mapping_id: string
    version: number
    original_category_path: string
    product_kind: string
    confirmed_category_id: string
    confirmed_category_version: number
    confirmed_category_path: string
    category: PortalDictionary | null
    status: "confirmation_required" | "recheck_required"
    requires_confirmation: true
}
