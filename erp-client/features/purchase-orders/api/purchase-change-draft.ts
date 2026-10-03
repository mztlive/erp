import { apiGet } from "@/lib/api"

/** 与采购变更提交接口一致的完整行，关联与名称快照保持原值。 */
export type PurchaseChangeDraftLine = {
    line_type: "ITEM_SERVICE" | "LOGISTICS_FEE"
    procurement_confirmation_line_id: string | null
    sku_id: string | null
    sku_revision_id: string | null
    product_name: string | null
    specification: string | null
    quantity: string | null
    base_unit_code: string | null
    unit_cost_gross: string | null
    input_tax_rate: string | null
    expected_delivery_date: string | null
    sales_order_line_id: string | null
    sales_order_revision_line_id: string | null
    sales_order_submission_line_id: string | null
    allocated_quantity: string | null
    gross_amount: string | null
}

export type PurchaseChangeDraft = {
    version: number
    reason: string
    payment_term_code: string
    lines: PurchaseChangeDraftLine[]
    /** 冻结行或基准版本行主键，与 lines 同序，仅用于表单身份。 */
    line_keys: string[]
}

/** 按原变更单读取上次提交的完整目标，首次草稿由服务端恢复基准行。 */
export function fetchPurchaseChangeDraft(changeOrderId: string) {
    return apiGet<PurchaseChangeDraft>(
        `/admin/purchase-change-orders/${encodeURIComponent(changeOrderId)}/draft`,
    )
}

export type PurchaseChangeSubmissionState = {
    id: string
    purchase_order_id: string
    version: number
    status: string
    current_submission_id: string | null
}

/** 草稿读取受状态约束，未知结果核对改用受控原单详情读取真实提交状态。 */
export function fetchPurchaseChangeSubmissionState(changeOrderId: string) {
    return apiGet<PurchaseChangeSubmissionState>(
        `/admin/purchase-change-orders/${encodeURIComponent(changeOrderId)}`,
    )
}
