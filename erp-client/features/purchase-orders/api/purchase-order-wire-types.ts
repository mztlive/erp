/**
 * W08 采购单 · 后端 wire 类型（真实 HTTP API 响应形状）。
 * 本文件为纯类型声明表，仅描述后端契约，不承载逻辑。
 */

import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"
import type {
    FulfillmentResponsibility,
    PurchaseType,
} from "@/features/purchase-orders/types"
import type {
    WorkItemProcessingState,
    WorkItemStatus,
} from "@/features/work-items"

export type BackendPage<T> = {
    items: T[]
    total: number
    page?: number
    page_size?: number
}

export type BackendListItem = {
    id: string
    purchase_no: string
    sales_order_id: string
    sales_order_no: string
    supplier_id: string
    supplier_name: string
    purchase_type: PurchaseType | string
    payment_term_code?: string | null
    owner_name?: string | null
    status: string
    review_status: string
    gross_amount: string
    net_amount: string
    tax_amount: string
    payment_progress: string
    invoice_progress: string
    fulfillment_progress: string
    current_submission_id?: string | null
    current_revision_id?: string | null
    version: number
    created_at: number
}

export type BackendLine = {
    line_id: string
    line_no: number
    line_type: "ITEM_SERVICE" | "LOGISTICS_FEE" | string
    procurement_confirmation_line_id?: string | null
    sku_id?: string | null
    sku_revision_id?: string | null
    product_name?: string | null
    specification?: string | null
    quantity?: string | null
    base_unit_code?: string | null
    unit_cost_gross?: string | null
    input_tax_rate?: string | null
    gross_amount: string
    net_amount: string
    tax_amount: string
    expected_delivery_date?: string | null
    sales_order_line_id?: string | null
    sales_order_revision_line_id?: string | null
    sales_order_submission_line_id?: string | null
    allocated_quantity?: string | null
}

export type BackendCenter = {
    id: string
    purchase_no: string
    status: string
    review_status: string
    version: number
    sales_order_id: string
    sales_order_no: string
    supplier_id: string
    supplier_name: string
    purchase_type: PurchaseType | string
    payment_term_code: string
    fulfillment_responsibility: FulfillmentResponsibility | string
    payment_progress: string
    invoice_progress: string
    fulfillment_progress: string
    current_submission_id?: string | null
    current_revision_id?: string | null
    revision_no?: number | null
    content_source: string
    lines: BackendLine[]
    totals: { gross: string; net: string; tax: string }
    allocations: Array<{
        id: string
        purchase_order_revision_line_id: string
        sales_order_revision_line_id: string
        allocated_quantity: string
        allocated_cost_gross: string
        allocated_cost_net: string
    }>
    changes: Array<{
        change_id: string
        status: string
        base_revision_id: string
        effective_revision_id?: string | null
        reason: string
        created_at: number
    }>
    review_work_item?: {
        work_item_id: string
        work_item_type: "PURCHASE_ORDER_REVIEW"
        task_version: string | number
        subject_version: string
        status: WorkItemStatus
        owner_role: string
        owner_organization_id: string
        owner_user_id?: string | null
        processing_state: WorkItemProcessingState
        domain_allowed_actions: readonly ("APPROVE" | "REJECT")[]
        action_blockers: readonly {
            action: string
            code: string
            message: string
        }[]
    } | null
    payable_summary?: {
        payable_open_amount: string
        paid_allocated_amount: string
        purchase_invoice_allocated_amount: string
    } | null
    approval?: DocumentApprovalViewDto | null
    created_at: number
}

export type BackendBasisLine = {
    sales_order_line_id: string
    sales_order_revision_line_id: string
    sales_line_no: number
    supplier_id: string
    confirmed_quantity: string
    sales_quantity?: string | null
    covered_quantity?: string | null
    remaining_quantity?: string | null
    max_create_quantity?: string | null
    latest_cost_gross: string
    input_tax_rate: string
    expected_delivery_date: string
    sales_delivery_deadline?: string | null
    product_name?: string | null
    specification?: string | null
    unit?: string | null
    gross_amount: string
}

export type BackendBasis = {
    basis_id: string
    work_item_id: string
    sales_order_id: string
    sales_order_no: string
    customer_name: string
    contract_no?: string | null
    sales_owner_name?: string | null
    sales_order_revision_id: string
    supplier_id: string
    supplier_name: string
    purchase_type?: string | null
    fulfillment_responsibility?: string | null
    payment_term_code: string
    lines: BackendBasisLine[]
    estimated_gross: string
}

export type BackendCreateResult = {
    purchase_order_id: string
    purchase_no: string
    lock_version: number
    replayed?: boolean
    reference: string
}

export type BackendSourcingCreateResult = {
    orders: BackendCreateResult[]
    replayed?: boolean
    reference: string
}

export type BackendSaveResult = {
    lock_version: number
    totals: { gross: string; net: string; tax: string }
    reference: string
}

export type BackendVoidResult = {
    purchase_order_id: string
    status: "VOIDED"
    lock_version: number
    replayed: boolean
    reference: string
}

export type BackendSubmitResult = {
    purchase_order_id: string
    purchase_no: string
    submission_id: string
    submission_no: string
    work_item_id: string
    task_version: string | number
    subject_version: string
    lock_version: number
    reference: string
}

export type BackendReviewResult = {
    work_item_id: string
    work_item_status: "COMPLETED"
    task_version: string | number
    subject_version: string
    review_result: string
    revision_id?: string | null
    revision_no?: number | null
    payable_entry_id?: string | null
    lock_version: number
    reference: string
}

export type BackendChangeStartResult = {
    change_id: string
    base_revision_id: string
    base_revision_no: number
    lock_version: number
    reference: string
}

export type BackendPurchaseChangeOrder = {
    id: string
    purchase_order_id: string
    base_revision_id: string
    reason: string
    status: string
    current_submission_id?: string | null
    effective_revision_id?: string | null
    version: number
    created_at: number
    approval?: DocumentApprovalViewDto | null
}

export type BackendPurchaseChangeSubmitResult = {
    change_id: string
    submission_id: string
    submission_no: string
    status: string
    lock_version: number
    reference: string
}
