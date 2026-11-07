"use client"

import { DocumentApprovalArea } from "@/features/approval-workflow/components/document-approval-area"
import {
    mergeDocumentAllowedActions,
    type ApprovalCommandView,
} from "@/features/approval-workflow/types"
import { STOCK_ADJUSTMENT_DOCUMENT_TYPE } from "@/features/inventory/api/adjustment"
import { isDraftAdjustmentStatus } from "@/features/inventory/api/display"
import { CancelAdjustmentApprovalDialog } from "@/features/inventory/components/cancel-adjustment-approval-dialog"
import type { StockAdjustmentApprovalView } from "@/features/inventory/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export type AdjustmentApprovalPhase = "draft" | "confirm" | "runtime"

/**
 * 按库存调整生命周期选择审批区相位。提交确认由调用方显式传入。
 */
export const adjustmentApprovalPhase = (
    status?: string,
): Exclude<AdjustmentApprovalPhase, "confirm"> =>
    isDraftAdjustmentStatus(status) ? "draft" : "runtime"

/**
 * 库存调整试点审批区。
 *
 * 未提交展示绑定卡，提交确认展示固定路线，运行中/终态展示摘要与历史。
 * 动作入口只读 `allowed_actions` 与 `recovery_options`。
 */
export function AdjustmentApprovalArea({
    phase,
    approval,
    documentId,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
    id,
    idPrefix,
    onEditOriginal,
    onCancelled,
}: {
    phase: AdjustmentApprovalPhase
    approval?: StockAdjustmentApprovalView
    documentId?: string
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
    id?: string
    idPrefix?: string
    onEditOriginal?: (stockAdjustmentId: string) => void
    onCancelled?: () => void
}) {
    const derivedApprovalBarId =
        idPrefix ??
        id ??
        (documentId
            ? `inventory-adjustment-approval-bar-${toAutomationIdSegment(documentId)}`
            : "inventory-adjustment-approval-bar")
    const cancelCommand = approval?.cancelCommand
    const showCancelApproval =
        Boolean(documentId) &&
        mergeDocumentAllowedActions(
            approval?.allowedActions,
            workItemAllowedActions,
        ).includes("CANCEL") &&
        Boolean(cancelCommand)

    return (
        <DocumentApprovalArea
            phase={phase}
            approval={approval}
            surface="adjustment-approval-area"
            documentType={STOCK_ADJUSTMENT_DOCUMENT_TYPE}
            documentId={documentId}
            actionBarId={derivedApprovalBarId}
            hiddenActions={["CANCEL", "CANCEL_APPROVAL"]}
            workItemId={workItemId}
            expectedTaskVersion={expectedTaskVersion}
            workItemAllowedActions={workItemAllowedActions}
            onDecisionApplied={onDecisionApplied}
            runtimeClassName="space-y-3"
            runtimeFooter={
                showCancelApproval && documentId && cancelCommand ? (
                    <div className="flex flex-wrap gap-2">
                        <CancelAdjustmentApprovalDialog
                            id={`${derivedApprovalBarId}-cancel-approval`}
                            stockAdjustmentId={documentId}
                            command={cancelCommand}
                            currentNodeName={
                                approval?.instance?.currentNodeName
                            }
                            reviseRejected={Boolean(
                                approval?.instance?.latestRejection,
                            )}
                            onCancelled={(stockAdjustmentId) => {
                                onCancelled?.()
                                if (approval?.instance?.latestRejection)
                                    onEditOriginal?.(stockAdjustmentId)
                            }}
                        />
                    </div>
                ) : null
            }
        />
    )
}
