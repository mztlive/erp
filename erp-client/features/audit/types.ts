/** 业务审计只接受服务器保存的安全投影与历史快照。 */
export type AuditValue =
    | { kind: "code"; code: string; label: string }
    | { kind: "quantity"; value: string }
    | { kind: "amount"; value: string }
    | { kind: "changed" }

export type BusinessAuditResult = "succeeded" | "rejected" | "unknown"

export type BusinessAuditEvent = {
    schema_version: number
    event_sequence: number
    action_code: string
    action_version: number
    action_label: string
    actor_id: string
    actor_account: string
    actor_type: string
    actor_name_snapshot?: string | null
    resource_type: string
    resource_id: string
    resource_number_snapshot?: string | null
    result: BusinessAuditResult
    field_changes: {
        field: string
        field_label: string
        before: AuditValue
        after: AuditValue
    }[]
    facts: { field: string; field_label: string; value: AuditValue }[]
    command_id?: string | null
    request_id?: string | null
    occurred_at: number
}

export type AuditLogItem = {
    id: string
    actor_id: string
    actor_account: string
    actor_type: string
    action: string
    resource_type: string
    resource_id?: string | null
    success: boolean
    message?: string | null
    created_at: number
    structured_event?: BusinessAuditEvent | null
}

export type AuditLogListParams = {
    actor_account?: string
    action?: string
    event_result?: BusinessAuditResult
    resource_number?: string
    page: number
    page_size: number
}
