import type { InvoiceRequest } from "./api"

/**
 * 提交成功后要打开的申请。
 * 不放进 invoice-requests 查询缓存：成功回调里的 invalidate 会重挂载面板，useState 跟着丢。
 */
let pendingDetailId: string | undefined
const submittedById = new Map<string, InvoiceRequest>()

/** 记下刚提交成功的申请，供重挂载后打开详情，并补齐范围详情缺少的版本与资料。 */
export function rememberSubmittedInvoiceRequest(request: InvoiceRequest): void {
    const id = request.id.trim()
    if (!id) return
    pendingDetailId = id
    submittedById.set(id, {
        ...request,
        id,
        data: { ...request.data },
    })
}

/** 刷新重挂载后仍应打开的申请。用户返回列表或重新发起申请后清空。 */
export function pendingInvoiceRequestDetailId(): string | undefined {
    return pendingDetailId
}

/** 只清导航锚点，保留提交快照，打开同一张申请时仍能补齐撤回所需版本。 */
export function dismissPendingInvoiceRequestDetail(): void {
    pendingDetailId = undefined
}

/** 刚提交的完整申请；详情查询用来先画出详情，避免范围行缺版本时丢掉撤回。 */
export function submittedInvoiceRequest(
    id: string,
): InvoiceRequest | undefined {
    return submittedById.get(id)
}

function textOr(primary: string, fallback: string): string {
    return primary.trim() ? primary : fallback
}

function amountOr(primary: string, fallback: string): string {
    if (primary !== "0.00" && primary !== "0" && primary !== "") return primary
    if (fallback !== "0.00" && fallback !== "0" && fallback !== "")
        return fallback
    return primary || fallback || "0.00"
}

/**
 * 范围详情没有版本和开票资料时，用提交响应补齐。
 * 服务端已经给出的状态保持不变，避免把真实草稿改成审批中。
 */
export function enrichSubmittedInvoiceRequest(
    request: InvoiceRequest,
    statusKnown: boolean,
): InvoiceRequest {
    const saved = request.id ? submittedById.get(request.id) : undefined
    if (!saved) return request
    return {
        ...request,
        version: request.version > 0 ? request.version : saved.version,
        request_no: textOr(request.request_no, saved.request_no),
        sales_order_id: textOr(request.sales_order_id, saved.sales_order_id),
        sales_order_no: textOr(request.sales_order_no, saved.sales_order_no),
        receivable_account_id: textOr(
            request.receivable_account_id,
            saved.receivable_account_id,
        ),
        customer_id: textOr(request.customer_id, saved.customer_id),
        counterparty_party_id: textOr(
            request.counterparty_party_id,
            saved.counterparty_party_id,
        ),
        created_by: textOr(request.created_by, saved.created_by),
        created_by_name: request.created_by_name ?? saved.created_by_name,
        created_at:
            request.created_at > 0 ? request.created_at : saved.created_at,
        status: statusKnown ? request.status : saved.status,
        data: {
            amount: amountOr(request.data.amount, saved.data.amount),
            invoice_title: textOr(
                request.data.invoice_title,
                saved.data.invoice_title,
            ),
            tax_number: textOr(request.data.tax_number, saved.data.tax_number),
            invoice_content: textOr(
                request.data.invoice_content,
                saved.data.invoice_content,
            ),
            reason: textOr(request.data.reason, saved.data.reason),
        },
        invoiced_amount: amountOr(
            request.invoiced_amount,
            saved.invoiced_amount,
        ),
        work_item_id: request.work_item_id ?? saved.work_item_id,
        approval: request.approval ?? saved.approval,
    }
}
