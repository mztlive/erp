"use client"

import { useState } from "react"
import { ChevronRightIcon } from "lucide-react"

import { BusinessFailureState } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Timeline,
    TimelineHeader,
    TimelineItem,
    TimelineMarker,
    TimelineTime,
    TimelineTitle,
} from "@/components/ui/timeline"
import { ApprovalRoute } from "@/features/approval-workflow/components/approval-route"
import { ApprovalCopyHistory } from "@/features/approval-workflow/components/copy-history"
import { groupByRound } from "@/features/approval-workflow/components/execution-history"
import {
    displayActorName,
    displayExecutionStatus,
    displayInstanceStatus,
    displayProcessVersion,
    displayReadableName,
    displayRound,
    displayUnixSeconds,
    executionStatusTone,
    instanceStatusTone,
    isBlockedStatus,
    isOpenInstanceStatus,
} from "@/features/approval-workflow/display"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import type {
    ApprovalDefinitionBinding,
    ApprovalHistoryItem,
    ApprovalRuntimeInstance,
    DocumentApprovalView,
} from "@/features/approval-workflow/types"
import { getErrorPresentation } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { PurchaseOrderDetailSection } from "./purchase-order-detail-section"

const HISTORY_IMPACT = "采购单详情仍可查看。"

/**
 * 采购单只读审批分区：纯白无套娃布局，与销售单审批面板保持同一视觉语言。
 * 展示当前审批摘要、完整审批链条、按轮次倒序的审批记录及抄送记录。
 */
export function PurchaseOrderDetailApprovalSection({
    approval,
    idPrefix = "purchase-order-approval",
}: {
    approval?: DocumentApprovalView
    idPrefix?: string
}) {
    const instance = approval?.instance
    const history = useApprovalHistoryInfiniteQuery(
        { instanceId: instance?.id ?? "", order: "desc" },
        Boolean(instance),
        HISTORY_IMPACT,
    )

    if (!approval) {
        return (
            <PurchaseOrderDetailSection width="form" title="审批进度与记录">
                <p className="text-xs text-muted-foreground">暂无审批记录。</p>
            </PurchaseOrderDetailSection>
        )
    }

    if (!instance) {
        return (
            <PurchaseOrderDetailSection width="form" title="审批流程">
                <FlatDefinitionBinding definition={approval.definition} />
            </PurchaseOrderDetailSection>
        )
    }

    const historyFailure = history.isError
        ? getErrorPresentation(history.error)
        : undefined
    const items =
        history.data?.pages.flatMap((page) => page.items) ??
        (approval.historyHasMore ? [] : approval.recentHistory)
    const hasMore = history.hasNextPage ?? approval.historyHasMore
    const route = history.data?.pages[0]?.route

    return (
        <PurchaseOrderDetailSection width="form" title="审批进度与记录">
            <div className="space-y-6">
                <FlatRuntimeSummary instance={instance} idPrefix={idPrefix} />

                {route ? (
                    <section className="space-y-4" aria-label="本次审批路线">
                        <h3 className="text-body-compact font-semibold text-foreground">
                            审批链条
                            <span className="num ml-2 text-xs font-normal text-muted-foreground">
                                共 {route.nodes.length} 个节点
                            </span>
                        </h3>
                        <ApprovalRoute route={route} orientation="vertical" />
                    </section>
                ) : null}

                {history.isError ? (
                    <BusinessFailureState
                        id={`${idPrefix}-history-retry`}
                        title={
                            items.length > 0 ? "审批记录刷新失败" : undefined
                        }
                        readContext={{
                            subject: "审批记录",
                            impact: HISTORY_IMPACT,
                        }}
                        error={history.error}
                        onRetry={() => void history.refetch()}
                    />
                ) : null}

                {history.isPending && items.length === 0 ? (
                    <p
                        role="status"
                        className="py-2 text-xs text-muted-foreground"
                    >
                        正在读取审批记录…
                    </p>
                ) : items.length > 0 ? (
                    <FlatApprovalHistory
                        key={`${instance.id}-${instance.currentRoundNo}`}
                        instance={instance}
                        items={items}
                        hasMore={hasMore}
                        idPrefix={idPrefix}
                    />
                ) : !history.isError ? (
                    <p className="text-xs text-muted-foreground">
                        暂无审批记录。
                    </p>
                ) : null}

                {history.hasNextPage && !historyFailure?.accessDenied ? (
                    <Button
                        id={`${idPrefix}-history-load-more`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={history.isFetchingNextPage}
                        onClick={() => void history.fetchNextPage()}
                    >
                        {history.isFetchingNextPage
                            ? "正在读取…"
                            : "加载更早记录"}
                    </Button>
                ) : null}

                <ApprovalCopyHistory
                    instanceId={instance.id}
                    idPrefix={`${idPrefix}-copies`}
                />
            </div>
        </PurchaseOrderDetailSection>
    )
}

function FlatDefinitionBinding({
    definition,
}: {
    definition?: ApprovalDefinitionBinding
}) {
    if (!definition) {
        return (
            <p className="text-xs text-muted-foreground">尚未绑定审批流程。</p>
        )
    }
    const nodes =
        definition.nodes.length > 0
            ? definition.nodes
            : definition.publishedNodes
    return (
        <div className="space-y-3">
            <p className="text-xs text-muted-foreground">
                提交后将按{" "}
                <span className="font-medium text-foreground">
                    {displayProcessVersion({
                        name: definition.name,
                        version: definition.version,
                        id: definition.id,
                    })}
                </span>{" "}
                依次审批。
            </p>
            {nodes.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                    当前流程尚未配置审批节点。
                </p>
            ) : (
                <ol className="divide-y divide-border/60 border-y border-border/60">
                    {nodes.map((node, index) => {
                        const assigneeName = displayActorName(node.assigneeName)
                        return (
                            <li
                                key={node.key}
                                className="flex flex-wrap items-baseline justify-between gap-2 py-2.5 text-body-compact"
                            >
                                <span className="flex items-baseline gap-2 font-medium text-foreground">
                                    <span className="num text-xs text-muted-foreground">
                                        {index + 1}.
                                    </span>
                                    <span>
                                        {displayReadableName(
                                            node.name,
                                            node.key,
                                        ) ?? "审批节点未标注"}
                                    </span>
                                </span>
                                {assigneeName ? (
                                    <span className="text-xs text-muted-foreground">
                                        {assigneeName}
                                    </span>
                                ) : null}
                            </li>
                        )
                    })}
                </ol>
            )}
        </div>
    )
}

