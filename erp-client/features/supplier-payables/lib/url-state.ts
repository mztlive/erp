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
    if (
        raw === "payment" ||
        raw === "refund" ||
        raw === "reversal" ||
        raw === "payable"
    ) {
        return raw
    }
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

/**
 * 切换工作视图时写入 URL 的补丁：回第 1 页，并清掉目标视图不使用的筛选。
 *
 * 应付专用条件：`sourceType` / `status` / `due` / `paymentGate`。
 * 待核销专用条件：`track`。
 * 关键词、供应商、采购单来源锁定保留。
 *
 * @param nextView 目标工作视图。
 */
export function patchForViewChange(
    nextView: SupplierAccountsView,
): Record<string, string | null | undefined> {
    const patch: Record<string, string | null | undefined> = {
        view: nextView,
        page: null,
    }
    if (nextView !== "payable") {
        patch.sourceType = null
        patch.status = null
        patch.due = null
        patch.paymentGate = null
    }
    if (nextView !== "unallocated") {
        patch.track = null
    }
    return patch
}
