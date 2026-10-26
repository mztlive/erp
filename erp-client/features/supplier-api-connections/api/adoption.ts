import { apiGet, apiPost } from "@/lib/api"
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

export type AdoptionTarget = {
    sku_id: string
    sku_no: string
    product_id: string
    sku_version: number
    product_version: number
    unit_id: string
    specification: string
}

export type AdoptionRowInput = {
    item_id: string
    target:
        | {
              kind: "CREATE"
              category_id: string
              brand_id: string
              unit_id: string
              product_kind: ProductKind
              company_name?: string
              company_description?: string
              spec_entries: Array<{
                  attribute_code: string
                  attribute_value_code: string
              }>
              unit_confirmed: boolean
              specification_confirmed: boolean
          }
        | {
              kind: "EXISTING"
              sku_id: string
              expected_sku_version: number
              expected_product_version: number
              unit_id: string
              specification_confirmed: boolean
          }
    terms: SupplierOfferingTermsInput
    delivery_template_id?: string | null
}

export type AdoptionResultRow = {
    item_id: string
    status: "SUCCEEDED" | "FAILED"
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

export function fetchAdoptionTarget(connectionId: string, skuId: string) {
    return apiGet<AdoptionTarget>(
        `/admin/supplier-api-connections/${encodeURIComponent(connectionId)}/catalog/adoption-targets/${encodeURIComponent(skuId)}`,
    )
}
