import { apiGet, apiPost } from "@/lib/api"

export type PurchasePaymentArrangement = {
    state: "inactive" | "available" | "blocked" | "pending" | "settled"
    payment_blocked: boolean
    available_amount: string
    reserved_amount: string
    paid_amount: string
    payment_term_label: string
    prepayment_gap: string | null
    task_id: string | null
    task_version: number | null
    follow_up_at: number | null
    can_schedule: boolean
    requests: {
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
    }[]
}
export type SchedulePaymentFollowUp = {
    task_id: string
    expected_version: number
    follow_up_date: string
    idempotency_key: string
}
export function fetchPaymentArrangement(id: string) {
    return apiGet<PurchasePaymentArrangement>(
        `/admin/purchase-orders/${encodeURIComponent(id)}/payment-arrangement`,
    )
}
export function schedulePaymentFollowUp(
    id: string,
    data: SchedulePaymentFollowUp,
) {
    return apiPost<void>(
        `/admin/purchase-orders/${encodeURIComponent(id)}/payment-follow-up`,
        data,
    )
}
