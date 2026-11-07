"use client"

import type * as React from "react"

import { ApprovalActionBar } from "./approval-action-bar"
import { ApprovalQueryFeedback } from "./approval-query-feedback"
import { DefinitionBindingCard } from "./definition-binding-card"
import { ExecutionHistory } from "./execution-history"
import { RuntimeSummary } from "./runtime-summary"
import { SubmissionRouteConfirmation } from "./submission-route-confirmation"
import {
    useApprovalHistoryInfiniteQuery,
    useRecoveryOptionsQuery,
} from "../queries"
import {
    mergeDocumentAllowedActions,
    type ApprovalCommandView,
    type DocumentApprovalView,
} from "../types"

export type DocumentApprovalPhase = "draft" | "confirm" | "runtime"

/**
 * 单据审批区的公共骨架。
 *
 * 未提交展示绑定卡，提交确认展示固定路线，运行中/终态展示摘要、历史与动作。
 * 各域只传入单据类型、稳定 id 与少量差异插槽，审批状态推导仍由服务端
 * `allowed_actions` 与 `recovery_options` 决定。
 */
export function DocumentApprovalArea({
    phase,
    approval,
    surface,
    documentType,
    documentId,
    documentVersion,
    editDocumentHref,
    actionBarId,
    historyId,
    hiddenActions,
    workItemId,
    expectedTaskVersion,
    workItemAllowedActions,
    onDecisionApplied,
    compact = false,
    runtimeClassName = "flex flex-col gap-4",
    historyInDraft = false,
    draftAddon,
    runtimeHistoryAddon,
    runtimeFooter,
}: {
    phase: DocumentApprovalPhase
    approval?: DocumentApprovalView
    /** 审批查询反馈的稳定区块标识，用于定位失败来源。 */
    surface: string
    documentType: string
    documentId?: string
    documentVersion?: number
    editDocumentHref?: string
    actionBarId?: string
    historyId?: string
    hiddenActions?: readonly string[]
    workItemId?: string
    expectedTaskVersion?: string
    workItemAllowedActions?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
    /** 对象中心 tab 内使用：标题与概览 text-sm 对齐。 */
    compact?: boolean
    /** 运行中/终态容器样式；各域页面的既有间距不同。 */
    runtimeClassName?: string
    /** 提交前也展示已有审批历史（变更单）。 */
    historyInDraft?: boolean
    /** 绑定卡与动作区之间的域内入口。 */
    draftAddon?: React.ReactNode
    /** 审批摘要与历史之间的域内入口。 */
    runtimeHistoryAddon?: React.ReactNode
    /** 动作区之后的域内补充操作。 */
    runtimeFooter?: React.ReactNode
}) {
    const instanceId = approval?.instance?.id
    const runtime = phase === "runtime"
    const recoveryQuery = useRecoveryOptionsQuery(
        instanceId,
        runtime && Boolean(instanceId),
    )
    const historyQuery = useApprovalHistoryInfiniteQuery(
        { instanceId: instanceId ?? "" },
        (runtime || historyInDraft) && Boolean(instanceId),
    )
    const historyItems = historyQuery.data
        ? historyQuery.data.pages.flatMap((page) => page.items)
        : (approval?.recentHistory ?? [])
    const allowedActions = mergeDocumentAllowedActions(
        approval?.allowedActions,
        workItemAllowedActions,
    )

    const historyBlock = (withRecovery: boolean, addon?: React.ReactNode) => (
        <>
            <ApprovalQueryFeedback
                surface={surface}
                instanceId={instanceId}
                historyQuery={historyQuery}
                recoveryQuery={withRecovery ? recoveryQuery : undefined}
            />
            <RuntimeSummary instance={approval?.instance} compact={compact} />
            {addon}
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
                compact={compact}
            />
        </>
    )

    if (phase === "draft") {
        return (
            <div className="space-y-3">
                <DefinitionBindingCard
                    definition={approval?.definition}
                    compact={compact}
                />
                {historyInDraft && instanceId ? historyBlock(false) : null}
                {draftAddon}
                {documentId ? (
                    <ApprovalActionBar
                        id={actionBarId}
                        allowedActions={allowedActions}
                        definition={approval?.definition}
                        documentType={documentType}
                        documentId={documentId}
                        documentVersion={documentVersion}
                        editDocumentHref={editDocumentHref}
                        hiddenActions={hiddenActions}
                    />
                ) : null}
            </div>
        )
    }

    if (phase === "confirm") {
        return <SubmissionRouteConfirmation definition={approval?.definition} />
    }

    return (
        <div className={runtimeClassName}>
            {historyBlock(true, runtimeHistoryAddon)}
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
                hiddenActions={hiddenActions}
                onDecisionApplied={onDecisionApplied}
            />
            {runtimeFooter}
        </div>
    )
}
