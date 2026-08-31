"use client"

import {
    SequentialProcessBar,
    type ResponsibilityStatus,
} from "@/components/business"

/**
 * 连续处理条：复核通过 / 通过并打开下一条 / 返回工作台。
 * 证据缺失时通过 onMissingEvidence 提示页面展示错误。
 */
export function QueueNavBar({
    current,
    total,
    responsibilityStatus,
    responsibilityStatusLabel,
    formalPending,
    evidenceOk,
    canApprove,
    onBack,
    onApprove,
    onMissingEvidence,
    id,
    idPrefix,
}: {
    current: number
    total: number
    responsibilityStatus: ResponsibilityStatus
    responsibilityStatusLabel: string | undefined
    formalPending: boolean
    evidenceOk: boolean
    canApprove: boolean
    onBack: () => void
    onApprove: (advance: boolean) => void
    onMissingEvidence: () => void
    id?: string
    idPrefix?: string
}) {
    return (
        <SequentialProcessBar
            id={id}
            idPrefix={idPrefix ?? "card-contracts-funds-review-queue-nav"}
            current={current}
            total={total}
            responsibilityStatus={responsibilityStatus}
            responsibilityStatusLabel={responsibilityStatusLabel}
            processLabel="复核通过"
            processNextLabel="通过并打开下一条"
            processDisabled={formalPending || !canApprove}
            pending={formalPending}
            backLabel="返回工作台"
            onBack={onBack}
            onProcess={() => {
                if (!evidenceOk) {
                    onMissingEvidence()
                    return
                }
                onApprove(false)
            }}
            onProcessNext={() => {
                if (!evidenceOk) {
                    onMissingEvidence()
                    return
                }
                onApprove(true)
            }}
        />
    )
}
