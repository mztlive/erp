"use client"

import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { DefinitionBindingCard } from "@/features/approval-workflow/components/definition-binding-card"
import { ExecutionHistory } from "@/features/approval-workflow/components/execution-history"
import { RuntimeSummary } from "@/features/approval-workflow/components/runtime-summary"
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import {
    useApprovalHistoryInfiniteQuery,
    useRecoveryOptionsQuery,
} from "@/features/approval-workflow/queries"
import {
    filterAllowedActions,
    type ApprovalAllowedAction,
    type ApprovalCommandView,
    type DocumentApprovalView,
} from "@/features/approval-workflow/types"
import { STOCK_ADJUSTMENT_DOCUMENT_TYPE } from "@/features/inventory/api/adjustment"
import { isDraftAdjustmentStatus } from "@/features/inventory/api/display"
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
 * 合并单据与当前任务的服务端动作白名单。只做并集过滤，不补默认动作。
 */
export const mergeAdjustmentAllowedActions = (
    documentActions?: readonly ApprovalAllowedAction[] | readonly string[],
    workItemActions?: readonly string[],
): readonly ApprovalAllowedAction[] =>
    filterAllowedActions([
        ...(documentActions ?? []),
        ...(workItemActions ?? []),
    ])

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
}: {
    phase: AdjustmentApprovalPhase
    approval?: DocumentApprovalView
    documentId?: string
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
    id?: string
    idPrefix?: string
}) {
    const instanceId = approval?.instance?.id
    const recoveryQuery = useRecoveryOptionsQuery(
        instanceId,
        phase === "runtime" && Boolean(instanceId),
    )
    const historyQuery = useApprovalHistoryInfiniteQuery(
        { instanceId: instanceId ?? "" },
        phase === "runtime" && Boolean(instanceId),
    )
    const historyItems = historyQuery.data
        ? historyQuery.data.pages.flatMap((page) => page.items)
        : (approval?.recentHistory ?? [])
    const allowedActions = mergeAdjustmentAllowedActions(
        approval?.allowedActions,
        workItemAllowedActions,
    )
    const derivedApprovalBarId =
        idPrefix ??
        id ??
        (documentId
            ? `inventory-adjustment-approval-bar-${toAutomationIdSegment(documentId)}`
            : "inventory-adjustment-approval-bar")

    if (phase === "draft") {
        return (
            <div className="space-y-3">
                <DefinitionBindingCard definition={approval?.definition} />
                {documentId ? (
                    <ApprovalActionBar
                        id={derivedApprovalBarId}
                        allowedActions={allowedActions}
                        definition={approval?.definition}
                        documentType={STOCK_ADJUSTMENT_DOCUMENT_TYPE}
                        documentId={documentId}
                    />
                ) : null}
            </div>
        )
    }

    if (phase === "confirm") {
        return <SubmissionRouteConfirmation definition={approval?.definition} />
    }

    return (
        <div className="space-y-3">
            <RuntimeSummary instance={approval?.instance} />
            <ExecutionHistory
                items={historyItems}
                hasMore={historyQuery.hasNextPage}
                loadingMore={historyQuery.isFetchingNextPage}
                onLoadMore={
                    historyQuery.hasNextPage
                        ? () => {
                              void historyQuery.fetchNextPage()
                          }
                        : undefined
                }
            />
            <ApprovalActionBar
                id={derivedApprovalBarId}
                allowedActions={allowedActions}
                recoveryOptions={recoveryQuery.data?.actions ?? []}
                workItemId={workItemId}
                expectedTaskVersion={expectedTaskVersion}
                instance={approval?.instance}
                definition={approval?.definition}
                documentType={STOCK_ADJUSTMENT_DOCUMENT_TYPE}
                documentId={documentId}
                afterCancelStatusLabel="草稿"
                onDecisionApplied={onDecisionApplied}
            />
        </div>
    )
}
