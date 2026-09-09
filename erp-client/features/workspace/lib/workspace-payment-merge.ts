import type { PaymentMergeCandidate } from "@/features/supplier-payables/types"
import { sumFixed } from "@/lib/fixed-decimal"

export type PaymentMergeTask = PaymentMergeCandidate

export type PaymentMergeSelectionItem = Readonly<{
    workItemId: string
    taskVersion: string
    payableAccountId: string
    openTotal: string
}>

/**
 * 打开合并弹窗时默认勾选全部候选；当前任务始终包含在内。
 */
export function defaultMergeSelectedIds(
    items: readonly PaymentMergeTask[],
): Set<string> {
    return new Set(items.map((item) => item.payableAccountId))
}

/**
 * 当前任务必须保留，且至少还要勾选一笔，才构成合并付款。
 */
export function canConfirmMergeSelection(
    selectedPayableIds: ReadonlySet<string>,
    items: readonly PaymentMergeTask[],
): boolean {
    const anchor = items.find((item) => item.isAnchor)
    if (!anchor || !selectedPayableIds.has(anchor.payableAccountId)) {
        return false
    }
    return (
        items.filter((item) => selectedPayableIds.has(item.payableAccountId))
            .length >= 2
    )
}

/**
 * 已勾选候选的未付合计，供弹窗展示一次打款金额。
 */
export function selectedMergeOpenTotal(
    selectedPayableIds: ReadonlySet<string>,
    items: readonly PaymentMergeTask[],
): string {
    const amounts = items
        .filter((item) => selectedPayableIds.has(item.payableAccountId))
        .map((item) => item.openTotal)
    return amounts.length === 0
        ? "0.00"
        : sumFixed(amounts, { maxScale: 2, outputScale: 2 })
}

/**
 * 把弹窗勾选结果变成提交用的附加任务，不含当前任务。
 */
export function additionalPaymentWorkItems(
    selectedPayableIds: ReadonlySet<string>,
    currentPayableAccountId: string,
    items: readonly PaymentMergeTask[],
): PaymentMergeSelectionItem[] {
    return items
        .filter(
            (item) =>
                selectedPayableIds.has(item.payableAccountId) &&
                item.payableAccountId !== currentPayableAccountId,
        )
        .map((item) => ({
            workItemId: item.workItemId,
            taskVersion: item.taskVersion,
            payableAccountId: item.payableAccountId,
            openTotal: item.openTotal,
        }))
}
