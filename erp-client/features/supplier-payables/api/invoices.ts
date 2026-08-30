/**
 * W12 供应商往来 · 进项发票相关请求（登记+分配提交、红票）。
 * 正式幂等只由服务端命令收据保证。
 */

import { apiPost } from "@/lib/api"
import type {
    BackendInvoice,
    BackendPurchaseInvoiceAllocation,
} from "@/features/supplier-payables/api/mappers"
import { errorMessage } from "@/features/supplier-payables/api/shared"
import { compareDecimal } from "@/lib/fixed-decimal"
import type {
    FormalSubmitResult,
    PostInvoiceInput,
    ReverseInvoiceInput,
} from "@/features/supplier-payables/types"

export async function submitInvoice(
    input: PostInvoiceInput,
): Promise<FormalSubmitResult> {
    try {
        const targets = input.targets.filter((target) => {
            try {
                return (
                    Boolean(target.amount) &&
                    compareDecimal(target.amount, "0", 2) > 0
                )
            } catch {
                return false
            }
        })
        if (input.existingInvoiceId) {
            // Continue allocate: purchase-invoice-allocations is create+post only;
            // additional allocation on existing invoice is a backend gap if no partial post.
            return {
                status: "failed",
                title: "继续核销不可用",
                description:
                    "当前发票不能继续追加核销。请登记新发票，或联系管理员处理。",
                errorCode: "BACKEND_GAP",
                existingDocumentId: input.existingInvoiceId,
            }
        }

        if (targets.length === 0) {
            return {
                status: "failed",
                title: "缺少分配",
                description: "进项发票登记要求至少一条分配行。",
                errorCode: "ALLOCATION_REQUIRED",
            }
        }

        const registered = await apiPost<{
            invoice_id: string
            invoice_no: string
            gross_amount: string
            allocations: BackendPurchaseInvoiceAllocation[]
        }>("/admin/purchase-invoice-allocations", {
            invoice_code: input.invoiceCode || undefined,
            invoice_no: input.invoiceNo,
            invoice_date: input.invoiceDate,
            gross_amount: input.grossAmount,
            net_amount: input.netAmount,
            tax_amount: input.taxAmount,
            supplier_id: input.supplierId,
            idempotency_key: input.idempotencyKey,
            allocations: targets.map((t) => ({
                payable_account_id: t.payableAccountId,
                allocated_gross_amount: t.amount,
                allocated_net_amount: input.netAmount,
                allocated_tax_amount: input.taxAmount,
            })),
        })

        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "进项发票已登记",
            description: "进项发票与分配已过账。",
            reference: registered.invoice_no,
            operationId: input.idempotencyKey,
            documentNo: registered.invoice_no,
            allocatedTotal: registered.gross_amount,
            unallocatedAmount: "0.00",
        }
        return result
    } catch (err) {
        return {
            status: "failed",
            title: "进项发票失败",
            description: errorMessage(err, "进项发票提交失败"),
            errorCode: "HTTP_ERROR",
        }
    }
}

export async function reverseInvoice(
    input: ReverseInvoiceInput,
): Promise<FormalSubmitResult> {
    try {
        const red = await apiPost<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(input.invoiceId)}/red-issue`,
            {
                invoice_no: input.redInvoiceNo,
                reason: input.reason,
                idempotency_key: input.idempotencyKey,
            },
        )
        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "红票已登记",
            description: "已登记红票并反向分配，原蓝票保留。",
            reference: red.invoice_no,
            operationId: input.idempotencyKey,
            documentNo: red.invoice_no,
        }
        return result
    } catch (err) {
        return {
            status: "failed",
            title: "红票失败",
            description: errorMessage(err, "红票失败"),
            errorCode: "HTTP_ERROR",
        }
    }
}