function FlatRuntimeSummary({
    instance,
    idPrefix,
}: {
    instance: ApprovalRuntimeInstance
    idPrefix: string
}) {
    const inProgress = isOpenInstanceStatus(instance.status)
    const blocked = isBlockedStatus(instance.status)
    const node = displayReadableName(
        instance.currentNodeName,
        instance.currentNode,
    )
    const assignee = displayActorName(
        instance.currentAssigneeName,
        instance.currentAssignee,
    )
    const rejectionBy = displayActorName(
        instance.latestRejectionByName,
        instance.latestRejectionBy,
    )

    return (
        <section
            aria-label="当前审批"
            className="space-y-3 border-b border-border/60 pb-5"
        >
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1.5">
                    <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5">
                        <h3 className="text-body-compact font-semibold text-foreground">
                            {inProgress
                                ? node || "当前审批"
                                : node || "审批结果"}
                        </h3>
                        <StatusBadge
                            tone={instanceStatusTone(instance.status)}
                            label={displayInstanceStatus(instance.status)}
                        />
                        {inProgress ? (
                            <span className="text-body-compact text-foreground">
                                {assignee || "审批人待确认"}
                            </span>
                        ) : null}
                    </div>
                    <p className="text-xs text-muted-foreground">
                        {blocked
                            ? "当前审批无法继续，请查看下方受阻原因。"
                            : inProgress
                              ? "等待当前审批人处理，办理结果将在记录中更新。"
                              : "本次审批已结束，处理意见见下方记录。"}
                    </p>
                </div>
                <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 text-xs text-muted-foreground">
                    <span className="num">
                        {displayRound(instance.currentRoundNo)}
                    </span>
                    <details className="group max-w-full">
                        <summary
                            id={`${idPrefix}-process-info`}
                            className="cursor-pointer rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        >
                            流程信息
                        </summary>
                        <p className="mt-1.5 break-words">
                            {displayProcessVersion({
                                name: instance.processName,
                                version: instance.processVersion,
                            })}
                        </p>
                    </details>
                </div>
            </div>

            {instance.latestRejection ? (
                <Alert variant="destructive">
                    <AlertTitle>
                        最近驳回{rejectionBy ? ` · ${rejectionBy}` : ""}
                    </AlertTitle>
                    <AlertDescription className="whitespace-pre-wrap break-words">
                        {instance.latestRejection}
                    </AlertDescription>
                </Alert>
            ) : null}

            {blocked ? (
                <Alert variant="destructive">
                    <AlertTitle>审批受阻</AlertTitle>
                    <AlertDescription>
                        {instance.blockerMessage ??
                            "当前审批无法继续，请按系统给出的恢复方式处理。"}
                    </AlertDescription>
                </Alert>
            ) : null}
        </section>
    )
}

function FlatApprovalHistory({
    instance,
    items,
    hasMore,
    idPrefix,
}: {
    instance: ApprovalRuntimeInstance
    items: readonly ApprovalHistoryItem[]
    hasMore: boolean
    idPrefix: string
}) {
    const rounds = [...groupByRound(items)].reverse()
    return (
        <section aria-label="审批记录" className="space-y-2">
            <h3 className="text-body-compact font-semibold text-foreground">
                审批记录
            </h3>
            <div className="divide-y divide-border/60">
                {rounds.map((round, index) => (
                    <ApprovalRound
                        key={round.roundNo}
                        instance={instance}
                        idPrefix={idPrefix}
                        roundNo={round.roundNo}
                        items={round.items}
                        defaultExpanded={index === 0}
                        incomplete={hasMore && index === rounds.length - 1}
                    />
                ))}
            </div>
        </section>
    )
}

