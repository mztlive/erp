"use client"

import { DocumentApprovalArea } from "@/features/approval-workflow/components/document-approval-area"
import type {
    ApprovalCommandView,
    DocumentApprovalView,
} from "@/features/approval-workflow/types"
import {
    PURCHASE_ORDER_DOCUMENT_TYPE,
    type PurchaseOrderApprovalPhase,
} from "@/features/purchase-orders/lib/purchase-order-approval"

/**
 * 采购单审批区。
 *
 * 未提交展示绑定卡，提交确认展示固定路线，运行中/终态展示摘要与历史。
 * 动作入口只读 `allowed_actions` 与 `recovery_options`，不复制审批状态推导。
 */
export function PurchaseOrderApprovalArea({
    phase,
    approval,
    documentId,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    phase: PurchaseOrderApprovalPhase
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
            surface="purchase-order-approval-area"
            documentType={PURCHASE_ORDER_DOCUMENT_TYPE}
            documentId={documentId}
            hiddenActions={["CANCEL", "CANCEL_APPROVAL"]}
            workItemId={workItemId}
            expectedTaskVersion={expectedTaskVersion}
            workItemAllowedActions={workItemAllowedActions}
            onDecisionApplied={onDecisionApplied}
            compact
        />
    )
}
