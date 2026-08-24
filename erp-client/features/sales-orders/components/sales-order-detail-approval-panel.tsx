"use client"

import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SalesOrderApprovalArea } from "@/features/sales-orders/components/sales-order-approval-area"
import { VoucherSalesOrderApprovalArea } from "@/features/sales-orders/components/voucher-sales-order-approval-area"
import { salesOrderApprovalPhase } from "@/features/sales-orders/lib/sales-order-approval"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import { voucherSalesOrderApprovalPhase } from "@/features/sales-orders/lib/voucher-sales-order-approval"

export function ApprovalPanel({
    order,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onApprovalResult,
}: {
    order: SalesOrderDetailView
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onApprovalResult?: (result: SalesOrderDetailActionResult) => void
}) {
    if (!order.approval) {
        return (
            <p className="text-sm text-muted-foreground">本单暂无审批绑定。</p>
        )
    }

    if (order.nature === "card_voucher") {
        return (
            <VoucherSalesOrderApprovalArea
                phase={voucherSalesOrderApprovalPhase(
                    order.approval,
                    order.primaryStatus.code,
                )}
                approval={order.approval}
                documentId={order.id}
                workItemId={workItemId}
                expectedTaskVersion={expectedTaskVersion}
                workItemAllowedActions={workItemAllowedActions}
                onDecisionApplied={(view: ApprovalCommandView) =>
                    onApprovalResult?.({
                        status: "succeeded",
                        title: "审批决定已提交",
                        description: view.latestRejectionReason
                            ? `已按当前任务提交决定。${view.latestRejectionReason}`
                            : "已按当前任务提交决定。",
                        reference: order.documentNumber,
                        nextResponsible: view.currentAssigneeName,
                    })
                }
            />
        )
    }

    return (
        <SalesOrderApprovalArea
            phase={salesOrderApprovalPhase(
                order.approval,
                order.primaryStatus.code,
            )}
            approval={order.approval}
            documentId={order.id}
            workItemId={workItemId}
            expectedTaskVersion={expectedTaskVersion}
            workItemAllowedActions={workItemAllowedActions}
            onDecisionApplied={(view: ApprovalCommandView) =>
                onApprovalResult?.({
                    status: "succeeded",
                    title: "审批决定已提交",
                    description: view.latestRejectionReason
                        ? `已按当前任务提交决定。${view.latestRejectionReason}`
                        : "已按当前任务提交决定。",
                    reference: order.documentNumber,
                    nextResponsible: view.currentAssigneeName,
                })
            }
        />
    )
}
