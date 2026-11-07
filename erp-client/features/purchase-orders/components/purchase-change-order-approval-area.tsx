"use client"

import { DocumentApprovalArea } from "@/features/approval-workflow/components/document-approval-area"
import type {
    ApprovalCommandView,
    DocumentApprovalView,
} from "@/features/approval-workflow/types"
import {
    PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE,
    type PurchaseChangeOrderApprovalPhase,
} from "@/features/purchase-orders/lib/purchase-change-order-approval"

/**
 * 采购变更单审批区。
 *
 * 未提交展示绑定卡及已有审批历史，提交确认展示固定路线，运行中/终态展示摘要与历史。
 * 动作入口只读 `allowed_actions` 与 `recovery_options`，不恢复仓配影响或财务复核专用决定。
 */
export function PurchaseChangeOrderApprovalArea({
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
    phase: PurchaseChangeOrderApprovalPhase
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
            surface="purchase-change-order-approval-area"
            documentType={PURCHASE_CHANGE_ORDER_DOCUMENT_TYPE}
            documentId={documentId}
            documentVersion={documentVersion}
            editDocumentHref={editDocumentHref}
            workItemId={workItemId}
            expectedTaskVersion={expectedTaskVersion}
            workItemAllowedActions={workItemAllowedActions}
            onDecisionApplied={onDecisionApplied}
            runtimeClassName="space-y-3"
            historyInDraft
        />
    )
}
