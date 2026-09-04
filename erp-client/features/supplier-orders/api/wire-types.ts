/**
 * W26 供应商订单 · 后端 Wire DTO 类型。
 * 仅描述 /admin/supplier-fulfillment-orders 系列端点返回的原始形状，
 * 客户端视图映射见 mapping.ts。
 */

import type { WorkItemDto } from "@/features/work-items/types"

export type BackendOrder = {
    id: string
    fulfillment_order_no: string
    supplier_id: string
    connection_id: string
    split_no: number
    fulfillment_status: string
    cancel_status: string
    refund_status: string
    external_order_no?: string | null
    submitted_at?: number | null
    accepted_at?: number | null
    completed_at?: number | null
    version: number
    created_at: number
}

export type BackendItem = {
    id: string
    supplier_fulfillment_order_id: string
    supplier_offering_revision_id: string
    supplier_sku_code_snapshot: string
    supplier_product_code_snapshot?: string | null
    quantity: string
    unit_cost_snapshot_gross: string
    cost_snapshot_total_gross: string
    input_tax_rate: string
}

export type BackendStatusHistory = {
    id: string
    previous_status: string
    new_status: string
    supplier_status_version: string
    occurred_at: number
    received_at: number
    external_event_id: string
    source_type: string
    created_at: number
}

export type BackendAction = {
    id: string
    supplier_fulfillment_order_id: string
    action_type: string
    status: string
    external_request_id?: string | null
    request_summary?: string | null
    response_summary?: string | null
    attempt_count: number
    created_at: number
}

export type BackendInvestigationEvidence = {
    evidence_id: string
    target_supplier_action_id: string
    outcome: "VERIFIED_TERMINAL" | "VERIFIED_NO_RESULT" | "RESULT_UNKNOWN"
    recorded_at: number
    can_safe_retry: boolean
    external_order_no?: string | null
    summary: string
    verified_supplier_action_result_id?: string | null
    verified_resolution?:
        | "ORDER_ACCEPTED"
        | "ORDER_REJECTED"
        | "ORDER_COMPLETED"
        | "CANCELED"
        | "REFUNDED"
        | null
}

export type BackendInvestigationResult = {
    result_status: "SUCCEEDED" | "UNKNOWN" | "BLOCKED"
    message: string
    operation_id: string
    evidence: BackendInvestigationEvidence
    order: BackendOrder
    work_item?: {
        id: string
        status: "OPEN"
        task_version: string | number
    } | null
    allowed_actions: string[]
    action_blockers: Array<{
        action: string
        code: string
        message: string
        destination_workspace_id?: string | null
    }>
}

export type BackendDetail = {
    order: BackendOrder
    items: BackendItem[]
    status_history: BackendStatusHistory[]
    actions: BackendAction[]
    refund_facts: Array<{
        id: string
        supplier_fulfillment_order_id: string
        external_refund_no: string
        refund_amount: string
        refunded_at: number
    }>
    supplier_name?: string | null
    address: {
        masked?: string | null
        can_reveal: boolean
        blocker_code?: string | null
        blocker_message?: string | null
    }
    work_item?: WorkItemDto | null
    target_supplier_action_id?: string | null
    last_investigation?: BackendInvestigationResult["evidence"] | null
    allowed_actions?: Array<
        "QUERY_RESULT" | "REPLAY" | "CONFIRM_VERIFIED_TERMINAL_RESULT"
    >
    action_blockers?: BackendInvestigationResult["action_blockers"]
}

export type BackendSubmitResult = {
    action: BackendAction
    lines: unknown[]
    order: BackendOrder
}

export type BackendBackgroundJob = {
    id: string
    job_no: string
    status: string
    result_expires_at?: number | null
}

export type BackendTaskCompletionResult = {
    operation_id: string
    work_item_id: string
    work_item_status: "COMPLETED"
    task_version: string | number
    order_lock_version: number
    resolution:
        | "ORDER_ACCEPTED"
        | "ORDER_REJECTED"
        | "ORDER_COMPLETED"
        | "CANCELED"
        | "REFUNDED"
}
