/**
 * 后端 DTO（snake_case；时间秒级时间戳）。
 * 仅供 api/ 内部映射使用；页面消费 types.ts 的 camelCase 视图。
 */

export type BackendJob = {
    id: string
    source_system_id: string
    job_type:
        | "baseline"
        | "incremental"
        | "monthly_reconciliation"
        | "single_order_backfill"
    range_start?: number | null
    range_end?: number | null
    external_order_no?: string | null
    trigger_source: "SCHEDULED" | "MANUAL"
    trigger_reason?: string | null
    triggered_by?: string | null
    source_job_id?: string | null
    started_at: number
    finished_at?: number | null
    status: "running" | "success" | "partial_failure" | "failed"
    page_count: number
    item_count: number
    error_count: number
    version: number
    created_at: number
}

export type BackendSnapshot = {
    id: string
    source_system_id: string
    external_order_no: string
    source_updated_at: number
    content_hash?: string | null
    source_status_code: string
    observed_at: number
    mapping_status: "pending" | "applied" | "difference" | "no_change"
    applied_sales_order_revision_id?: string | null
    sync_job_id: string
    version: number
    created_at: number
}

export type BackendCursor = {
    id: string
    source_system_id: string
    high_water_updated_at: number
    last_success_job_id?: string | null
    version: number
    created_at: number
}

export type BackendMappingTask = {
    id: string
    source_snapshot_id: string
    mapping_type:
        | "customer"
        | "contract"
        | "settlement_entity"
        | "voucher_category"
        | "unique_line_item"
        | "amount_format"
    status: "pending" | "resolved" | "unresolvable" | "closed"
    owner_role?: string | null
    owner_user_id?: string | null
    resolution?: string | null
    resolved_at?: number | null
    owner_routing_state: "MISSING" | "CONFIGURED"
    work_item?: {
        work_item_id: string
        task_version: string
        work_item_type: "BUSINESS_EXCEPTION"
        business_object_type: "MASTER_MAPPING_TASK"
        business_object_id: string
        subject_version: string
        status: "OPEN" | "COMPLETED" | "CLOSED"
        owner_user_id?: string | null
    } | null
    external_identity_map_id?: string | null
    source_evidence: Array<{
        field: string
        label: string
        value: string
        sensitive: boolean
    }>
    candidate_targets: Array<{
        object_type: string
        object_id: string
        stable_no: string
        label: string
        current_revision_id: string
        eligibility: "ELIGIBLE" | "INELIGIBLE"
        reason: string
    }>
    current_targets: Array<{
        mapping_target_id: string
        object_type: string
        object_id: string
        relation_role: string
        valid_from: number
        valid_to?: number | null
        status: string
    }>
    impact_summary: string
    resolution_history: Array<{
        action: string
        result: string
        handled_by: string
        handled_at: number
        evidence_reference?: string | null
    }>
    allowed_actions: string[]
    action_blockers: Array<{
        action: string
        code: string
        message: string
    }>
    reapply_operation?: {
        operation_id: string
        status: "QUEUED" | "RUNNING" | "SUCCEEDED" | "FAILED" | "UNKNOWN"
        sales_order_id?: string | null
        sales_order_revision_id?: string | null
        receivable_result_reference?: string | null
        failure_code?: string | null
        failure_message?: string | null
        last_updated_at: number
    } | null
    lock_version: number
    version: number
    created_at: number
}

export type BackendReconJob = {
    id: string
    source_system_id: string
    job_no: string
    source_list_as_of: number
    source_count: number
    erp_count: number
    difference_count: number
    status: "running" | "completed" | "has_difference" | "failed"
    started_at: number
    finished_at?: number | null
    version: number
    created_at: number
}

export type BackendReconItem = {
    id: string
    reconciliation_job_id: string
    external_order_no: string
    source_status_code: string
    source_updated_at: number
    difference_type:
        | "mall_missing"
        | "erp_missing"
        | "status_difference"
        | "content_fingerprint_difference"
        | "duplicate_identity"
    status: "pending" | "backfilling" | "resolved" | "confirmed_no_difference"
    single_order_sync_job_id?: string | null
    resolution?: string | null
    resolved_by?: string | null
    resolved_at?: number | null
    version: number
    created_at: number
}

export type BackendSourceSystem = {
    id: string
    code: string
    name: string
    system_type: "ERP" | "MALL" | "SUPPLIER"
    status: "active" | "disabled"
    mall_sync_stage?: "FIRST_PHASE_MALL_OWNED" | "ARCHIVED" | null
    created_at: number
    version?: number
}
