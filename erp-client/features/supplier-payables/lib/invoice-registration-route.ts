import { nextSessionId } from "@/features/supplier-payables/api/shared"
import type { SessionState } from "@/features/supplier-payables/types"

export const INVOICE_REGISTRATION_ROUTE =
    "/finance/supplier-accounts/invoices/new"

/** 登记页只携带作业参数；列表查询条件完整保存在返回地址中。 */
export function invoiceRegistrationHref(
    session: SessionState,
    listHref: string,
) {
    const params = new URLSearchParams({
        supplierId: session.supplierId,
        returnTo: listHref,
    })
    const context = {
        draftSessionId: session.draftSessionId ?? nextSessionId(),
        invoiceId: session.existingInvoiceId,
        payableAccountId: session.preselectPayableAccountId,
        purchaseOrderId: session.purchaseOrderId,
    }
    for (const [key, value] of Object.entries(context)) {
        if (value) params.set(key, value)
    }
    return `${INVOICE_REGISTRATION_ROUTE}?${params}`
}

/** 返回供应商往来时保留筛选，拒绝外部地址和登记页循环。 */
export function invoiceReturnHref(value: string | null) {
    const listPath = "/finance/supplier-accounts"
    return value === listPath || value?.startsWith(`${listPath}?`)
        ? value
        : listPath
}
