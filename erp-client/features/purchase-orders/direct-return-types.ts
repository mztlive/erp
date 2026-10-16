export type DirectReturnPlan = {
    sales_revision_id: string
    target_gross: string
    return_numbers: string[]
    pending_settlement: boolean
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
