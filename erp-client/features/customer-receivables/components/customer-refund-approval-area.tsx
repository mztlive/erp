"use client"

import { FinancialApprovalArea } from "@/features/financial-draft-edit/components/financial-approval-area"
import {
    CUSTOMER_REFUND_DOCUMENT_TYPE,
    type CustomerRefundApprovalPhase,
} from "@/features/customer-receivables/lib/customer-refund-approval"
import type {
    ApprovalCommandView,
    DocumentApprovalView,
} from "@/features/approval-workflow/types"

/**
 * 客户退款单审批区。
 *
 * 未提交展示绑定卡，提交确认展示固定路线，运行中/终态展示摘要与历史。
 * 动作入口只读 `allowed_actions` 与 `recovery_options`，不复制审批状态推导。
 */
export function CustomerRefundApprovalArea({
    phase,
    approval,
    documentId,
    documentVersion,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    phase: CustomerRefundApprovalPhase
    approval?: DocumentApprovalView
    documentId?: string
    documentVersion?: number
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    return (
        <FinancialApprovalArea
            kind="customer_refund"
            documentType={CUSTOMER_REFUND_DOCUMENT_TYPE}
            phase={phase}
            approval={approval}
            documentId={documentId}
            documentVersion={documentVersion}
            workItemId={workItemId}
            expectedTaskVersion={expectedTaskVersion}
            workItemAllowedActions={workItemAllowedActions}
            onDecisionApplied={onDecisionApplied}
            surface="customer-refund-approval-area"
            historyId="customer-receivables-refund-approval-history"
            actionBarId="customer-receivables-refund-approval-action-bar"
        />
    )
}
