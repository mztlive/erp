import type { ProcurementInquiry, SalesInquiry } from "../types"

/** 查看历史轮次不得改变费用办理依据。 */
export function inquiryFeeBasisRound(
    inquiry: SalesInquiry | ProcurementInquiry,
) {
    const acceptedRound =
        "acceptances" in inquiry
            ? inquiry.acceptances.at(-1)?.feedback_round
            : undefined
    return (
        acceptedRound ??
        inquiry.fee_statuses[0]?.feedback_round ??
        inquiry.feedback_round
    )
}
