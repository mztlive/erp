/**
 * 采购退货单 · 后端 wire 类型（真实 HTTP 响应形状）。
 *
 * PurchaseReturnOrder 为合同 NO_APPROVAL：本 DTO 不得携带审批绑定。
 */

export type BackendPurchaseReturnLine = {
    id: string
    purchase_order_revision_line_id: string
    return_quantity: string
    warehouse_id?: string | null
}

/** PurchaseReturnOrder 为 NO_APPROVAL：创建/详情 DTO 不得携带审批绑定。 */
export type BackendPurchaseReturnOrder = {
    id: string
    purchase_return_no: string
    purchase_order_id: string
    sales_return_case_id?: string | null
    return_mode: string
    status: string
    version: number
    created_at: number
    lines: BackendPurchaseReturnLine[]
}

export type BackendPurchaseReturnOrderPage = {
    items: BackendPurchaseReturnOrder[]
    total: number
    page?: number
    page_size?: number
}
