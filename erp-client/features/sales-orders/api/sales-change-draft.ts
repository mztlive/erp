import type { SalesDeliveryAllocation } from "../types"
import { apiGet, apiPost } from "@/lib/api/client"

/** 原销售变更目标行的完整字段组，冻结关联随原单保留。 */
export type SalesChangeDraftLine = {
    sales_order_line_id: string
    line_no: number
    line_type: "GOODS_SERVICE" | "VOUCHER"
    sales_tax_rate: string
    item_name_snapshot: string
    spec_snapshot: string | null
    unit_snapshot: string | null
    goods: null | {
        sku_id: string
        sku_revision_id: string
        base_unit_code: string
        fulfillment_due_at: number
        service_region: string | null
        welfare_scenario: string | null
        quantity: string
        unit_price_gross: string
        delivery_allocations?: SalesDeliveryAllocation[]
        pricing_mode?: "AUTO" | "MANUAL" | "GIFT"
        [field: string]: unknown
    }
    voucher: null | {
        face_value: string
        card_count: number | string
        unit_price_gross: string
        face_value_total: string
        transaction_amount: string
        gift_amount: string
        gift_rate: string | null
        [field: string]: unknown
    }
}

export type SalesChangeTerms = {
    project_name: string
    payment_term_code: string
    payment_term_name: string
    invoice_type: string
    tax_point: string
}

export type SalesChangeDraft = {
    base_gross_amount: string
    base_lines: SalesChangeDraftLine[]
    terms: SalesChangeTerms
    next_line_no: number
    version: number
    working_copy_version: number
    content_hash: string
    reason: string
    business_remark: string | null
    lines: SalesChangeDraftLine[]
}

export type SaveSalesChangeDraft = {
    terms: SalesChangeTerms
    expected_version: number
    expected_working_copy_version: number
    reason: string
    business_remark: string | null
    lines: SalesChangeDraftLine[]
}

/** 读取同一销售变更单可编辑目标，不创建新的变更单。 */
export async function fetchSalesChangeDraft(
    id: string,
): Promise<SalesChangeDraft> {
    const draft = await apiGet<SalesChangeDraft>(
        `/admin/sales-change-orders/${encodeURIComponent(id)}/draft`,
    )
    if (
        !draft.terms ||
        !Array.isArray(draft.base_lines) ||
        !Number.isSafeInteger(draft.next_line_no) ||
        !draft.base_gross_amount
    ) {
        throw new Error("改单服务尚未更新，请更新后端服务后重新打开。")
    }
    return draft
}

/** 保存完整目标并返回供再次提交使用的两个新版本。 */
export function saveSalesChangeDraft(
    id: string,
    request: SaveSalesChangeDraft,
): Promise<SalesChangeDraft> {
    return apiPost(
        `/admin/sales-change-orders/${encodeURIComponent(id)}/draft`,
        request,
    )
}

/** 以完整目标身份确认原单已有相应不可变提交，避免未知结果后重复提交。 */
export async function salesChangeSubmissionMatches(
    id: string,
    version: number,
    contentHash: string,
): Promise<boolean> {
    const current = await apiGet<{
        id: string
        version: number
        current_submission_id: string | null
        submitted_working_copy_content_hash: string | null
        status: string
    }>(`/admin/sales-change-orders/${encodeURIComponent(id)}`)
    return (
        current.id === id &&
        current.version > version &&
        Boolean(current.current_submission_id) &&
        (current.status === "IN_APPROVAL" || current.status === "EFFECTIVE") &&
        current.submitted_working_copy_content_hash === contentHash
    )
}
