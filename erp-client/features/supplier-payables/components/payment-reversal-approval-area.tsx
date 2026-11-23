"use client"

import { FinancialApprovalArea } from "@/features/financial-draft-edit/components/financial-approval-area"
import {
    PAYMENT_REVERSAL_DOCUMENT_TYPE,
    type PaymentReversalApprovalPhase,
} from "@/features/supplier-payables/lib/payment-reversal-approval"
import type {
    ApprovalCommandView,
    DocumentApprovalView,
} from "@/features/approval-workflow/types"

/**
 * 付款冲正单审批区。
 *
 * 未提交展示绑定卡，提交确认展示固定路线，运行中/终态展示摘要与历史。
 * 动作入口只读 `allowed_actions` 与 `recovery_options`，不复制审批状态推导。
 */
export function PaymentReversalApprovalArea({
    phase,
    approval,
    documentId,
    documentVersion,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
    presentation = "default",
}: {
    phase: PaymentReversalApprovalPhase
    approval?: DocumentApprovalView
    documentId?: string
    documentVersion?: number
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
    presentation?: "default" | "timeline" | "actions"
}) {
    return (
        <FinancialApprovalArea
            kind="payment_reversal"
            documentType={PAYMENT_REVERSAL_DOCUMENT_TYPE}
            phase={phase}
            approval={approval}
            documentId={documentId}
            documentVersion={documentVersion}
            workItemId={workItemId}
            expectedTaskVersion={expectedTaskVersion}
            workItemAllowedActions={workItemAllowedActions}
            onDecisionApplied={onDecisionApplied}
            surface="payment-reversal-approval-area"
            presentation={presentation}
        />
    )
}
