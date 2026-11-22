export type DepositAccountStatusFilter =
    | "available"
    | "reconciliation_required"
    | "disabled"

export type DepositAccountListFilters = {
    page: number
    supplierId?: string
    payerPartyId?: string
    status?: DepositAccountStatusFilter
}

export type DepositAccount = {
    id: string
    version: number
    identity: {
        payer_party_id: string
        supplier_id: string
        external_account_ref: string
        currency: "CNY"
    }
    book_balance: string
    reserved_amount: string
    unavailable_amount: string
    reconciliation_required: boolean
    enabled: boolean
}
export type AccountRow = {
    account: DepositAccount
    payer_party_name: string
    supplier_name: string
    external_account_label?: string
}
export type RechargeRequest = {
    id: string
    version: number
    request_no?: string
    amount: string
    reason: string
    status:
        | "draft"
        | "requested"
        | "bank_paid"
        | "approved"
        | "paid"
        | "received"
        | "reversed"
        | "DRAFT"
        | "APPROVED"
        | "PAID"
        | "RECEIVED"
        | "REVERSED"
    approval_reference?: string | null
    bank_reference?: string | null
    bank_receipt_asset_id?: string | null
    supplier_payment_id?: string | null
}
export type DepositAuthorization = {
    mode?:
        | { type: "automatic" }
        | { type: "manual"; work_item_id: string; task_version: string }
    id: string
    version: number
    approval_reference: string
    valid_from: number
    valid_until: number
    per_execution_limit: string
    cumulative_limit: string
    committed_amount: string
    revoked: boolean
}
export type DepositReservation = {
    id: string
    version: number
    amount: string
    consumed_amount: string
    released_amount: string
    exposure: "not_sent" | "possibly_sent" | "cleared"
    purchase_order_no?: string | null
    payment_request_no?: string | null
    basis: {
        purchase_order_id: string
        payment_request_id: string
        approved_goods_amount: string
        approved_freight: string
    }
}
export type DepositUsage = {
    id: string
    amount: string
    settled_amount: string
    returned_amount: string
    reversed_settlement_amount: string
    reservation_id: string
    ledger_entry_id: string
    occurred_at?: number | null
    external_order_id?: string | null
    purchase_order_no?: string | null
}
export type DepositLedgerEntry = {
    id: string
    kind: "recharge" | "debit" | "return" | "recharge_reversal"
    amount: string
    evidence: {
        external_transaction_id: string
        external_order_id: string
        evidence_reference: string
        occurred_at: number
    }
}
export type DepositReconciliation = {
    id: string
    version: number
    covered_from: number
    covered_until: number
    observed_at: number
    remote_balance: string
    local_balance_at_cutoff: string
    unresolved_transaction_ids: string[]
    evidence_reference: string
    resolved_by?: string | null
    resolution_reference?: string | null
    resolution?: {
        evidence_asset_id: string
    } | null
}
export type DepositBatch = {
    id: string
    amount: string
    reserved_amount: string
    consumed_amount: string
    returned_amount: string
    reversed_amount: string
    evidence: {
        bank_reference: string
        bank_receipt_asset_id?: string | null
        external_transaction_id: string
        external_evidence_reference: string
        received_at: number
    }
}
export type AccountDetail = AccountRow & {
    available_amount: string
    batches: DepositBatch[]
    reservations: DepositReservation[]
    usages: DepositUsage[]
    ledger: DepositLedgerEntry[]
    authorizations: DepositAuthorization[]
    reconciliations: DepositReconciliation[]
    recharge_requests: RechargeRequest[]
    payee_bank_accounts?: {
        id: string
        account_name: string
        bank_name: string
        account_number_masked: string
    }[]
}
export type DepositCommand = {
    accountId?: string
    path: string
    body: Record<string, unknown>
}
