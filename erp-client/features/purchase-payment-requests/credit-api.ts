import { apiGet, apiPost } from "@/lib/api"
export type SupplierCredit = {
    id: string
    source_purchase_order_id: string
    original_payment_id: string
    supplier_id: string
    payer_party_id: string
    amount: string
    used_amount: string
}
export type CreditUseInput = {
    requestId: string
    work_item_id: string
    task_version: string
    expected_version: number
    credit_id: string
    payable_account_id: string
    payable_entry_id: string
    amount: string
    idempotency_key: string
}
export function fetchSupplierCredits(id: string) {
    return apiGet<SupplierCredit[]>(
        `/admin/purchase-payment-requests/${encodeURIComponent(id)}/supplier-credits`,
    )
}
export function useSupplierCredit({ requestId, ...body }: CreditUseInput) {
    return apiPost<string>(
        `/admin/purchase-payment-requests/${encodeURIComponent(requestId)}/supplier-credits/use`,
        body,
    )
}
