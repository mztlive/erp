/**
 * W27 API 供应商结算 · 后端 wire 类型
 * 从 api/settlements.ts 拆出：仅含后端响应形状；
 * wire → 视图 映射纯函数在 settlements-mappers.ts（底部再导出）。
 */

import type { Page } from "@/lib/api"
import type { WorkItemAllowedAction } from "@/features/work-items"

// ---------------------------------------------------------------------------
// Backend wire types
// ---------------------------------------------------------------------------

export type BackendStatement = {
    id: string
    statement_no: string
    supplier_id: string
    period_start: string
    period_end: string
    period_policy_id: string
    period_policy_version: string
    period_timezone: string
    external_bill_no?: string | null
    external_bill_version?: string | null
    erp_amount: string
    supplier_amount: string
    difference_amount: string
    status: string
    prepared_by: string
    reviewed_by?: string | null
    confirmed_at?: number | null
    payable_account_id?: string | null
    subject_hash?: string | null
    source_as_of?: number | null
    source_snapshot_at?: number | null
    source_snapshot_hash?: string | null
    refresh_cutoff_policy_id?: string | null
    refresh_cutoff_policy_version?: string | number | null
    version: number
    created_at: number
}

export type BackendItem = {
    id: string
    statement_id: string
    supplier_fulfillment_order_id: string
    supplier_fulfillment_item_id: string
    quantity: string
    order_amount: string
    freight_amount: string
    service_fee_amount: string
    refund_amount: string
    erp_calculated_amount: string
    erp_calculated_net_amount: string
    erp_calculated_tax_amount: string
    supplier_billed_amount: string
    supplier_billed_net_amount: string
    supplier_billed_tax_amount: string
    created_at: number
}

export type BackendDifference = {
    id: string
    statement_item_id: string
    difference_type: string
    difference_amount: string
    status: string
    resolution?: string | null
    resolved_by?: string | null
    resolved_at?: number | null
    version: number
    created_at: number
    evidence?: BackendDifferenceEvidence[]
}

export type BackendDifferenceEvidence = {
    evidence_id: string
    evidence_reference_ids: string[]
    opinion_code?: string | null
    comment?: string | null
    provided_by: string
    provided_at: number
}

export type BackendDetail = {
    statement: BackendStatement
    items: BackendItem[]
    differences: BackendDifference[]
    review_work_item?: BackendReviewWorkItem | null
    review_action_blockers?: BackendReviewActionBlocker[]
    allowed_actions?: string[]
    action_blockers?: BackendReviewActionBlocker[]
    processing_state?: string
    stats?: {
        item_count: number
        difference_count: number
        pending_difference_count: number
        evidenced_difference_count: number
        order_amount: string
        freight_amount: string
        service_fee_amount: string
        refund_amount: string
        erp_amount: string
        supplier_amount: string
        difference_amount: string
    }
}

export type BackendSourceEvidence = {
    id: string
    request_id: string
    supplier_id: string
    period_start: string
    period_end: string
    period_policy_id: string
    period_policy_version: string
    timezone: string
    source_version: number
    external_bill_no: string
    external_bill_version: string
    source_as_of: number
    source_hash: string
    line_count: number
}

export type BackendDraftCommandResult = {
    result_status: "CREATED" | "REFRESHED" | "UNCHANGED" | "REPLAYED"
    message: string
    request_id: string
    statement: BackendStatement
    item_count: number
    difference_count: number
}

export type BackendEvidenceResult = {
    result_status: "RECORDED" | "REPLAYED"
    message: string
    request_id: string
    statement_id: string
    difference_id: string
    evidence: BackendDifferenceEvidence
}

export type BackendStatementPage = Page<BackendStatement> & {
    stats: {
        pending_reconciliation_count: number
        has_difference_count: number
        pending_review_count: number
        confirmed_amount: string
    }
    processing_state: "READY" | "EMPTY"
}

export type BackendReviewActionBlocker = {
    action: string
    code: string
    message: string
}

export type BackendReviewWorkItem = {
    work_item_id: string
    work_item_type: "SUPPLIER_SETTLEMENT_REVIEW"
    task_version: string | number
    subject_version: string
    status: "OPEN" | "COMPLETED" | "CLOSED"
    assignment_mode: "DIRECT" | "POOL"
    owner_role: string
    owner_organization_id: string
    owner_user_id?: string | null
    allowed_actions: WorkItemAllowedAction[]
    action_blockers: BackendReviewActionBlocker[]
}

export type BackendDifferenceDecisionResult = {
    result_status: "RESOLVED" | "UNKNOWN"
    message: string
    operation_id: string
    statement_id: string
    statement_lock_version: number
    difference: BackendDifference
}

export type BackendReviewSubmissionResult = {
    result_status: "SUBMITTED" | "UNKNOWN"
    message: string
    operation_id: string
    statement: BackendStatement
    work_item_id?: string | null
}

export type BackendReviewDecisionResult = {
    result_status: "CONFIRMED" | "REJECTED" | "UNKNOWN"
    message: string
    operation_id: string
    statement: BackendStatement
    work_item_id: string
    work_item_status: "COMPLETED"
    task_version: string | number
    payable_no?: string | null
    payable_account_id?: string | null
    cost_delta_gross?: string | null
}

// wire → 视图映射已拆至 settlements-mappers.ts（此处再导出，保持原有引用路径）。
export { asStatus, mapFormalReviewTask, toDetail, toListRow } from "./settlements-mappers"
