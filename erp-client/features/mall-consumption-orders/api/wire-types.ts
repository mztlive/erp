/**
 * W25 商城消费订单 · 后端返回结构（snake_case）
 * 仅在本目录 api 模块间共享，不对外导出；字段映射见 mapping.ts / detail-mapping.ts。
 */

export type BackendFactSummary = {
    fact_type: string
    latest_occurred_at: number
    count: number
}

export type BackendPaymentComposition = {
    card_amount: string
    wechat_amount: string
    source_count: number
}

export type BackendCostBasisBreakdown = {
    basis: string
    line_count: number
    cost_amount?: string | null
}

export type BackendSupplierOrderSummary = {
    total: number
    statuses: string[]
    has_exception: boolean
}

export type BackendListRow = {
    mall_order_id: string
    mall_id: string
    mall_name: string
    external_order_no: string
    customer_id?: string | null
    customer_label?: string | null
    paid_at: number
    paid_amount: string
    payment_composition: BackendPaymentComposition
    fact_summary: BackendFactSummary[]
    fulfillment_chain: string
    supplier_order_summary: BackendSupplierOrderSummary
    attribution_status: string
    cost_basis_breakdown: BackendCostBasisBreakdown[]
    data_source: string
    allowed_actions: string[]
    action_blockers: string[]
    cost_basis_policy_state: string
    normalized_cost_basis?: string | null
}

export type BackendFact = {
    fact_id: string
    fact_type: string
    business_fact_key: string
    external_order_version: string
    after_sales_request_id?: string | null
    original_payment_fact_id?: string | null
    occurred_at: number
    received_at: number
    data_source: string
    processing_status: string
}

export type BackendItem = {
    mall_order_item_id: string
    external_item_id: string
    sku_id?: string | null
    product_publication_revision_id?: string | null
    supplier_offering_revision_id?: string | null
    name_snapshot: string
    spec_snapshot?: string | null
    quantity: string
    unit_price_gross: string
    line_gross_amount: string
    allocated_discount_amount: string
    allocated_freight_amount: string
    paid_amount: string
    sales_tax_rate: string
    unit_cost_snapshot?: string | null
    cost_snapshot_total?: string | null
    cost_tax_inclusion?: boolean | null
    cost_input_tax_rate?: string | null
    attribution_status: string
}

export type BackendPaymentSource = {
    payment_source_id: string
    source_no: number
    source_type: string
    amount: string
    source_reference: string
    mall_card_instance_id?: string | null
    attribution_status: string
    origin?: {
        customer_id?: string | null
        sales_order_id: string
    } | null
}

export type BackendFunding = {
    mall_order_item_id: string
    payment_source_id: string
    allocated_payment_amount: string
}

export type BackendCostAssessment = {
    assessment_id: string
    assessment_no: number
    cost_basis: string
    basis_source_label: string
    gross_amount?: string | null
    net_amount?: string | null
    tax_amount?: string | null
    tax_inclusion?: boolean | null
    input_tax_rate?: string | null
    assessed_at: number
}

export type BackendConsumption = {
    consumption_entry_id: string
    fact_id: string
    item_id: string
    payment_source_id: string
    direction: string
    amount: string
    occurred_at: number
    attribution_status: string
    origin_sales_order_id?: string | null
    reverses_consumption_entry_id?: string | null
    current_cost_assessment?: BackendCostAssessment | null
}

export type BackendConservationRow = {
    id: string
    expected: string
    actual: string
    valid: boolean
}

export type BackendDetail = {
    identity: {
        mall_order_id: string
        mall_id: string
        mall_name: string
        external_order_no: string
        payment_fact_id: string
    }
    customer: {
        source_customer_ref?: string | null
        customer_id?: string | null
        customer_label?: string | null
        attribution_status: string
    }
    ordered_at: number
    paid_at: number
    amounts: {
        gross: string
        discount: string
        freight: string
        paid: string
        conservation_status: string
    }
    fulfillment: {
        chain: string
        cutover_id?: string | null
        cutover_at?: number | null
        decided_by_occurred_at: number
    }
    facts: BackendFact[]
    items: BackendItem[]
    payment_sources: BackendPaymentSource[]
    funding_allocations: BackendFunding[]
    conservation: {
        item_row_results: BackendConservationRow[]
        source_column_results: BackendConservationRow[]
        order_total: BackendConservationRow
    }
    consumption_entries: BackendConsumption[]
    supplier_orders: Array<{
        supplier_fulfillment_order_id: string
        fulfillment_order_no: string
        supplier_label: string
        item_ids: string[]
        fulfillment_status: string
    }>
    address: { masked_summary: string; reveal_allowed: boolean }
    allowed_actions: string[]
    action_blockers: string[]
}

export type BackendBackgroundJob = {
    id: string
    job_no: string
    status: string
    result_expires_at?: number | null
    total_count: number
}
