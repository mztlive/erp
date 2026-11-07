"use client"

import {
    FinancialSubmitConfirmDialog,
    type FinancialSubmitConfirmControls,
} from "@/features/financial-draft-edit/components/financial-submit-confirm-dialog"
import { SupplierRefundApprovalArea } from "@/features/supplier-payables/components/supplier-refund-approval-area"

/**
 * 供应商退款提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function SupplierRefundSubmitConfirmDialog({
    approval,
    ...props
}: FinancialSubmitConfirmControls) {
    return (
        <FinancialSubmitConfirmDialog
            kind="supplier_refund"
            actionLabel="提交退款"
            effects={["全部节点通过后过账并入账"]}
            approvalArea={
                <SupplierRefundApprovalArea
                    phase="confirm"
                    approval={approval}
                />
            }
            approval={approval}
            {...props}
            defaultId="supplier-payables-refund-submit-confirm"
        />
    )
}
