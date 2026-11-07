"use client"

import {
    FinancialSubmitConfirmDialog,
    type FinancialSubmitConfirmControls,
} from "@/features/financial-draft-edit/components/financial-submit-confirm-dialog"
import { PaymentReversalApprovalArea } from "@/features/supplier-payables/components/payment-reversal-approval-area"

/**
 * 付款冲正提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function PaymentReversalSubmitConfirmDialog({
    approval,
    ...props
}: FinancialSubmitConfirmControls) {
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
            {...props}
            defaultId="supplier-payables-reversal-submit-confirm"
        />
    )
}
