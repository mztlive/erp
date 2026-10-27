import { apiPost } from "@/lib/api"
import type { SupplierOfferingTermsInput } from "@/features/supplier-offerings/types"
import type { ProductKind } from "@/features/master-data/types"
import type { CatalogFilters, CatalogItem } from "./catalog"

export type AdoptionSelection =
    | { item_ids: string[]; filter?: never }
    | { filter: CatalogFilters; item_ids?: never }

export type AdoptionPreview = {
    preview_id: string
    version: number
    rows: Array<{
        item: CatalogItem
        adopted: boolean
        blocked: boolean
        blocking_reasons: string[]
        missing_fields: string[]
    }>
    total: number
    max_confirm_rows: number
}

export type AdoptionMapping = {
    category_id: string
    unit_id: string
    product_kind: ProductKind
    company_name?: string
    company_description?: string
}
export type AdoptionResolution = {
    kind: "EXISTING" | "CREATE_SKU" | "CREATE_PRODUCT"
    expected_match: string
    sku_no: string | null
    product_id: string | null
    reason: string
}
export type AdoptionRowInput = {
    item_id: string
    target: { kind: "AUTO"; mapping: AdoptionMapping; expected_match: string }
    terms: SupplierOfferingTermsInput
    delivery_template_id?: string | null
}

export type AdoptionResultRow = {
    item_id: string
    status: "SUCCEEDED" | "FAILED"
    retry_original?: boolean
    product_id?: string | null
    sku_id?: string | null
    offering_id?: string | null
    message?: string | null
}

export function previewAdoption(input: {
    connectionId: string
    selection: AdoptionSelection
}) {
    return apiPost<AdoptionPreview>(
        `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}/catalog/adoptions/preview`,
        input.selection,
    )
}

export function confirmAdoption(input: {
    connectionId: string
    previewId: string
    expected_version: number
    idempotency_key: string
    rows: AdoptionRowInput[]
}) {
    const { connectionId, previewId, ...body } = input
    return apiPost<{ rows: AdoptionResultRow[] }>(
        `/admin/supplier-api-connections/${encodeURIComponent(connectionId)}/catalog/adoptions/${encodeURIComponent(previewId)}/confirm`,
        body,
    )
}

export function resolveAdoption(
    connectionId: string,
    previewId: string,
    itemId: string,
    mapping: AdoptionMapping,
) {
    return apiPost<AdoptionResolution>(
        `/admin/supplier-api-connections/${encodeURIComponent(connectionId)}/catalog/adoptions/${encodeURIComponent(previewId)}/resolve`,
        { item_id: itemId, mapping },
    )
}
