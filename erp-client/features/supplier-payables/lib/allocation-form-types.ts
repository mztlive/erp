/** W12 供应商往来 · 核销记录表单值类型与表单实例类型（供拆分子表单组件使用）。 */

import type { FieldComponent, ReactFormExtendedApi } from "@tanstack/react-form"

export type PaymentFormValues = {
    paidAt: string
    amount: string
    bankReference: string
    bankReceiptAssetId: string
    bankReceipt: File | null
    note: string
}

export type InvoiceFormValues = {
    invoiceCode: string
    invoiceNo: string
    invoiceDate: string
    grossAmount: string
    netAmount: string
    taxAmount: string
}

/**
 * 核销记录表单实例的共享类型。校验槽位用 `any`：各拆分组件只关心字段与值，
 * 不需要复述 TanStack Form 的完整校验泛型。
 */
export type PaymentFormApi = ReactFormExtendedApi<
    PaymentFormValues,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any
> & {
    AppField: FieldComponent<
        PaymentFormValues,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any
    >
}

export type InvoiceFormApi = ReactFormExtendedApi<
    InvoiceFormValues,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any
> & {
    AppField: FieldComponent<
        InvoiceFormValues,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any
    >
}