function ApprovalRound({
    instance,
    roundNo,
    items,
    defaultExpanded,
    incomplete,
    idPrefix,
}: {
    instance: ApprovalRuntimeInstance
    roundNo: number
    items: readonly ApprovalHistoryItem[]
    defaultExpanded: boolean
    incomplete: boolean
    idPrefix: string
}) {
    const [expanded, setExpanded] = useState(defaultExpanded)
    const current = roundNo === instance.currentRoundNo
    const rejection = items.some((item) => item.result === "REJECTED")
    const cancelled = items.some((item) => item.result === "CANCELLED")
    const label = current
        ? displayInstanceStatus(instance.status)
        : rejection
          ? "已驳回"
          : cancelled
            ? "已撤回"
            : "已处理"
    const tone = current
        ? instanceStatusTone(instance.status)
        : rejection
          ? "destructive"
          : "neutral"
    const lastDecision = items.findLast((item) => item.decidedAt)
    const time = displayUnixSeconds(lastDecision?.decidedAt)

    return (
        <details
            className="group/round py-1"
            open={expanded}
            onToggle={(event) => setExpanded(event.currentTarget.open)}
        >
            <summary
                id={`${idPrefix}-round-${roundNo}`}
                className="flex cursor-pointer list-none flex-wrap items-center gap-x-3 gap-y-2 rounded-md py-2.5 text-body-compact focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden"
            >
                <ChevronRightIcon
                    aria-hidden="true"
                    className="size-4 shrink-0 text-muted-foreground transition-transform group-open/round:rotate-90"
                />
                <span className="num font-medium">{displayRound(roundNo)}</span>
                <StatusBadge tone={tone} label={label} appearance="text" />
                {time ? (
                    <time
                        dateTime={time.dateTime}
                        className="num text-xs text-muted-foreground sm:ml-auto"
                    >
                        {time.label}
                    </time>
                ) : null}
            </summary>
            <div className="pb-4 pl-3 sm:pl-7">
                {incomplete ? (
                    <p className="mb-3 text-xs text-muted-foreground">
                        此处仅展示已加载记录，更多记录可继续加载。
                    </p>
                ) : null}
                <Timeline aria-label={`${displayRound(roundNo)}审批记录`}>
                    {items.map((item) => (
                        <ApprovalEntry
                            key={item.executionId}
                            item={item}
                            idPrefix={idPrefix}
                        />
                    ))}
                </Timeline>
            </div>
        </details>
    )
}

function ApprovalEntry({
    item,
    idPrefix,
}: {
    item: ApprovalHistoryItem
    idPrefix: string
}) {
    const actor = displayActorName(item.assigneeName, item.decidedBy)
    const time = displayUnixSeconds(item.decidedAt)
    const reason = item.decisionReason?.trim()
    const longReason =
        reason && (reason.length > 160 || reason.split("\n").length > 3)

    return (
        <TimelineItem className="min-w-0 pb-4">
            <TimelineMarker aria-hidden="true">
                <span className="size-1.5 rounded-full bg-muted-foreground" />
            </TimelineMarker>
            <TimelineHeader className="gap-x-3">
                <TimelineTitle className="break-words">
                    {displayReadableName(item.nodeName, item.nodeKey) ||
                        "审批节点未标注"}
                </TimelineTitle>
                <StatusBadge
                    tone={executionStatusTone(item.result)}
                    label={displayExecutionStatus(item.result)}
                    appearance="text"
                />
                {actor ? (
                    <span className="text-body-compact text-muted-foreground">
                        {actor}
                    </span>
                ) : null}
                {time ? (
                    <TimelineTime dateTime={time.dateTime}>
                        {time.label}
                    </TimelineTime>
                ) : null}
            </TimelineHeader>
            {reason ? (
                longReason ? (
                    <details className="group/reason mt-1.5 text-body-compact text-muted-foreground">
                        <summary
                            id={`${idPrefix}-reason-${toAutomationIdSegment(item.executionId)}`}
                            className="cursor-pointer rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        >
                            查看完整处理意见
                            <span className="mt-1 line-clamp-2 whitespace-pre-wrap break-words group-open/reason:hidden">
                                {reason}
                            </span>
                        </summary>
                        <p className="mt-1.5 whitespace-pre-wrap break-words">
                            {reason}
                        </p>
                    </details>
                ) : (
                    <p className="mt-1.5 whitespace-pre-wrap break-words text-body-compact text-muted-foreground">
                        {reason}
                    </p>
                )
            ) : null}
        </TimelineItem>
    )
}
