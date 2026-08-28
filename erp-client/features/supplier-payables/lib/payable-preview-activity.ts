/** 应付预览往来记录：把付款核销与进项核销合成一条时间线。 */

import {
    ALLOCATION_ACTION_LABEL,
    type PayableDetailView,
} from "@/features/supplier-payables/types"

export type PayableActivityTrack = "payment" | "purchase_invoice"

export type PayableActivityItem = Readonly<{
    id: string
    track: PayableActivityTrack
    trackLabel: string
    actionLabel: string
    documentNo: string
    href?: string
    amount: string
    occurredAt: string
}>

/**
 * 合并付款分配与进项票分配，按发生时间倒序；无时间的记录排在末尾。
 *
 * @param detail 应付详情中的两类分配行。
 */
export function buildPayableActivity(
    detail: Pick<
        PayableDetailView,
        "paymentAllocations" | "invoiceAllocations"
    >,
): readonly PayableActivityItem[] {
    const items: PayableActivityItem[] = [
        ...detail.paymentAllocations.map((allocation) => ({
            id: `payment:${allocation.allocationId}`,
            track: "payment" as const,
            trackLabel: "付款",
            actionLabel: ALLOCATION_ACTION_LABEL[allocation.action],
            documentNo: allocation.sourceDocumentNo,
            href: allocation.sourceHref,
            amount: allocation.amount,
            occurredAt: allocation.occurredAt,
        })),
        ...detail.invoiceAllocations.map((allocation) => ({
            id: `invoice:${allocation.allocationId}`,
            track: "purchase_invoice" as const,
            trackLabel: "进项发票",
            actionLabel: ALLOCATION_ACTION_LABEL[allocation.action],
            documentNo: allocation.sourceDocumentNo,
            href: allocation.sourceHref,
            amount: allocation.amountGross,
            occurredAt: allocation.occurredAt,
        })),
    ]
    return items.sort(compareActivityByTime)
}

function compareActivityByTime(
    left: PayableActivityItem,
    right: PayableActivityItem,
): number {
    const leftEmpty = !left.occurredAt.trim()
    const rightEmpty = !right.occurredAt.trim()
    if (leftEmpty && rightEmpty) return 0
    if (leftEmpty) return 1
    if (rightEmpty) return -1
    if (left.occurredAt === right.occurredAt) return 0
    return left.occurredAt < right.occurredAt ? 1 : -1
}
