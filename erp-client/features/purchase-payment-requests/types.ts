import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"

export type RequestStatus =
    | "draft"
    | "in_approval"
    | "approved"
    | "partially_paid"
    | "paid"
    | "rejected"
    | "withdrawn"
    | "closed"

export type PaymentRecipient = {
    bank_account_id: string
    version: number
    account_name: string
    bank_name: string
    bank_branch_name?: string | null
    account_number_masked: string
}

export type PaymentCandidate = {
    payable_account_id: string
    purchase_order_id: string
    purchase_order_no: string
    purchase_version: number
    supplier_id: string
    supplier_name: string
    our_company_id: string
    our_company_name: string
    payment_term_name: string
    total_amount: string
    paid_amount: string
    open_amount: string
    reserved_amount: string
    available_amount: string
    payment_recipient?: PaymentRecipient | null
    bank_accounts: PaymentRecipient[]
}

export type RequestLine = PaymentCandidate & {
    requested_amount: string
    paid_amount: string
    closed_amount: string
    unpaid_amount: string
    current_open_amount?: string
    current_reserved_amount?: string
    current_fulfillment?: string
    source_paid_amount: string
    source_changed?: boolean
    payable_entries: { id: string; open_total: string }[]
}

export type RequestAttachment = {
    file_asset_id: string
    file_name: string
    content_type?: string
}
export type PaymentRequest = {
    id: string
    request_no: string
    version: number
    submission_version: number
    status: RequestStatus
    supplier_id: string
    supplier_name: string
    our_company_id: string
    our_company_name: string
    payment_recipient: PaymentRecipient
    applicant_name: string
    desired_payment_date: string
    description: string
    total_amount: string
    paid_amount: string
    unpaid_amount: string
    closed_amount: string
    close_reason?: string
    lines: RequestLine[]
    /** 本申请上传的附件，编辑时不回写展示材料中的采购、销售来源附件。 */
    attachment_ids: string[]
    attachments: RequestAttachment[]
    approval?: DocumentApprovalViewDto | null
    allowed_actions: string[]
    payments: RequestPaymentRecord[]
    materials?: RequestMaterials
}

export type RequestPaymentRecord = {
    payment_id: string
    payment_no: string
    paid_at: number
    amount: string
    reversed_amount: string
    effective_amount: string
    allocations: { payable_account_id: string; amount: string }[]
    receipt?: RequestAttachment | null
}

export type RequestDraftInput = {
    supplier_id: string
    desired_payment_date: string
    description: string
    bank_account_id: string
    bank_account_version: number
    lines: { payable_account_id: string; amount: string }[]
    attachment_ids: string[]
}
export type RequestCommand = {
    expected_version: number
    idempotency_key: string
    reason?: string
}
export type PaymentInput = {
    requestId: string
    expectedVersion: number
    workItemId: string
    expectedTaskVersion: string
    amount: string
    paidAt: string
    bankReference: string
    receipt: File
    lines: { payable_account_id: string; amount: string }[]
    allocations: { payable_entry_id: string; allocated_amount: string }[]
    idempotencyKey: string
    supplierId: string
}
export type RequestMaterials = {
    sales: SalesSnapshot[]
    purchases: {
        purchase_order_id: string
        purchase_order_no: string
        version: number
        snapshot: PurchaseSnapshot
    }[]
}
export type SalesSnapshot = {
    sales_order_id: string
    sales_order_no: string
    revision_id: string
    revision_no: number
    customer_name: string
    contract_no?: string | null
    totals: { gross: string; net: string; tax: string }
    lines: {
        sales_order_revision_line_id: string
        line_no: number
        item_name: string
        specification?: string | null
        quantity: string
        unit: string
        unit_price_gross: string
        gross_amount: string
    }[]
    materials: RequestAttachment[]
}
export type PurchaseSnapshot = {
    supplier_name: string
    our_company_name: string
    payment_term_name: string
    total_amount: string
    items: {
        id: string
        item_name: string
        spec?: string
        quantity: string
        unit?: string
        unit_price: string
        amount: string
    }[]
}

export type PaymentTaskContext = {
    workItemId: string
    taskVersion: string
    workItemType: string
    businessObjectId: string
    allowedActions: readonly string[]
}
