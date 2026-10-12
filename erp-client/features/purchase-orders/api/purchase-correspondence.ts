import { apiGet, apiPost } from "@/lib/api"

export type PurchasePaper = { filename: string; html: string }
export type PurchaseEmailResult = {
    status: "SENDING" | "ACCEPTED" | "FAILED" | "UNKNOWN"
    message_id?: string
}
export const fetchPurchasePaper = (id: string) =>
    apiGet<PurchasePaper>(
        `/admin/purchase-orders/${encodeURIComponent(id)}/paper`,
    )
export const sendPurchaseEmail = (
    id: string,
    request: {
        expected_version: number
        recipient: string
        idempotency_key: string
    },
) =>
    apiPost<PurchaseEmailResult>(
        `/admin/purchase-orders/${encodeURIComponent(id)}/email`,
        request,
    )
export type PurchaseEmailHistory = {
    id: string
    recipient: string
    status: PurchaseEmailResult["status"]
    created_at: number
}
export const fetchPurchaseEmails = (id: string) =>
    apiGet<PurchaseEmailHistory[]>(
        `/admin/purchase-orders/${encodeURIComponent(id)}/emails`,
    )
