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

export type DocumentApprovalPresentation = "default" | "timeline" | "actions"

/**
 * 单据审批区的公共骨架。
 *
 * 未提交展示绑定摘要，提交确认展示固定路线，运行中/终态展示摘要、历史与动作。
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
    presentation = "default",
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
    /** 对象中心 tab 或侧边抽屉内使用：标题与正文紧凑字号对齐。 */
    compact?: boolean
    /** 运行中/终态容器样式；各域页面的既有间距不同。 */
    runtimeClassName?: string
    /** 提交前也展示已有审批历史（变更单）。 */
    historyInDraft?: boolean
    /** 绑定摘要与动作区之间的域内入口。 */
    draftAddon?: React.ReactNode
    /** 审批摘要与历史之间的域内入口。 */
    runtimeHistoryAddon?: React.ReactNode
    /** 动作区之后的域内补充操作。 */
    runtimeFooter?: React.ReactNode
    /**
     * `"default"`：完整摘要、历史与动作；
     * `"timeline"`：仅渲染绑定/摘要与历史（供抽屉正文使用，不渲染底部动作）；
     * `"actions"`：仅内联渲染动作栏与草稿编辑入口（供 `TaskActionSheet` 页脚使用）。
     */
    presentation?: DocumentApprovalPresentation
}) {
    const instanceId = approval?.instance?.id
    const runtime = phase === "runtime"
    const recoveryQuery = useRecoveryOptionsQuery(
        instanceId,
        presentation !== "timeline" && runtime && Boolean(instanceId),
    )
    const historyQuery = useApprovalHistoryInfiniteQuery(
        { instanceId: instanceId ?? "" },
        presentation !== "actions" &&
            (runtime || historyInDraft) &&
            Boolean(instanceId),
    )
    const historyItems = historyQuery.data
        ? historyQuery.data.pages.flatMap((page) => page.items)
        : (approval?.recentHistory ?? [])
    const allowedActions = mergeDocumentAllowedActions(
        approval?.allowedActions,
        workItemAllowedActions,
    )
    const headingCompact = compact || presentation === "timeline"

    const historyBlock = (withRecovery: boolean, addon?: React.ReactNode) => (
        <>
            <ApprovalQueryFeedback
                surface={surface}
                instanceId={instanceId}
                historyQuery={historyQuery}
                recoveryQuery={withRecovery ? recoveryQuery : undefined}
            />
            <RuntimeSummary
                instance={approval?.instance}
                compact={headingCompact}
            />
            {addon}
            <div className="border-t border-border/60 pt-4">
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
                    compact={headingCompact}
                />
            </div>
        </>
    )

    if (phase === "confirm") {
        if (presentation === "actions") return null
        return <SubmissionRouteConfirmation definition={approval?.definition} />
    }

    if (presentation === "actions") {
        if (phase === "draft") {
            return (
                <div className="flex min-w-0 max-w-full flex-wrap items-center justify-end gap-2 empty:hidden">
                    {draftAddon}
                    {documentId ? (
                        <ApprovalActionBar
                            id={actionBarId}
                            presentation="sheet"
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

        return (
            <div className="flex min-w-0 max-w-full flex-wrap items-center justify-end gap-2 empty:hidden">
                <ApprovalQueryFeedback
                    surface={`${surface}-actions`}
                    instanceId={instanceId}
                    recoveryQuery={recoveryQuery}
                />
                {runtimeHistoryAddon}
                <ApprovalActionBar
                    id={actionBarId}
                    presentation="sheet"
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

    if (presentation === "timeline") {
        if (phase === "draft") {
            return (
                <div className="space-y-4">
                    <DefinitionBindingCard
                        definition={approval?.definition}
                        compact={headingCompact}
                    />
                    {historyInDraft && instanceId ? (
                        <div className="space-y-4 border-t border-border/60 pt-4">
                            {historyBlock(false)}
                        </div>
                    ) : null}
                </div>
            )
        }

        return <div className={runtimeClassName}>{historyBlock(false)}</div>
    }

    if (phase === "draft") {
        return (
            <div className="space-y-4">
                <DefinitionBindingCard
                    definition={approval?.definition}
                    compact={compact}
                />
                {historyInDraft && instanceId ? (
                    <div className="space-y-4 border-t border-border/60 pt-4">
                        {historyBlock(false)}
                    </div>
                ) : null}
                {draftAddon}
                {documentId ? (
                    <div className="border-t border-border/60 pt-4 empty:hidden">
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
                    </div>
                ) : null}
            </div>
        )
    }

    return (
        <div className={runtimeClassName}>
            {historyBlock(true, runtimeHistoryAddon)}
            <div className="border-t border-border/60 pt-4">
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
            </div>
            {runtimeFooter}
        </div>
    )
}
