import { apiGet } from "@/lib/api"

export type FulfillmentPaymentRequest = {
    request_id: string
    request_no: string
    status:
        | "draft"
        | "in_approval"
        | "approved"
        | "partially_paid"
        | "paid"
        | "rejected"
        | "withdrawn"
        | "closed"
    requested_amount: string
    paid_amount: string
    remaining_amount: string
    closed_amount: string
    desired_payment_date: string
}

export type FulfillmentPayment = {
    gross_amount: string
    paid_amount: string
    open_amount: string
    payment_term_label: string
    requests: FulfillmentPaymentRequest[]
}

export function fetchFulfillmentPayment(workItemId: string) {
    return apiGet<FulfillmentPayment>(
        `/admin/work-items/${encodeURIComponent(workItemId)}/purchase-payment`,
        { cache: "no-store" },
    )
}
