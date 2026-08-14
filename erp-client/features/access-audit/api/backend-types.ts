// 后端 access_control + iam 原始 DTO：只做契约描述，映射逻辑见 mappers.ts。

type BackendRole = {
    id: string
    name: string
    permissions: string[]
    created_at: number
}

type BackendAdmin = {
    id: string
    account: string
    name: string
    role_ids: string[]
    created_at: number
}

type BackendPermission = {
    id: string
    resource: string
    action: string
    name: string
    description?: string | null
    system: boolean
    disabled: boolean
    version: number
    created_at: number
}

type BackendDataScope = {
    id: string
    subject_type: "role" | "user"
    subject_id: string
    scope_type:
        | "company"
        | "organization"
        | "team"
        | "self_owned"
        | "collaborative"
    scope_targets: string[]
    version: number
    created_at: number
}

type BackendUserRole = {
    id: string
    user_id: string
    role_id: string
    effective_from: number
    effective_to?: number | null
    assigned_by: string
    revoked_at?: number | null
    revoked_by?: string | null
    revoke_reason_code?: string | null
    revoke_reason_text?: string | null
    version: number
    created_at: number
}

type BackendAuditEvent = {
    id: string
    actor_id: string
    actor_label: string
    actor_role: string
    action_type: string
    object_type: string
    object_id?: string | null
    object_label?: string | null
    request_id?: string | null
    trace_id?: string | null
    result: "SUCCESS" | "DENIED" | "FAILED" | "UNKNOWN"
    changed_field_names: string[]
    safe_digest?: string | null
    source_ip?: string | null
    device_context?: string | null
    created_at: number
}

export type {
    BackendAdmin,
    BackendAuditEvent,
    BackendDataScope,
    BackendPermission,
    BackendRole,
    BackendUserRole,
}
