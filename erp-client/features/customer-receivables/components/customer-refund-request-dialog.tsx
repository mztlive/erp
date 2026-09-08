"use client"

import { FinancialRequestDialog } from "@/components/business/financial-request-dialog"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { CustomerRefundApprovalArea } from "@/features/customer-receivables/components/customer-refund-approval-area"

/** Collect the reason and submit the original full-amount intent once. */
export function CustomerRefundRequestDialog({
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
            id="customer-receivables-refund-request"
            title="客户退款"
            description="审批通过后登记客户全额退款，原记录保留。"
            submitLabel="提交退款审批"
            approvalContent={
                approval ? (
                    <CustomerRefundApprovalArea
                        phase="draft"
                        approval={approval}
                    />
                ) : undefined
            }
        />
    )
}
