"use client"

import { DocumentApprovalArea } from "@/features/approval-workflow/components/document-approval-area"
import type {
    ApprovalCommandView,
    DocumentApprovalView,
} from "@/features/approval-workflow/types"
import {
    VOUCHER_SALES_ORDER_DOCUMENT_TYPE,
    type VoucherSalesOrderApprovalPhase,
} from "@/features/sales-orders/lib/voucher-sales-order-approval"

/**
 * 卡券销售单审批区。
 *
 * 未提交展示绑定卡，提交确认展示固定路线，运行中/终态展示摘要与历史。
 * 动作入口只读 `allowed_actions` 与 `recovery_options`，不恢复卡券专用决定。
 */
export function VoucherSalesOrderApprovalArea({
    phase,
    approval,
    documentId,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    phase: VoucherSalesOrderApprovalPhase
    approval?: DocumentApprovalView
    documentId?: string
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    return (
        <DocumentApprovalArea
            phase={phase}
            approval={approval}
            surface="voucher-sales-order-approval-area"
            documentType={VOUCHER_SALES_ORDER_DOCUMENT_TYPE}
            documentId={documentId}
            actionBarId="sales-orders-voucher-approval-bar"
            hiddenActions={["CANCEL", "CANCEL_APPROVAL"]}
            workItemId={workItemId}
            expectedTaskVersion={expectedTaskVersion}
            workItemAllowedActions={workItemAllowedActions}
            onDecisionApplied={onDecisionApplied}
            compact
        />
    )
}
