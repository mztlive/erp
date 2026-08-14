/**
 * W30 历史消费回填 · 后端 wire 类型（snake_case）
 */

export type BackendJob = {
    id: string
    mall_id: string
    cutover_id: string
    range_start: number
    range_end: number
    status: string
    total_count: number
    total_amount: string
    deduplicated_count: number
    actual_count: number
    standard_count: number
    none_count: number
    unattributed_count: number
    report_file_id?: string | null
    version: number
    created_at: number
}

export type BackendJobDetail = {
    job: BackendJob
    item_total_count: number
}

export type BackendItem = {
    id: string
    job_id: string
    business_fact_key: string
    source_event_reference: string
    mall_order_fact_id?: string | null
    result: string
    cost_basis: string
    error_code?: string | null
    error_detail?: string | null
    created_at: number
}

export type BackendCommandResult = {
    status: string
    job_id: string
    job_no: string
    operation_id: string
    idempotency_key: string
    next_step: string
}
