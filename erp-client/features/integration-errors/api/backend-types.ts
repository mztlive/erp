/**
 * W29 后端 DTO 形状（snake_case 传输层），仅供映射器消费。
 * 从 mappers.ts 拆出；mappers.ts 统一再导出，导入路径不变。
 */

import type { IntegrationResolutionItemView } from "../types"
import type {
    BackendControlledEvidenceRef,
    BackendReconciliationReasonRegistry,
    BackendResolutionEvidencePolicy,
} from "./wire"

export type BackendErrorTask = {
    id: string
    message_id?: string | null
    business_object_id?: string | null
    error_class: string
    status: string
    owner_role?: string | null
    owner_user_id?: string | null
    attempt_count: number
    last_attempt_at?: number | null
    last_attempt_summary?: string | null
    resolution_type?: string | null
    resolved_at?: number | null
    version: number
    created_at: number
    resolution?: string | null
    allowed_actions?: string[] | null
    action_blockers?: IntegrationResolutionItemView["actionBlockers"] | null
    linked_evidence?: BackendControlledEvidenceRef[] | null
    resolution_evidence_policy?: BackendResolutionEvidencePolicy | null
    reconciliation_reason_registry?: BackendReconciliationReasonRegistry | null
}

export type BackendDifference = {
    id: string
    business_object_type: string
    business_object_id: string
    difference_type: string
    left_fact_reference?: string | null
    right_fact_reference?: string | null
    status?: string | null
    version: number
    created_at: number
    resolutions?: Array<{
        id: string
        resolution_no: number
        resolution_action: string
        resulting_status: string
        evidence_reference?: string | null
        handled_by: string
        handled_at: number
    }>
    allowed_actions?: string[] | null
    action_blockers?: IntegrationResolutionItemView["actionBlockers"] | null
    linked_evidence?: BackendControlledEvidenceRef[] | null
    resolution_evidence_policy?: BackendResolutionEvidencePolicy | null
    reconciliation_reason_registry?: BackendReconciliationReasonRegistry | null
}

export type BackendReplayResult = {
    task_id: string
    original_action_idempotency_key_summary: string
    original_action_idempotency_key_locked: boolean
    replay_accepted: boolean
    task_status: string
    attempt_count: number
    task_version: number
}
