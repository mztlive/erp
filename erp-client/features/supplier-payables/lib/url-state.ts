/** W12 供应商往来 · URL 查询参数解析（纯函数）。 */

import type { SupplierAccountsView } from "@/features/supplier-payables/types"

export type SupplierAccountsPreviewKind =
    | "payable"
    | "payment"
    | "refund"
    | "reversal"

/**
 * 解析详情预览种类。缺省或未知值按应付台账处理。
 *
 * @param raw URL `previewKind` 参数。
 */
export function parsePreviewKind(
    raw: string | null,
): SupplierAccountsPreviewKind {
    if (raw === "payment" || raw === "refund" || raw === "reversal") return raw
    return "payable"
}

/**
 * 读取工作台深链任务主键。`currentWorkItemId` 优先于 `workItemId`。
 *
 * @param searchParams 当前 URL 查询。
 */
export function parseWorkItemId(
    searchParams: Pick<URLSearchParams, "get">,
): string | undefined {
    return (
        searchParams.get("currentWorkItemId") ??
        searchParams.get("workItemId") ??
        undefined
    )
}

export function parseView(raw: string | null): SupplierAccountsView {
    if (
        raw === "payment" ||
        raw === "purchase_invoice" ||
        raw === "unallocated" ||
        raw === "payable"
    ) {
        return raw
    }
    return "payable"
}
