import type { WorkspaceWorkItem } from "../types"

const INVOICE_REASON_CODES = new Set([
    "SALES_INVOICE_REQUEST_APPROVED",
    "RECEIVABLE_INVOICE_REQUIRED",
    "INVOICEABLE_REOPENED_BY_RED_INVOICE",
    "INVOICEABLE_REOPENED_BY_SALES_CHANGE",
])

export type WorkspaceInvoiceDescriptor = Readonly<{
    receivableAccountId: string
}>

/**
 * 把 W01 开票任务的服务端身份解析为唯一应收对象。
 * 任务类型、处理器、对象类型、责任角色与原因码任一不一致时失败关闭。
 */
export function workspaceInvoiceDescriptor(
    item: Pick<
        WorkspaceWorkItem,
        | "workItemType"
        | "handlerKey"
        | "businessObjectType"
        | "businessObjectId"
        | "ownerRole"
        | "reasonCode"
    >,
): WorkspaceInvoiceDescriptor | null {
    if (
        item.workItemType !== "SALES_INVOICE_EXECUTION" ||
        item.handlerKey !== "sales_invoice_execution"
    ) {
        return null
    }

    const objectType = item.businessObjectType.trim().toLowerCase()
    const receivableAccountId = item.businessObjectId.trim()
    if (
        objectType !== "receivable_account" ||
        item.ownerRole !== "role-finance" ||
        !INVOICE_REASON_CODES.has(item.reasonCode ?? "") ||
        !receivableAccountId
    ) {
        return null
    }

    return { receivableAccountId }
}

/** 只在本次正式开票覆盖提交前全部待开票金额时离开当前任务。 */
export function invoiceExecutionIsComplete(
    allocatedTotal: string,
    openInvoiceableTotalBeforePost: string,
): boolean {
    const allocated = amountInCents(allocatedTotal)
    const openBefore = amountInCents(openInvoiceableTotalBeforePost)
    return (
        allocated !== null &&
        openBefore !== null &&
        openBefore !== "0" &&
        (allocated.length > openBefore.length ||
            (allocated.length === openBefore.length &&
                allocated.localeCompare(openBefore) >= 0))
    )
}

function amountInCents(value: string): string | null {
    const match = /^(\d+)(?:\.(\d{1,2}))?$/.exec(value.trim())
    if (!match) return null
    const fraction = (match[2] ?? "").padEnd(2, "0")
    return `${match[1]}${fraction}`.replace(/^0+(?=\d)/, "")
}
