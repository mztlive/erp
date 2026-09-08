"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { CustomerReceiptApprovalArea } from "@/features/customer-receivables/components/customer-receipt-approval-area"

/**
 * 客户回款提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function CustomerReceiptSubmitConfirmDialog({
    open,
    pending,
    approval,
    onOpenChange,
    onConfirm,
    id = "customer-receivables-session-receipt-confirm-dialog",
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
            id={idPrefix ?? id}
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="提交回款"
            confirmLabel="确认提交"
            fromStatus={{ label: "草稿", tone: "neutral" }}
            toStatus={{ label: "审批中", tone: "warning" }}
            description={
                <div className="space-y-3">
                    <p>审批期间不可修改；驳回后重新提交将从首节点审批。</p>
                    <CustomerReceiptApprovalArea
                        phase="confirm"
                        approval={approval}
                    />
                </div>
            }
            effects={["全部节点通过后过账并核销"]}
            pending={pending}
            onConfirm={onConfirm}
        />
    )
}
