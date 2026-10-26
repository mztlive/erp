import { apiGet, apiPost } from "@/lib/api"

export type CatalogIdentityReview = {
    item_id: string
    item_version: number
    source_specification: string
    source_unit: string | null
    company_sku_id: string
    company_sku_no: string
    company_specification: string
    company_unit_id: string
    company_unit_code: string
    company_unit_name: string
    sku_revision_id: string
    offering_version: number
    requires_identity_review: boolean
}

export type ConfirmCatalogIdentity = {
    expected_version: number
    expected_sku_revision_id: string
    expected_offering_version: number
    source_specification: string
    source_unit: string | null
    confirmed_base_unit_id: string
    confirm_same_identity: boolean
    reason: string
    idempotency_key: string
}

const endpoint = (connection: string, item: string) =>
    `/admin/supplier-api-connections/${encodeURIComponent(connection)}/catalog/${encodeURIComponent(item)}/identity-review`

export function fetchCatalogIdentityReview(
    connectionId: string,
    itemId: string,
) {
    return apiGet<CatalogIdentityReview>(endpoint(connectionId, itemId))
}

export function confirmCatalogIdentityReview(input: {
    connectionId: string
    itemId: string
    request: ConfirmCatalogIdentity
}) {
    return apiPost<CatalogIdentityReview>(
        endpoint(input.connectionId, input.itemId),
        input.request,
    )
}
