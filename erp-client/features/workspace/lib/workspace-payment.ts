import type { WorkspaceWorkItem } from "../types"

const PAYMENT_REASON_CODES = new Set([
    "PAYABLE_PAYMENT_REQUIRED",
    "PAYABLE_REOPENED_BY_REVERSAL",
])

export type WorkspacePaymentDescriptor = Readonly<{
    payableAccountId: string
    purchaseOrderId: string
}>

/**
 * 把 W01 付款任务的服务端身份解析为唯一应付对象。
 * 任务类型、处理器、对象类型、责任角色与原因码任一不一致时失败关闭。
 */
export function workspacePaymentDescriptor(
    item: Pick<
        WorkspaceWorkItem,
        | "workItemType"
        | "handlerKey"
        | "businessObjectType"
        | "businessObjectId"
        | "rootBusinessObjectId"
        | "ownerRole"
        | "reasonCode"
    >,
): WorkspacePaymentDescriptor | null {
    if (
        item.workItemType !== "SUPPLIER_PAYMENT_EXECUTION" ||
        item.handlerKey !== "supplier_payment_execution"
    ) {
        return null
    }

    const objectType = item.businessObjectType.trim().toLowerCase()
    const payableAccountId = item.businessObjectId.trim()
    const purchaseOrderId = item.rootBusinessObjectId?.trim() ?? ""
    if (
        objectType !== "payable_account" ||
        item.ownerRole !== "role-finance" ||
        !PAYMENT_REASON_CODES.has(item.reasonCode ?? "") ||
        !payableAccountId ||
        !purchaseOrderId ||
        purchaseOrderId === payableAccountId
    ) {
        return null
    }

    return { payableAccountId, purchaseOrderId }
}

/**
 * 核对应付子账与工作项冻结的采购来源是否一致。
 */
export function workspacePaymentMatchesPayable(
    descriptor: WorkspacePaymentDescriptor,
    payable: {
        payableAccountId: string
        sourceType: string
        sourceDocumentId: string
    },
): boolean {
    return (
        payable.payableAccountId === descriptor.payableAccountId &&
        payable.sourceType === "PURCHASE_ORDER" &&
        payable.sourceDocumentId === descriptor.purchaseOrderId
    )
}
