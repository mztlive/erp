"use client"

import { CustomerRefundApprovalArea } from "@/features/customer-receivables/components/customer-refund-approval-area"
import {
    FinancialSubmitConfirmDialog,
    type FinancialSubmitConfirmControls,
} from "@/features/financial-draft-edit/components/financial-submit-confirm-dialog"

/**
 * 客户退款提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function CustomerRefundSubmitConfirmDialog({
    id = "customer-receivables-refund-submit-confirm-dialog",
    approval,
    ...props
}: FinancialSubmitConfirmControls) {
    return (
        <FinancialSubmitConfirmDialog
            kind="customer_refund"
            actionLabel="提交退款"
            effects={["全部节点通过后过账并出账"]}
            approvalArea={
                <CustomerRefundApprovalArea
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
