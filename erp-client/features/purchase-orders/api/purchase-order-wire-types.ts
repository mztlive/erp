import type { SourceImpactReasonWire } from "@/features/purchase-orders/lib/source-impact"
/**
 * W08 采购单 · 后端 wire 类型（真实 HTTP API 响应形状）。
 * 本文件为纯类型声明表，仅描述后端契约，不承载逻辑。
 */

import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"
import type {
    FulfillmentResponsibility,
    PurchaseType,
    PurchaseDeliveryAllocation,
} from "@/features/purchase-orders/types"

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
    fulfillment_responsibility: FulfillmentResponsibility | string
    payment_term_code?: string | null
    owner_user_id?: string | null
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
    inquiry_quote_source?: import("../types").PurchaseInquiryQuoteSource | null
    delivery_allocations?: PurchaseDeliveryAllocation[]
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
    expense?: BackendExpenseDetail | null
    expense_occurrence?: {
        occurred_on: string
        confirmed_by: string
        evidence_attachment_id: string
        gross_amount: string
    } | null
}

export type BackendExpenseDetail = {
    expense_line_id: string
    category: string
    description: string
    sales_order_line_id?: string | null
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
    owner_user_id: string
    owner_name: string
    target_warehouse_id?: string | null
    target_warehouse_name?: string | null
    payment_progress: string
    invoice_progress: string
    fulfillment_progress: string
    current_submission_id?: string | null
    current_revision_id?: string | null
    revision_no?: number | null
    content_source: string
    lines: BackendLine[]
    source_sales_order?: BackendPurchaseSourceSalesOrder | null
    source_update?: BackendPurchaseSourceUpdate | null
    delivery?: {
        price_basis: "DROPSHIP" | "BULK"
        lines: {
            sales_order_line_id: string
            template_name?: string | null
            freight_extra: boolean
            freight_pending: boolean
            waiver?: {
                reason: string
                waived_by: string
                waived_at: number
            } | null
        }[]
    } | null
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
    payable_summary?: {
        payable_open_amount: string
        paid_allocated_amount: string
        purchase_invoice_allocated_amount: string
    } | null
    approval?: DocumentApprovalViewDto | null
    created_at: number
}

export type BackendPurchaseSourceSalesOrder = {
    sales_order_id: string
    sales_order_no: string
    status: string
    revision_id: string
    revision_no: number
    customer_name: string
    sales_owner_name?: string | null
    contract_no?: string | null
    totals: { gross: string; net: string; tax: string }
    lines: {
        sales_order_revision_line_id: string
        sales_order_line_id: string
        line_no: number
        item_name: string
        specification?: string | null
        delivery_allocations?: {
            recipient: string
            phone: string
            region_code?: string
            region_name?: string
            address: string
            quantity: string
        }[]
        quantity: string
        unit: string
        unit_price_gross: string
        gross_amount: string
    }[]
    materials: {
        file_asset_id: string
        kind: string
        file_name: string
        content_type: string
        byte_size: number
    }[]
    materials_unavailable?: boolean
}

export type BackendDeliveryTerm =
    | "FREIGHT_INCLUDED"
    | "FREIGHT_EXTRA"
    | "NOT_DELIVERABLE"

export type BackendBasisLine = {
    delivery_template_name?: string | null
    delivery_terms?: { region_code: string; term: BackendDeliveryTerm }[]
    warehouse_delivery_terms?: {
        warehouse_id: string
        region_name: string
        term: BackendDeliveryTerm
    }[]
    bulk_minimum_order_quantity?: string | null
    supplier_offering_id?: string | null
    delivery_allocations?: PurchaseDeliveryAllocation[]
    assigned_delivery_allocations?: PurchaseDeliveryAllocation[]
    sales_order_line_id: string
    sales_order_revision_line_id: string
    sales_line_no: number
    supplier_id: string
    confirmed_quantity: string
    sales_quantity?: string | null
    covered_quantity?: string | null
    remaining_quantity?: string | null
    quantity_scale?: number | null
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
    source_type?: "PURCHASE" | "EXISTING_STOCK" | string
    inquiry_source?: import("../types").InquirySourcingSource | null
    inquiry_reselection_required?: boolean
    recommendation_reason?: string | null
    work_item_id: string
    sales_order_id: string
    sales_order_no: string
    customer_name: string
    contract_no?: string | null
    sales_owner_name?: string | null
    sales_order_revision_id: string
    supplier_id: string
    supplier_name: string
    stock_balance_id?: string | null
    warehouse_id?: string | null
    warehouse_name?: string | null
    source_available_quantity?: string | null
    purchase_type?: string | null
    fulfillment_responsibility?: string | null
    payment_term_code: string
    business_category?: string | null
    price_basis?: "DROPSHIP" | "BULK" | null
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
    stock_reservations?: Array<{
        stock_reservation_id: string
        sales_order_line_id: string
        stock_balance_id: string
        warehouse_id: string
        quantity: string
    }>
    work_item_status: "OPEN" | "COMPLETED"
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
    /** 在途改单按当前提交预判的销售覆盖变化；列表行为空。 */
    coverage_effects?: Array<{
        sales_order_line_id: string
        line_no: number
        item_name: string
        base_quantity: string
        target_quantity: string
        sales_quantity: string
        other_covered_quantity: string
        gap_quantity: string
        excess_quantity: string
    }>
}

/** 来源销售变更对采购的影响；采购引用当前销售版本且无待处理记录时为空。 */
export type BackendPurchaseSourceUpdate = {
    return_handling?: import("../direct-return-types").DirectReturnPlan | null
    affected: boolean
    awaiting_submission?: boolean
    record_id?: string | null
    record_version?: number | null
    blocks_payment: boolean
    reasons: SourceImpactReasonWire[]
    purchase_sales_revision_no?: number | null
    current_sales_revision_no?: number | null
    sales_revision_outdated: boolean
    last_resolution?: string | null
    last_resolution_label?: string | null
    last_maintain_note?: string | null
}
