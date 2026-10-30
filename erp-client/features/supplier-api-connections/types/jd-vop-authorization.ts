export type JdVopAuthorizationState =
    | "unconfigured"
    | "missing"
    | "identity_unverified"
    | "unreadable"
    | "expired"
    | "authorizing"
    | "unknown"
    | "authorized"
    | "refreshing"
    | "reauthorization_required"
    | "rejected"

/** 仅包含脱敏授权状态，不接收或保存访问令牌。 */
export type JdVopAuthorizationStatus = {
    state: JdVopAuthorizationState
    version: number | null
    expires_at: number | null
    /** 指定原操作查询时，这两项为原回执；其余字段仍为当前授权状态。 */
    operation_id: string | null
    operation_state:
        | "claimed"
        | "sent_unknown"
        | "completed"
        | "rejected"
        | "superseded"
        | null
    can_authorize: boolean
    recovery_after: number | null
    message: string
}

/** 敏感授权资料只用于本次请求，禁止写入查询键、持久化或日志。 */
export type AuthorizeJdVopAccount = {
    operation_id: string
    expected_version: number | null
    username: string
    password: string
    redirect_uri: string
    private_key: string
    evidence_reference?: string
}
