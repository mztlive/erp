import { apiGet } from "@/lib/api"
export type SettlementReviewerOption = {
    user_id: string
    display_name: string
    account: string
}
export function fetchSettlementReviewerOptions(statementId: string) {
    return apiGet<SettlementReviewerOption[]>(
        `/admin/supplier-settlement-statements/${encodeURIComponent(statementId)}/reviewer-options`,
    )
}
