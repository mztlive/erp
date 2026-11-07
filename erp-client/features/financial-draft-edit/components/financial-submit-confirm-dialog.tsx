"use client"

import { useState, type ReactNode } from "react"

import { FormalActionConfirmDialog } from "@/components/business"
import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { FinancialSubmitRecovery } from "@/features/financial-draft-edit/components/financial-submit-recovery"
import type { FinancialApprovalKind } from "@/features/financial-draft-edit/components/financial-approval-area"

/** 四个提交确认壳共用的开关、单据和审批参数。 */
export type FinancialSubmitConfirmControls = {
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
}

/**
 * 逆向财务单据（退款、冲正）的提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function FinancialSubmitConfirmDialog({
    kind,
    actionLabel,
    effects,
    approvalArea,
    approval,
    open,
    pending,
    unknown = false,
    frozenReason,
    documentId,
    expectedVersion,
    onEndRecovery,
    onOpenChange,
    onConfirm,
    id,
    idPrefix,
    defaultId = "financial-submit-confirm",
}: FinancialSubmitConfirmControls & {
    kind: FinancialApprovalKind
    actionLabel: string
    effects: readonly ReactNode[]
    approvalArea: ReactNode
    defaultId?: string
}) {
    const baseId = idPrefix ?? id ?? defaultId
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
            actionLabel={actionLabel}
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
                            kind={kind}
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
                    {approvalArea}
                </div>
            }
            effects={effects}
            pending={pending || recovering}
            confirmDisabled={!unknown && !resubmission.valid}
            onConfirm={() => onConfirm(resubmission.reason)}
        />
    )
}
