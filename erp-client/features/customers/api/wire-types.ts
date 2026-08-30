import type { ContractStatus } from "@/lib/contract-status"

/**
 * 客户资料 HTTP 适配层的 Wire DTO 类型。
 * 仅限 api/ 内部使用；页面只消费 camelCase 视图（见 types.ts）。
 */

export type BackendCustomerStatus = "active" | "disabled"
export type BackendCustomerScope =
    | "mine"
    | "collaborating"
    | "assigned"
    | "all_authorized"
export type BackendSensitiveKind =
    | "contact_mobile"
    | "address"
    | "bank_account_number"

export type BackendCustomerView = {
    id: string
    party_id: string
    party_no?: string | null
    legal_name?: string | null
    short_name?: string | null
    customer_no: string
    default_payment_term_id?: string | null
    status: BackendCustomerStatus
    owner_user_id?: string | null
    owner_user_name?: string | null
    collaborator_count: number
    scope_tags: BackendCustomerScope[]
    version: number
    created_at: number
    updated_at: number
}

export type BackendPartyRevision = {
    id: string
    revision_no: number
    legal_name: string
    short_name?: string | null
    change_reason: string
    version: number
    created_at: number
}

export type BackendAssignment = {
    id: string
    customer_id: string
    user_id: string
    user_name: string
    assignment_role: "OWNER" | "COLLABORATOR"
    valid_from: string
    valid_to?: string | null
    change_reason: string
    version: number
    created_at: number
}

export type BackendContact = {
    id: string
    contact_name: string
    title?: string | null
    telephone?: string | null
    mobile_masked: string
    email?: string | null
    valid_from: string
    valid_to?: string | null
    is_default: boolean
}

export type BackendAddress = {
    id: string
    address_type: "registered" | "operating" | "fulfillment"
    contact_name?: string | null
    valid_from: string
    valid_to?: string | null
    is_default: boolean
}

export type BackendBankAccount = {
    id: string
    bank_account_no: string
    account_name: string
    bank_name: string
    account_number_masked: string
    bank_branch_name?: string | null
    valid_from: string
    valid_to?: string | null
    is_default: boolean
}

export type BackendSensitiveField = {
    kind: BackendSensitiveKind
    record_id: string
    masked_value: string
    reveal_token: string
    expires_at: number
}

export type BackendCustomerProfile = BackendCustomerView & {
    party_status: string
    party_version: number
    unified_credit_code?: string | null
    current_revision: BackendPartyRevision
    revisions: BackendPartyRevision[]
    assignments: BackendAssignment[]
    contacts: BackendContact[]
    addresses: BackendAddress[]
    bank_accounts: BackendBankAccount[]
    sensitive_fields: BackendSensitiveField[]
    allowed_actions: string[]
    action_blockers: { action: string; code: string; message: string }[]
}

export type BackendProfileMutation = {
    customer_id: string
    customer_no: string
    party_id: string
    revision_id: string
    revision_no: number
    customer_version: number
    party_version: number
    effective_from: string
    recorded_at: number
    change_reason: string
}

export type BackendContractListRow = {
    id: string
    contract_no: string
    customer_id: string
    status: ContractStatus | string
}

export type BackendSalesOrderListRow = {
    id: string
    order_no: string
    customer_id: string
    commercial_status: string
    close_status: string
    created_at: number
}
