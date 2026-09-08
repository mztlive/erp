"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { PurchaseChangeOrderApprovalArea } from "@/features/purchase-orders/components/purchase-change-order-approval-area"

/**
 * 采购变更单提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function PurchaseChangeOrderSubmitConfirmDialog({
    id,
    idPrefix,
    open,
    pending,
    approval,
    onOpenChange,
    onConfirm,
}: {
    id?: string
    idPrefix?: string
    open: boolean
    pending: boolean
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onConfirm: () => void | Promise<void>
}) {
    return (
        <FormalActionConfirmDialog
            actionVariant="default"
            id={id}
            idPrefix={idPrefix}
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="提交改单"
            confirmLabel="确认提交"
            fromStatus={{ label: "草稿", tone: "neutral" }}
            toStatus={{ label: "审批中", tone: "warning" }}
            description={
                <div className="space-y-3">
                    <p>审批期间不可修改；驳回后重新提交将从首节点审批。</p>
                    <PurchaseChangeOrderApprovalArea
                        phase="confirm"
                        approval={approval}
                    />
                </div>
            }
            effects={["全部节点通过后生成新的采购版本"]}
            pending={pending}
            onConfirm={onConfirm}
        />
    )
}
