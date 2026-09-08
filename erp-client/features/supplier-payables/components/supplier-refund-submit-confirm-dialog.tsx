"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { SupplierRefundApprovalArea } from "@/features/supplier-payables/components/supplier-refund-approval-area"

/**
 * 供应商退款提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function SupplierRefundSubmitConfirmDialog({
    open,
    pending,
    approval,
    onOpenChange,
    onConfirm,
    id,
    idPrefix,
}: {
    open: boolean
    pending: boolean
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onConfirm: () => void | Promise<void>
    id?: string
    idPrefix?: string
}) {
    return (
        <FormalActionConfirmDialog
            actionVariant="default"
            id={id}
            idPrefix={idPrefix ?? "supplier-payables-refund-submit-confirm"}
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="提交退款"
            confirmLabel="确认提交"
            fromStatus={{ label: "草稿", tone: "neutral" }}
            toStatus={{ label: "审批中", tone: "warning" }}
            description={
                <div className="space-y-3">
                    <p>审批期间不可修改；驳回后重新提交将从首节点审批。</p>
                    <SupplierRefundApprovalArea
                        phase="confirm"
                        approval={approval}
                    />
                </div>
            }
            effects={["全部节点通过后过账并入账"]}
            pending={pending}
            onConfirm={onConfirm}
        />
    )
}
