/** 客户往来范围行 wire 形态（M07/M08 销售方向；金额为字符串，null 表受限）。 */

import type {
    FundsScopedPageWire,
    FundsScopedResultWire,
    FundsSummaryWire,
} from "@/lib/funds-scope"

export type ScopedReceivableAccountWire = Readonly<{
    id: string
    sales_order_id: string
    account_seq: number
    status: string
    created_at: number
    visible_settled_share: string
    gross_total: string | null
    settled_total: string | null
    open_total: string | null
    permission_limited: boolean
    sales_owner_user_id: string | null
    business_org_unit_id: string | null
}>

export type FundsAllocationLineWire = Readonly<{
    id: string
    allocation_seq: number
    allocation_action: string
    receivable_entry_id?: string | null
    receivable_account_id?: string | null
    allocated_amount?: string | null
    allocated_gross_amount?: string | null
    allocated_net_amount?: string | null
    allocated_tax_amount?: string | null
    allocated_at?: number | null
    reverses_allocation_id?: string | null
}>

export type ScopedCustomerReceiptWire = Readonly<{
    id: string
    receipt_no: string
    status: string
    received_at: number
    created_at: number
    visible_allocated_share: string
    amount: string | null
    allocated_total: string | null
    unallocated_amount: string | null
    allocations: readonly FundsAllocationLineWire[] | null
    permission_limited: boolean
}>

export type ScopedInvoiceWire = Readonly<{
    id: string
    invoice_no: string
    invoice_direction: string
    invoice_kind: string
    status: string
    invoice_date: string
    created_at: number
    visible_allocated_share: string
    gross_amount: string | null
    allocated_total: string | null
    unallocated_amount: string | null
    allocations: readonly FundsAllocationLineWire[] | null
    purchase_allocations: readonly FundsAllocationLineWire[] | null
    permission_limited: boolean
}>

export type ReceivableScopeQuery = {
    view: "receivable" | "receipt" | "sales_invoice" | "unallocated"
    page: number
    pageSize: number
    /** 跨页回传的范围版本；第二页起必填。 */
    scopeVersion?: string
    /** 关联销售单当前负责销售（逗号分隔稳定 ID）。 */
    salesOwnerUserIds?: string
    /** 登记经办人（子账/发票）或登记/核销经办人（回款）。 */
    operatorUserIds?: string
    /** 回款经办人口径：登记或核销；提供经办人条件时必填。 */
    operatorKind?: "register" | "settle"
    /** 关联销售单当前业务组织（逗号分隔）。 */
    orgUnitIds?: string
    includeDescendants?: boolean
    q?: string
    status?: string
    counterpartyPartyId?: string
    customerId?: string
    due?: string
    salesOrderId?: string
    receivableAccountId?: string
}

export type ReceivableScopeListView = {
    view: ReceivableScopeQuery["view"]
    receivables: readonly ScopedReceivableAccountWire[]
    receipts: readonly ScopedCustomerReceiptWire[]
    invoices: readonly ScopedInvoiceWire[]
    total: number
    summary: FundsSummaryWire | undefined
    scopeVersion: string | undefined
    policyVersion: number | undefined
    organizationVersion: number | undefined
    scopeSummary: string | undefined
    asOf: string | undefined
    emptyReason: string | null | undefined
    hasScope: boolean
}

export type ReceivableScopeDetail =
    | { kind: "receivable"; receivable: ScopedReceivableAccountWire }
    | { kind: "receipt"; receipt: ScopedCustomerReceiptWire }
    | { kind: "invoice"; invoice: ScopedInvoiceWire }

export type FundsScopedReceivablePageWire =
    FundsScopedPageWire<ScopedReceivableAccountWire>
export type FundsScopedReceiptPageWire =
    FundsScopedPageWire<ScopedCustomerReceiptWire>
export type FundsScopedInvoicePageWire = FundsScopedPageWire<ScopedInvoiceWire>
export type FundsScopedReceivableResultWire =
    FundsScopedResultWire<ScopedReceivableAccountWire>
export type FundsScopedReceiptResultWire =
    FundsScopedResultWire<ScopedCustomerReceiptWire>
export type FundsScopedInvoiceResultWire =
    FundsScopedResultWire<ScopedInvoiceWire>
