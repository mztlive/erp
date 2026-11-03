"use client"

import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
import { FormalActionConfirmDialog } from "@/components/business"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { PurchaseOrderApprovalArea } from "@/features/purchase-orders/components/purchase-order-approval-area"

/**
 * 采购单提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function PurchaseOrderSubmitConfirmDialog({
    id,
    idPrefix,
    open,
    pending,
    approval,
    rejected = false,
    onOpenChange,
    onConfirm,
}: {
    id?: string
    idPrefix?: string
    open: boolean
    pending: boolean
    rejected?: boolean
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onConfirm: (resubmissionReason?: string) => void | Promise<void>
}) {
    const resubmitting = rejected || approval?.instance?.status === "REJECTED"
    const rework = useResubmissionReason({
        open,
        required: resubmitting,
        id: `${idPrefix ?? id ?? "purchase-submit"}-resubmission-reason`,
        disabled: pending,
    })
    return (
        <FormalActionConfirmDialog
            actionVariant="default"
            id={id}
            idPrefix={idPrefix}
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="提交采购单"
            confirmLabel="确认提交"
            fromStatus={{
                label: resubmitting ? "已驳回" : "草稿",
                tone: resubmitting ? "destructive" : "neutral",
            }}
            toStatus={{ label: "审批中", tone: "warning" }}
            description={
                <div className="space-y-3">
                    <p>
                        审批期间不可修改；驳回后可修改原单，填写重提说明后重新发起审批。
                    </p>
                    <PurchaseOrderApprovalArea
                        phase="confirm"
                        approval={approval}
                    />
                </div>
            }
            effects={["全部节点通过后形成采购生效版本"]}
            pending={pending}
            formContent={rework.field}
            confirmDisabled={!rework.valid}
            onConfirm={() => onConfirm(rework.reason)}
        />
    )
}
