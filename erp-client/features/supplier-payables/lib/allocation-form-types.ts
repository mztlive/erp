/** W12 供应商往来 · 核销记录表单值类型与表单实例类型（供拆分子表单组件使用）。 */

import type { AppFormApi } from "@/components/form/form-api"

export type PaymentFormValues = {
    paidAt: string
    amount: string
    bankReference: string
    bankReceiptAssetId: string
    bankReceipt: File | null
    note: string
}

export type InvoiceFormValues = {
    invoiceFiles: File[]
    allocationTaxes: Record<string, string>
    invoiceCode: string
    invoiceNo: string
    invoiceDate: string
    grossAmount: string
    netAmount: string
    taxAmount: string
}

/** 本机进项发票草稿只保存票面字段和显式分配，不保存权限或服务器版本。 */
export type InvoiceRegistrationSnapshot = {
    invoice: InvoiceFormValues
    selected: string[]
    amounts: Record<string, string>
}

/** 核销记录表单实例的共享类型；拆分组件只关心字段与值。 */
export type PaymentFormApi = AppFormApi<PaymentFormValues>

export type InvoiceFormApi = AppFormApi<InvoiceFormValues>
