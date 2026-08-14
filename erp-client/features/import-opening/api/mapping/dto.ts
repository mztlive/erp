/** W18 导入与期初 · 后端 DTO 形状（P4 F8）。 */

export type BackendBatchListItem = {
    id: string
    batch_no: string
    source_system_id: string
    source_object_set: string
    baseline_date: string
    import_rule_version: string
    status:
        | "pending_validation"
        | "validating"
        | "pending_confirmation"
        | "ready_to_apply"
        | "importing"
        | "completed"
        | "partial_failed"
        | "failed"
    total_rows: number
    success_rows: number
    failed_rows: number
    failure_code_summary?: string | null
    confirmation_status_summary?: string | null
    version: number
    created_at: number
}

export type BackendBatchDetail = BackendBatchListItem & {
    successful_sanitized_file_asset_id?: string | null
    success_manifest_file_asset_id?: string | null
    failure_diagnostic_file_asset_id?: string | null
    source_file_hmac?: string | null
    background_job_id?: string | null
}

export type BackendRow = {
    id: string
    batch_id: string
    source_object_type: string
    source_row_key: string
    parse_status: "pending_parse" | "valid" | "invalid"
    mapping_status: "pending_mapping" | "mapped" | "conflict"
    import_status: "pending_import" | "imported" | "failed" | "skipped"
    external_identity_map_id?: string | null
    error_code?: string | null
    target_document_id?: string | null
    version: number
    created_at: number
}

export type BackendConfirmation = {
    id: string
    batch_id: string
    confirmation_scope: string
    owner_role: string
    batch_version: number
    trial_version: number
    status: "PENDING" | "CONFIRMED" | "REJECTED" | "INVALIDATED"
    decision?: "CONFIRM_SCOPE" | "RETURN_FOR_FIX" | null
    reason_code?: string | null
    comment?: string | null
    work_item_id: string
    work_item?: {
        work_item_id: string
        work_item_type: string
        task_version: string
        subject_version: string
        status: "OPEN" | "COMPLETED" | "CLOSED"
        assignment_mode: "DIRECT" | "POOL"
        owner_role: string
        owner_organization_id: string
        owner_user_id?: string | null
        processing_state: "READY" | "APPROVAL_BLOCKED"
        allowed_actions: string[]
        action_blockers: string[]
        handler_key: string
        destination_workspace_id: string
    } | null
    decided_by?: string | null
    decided_at?: number | null
    version: number
    created_at: number
}
