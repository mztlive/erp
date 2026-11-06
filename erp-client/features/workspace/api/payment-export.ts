import { listWorkItems } from "@/features/work-items/api"
import {
    fetchRequest,
    revealPaymentRecipient,
} from "@/features/purchase-payment-requests/api"
import type {
    PaymentRequest,
    RevealedPaymentRecipient,
} from "@/features/purchase-payment-requests/types"
import type { WorkspaceUrlState } from "../lib/url-state"

export type PaymentExportRow = {
    request: PaymentRequest
    recipient?: RevealedPaymentRecipient
}

/** 沿当前筛选逐页读取本人付款任务；任一记录失效时整批停止，不生成部分文件。 */
export async function fetchPaymentExport(state: WorkspaceUrlState) {
    if (state.view !== "inbox")
        throw new Error("请在待我处理的采购付款队列导出")
    const items = []
    const seen = new Set<string>()
    let scopeVersion: string | undefined
    let total: number | undefined
    for (let page = 1; ; page++) {
        const result = await listWorkItems({
            scope: "mine",
            filters: {
                ...state.filters,
                item_types: ["task:SUPPLIER_PAYMENT_EXECUTION"],
            },
            query: state.query,
            sort: state.sort,
            timezone: "Asia/Shanghai",
            page,
            pageSize: 100,
            scopeVersion,
        })
        scopeVersion ??= result.scope_version
        total ??= result.total
        if (result.total !== total || result.scope_version !== scopeVersion)
            throw new Error("待付款记录已变化，请刷新列表后重新导出")
        for (const item of result.items) {
            if (
                seen.has(item.id) ||
                item.work_item_type !== "SUPPLIER_PAYMENT_EXECUTION" ||
                item.business_object_type !== "purchase_payment_request"
            )
                throw new Error("付款队列数据不一致，请刷新列表后重新导出")
            seen.add(item.id)
            items.push(item)
        }
        if (items.length === total) break
        if (!result.items.length || items.length > total)
            throw new Error("付款记录未读取完整，请刷新列表后重新导出")
    }
    if (!items.length) throw new Error("当前筛选没有可导出的待付款记录")
    const rows: PaymentExportRow[] = []
    for (let start = 0; start < items.length; start += 4) {
        rows.push(
            ...(await Promise.all(
                items.slice(start, start + 4).map(async (item) => {
                    const request = await fetchRequest(
                        item.business_object_id,
                        item.id,
                    )
                    if (
                        request.id !== item.business_object_id ||
                        String(request.submission_version) !==
                            item.subject_version ||
                        !request.allowed_actions.includes("PAY")
                    )
                        throw new Error(
                            `${request.request_no} 已变化或不可付款，请刷新后重试`,
                        )
                    const recipient =
                        request.execution_method.type === "bank_transfer"
                            ? await revealPaymentRecipient(request, {
                                  workItemId: item.id,
                                  taskVersion: String(item.task_version),
                              })
                            : undefined
                    return { request, recipient }
                }),
            )),
        )
    }
    return rows
}
