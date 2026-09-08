export type FinanceResponsibilityOperation =
    | "SUPPLIER_PAYMENT"
    | "SALES_INVOICE"

export type FinanceResponsibilityScope = "COUNTERPARTY" | "DEFAULT"

export type FinanceResponsibilityRule = Readonly<{
    id: string
    operation: FinanceResponsibilityOperation
    scope: FinanceResponsibilityScope
    counterpartyId?: string
    counterpartyNo?: string
    ownerUserId: string
    ownerName: string
    enabled: boolean
    version: number
}>

export type FinanceResponsibilityOwnerOption = Readonly<{
    userId: string
    displayName: string
    account: string
    supplierPaymentEligible: boolean
    salesInvoiceEligible: boolean
}>

export type SaveFinanceResponsibilityRuleInput = {
    id?: string
    operation: FinanceResponsibilityOperation
    scope: FinanceResponsibilityScope
    counterpartyId?: string
    ownerUserId: string
    enabled: boolean
    expectedVersion?: number
}

export const FINANCE_OPERATION_LABEL: Readonly<
    Record<FinanceResponsibilityOperation, string>
> = {
    SUPPLIER_PAYMENT: "供应商付款",
    SALES_INVOICE: "销项开票",
}

export const FINANCE_SCOPE_LABEL: Readonly<
    Record<FinanceResponsibilityScope, string>
> = {
    COUNTERPARTY: "指定往来方",
    DEFAULT: "默认负责人",
}
