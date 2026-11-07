"use client"

import { ReceiptReversalApprovalArea } from "@/features/customer-receivables/components/receipt-reversal-approval-area"
import {
    FinancialSubmitConfirmDialog,
    type FinancialSubmitConfirmControls,
} from "@/features/financial-draft-edit/components/financial-submit-confirm-dialog"

/**
 * 回款冲正提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function ReceiptReversalSubmitConfirmDialog({
    id = "customer-receivables-reversal-submit-confirm-dialog",
    approval,
    ...props
}: FinancialSubmitConfirmControls) {
    return (
        <FinancialSubmitConfirmDialog
            kind="receipt_reversal"
            actionLabel="提交冲正"
            effects={["全部节点通过后过账并冲减原回款"]}
            approvalArea={
                <ReceiptReversalApprovalArea
                    phase="confirm"
                    approval={approval}
                />
            }
            approval={approval}
            {...props}
            id={id}
        />
    )
}
