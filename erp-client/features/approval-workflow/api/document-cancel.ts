import { apiGet, apiPost } from "@/lib/api"

import {
    mapDocumentApprovalViewDto,
    type ApprovalCommandView,
    type CancelApprovalRequest,
    type DocumentApprovalViewDto,
} from "../types"

const documentResources: Readonly<Record<string, readonly [string, string]>> = {
    SalesOrder: ["sales-orders", "expected_version"],
    VoucherSalesOrder: ["sales-orders", "expected_version"],
    voucher_sales_order: ["sales-orders", "expected_version"],
    sales_order: ["sales-orders", "expected_version"],
    SalesChangeOrder: ["sales-change-orders", "expected_version"],
    sales_change_order: ["sales-change-orders", "expected_version"],
    PurchaseOrder: ["purchase-orders", "expected_lock_version"],
    purchase_order: ["purchase-orders", "expected_lock_version"],
    PurchaseChangeOrder: ["purchase-change-orders", "expected_lock_version"],
    purchase_change_order: ["purchase-change-orders", "expected_lock_version"],
    CustomerReceipt: ["customer-receipts", "expected_version"],
    customer_receipt: ["customer-receipts", "expected_version"],
    CustomerRefund: ["customer-refunds", "expected_version"],
    customer_refund: ["customer-refunds", "expected_version"],
    SupplierRefund: ["supplier-refunds", "expected_version"],
    supplier_refund: ["supplier-refunds", "expected_version"],
    ReceiptReversal: ["receipt-reversals", "expected_version"],
    receipt_reversal: ["receipt-reversals", "expected_version"],
    PaymentReversal: ["payment-reversals", "expected_version"],
    payment_reversal: ["payment-reversals", "expected_version"],
}

export type CancelDocumentApprovalParams = Readonly<{
    documentType: string
    documentId: string
    documentVersion: number
    instanceId: string
    currentRoundNo: number
    request: CancelApprovalRequest
}>

/** 原单撤回必须调用拥有领域的端口，并使用页面已经读取的单据版本。 */
export async function cancelDocumentApproval(
    params: CancelDocumentApprovalParams,
): Promise<ApprovalCommandView> {
    const resource = documentResources[params.documentType]
    if (
        !resource ||
        !Number.isSafeInteger(params.documentVersion) ||
        params.documentVersion < 1
    ) {
        throw new Error("当前单据不能撤回，请刷新单据详情后重试。")
    }
    const [path, versionField] = resource
    const result = await apiPost<{
        status?: string
        approval?: DocumentApprovalViewDto | null
    } | null>(
        `/admin/${path}/${encodeURIComponent(params.documentId)}/cancel-approval`,
        {
            [versionField]: params.documentVersion,
            reason: params.request.reason.trim(),
            idempotency_key: params.request.idempotency_key,
        },
    )
    const approval = result?.approval
        ? mapDocumentApprovalViewDto(result.approval)
        : undefined
    return {
        instanceId: approval?.instance?.id ?? params.instanceId,
        instanceStatus: approval?.instance?.status ?? "CANCELLED",
        currentRoundNo:
            approval?.instance?.currentRoundNo ?? params.currentRoundNo,
        subjectStatus: result?.status ?? "draft",
        outcome: "APPLIED",
    }
}

/** 查询原单的当前权威状态，供未知撤回结果恢复后打开同一草稿。 */
export async function documentIsEditableDraft(
    documentType: string,
    documentId: string,
): Promise<boolean> {
    const resource = documentResources[documentType]
    if (!resource) throw new Error("请打开原单核对当前状态")
    const current = await apiGet<{
        id: string
        status?: string
        commercial_status?: string
    }>(`/admin/${resource[0]}/${encodeURIComponent(documentId)}`)
    return (
        current.id === documentId &&
        (current.status ?? current.commercial_status)?.toUpperCase() === "DRAFT"
    )
}
