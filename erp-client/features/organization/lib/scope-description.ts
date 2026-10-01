/** 保留每条规则的业务、动作、维度和来源，不能用摘要替代鉴权。 */
export type ScopeRule = {
    id: string
    subject_type: "user" | "role"
    subject_id: string
    scope_type: string
    scope_targets: string[]
    resource: string
    actions?: string[]
    target_dimension?: string
    target_mode?: string | null
    include_descendants?: boolean | null
    enabled?: boolean
}
