/** 后端 snake_case 线格式 DTO 形状（W07 适配层内部使用）。 */

import type { FulfillmentMode } from "@/features/procurement-confirmation/types"
import type { WorkItemDto } from "@/features/work-items"

export type BackendPage<T> = {
    items: T[]
    total: number
    page?: number
    page_size?: number
}

export type BackendConfirmationLine = {
    id: string
    line_no: number
    sales_order_submission_line_id: string
    supplier_id: string
    supplier_offering_revision_id?: string | null
    confirmed_quantity: string
    latest_cost_gross: string
    input_tax_rate: string
    expected_delivery_date: string
    fulfillment_mode: FulfillmentMode | string
    supplier_capability_revision_id: string
}

export type BackendConfirmationDetail = {
    id: string
    sales_order_id: string
    submission_id: string
    status: "PENDING" | "APPROVED" | "REJECTED" | string
    handled_by?: string | null
    handled_at?: number | null
    version: number
    created_at: number
    lines: BackendConfirmationLine[]
    work_item?: WorkItemDto | null
    allowed_actions?: Array<"SAVE" | "APPROVE" | "REJECT">
    action_blockers?: Array<{
        action: "SAVE" | "APPROVE" | "REJECT" | string
        code: string
        message: string
    }>
}

export type BackendSalesOrderDetail = {
    id: string
    order_no: string
    business_type?: string
    origin_system?: string
    customer_id?: string
    contract_id?: string | null
    owner_user_id?: string
    owner_user_name?: string | null
    submissions?: Array<{
        id: string
        submission_no: number
        status?: string
        customer_name: string
        contract_no?: string | null
        settlement_party_name?: string | null
        payment_term_name: string
        project_name?: string | null
        business_remark?: string | null
        gross_amount?: string
        net_amount?: string
        tax_amount?: string
        submitted_by?: string
        submitted_at?: number
        lines: Array<{
            id: string
            sales_order_line_id?: string
            line_no: number
            item_name_snapshot?: string
            spec_snapshot?: string | null
            sku_id?: string | null
            unit_snapshot?: string | null
            base_unit_code?: string | null
            quantity?: string | null
            unit_price_gross?: string | null
            sales_tax_rate?: string | null
            fulfillment_mode?: string | null
            fulfillment_due_at?: number | null
            gross_amount?: string
        }>
    }>
    working_copy?: {
        gross_amount?: string
        lines?: Array<{
            id: string
            sales_order_line_id?: string
            line_no: number
            item_name_snapshot?: string
            unit_snapshot?: string | null
            quantity?: string | null
            gross_amount?: string
        }>
    } | null
}

export type BackendSupplierOffering = {
    sku_id: string
    supplier_id: string
    status: string
    current_revision_id?: string | null
    current_revision_no?: number | null
    dropship_supply_price_gross?: string | null
    bulk_supply_price_gross?: string | null
    input_tax_rate?: string | null
    bulk_minimum_order_quantity?: string | null
    availability_status?: string | null
    available_quantity?: string | null
    freight_amount?: string | null
    service_fee_amount?: string | null
    valid_from?: string | null
    valid_to?: string | null
}

export type BackendRecommendation = {
    confirmation_id: string
    policy_version: string
    calculated_at: number
    ready: boolean
    lines: Array<{
        line_no: number
        sales_order_submission_line_id: string
        item_name: string
        sku_id: string
        supplier_id: string
        supplier_name: string
        supplier_offering_revision_id: string
        confirmed_quantity: string
        latest_cost_gross: string
        input_tax_rate: string
        expected_delivery_date: string
        fulfillment_mode: string
        supplier_capability_revision_id: string
        landed_gross: string
        freight_amount?: string | null
        service_fee_amount?: string | null
        recommendation_reason: string
    }>
    purchase_orders: Array<{
        supplier_id: string
        supplier_name: string
        fulfillment_mode: string
        line_count: number
        estimated_gross: string
    }>
    estimated_purchase_gross: string
    sales_gross: string
    estimated_gross_margin: string
    blocking_issues: Array<{
        code: string
        message: string
        sales_order_submission_line_id?: string | null
    }>
    warnings: Array<{
        code: string
        message: string
        sales_order_submission_line_id?: string | null
    }>
}

export type BackendSupplierCapability = {
    supplier_id: string
    capability_code: string
    status: string
    current_revision_id?: string | null
    valid_from: string
    valid_to?: string | null
}

export type BackendSupplierDetail = {
    capabilities: BackendSupplierCapability[]
}
