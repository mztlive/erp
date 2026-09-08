"use client"

import { FinancialRequestDialog } from "@/components/business/financial-request-dialog"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { SupplierRefundApprovalArea } from "@/features/supplier-payables/components/supplier-refund-approval-area"

/** Collect the reason and submit the original full-amount intent once. */
export function SupplierRefundRequestDialog({ approval, ...props }: {
    open: boolean
    pending: boolean
    sourceLabel?: string
    amount?: string
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onSubmit: (reason: string) => void | Promise<void>
}) {
    return <FinancialRequestDialog {...props} id="supplier-payables-refund-request" title="供应商退款" description="审批通过后登记供应商退回的全额款项，原记录保留。" submitLabel="提交退款审批" approvalContent={approval ? <SupplierRefundApprovalArea phase="draft" approval={approval} /> : undefined} />
}
