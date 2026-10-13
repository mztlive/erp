"use client"

import Link from "next/link"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import type { SalesOrderProcurementProgress } from "@/features/sales-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { purchaseSourceUpdateText } from "@/lib/ui-text"

/**
 * 销售变更影响采购时的逐张提示：受影响采购由系统派给采购负责人处理，
 * 审批中的采购已退回草稿，修改后的草稿在重新提交审批前仍列出；
 * 无采购查看权限时只展示张数。
 */
export function SalesOrderProcurementSourceNotice({
    id,
    progress,
}: {
    id: string
    progress: SalesOrderProcurementProgress
}) {
    const awaiting = progress.awaitingSubmissionCount
    if (!progress.sourceChanged && awaiting === 0) return null
    const updates = progress.sourceUpdates

    return (
        <Alert id={id} variant="warning">
            <AlertTitle>
                {progress.sourceChanged
                    ? purchaseSourceUpdateText.title
                    : purchaseSourceUpdateText.awaitingSubmissionTitle}
            </AlertTitle>
            <AlertDescription className="space-y-3">
                {progress.sourceChanged ? (
                    <p>
                        销售变更影响了{" "}
                        <span className="num">
                            {progress.affectedPurchaseCount}
                        </span>{" "}
                        张采购单，已派给采购负责人处理；审批中的采购已退回草稿。数量减少或归零的采购在处理完成前暂停付款。其他商品可继续分配供给。
                    </p>
                ) : null}
                {awaiting > 0 ? (
                    <p>
                        <span className="num">{awaiting}</span>{" "}
                        张采购单已按新销售版本修改，等待采购负责人提交审批。
                    </p>
                ) : null}
                {updates.length ? (
                    <ul className="divide-y divide-warning-border/60">
                        {updates.map((update) => (
                            <li
                                key={update.purchaseOrderId}
                                className="space-y-1 py-2 first:pt-0 last:pb-0"
                            >
                                <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                                    <Link
                                        id={`${id}-${toAutomationIdSegment(update.purchaseOrderId)}-link`}
                                        href={`/procurement/orders/${encodeURIComponent(update.purchaseOrderId)}`}
                                        className="num font-medium underline underline-offset-2"
                                    >
                                        {update.purchaseNo}
                                    </Link>
                                    <span>{update.statusLabel}</span>
                                    {update.supplierName ? (
                                        <span>· {update.supplierName}</span>
                                    ) : null}
                                    {update.ownerName ? (
                                        <span>· 负责人 {update.ownerName}</span>
                                    ) : null}
                                    {update.blocksPayment ? (
                                        <Badge variant="warning">
                                            {
                                                purchaseSourceUpdateText.paymentPaused
                                            }
                                        </Badge>
                                    ) : null}
                                    {update.awaitingSubmission ? (
                                        <Badge variant="outline">
                                            {
                                                purchaseSourceUpdateText.awaitingSubmission
                                            }
                                        </Badge>
                                    ) : null}
                                </div>
                                {update.awaitingSubmission ? null : (
                                    <ul className="space-y-0.5 text-xs">
                                        {update.reasons.map((reason) => (
                                            <li
                                                key={`${reason.lineNo}-${reason.description}`}
                                            >
                                                第 {reason.lineNo} 行{" "}
                                                {reason.itemName}：
                                                {reason.description}
                                            </li>
                                        ))}
                                    </ul>
                                )}
                            </li>
                        ))}
                    </ul>
                ) : (
                    <p className="text-xs">
                        {purchaseSourceUpdateText.noPurchasePermission}
                    </p>
                )}
            </AlertDescription>
        </Alert>
    )
}
