export type ReturnSettlementDecision = {
    idempotency_key: string
    sales_revision_id: string
    target_gross: string
    refund_amount: string
    credit_amount: string
    supplier_confirmation: string
}
export type DirectReturnPlan = {
    payable_account_id?: string | null
    sales_revision_id: string
    target_gross: string
    return_numbers: string[]
    pending_settlement: boolean
    settlement_decision?: ReturnSettlementDecision | null
    settlements?: {
        decision_key?: string | null
        id: string
        number: string
        method: "CASH_REFUND" | "SUPPLIER_CREDIT"
        amount: string
        status: "in_approval" | "posted"
    }[]
    finance_tasks?: {
        kind: "PURCHASE_RETURN_REFUND" | "PURCHASE_RETURN_INVOICE"
        owner_name: string
        work_item_id: string
    }[]
    lines: {
        purchase_line_id: string
        sales_line_id: string
        item_name: string
        unit: string
        purchased_quantity: string
        target_quantity: string
        fulfilled_quantity: string
        returned_quantity: string
        required_return: string
        max_direct_return: string
    }[]
}
export type DirectReturnRequest = {
    expected_version: number
    idempotency_key: string
    reason: string
    receipt_reference: string
    lines: { purchase_line_id: string; quantity: string }[]
}
