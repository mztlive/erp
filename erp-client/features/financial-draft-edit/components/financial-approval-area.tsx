"use client"

import { financialDraftEditHref } from "@/features/financial-draft-edit/types"
import { FinancialDraftEditButton } from "@/features/financial-draft-edit/components/financial-draft-edit-button"
import type { FinancialDraftKind } from "@/features/financial-draft-edit/types"
import { ApprovalQueryFeedback } from "@/features/approval-workflow/components/approval-query-feedback"
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
    mergeDocumentAllowedActions,
    type ApprovalCommandView,
    type DocumentApprovalView,
} from "@/features/approval-workflow/types"

export type FinancialApprovalPhase = "draft" | "confirm" | "runtime"

/** 逆向财务单据共用审批区；回款单有专用读取，不走这里。 */
export type FinancialApprovalKind = Exclude<
    FinancialDraftKind,
    "customer_receipt"
>

/**
 * 逆向财务单据（退款、冲正）的审批区。
 *
 * 未提交展示绑定卡，提交确认展示固定路线，运行中/终态展示摘要与历史。
 * 动作入口只读 `allowed_actions` 与 `recovery_options`，不复制审批状态推导。
 */
export function FinancialApprovalArea({
    kind,
    documentType,
    phase,
    approval,
    documentId,
    documentVersion,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
    surface,
    historyId,
    actionBarId,
}: {
    kind: FinancialApprovalKind
    documentType: string
    phase: FinancialApprovalPhase
    approval?: DocumentApprovalView
    documentId?: string
    documentVersion?: number
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
    /** 审批查询反馈的稳定区块标识，用于定位失败来源。 */
    surface: string
    historyId?: string
    actionBarId?: string
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
    const allowedActions = mergeDocumentAllowedActions(
        approval?.allowedActions,
        workItemAllowedActions,
    )
    const editDocumentHref = documentId
        ? financialDraftEditHref(kind, documentId)
        : undefined

    if (phase === "draft") {
        return (
            <div className="space-y-3">
                <DefinitionBindingCard definition={approval?.definition} />
                {documentId ? (
                    <FinancialDraftEditButton
                        kind={kind}
                        documentId={documentId}
                    />
                ) : null}
                {documentId ? (
                    <ApprovalActionBar
                        id={actionBarId}
                        allowedActions={allowedActions}
                        definition={approval?.definition}
                        documentType={documentType}
                        documentId={documentId}
                        documentVersion={documentVersion}
                        editDocumentHref={editDocumentHref}
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
            <ApprovalQueryFeedback
                surface={surface}
                instanceId={instanceId}
                historyQuery={historyQuery}
                recoveryQuery={recoveryQuery}
            />
            <RuntimeSummary instance={approval?.instance} />
            {documentId ? (
                <FinancialDraftEditButton kind={kind} documentId={documentId} />
            ) : null}
            <ExecutionHistory
                id={historyId}
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
                id={actionBarId}
                allowedActions={allowedActions}
                recoveryOptions={
                    recoveryQuery.isError
                        ? []
                        : (recoveryQuery.data?.actions ?? [])
                }
                workItemId={workItemId}
                expectedTaskVersion={expectedTaskVersion}
                instance={approval?.instance}
                definition={approval?.definition}
                documentType={documentType}
                documentId={documentId}
                documentVersion={documentVersion}
                editDocumentHref={editDocumentHref}
                afterCancelStatusLabel="草稿"
                onDecisionApplied={onDecisionApplied}
            />
        </div>
    )
}
