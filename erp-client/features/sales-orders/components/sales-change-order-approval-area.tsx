"use client"

import { DocumentApprovalArea } from "@/features/approval-workflow/components/document-approval-area"
import type {
    ApprovalCommandView,
    DocumentApprovalView,
} from "@/features/approval-workflow/types"
import {
    SALES_CHANGE_ORDER_DOCUMENT_TYPE,
    type SalesChangeOrderApprovalPhase,
} from "@/features/sales-orders/lib/sales-change-order-approval"

/**
 * 销售变更单审批区。
 *
 * 未提交展示绑定卡及已有审批历史，提交确认展示固定路线，运行中/终态展示摘要与历史。
 * 动作入口只读 `allowed_actions` 与 `recovery_options`，不恢复影响确认或财务复核专用决定。
 */
export function SalesChangeOrderApprovalArea({
    phase,
    approval,
    documentId,
    documentVersion,
    editDocumentHref,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    phase: SalesChangeOrderApprovalPhase
    approval?: DocumentApprovalView
    documentId?: string
    documentVersion?: number
    editDocumentHref?: string
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    return (
        <DocumentApprovalArea
            phase={phase}
            approval={approval}
            surface="sales-change-order-approval-area"
            documentType={SALES_CHANGE_ORDER_DOCUMENT_TYPE}
            documentId={documentId}
            documentVersion={documentVersion}
            editDocumentHref={editDocumentHref}
            actionBarId="sales-orders-change-approval-bar"
            workItemId={workItemId}
            expectedTaskVersion={expectedTaskVersion}
            workItemAllowedActions={workItemAllowedActions}
            onDecisionApplied={onDecisionApplied}
            compact
            historyInDraft
        />
    )
}
