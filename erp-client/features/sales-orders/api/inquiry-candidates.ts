import { apiGet } from "@/lib/api"

/** 开单选择器只消费销售安全列表中的来源和可转资格。 */
export type InquirySalesCandidate = {
    id: string
    inquiry_no: string
    customer_id: string
    customer_name: string
    status: string
    acceptances: Array<{ feedback_round: number }>
    sales_progress: {
        can_create_sales: boolean
        transfer_status: string
        quote_validity: string
    }
}

export type InquiryCandidateQuery = {
    customer_id?: string
    q?: string
    page: number
    scope_version?: string
}

export function fetchInquiryCandidates(query: InquiryCandidateQuery) {
    return apiGet<{
        items: InquirySalesCandidate[]
        total: number
        page: number
        page_size: number
        scope_version: string
    }>("/admin/sales-inquiries", query)
}

export function inquiryCandidateBlocker(
    row: InquirySalesCandidate,
): string | null {
    if (row.status === "closed") return "已关闭，不能继续开单"
    if (!row.acceptances.length) return "尚未接受采购反馈"
    if (row.sales_progress.transfer_status === "complete")
        return "可转数量已用完（含销售草稿占用）"
    if (row.sales_progress.quote_validity === "expired")
        return "报价已过期，请重新询价"
    if (!row.sales_progress.can_create_sales)
        return "暂无可转结果，请在询价详情核对重询、报价及剩余数量"
    return null
}
