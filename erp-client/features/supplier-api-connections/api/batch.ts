import { apiGet, apiPost, apiPut } from "@/lib/api"
import type { SupplierOfferingTermsInput } from "@/features/supplier-offerings/types"
import type {
    AdoptionMapping,
    AdoptionResolution,
    AdoptionResultRow,
} from "./adoption"
import type { CatalogFilters, CatalogItem } from "./catalog"

export type PriceRule =
    | { method: "UNSET" }
    | { method: "PERCENTAGE" | "ADD" | "FIXED"; value: string }
export type BatchPricing =
    | { mode: "DROPSHIP_ONLY"; dropship: PriceRule }
    | { mode: "SAME"; rule: PriceRule; minimum_quantity: string | null }
    | {
          mode: "SEPARATE"
          dropship: PriceRule
          bulk: PriceRule
          minimum_quantity: string | null
      }
export type FillPolicy = { overwrite: boolean; preserve_manual: boolean }
export type BatchValue = {
    value: string | null
    origin: "UNSET" | "SOURCE" | "RULE" | "MANUAL" | "CLEARED" | "PRESERVED"
}
export type BatchSalesValues = {
    dropship: BatchValue
    bulk: BatchValue
    minimum_quantity: BatchValue
    market_price: BatchValue
    bulk_linked: boolean
}
export type BatchFieldEdit =
    | { action: "KEEP" }
    | { action: "SET"; value: string }
    | { action: "CLEAR" }
export type BatchSalesEdit = Partial<
    Record<
        "dropship" | "bulk" | "minimum_quantity" | "market_price",
        BatchFieldEdit
    >
>
export type BatchStep = "PRICING" | "DETAILS" | "REVIEW" | "RESULT" | "RECOVERY"
export type BatchRowStatus =
    | "DRAFT"
    | "READY"
    | "PROCESSING"
    | "SUCCEEDED"
    | "FAILED"
    | "UNKNOWN"
    | "SKIPPED"
    | "EXCLUDED"
export type BatchDefaults = {
    category_id?: string | null
    category_mappings?: Array<{
        source_category_id: string | null
        category_id: string
    }>
    input_tax_rate?: string | null
    valid_from?: string | null
    valid_to?: string | null
    delivery_template_id?: string | null
    clear_fields?: string[]
}
export type BatchItemEdit = {
    item_id: string
    mapping?: AdoptionMapping
    terms?: SupplierOfferingTermsInput
    sales_edit?: BatchSalesEdit
    delivery_template_id?: BatchFieldEdit
    restore_pricing?: boolean
    warnings_confirmed?: boolean
}
export type BatchRow = {
    id: string
    version: number
    source: CatalogItem
    suggested_unit: string | null
    unit_inferred: boolean
    previous_source?: CatalogItem | null
    mapping: AdoptionMapping
    terms: SupplierOfferingTermsInput | null
    sales_prices: BatchSalesValues
    delivery_template_id: string | null
    status: BatchRowStatus
    messages: string[]
    warnings: string[]
    warnings_confirmed: boolean
    resolution: AdoptionResolution | null
    result: Omit<AdoptionResultRow, "status"> | null
    operation_id?: string | null
    preview_version: number | null
}
export type BatchCounts = {
    total: number
    draft: number
    ready: number
    processing: number
    succeeded: number
    failed: number
    unknown: number
    skipped: number
    excluded: number
}
export type AdoptionBatch = {
    id: string
    version: number
    batch_no: string
    connection_id: string
    supplier_id: string
    supplier_name?: string
    draft_operation?: string | null
    draft_started_at?: number | null
    selection_complete?: boolean
    execution_pending?: boolean
    confirmations: Array<{ operation_id: string }>
    published_operations: string[]
    submitted: boolean
    step: BatchStep
    rules: BatchPricing
    inactive_pricing?: BatchPricing | null
    fill_policy: FillPolicy
    defaults: BatchDefaults
    excluded_ids: string[]
    preview_version: number | null
    selected_count: number
}
export type BatchView = {
    batch: AdoptionBatch
    rows: BatchRow[]
    counts: BatchCounts
    product_count: number
    manual_count: number
}
export type CreateBatchRequest = {
    item_ids?: string[]
    filter?: CatalogFilters
    excluded_ids?: string[]
}
export type SaveBatchRequest = {
    expected_version: number
    rules?: BatchPricing
    inactive_pricing?: BatchPricing | null
    fill_policy?: FillPolicy
    defaults?: BatchDefaults
    step?: BatchStep
    excluded_ids?: string[]
    items?: BatchItemEdit[]
}
export type BatchIdentity = { connectionId: string; batchId: string }
const base = (connectionId: string) =>
    `/admin/supplier-api-connections/${encodeURIComponent(connectionId)}/adoption-batches`
const path = ({ connectionId, batchId }: BatchIdentity) =>
    `${base(connectionId)}/${encodeURIComponent(batchId)}`
export function fetchAdoptionBatches(connectionId: string) {
    return apiGet<AdoptionBatch[]>(base(connectionId))
}
export function createAdoptionBatch({
    connectionId,
    request,
}: {
    connectionId: string
    request: CreateBatchRequest
}) {
    return apiPost<BatchView>(base(connectionId), request, {
        timeoutMs: 300000,
    })
}
export function fetchAdoptionBatch(identity: BatchIdentity) {
    return apiGet<BatchView>(path(identity))
}
export function saveAdoptionBatch({
    request,
    ...identity
}: BatchIdentity & { request: SaveBatchRequest }) {
    return apiPut<BatchView>(path(identity), request, { timeoutMs: 300000 })
}
export function previewAdoptionBatch({
    expected_version,
    item_ids,
    ...identity
}: BatchIdentity & { expected_version: number; item_ids?: string[] }) {
    return apiPost<BatchView>(
        `${path(identity)}/preview`,
        {
            expected_version,
            item_ids,
        },
        { timeoutMs: 300000 },
    )
}
export function submitAdoptionBatch({
    request,
    ...identity
}: BatchIdentity & {
    request: {
        expected_version: number
        preview_version: number
        item_ids: string[]
        idempotency_key: string
    }
}) {
    return apiPost<BatchView>(`${path(identity)}/submit`, request, {
        timeoutMs: 300000,
    })
}
export function reconcileAdoptionBatch({
    expected_version,
    ...identity
}: BatchIdentity & { expected_version: number }) {
    void expected_version
    return apiPost<BatchView>(`${path(identity)}/reconcile`, {})
}

export function queryAdoptionBatchSubmission({
    request,
    ...identity
}: Parameters<typeof submitAdoptionBatch>[0]) {
    return apiPost<{
        status: "ACCEPTED" | "NOT_ACCEPTED"
        view: BatchView
    }>(`${path(identity)}/submission-result`, request)
}
