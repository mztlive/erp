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
import type {
    ApprovalCommandView,
    DocumentApprovalView,
} from "@/features/approval-workflow/types"
import {
    RECEIPT_REVERSAL_DOCUMENT_TYPE,
    mergeReceiptReversalAllowedActions,
    type ReceiptReversalApprovalPhase,
} from "@/features/customer-receivables/lib/receipt-reversal-approval"

/**
 * 回款冲正单审批区。
 *
 * 未提交展示绑定卡，提交确认展示固定路线，运行中/终态展示摘要与历史。
 * 动作入口只读 `allowed_actions` 与 `recovery_options`，不复制审批状态推导。
 */
export function ReceiptReversalApprovalArea({
    phase,
    approval,
    documentId,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
}: {
    phase: ReceiptReversalApprovalPhase
    approval?: DocumentApprovalView
    documentId?: string
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
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
    const allowedActions = mergeReceiptReversalAllowedActions(
        approval?.allowedActions,
        workItemAllowedActions,
    )

    if (phase === "draft") {
        return (
            <div className="space-y-3">
                <DefinitionBindingCard definition={approval?.definition} />
                {documentId ? (
                    <ApprovalActionBar
                        id="customer-receivables-reversal-approval-action-bar"
                        allowedActions={allowedActions}
                        definition={approval?.definition}
                        documentType={RECEIPT_REVERSAL_DOCUMENT_TYPE}
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
                id="customer-receivables-reversal-approval-history"
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
                id="customer-receivables-reversal-approval-action-bar"
                allowedActions={allowedActions}
                recoveryOptions={recoveryQuery.data?.actions ?? []}
                workItemId={workItemId}
                expectedTaskVersion={expectedTaskVersion}
                instance={approval?.instance}
                definition={approval?.definition}
                documentType={RECEIPT_REVERSAL_DOCUMENT_TYPE}
                documentId={documentId}
                afterCancelStatusLabel="草稿"
                onDecisionApplied={onDecisionApplied}
            />
        </div>
    )
}
