"use client"

import { FinancialSubmitConfirmDialog } from "@/features/financial-draft-edit/components/financial-submit-confirm-dialog"
import { PaymentReversalApprovalArea } from "@/features/supplier-payables/components/payment-reversal-approval-area"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"

/**
 * 付款冲正提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function PaymentReversalSubmitConfirmDialog({
    open,
    pending,
    unknown = false,
    frozenReason,
    documentId,
    expectedVersion,
    onEndRecovery,
    approval,
    onOpenChange,
    onConfirm,
    id,
    idPrefix,
}: {
    open: boolean
    pending: boolean
    unknown?: boolean
    frozenReason?: string
    documentId?: string
    expectedVersion?: number
    onEndRecovery?: () => void
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onConfirm: (resubmissionReason?: string) => void | Promise<void>
    id?: string
    idPrefix?: string
}) {
    return (
        <FinancialSubmitConfirmDialog
            kind="payment_reversal"
            actionLabel="提交冲正"
            effects={["全部节点通过后过账并冲减原付款"]}
            approvalArea={
                <PaymentReversalApprovalArea
                    phase="confirm"
                    approval={approval}
                />
            }
            approval={approval}
            open={open}
            pending={pending}
            unknown={unknown}
            frozenReason={frozenReason}
            documentId={documentId}
            expectedVersion={expectedVersion}
            onEndRecovery={onEndRecovery}
            onOpenChange={onOpenChange}
            onConfirm={onConfirm}
            id={id}
            idPrefix={idPrefix}
            defaultId="supplier-payables-reversal-submit-confirm"
        />
    )
}
