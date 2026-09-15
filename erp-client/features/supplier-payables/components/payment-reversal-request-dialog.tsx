"use client"

import { FinancialRequestDialog } from "@/components/business/financial-request-dialog"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { PaymentReversalApprovalArea } from "@/features/supplier-payables/components/payment-reversal-approval-area"

/** Collect the reason and submit the original full-amount intent once. */
export function PaymentReversalRequestDialog({
    approval,
    ...props
}: {
    open: boolean
    pending: boolean
    sourceLabel?: string
    amount?: string
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onSubmit: (reason: string) => void | Promise<void>
}) {
    return (
        <FinancialRequestDialog
            {...props}
            id="supplier-payables-reversal-request"
            title="付款冲正"
            description="审批通过后冲减原付款记录，原记录保留。"
            submitLabel="提交冲正审批"
            approvalContent={
                approval ? (
                    <PaymentReversalApprovalArea
                        phase="draft"
                        approval={approval}
                    />
                ) : undefined
            }
        />
    )
}
