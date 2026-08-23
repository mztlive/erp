import type { ContractStatus } from "@/features/contracts/types"

/** 后端 Page/DTO 线格式：仅在 api/ 内使用，不外泄到视图类型。 */

export type BackendContractView = {
    id: string
    contract_no: string
    customer_id: string
    settlement_party_id: string
    status: ContractStatus | string
    current_revision_id?: string | null
    created_at: number
    version: number
}

export type BackendContractRevision = {
    id: string
    revision_no: number
    contract_pdf_file_id: string
    archive_source: string
    customer_name: string
    settlement_party_name: string
    payment_term_code: string
    payment_term_name: string
    invoice_type: string
    tax_point: string
    valid_from: string
    valid_to?: string | null
    signed_at: string
    created_at: number
}

export type BackendContractDetail = BackendContractView & {
    revisions: BackendContractRevision[]
}

export type BackendCustomerDetail = {
    id: string
    party_id: string
    customer_no: string
    legal_name?: string | null
    party_no?: string | null
    owner_user_id?: string | null
    owner_user_name?: string | null
    version: number
    created_at: number
}

export type BackendPartyView = {
    id: string
    party_no: string
    unified_credit_code?: string | null
}

export type BackendFileAsset = {
    id: string
    storage_object_key?: string
    file_name: string
    content_type: string
    byte_size: number
    security_scan_status: string
    created_by: string
    created_at: number
    version?: number
}
