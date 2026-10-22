import type { SalesInquiry } from "../types"

/** 默认展示当前需求；只有当前需求已有最终反馈时才自动带入报价。 */
export function salesInquiryFeedback(
    inquiry: SalesInquiry,
    chosenRound: number | null,
) {
    if (["draft", "awaiting_sales_supplement"].includes(inquiry.status))
        return undefined
    if (chosenRound !== null)
        return inquiry.feedbacks.find((item) => item.round === chosenRound)
    if (
        ["awaiting_procurement", "procurement_processing"].includes(
            inquiry.status,
        )
    )
        return undefined
    return inquiry.feedbacks.find(
        (item) =>
            item.round === inquiry.feedback_round &&
            item.requirement_version === inquiry.requirement_version,
    )
}
