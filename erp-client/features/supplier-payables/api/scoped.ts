/** 供应商往来范围行 wire 形态（M09；金额为字符串，null 表受限）。 */

import type {
    FundsScopedPageWire,
    FundsScopedResultWire,
    FundsSummaryWire,
} from "@/lib/funds-scope"

export type ScopedPayableAccountWire = Readonly<{
    id: string
    source_document_id: string
    source_document_no?: string | null
    source_type: string
    supplier_id: string
    status: string
    created_at: number
    visible_settled_share: string
    gross_total: string | null
    settled_total: string | null
    open_total: string | null
    permission_limited: boolean
    procurement_owner_user_id: string | null
    business_org_unit_id: string | null
}>

export type FundsPaymentAllocationWire = Readonly<{
    id: string
    supplier_payment_id?: string | null
    supplier_deposit_usage_id?: string | null
    allocation_seq: number
    allocation_action: "apply" | "reverse"
    payable_entry_id: string
    payable_account_id: string | null
    source_type: string | null
    source_document_id: string | null
    source_document_no: string | null
    allocated_amount: string
    allocated_at: number
    reverses_allocation_id: string | null
}>

export type ScopedSupplierPaymentWire = Readonly<{
    id: string
    payment_no: string
    status: string
    supplier_id: string
    paid_at: number
    created_at: number
    visible_allocated_share: string
    amount: string | null
    allocated_total: string | null
    unallocated_amount: string | null
    allocations: readonly FundsPaymentAllocationWire[] | null
    permission_limited: boolean
}>

export type ScopedPurchaseInvoiceAllocationWire = Readonly<{
    id: string
    invoice_id: string
    invoice_no: string | null
    payable_account_id: string
    payable_no?: string | null
    source_document_no?: string | null
    created_at: number
    visible_allocated_amount: string
    allocated_gross_amount: string | null
    permission_limited: boolean
}>

export type SupplierScopeQuery = {
    view: "payable" | "payment" | "purchase_invoice"
    page: number
    pageSize: number
    /** 跨页回传的范围版本；第二页起必填。 */
    scopeVersion?: string
    /** 来源采购单当前采购负责人（逗号分隔稳定 ID）。 */
    procurementOwnerUserIds?: string
    /** 付款/收票经办人（逗号分隔稳定 ID）。 */
    operatorUserIds?: string
    /** 来源采购单当前业务组织（逗号分隔）。 */
    orgUnitIds?: string
    includeDescendants?: boolean
    q?: string
    supplierId?: string
    purchaseOrderId?: string
    payableAccountId?: string
}

export type SupplierScopeListView = {
    view: SupplierScopeQuery["view"]
    payables: readonly ScopedPayableAccountWire[]
    payments: readonly ScopedSupplierPaymentWire[]
    allocations: readonly ScopedPurchaseInvoiceAllocationWire[]
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

export type SupplierScopeDetail =
    | { kind: "payable"; payable: ScopedPayableAccountWire }
    | { kind: "payment"; payment: ScopedSupplierPaymentWire }

export type FundsScopedPayablePageWire =
    FundsScopedPageWire<ScopedPayableAccountWire>
export type FundsScopedPaymentPageWire =
    FundsScopedPageWire<ScopedSupplierPaymentWire>
export type FundsScopedPurchaseAllocationPageWire =
    FundsScopedPageWire<ScopedPurchaseInvoiceAllocationWire>
export type FundsScopedPayableResultWire =
    FundsScopedResultWire<ScopedPayableAccountWire>
export type FundsScopedPaymentResultWire = ScopedSupplierPaymentWire &
    Omit<FundsScopedResultWire<ScopedSupplierPaymentWire>, "data"> &
    Readonly<{ effective_amount?: string }>
