import { apiGet, apiGetBlob } from "@/lib/api"
import type { FinancialFile } from "@/features/file-assets/api/business-finance-files"

export type PurchasePayment = {
    payment_id: string
    payment_no: string
    paid_at: number
    allocated_amount: string
    receipt: FinancialFile | null
}

export function fetchPurchasePayments(purchaseOrderId: string) {
    return apiGet<PurchasePayment[]>(
        `/admin/purchase-orders/${encodeURIComponent(purchaseOrderId)}/payments`,
    )
}

export function fetchPurchasePaymentReceipt(
    purchaseOrderId: string,
    paymentId: string,
) {
    return apiGetBlob(
        `/admin/purchase-orders/${encodeURIComponent(purchaseOrderId)}/payments/${encodeURIComponent(paymentId)}/receipt`,
        { timeoutMs: 30_000, cache: "no-store" },
    )
}

export async function downloadPurchasePaymentReceipt(
    purchaseOrderId: string,
    file: FinancialFile,
) {
    const blob = await fetchPurchasePaymentReceipt(
        purchaseOrderId,
        file.document_id,
    )
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = file.file_name
    document.body.append(anchor)
    anchor.click()
    anchor.remove()
    URL.revokeObjectURL(url)
}
