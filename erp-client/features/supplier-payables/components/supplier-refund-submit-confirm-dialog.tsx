"use client"

import { useState } from "react"

import { FinancialSubmitRecovery } from "@/features/financial-draft-edit/components/financial-submit-recovery"

import { FormalActionConfirmDialog } from "@/components/business"
import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
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
    unknown = false,
    frozenReason,
    documentId,
    expectedVersion,
    onEndRecovery,
    approval,
    onOpenChange,
    onConfirm,
    id,
    idPrefix,
}: {
    open: boolean
    pending: boolean
    unknown?: boolean
    frozenReason?: string
    documentId?: string
    expectedVersion?: number
    onEndRecovery?: () => void
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onConfirm: (resubmissionReason?: string) => void | Promise<void>
    id?: string
    idPrefix?: string
}) {
    const baseId = idPrefix ?? id ?? "supplier-payables-refund-submit-confirm"
    const [recovering, setRecovering] = useState(false)
    const rejected = approval?.instance?.status === "REJECTED"
    const resubmission = useResubmissionReason({
        open,
        required: rejected || (unknown && frozenReason !== undefined),
        id: `${baseId}-resubmission-reason`,
        disabled: pending || unknown,
        initialReason: unknown ? frozenReason : undefined,
    })
    return (
        <FormalActionConfirmDialog
            actionVariant="default"
            id={id}
            idPrefix={baseId}
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="提交退款"
            confirmLabel={unknown ? "重试本次提交" : "确认提交"}
            fromStatus={{
                label: rejected ? "已驳回" : "草稿",
                tone: rejected ? "destructive" : "neutral",
            }}
            toStatus={{ label: "审批中", tone: "warning" }}
            description={
                <div className="space-y-3">
                    {resubmission.field}
                    {unknown &&
                    documentId &&
                    expectedVersion !== undefined &&
                    onEndRecovery ? (
                        <FinancialSubmitRecovery
                            key={`${documentId}:${expectedVersion}`}
                            kind="supplier_refund"
                            documentId={documentId}
                            expectedVersion={expectedVersion}
                            idPrefix={baseId}
                            pending={pending}
                            onEnd={onEndRecovery}
                            onBusyChange={setRecovering}
                        />
                    ) : null}
                    {unknown ? (
                        <p
                            role="status"
                            className="text-sm text-muted-foreground"
                        >
                            本次提交结果待确认，说明已保留。可重试本次提交，或暂时关闭后核对原单；再次打开仍使用本次内容。
                        </p>
                    ) : null}
                    <p>
                        审批期间不可修改；驳回后可修改原单，填写重提说明后重新发起审批。
                    </p>
                    <SupplierRefundApprovalArea
                        phase="confirm"
                        approval={approval}
                    />
                </div>
            }
            effects={["全部节点通过后过账并入账"]}
            pending={pending || recovering}
            confirmDisabled={!unknown && !resubmission.valid}
            onConfirm={() => onConfirm(resubmission.reason)}
        />
    )
}
