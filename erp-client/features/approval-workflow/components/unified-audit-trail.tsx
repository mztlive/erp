"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { useInfiniteQuery } from "@tanstack/react-query"
import {
    BanIcon,
    CircleAlertIcon,
    CircleCheckIcon,
    CircleDashedIcon,
    CircleDotIcon,
    MailIcon,
    RefreshCwIcon,
    TriangleAlertIcon,
    UserRoundIcon,
    type LucideIcon,
} from "lucide-react"

import { OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    StatusBadge,
    statusToneMarkerClass,
    type StatusTone,
} from "@/components/ui/status-badge"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
    Timeline,
    TimelineDescription,
    TimelineHeader,
    TimelineItem,
    TimelineMarker,
    TimelineTime,
    TimelineTitle,
} from "@/components/ui/timeline"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import type { CopyTrigger } from "@/features/approval-processes/types"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { timelineCategories } from "@/features/document-timeline/components/sales-timeline"
import { TimelineEvents } from "@/features/document-timeline/components/timeline-events"
import { useTimelineQuery } from "@/features/document-timeline/hooks/queries"
import type {
    TimelineCategory,
    TimelineSource,
    TimelineTarget,
} from "@/features/document-timeline/types"
import { apiGet, getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getApprovalNodeStatusMeta } from "@/lib/task-status-dictionary"
import { cn } from "@/lib/utils"
import {
    displayActorName,
    displayReadableName,
    displayRound,
    displayUnixSeconds,
} from "../display"
import { useApprovalHistoryInfiniteQuery } from "../queries"
import type { ApprovalHistoryItem } from "../types"

export type AuditTrailTab = "approval" | "timeline"

export type UnifiedAuditTrailCopyItem = Readonly<{
    id: string
    recipientName: string
    trigger: CopyTrigger
    copiedAt: number
    readAt?: number | null
    delivered: boolean
    undeliveredReason?: string | null
    roundNo: number
}>

export type UnifiedAuditTrailProps = Readonly<{
    /** 审批实例 ID；传入后自动倒序拉取多轮执行历史与抄送记录（append 分页）。 */
    instanceId?: string
    /** 外部已加载的审批历史项（如抄送详情页）；传入后直接使用外部数据。 */
    historyItems?: readonly ApprovalHistoryItem[]
    historyHasMore?: boolean
    historyLoadingMore?: boolean
    onLoadMoreHistory?: () => void
    /** 外部已加载的抄送记录；传入后替代内部抄送查询。 */
    copyItems?: readonly UnifiedAuditTrailCopyItem[]
    /** 是否查询抄送记录（默认 true，仅在传入 instanceId 且未传 copyItems 时生效）。 */
    showCopyHistory?: boolean
    /** 单据全动态查询目标；传入时启用「单据全动态」分段页签。 */
    timelineTarget?: TimelineTarget
    /** 当前单据标识，用于在单据全动态中去重本单标签。 */
    currentDocument?: { type: string; id: string }
    /** 关联销售单列表（可选补充接口返回的 salesOrders）。 */
    salesOrders?: readonly TimelineSource[]
    /** 审批上下文提示（提醒审批以本次提交资料为准）。 */
    isApprovalContext?: boolean
    /** 默认激活分段页签（默认 "approval"）。 */
    defaultTab?: AuditTrailTab
    /** 受控分段页签。 */
    activeTab?: AuditTrailTab
    onActiveTabChange?: (tab: AuditTrailTab) => void
    /** 需要滚动定位并高亮的执行记录 ID（配合 ApprovalRouteStepper onJumpToHistory）。 */
    highlightedExecutionId?: string | null
    idPrefix?: string
    className?: string
}>

type CopyHistoryPageDto = {
    items: {
        id: string
        recipient_name: string
        trigger: CopyTrigger
        copied_at: number
        read_at: number | null
        delivered: boolean
        undelivered_reason: string | null
        round_no: number
    }[]
    next_cursor: string | null
}

type RoundBucket = Readonly<{
    roundNo: number
    executions: readonly ApprovalHistoryItem[]
    copies: readonly UnifiedAuditTrailCopyItem[]
}>

const TONE_ICON: Record<StatusTone, LucideIcon> = {
    neutral: CircleDashedIcon,
    info: CircleDotIcon,
    success: CircleCheckIcon,
    warning: CircleAlertIcon,
    destructive: TriangleAlertIcon,
    void: BanIcon,
}

const fetchCopyHistoryPage = (instanceId: string, after?: string) =>
    apiGet<CopyHistoryPageDto>(
        `/admin/approval-instances/${encodeURIComponent(instanceId)}/copies`,
        { after },
    )

/**
 * 倒序按轮次聚合执行历史与抄送记录：最新轮次置顶，同轮内最新执行置顶。
 * 保留跨轮次同一节点的所有记录，严禁按 nodeKey 去重。
 */
function buildReverseChronologicalRounds(
    executions: readonly ApprovalHistoryItem[],
    copies: readonly UnifiedAuditTrailCopyItem[],
): readonly RoundBucket[] {
    const executionMap = new Map<number, ApprovalHistoryItem[]>()
    const copyMap = new Map<number, UnifiedAuditTrailCopyItem[]>()
    const roundSet = new Set<number>()

    for (const item of executions) {
        const roundNo = item.roundNo > 0 ? item.roundNo : 1
        roundSet.add(roundNo)
        const bucket = executionMap.get(roundNo) ?? []
        bucket.push(item)
        executionMap.set(roundNo, bucket)
    }

    for (const copy of copies) {
        const roundNo = copy.roundNo > 0 ? copy.roundNo : 1
        roundSet.add(roundNo)
        const bucket = copyMap.get(roundNo) ?? []
        bucket.push(copy)
        copyMap.set(roundNo, bucket)
    }

    return [...roundSet]
        .sort((left, right) => right - left)
        .map((roundNo) => {
            const roundExecutions = [...(executionMap.get(roundNo) ?? [])].sort(
                (left, right) => {
                    if (right.executionNo !== left.executionNo) {
                        return right.executionNo - left.executionNo
                    }
                    return (right.decidedAt ?? 0) - (left.decidedAt ?? 0)
                },
            )
            const roundCopies = [...(copyMap.get(roundNo) ?? [])].sort(
                (left, right) => right.copiedAt - left.copiedAt,
            )
            return {
                roundNo,
                executions: roundExecutions,
                copies: roundCopies,
            }
        })
}

function AuditExecutionEntry({
    item,
    highlighted,
    registerRef,
}: {
    item: ApprovalHistoryItem
    highlighted: boolean
    registerRef: (id: string, element: HTMLLIElement | null) => void
}) {
    const meta = getApprovalNodeStatusMeta(item.result)
    const Icon = TONE_ICON[meta.tone]
    const actor = displayActorName(item.assigneeName, item.decidedBy)
    const decidedAt = displayUnixSeconds(item.decidedAt)
    const rejected = item.result === "REJECTED" || item.result === "BLOCKED"
    const current = item.result === "ACTIVE"

    return (
        <TimelineItem
            ref={(element) => registerRef(item.executionId, element)}
            className={cn(
                highlighted &&
                    "-mx-2 rounded-md bg-destructive-soft/45 px-2 py-2 pl-8 ring-1 ring-destructive-border",
            )}
        >
            <TimelineMarker
                className={cn(
                    statusToneMarkerClass[meta.tone],
                    highlighted && "left-0 top-2",
                )}
            >
                <Icon aria-hidden="true" />
            </TimelineMarker>
            <TimelineHeader>
                <TimelineTitle className="flex min-w-0 flex-wrap items-center gap-1.5">
                    <span className="wrap-anywhere">
                        {displayReadableName(item.nodeName, item.nodeKey) ||
                            "审批节点未标注"}
                    </span>
                    <StatusBadge tone={meta.tone} label={meta.label} />
                </TimelineTitle>
                {decidedAt ? (
                    <TimelineTime dateTime={decidedAt.dateTime}>
                        {decidedAt.label}
                    </TimelineTime>
                ) : null}
            </TimelineHeader>
            <TimelineDescription>
                {actor ? (
                    <p className="inline-flex items-center gap-1.5 text-xs">
                        <UserRoundIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0"
                        />
                        <span>{actor}</span>
                    </p>
                ) : current ? (
                    <p className="text-xs">等待办理</p>
                ) : null}
                {item.decisionReason ? (
                    <p
                        className={cn(
                            "mt-1.5 rounded-md px-2.5 py-1.5 text-xs whitespace-pre-wrap break-words",
                            rejected
                                ? "border border-destructive-border bg-destructive-soft text-destructive-soft-foreground"
                                : "bg-muted text-foreground",
                        )}
                    >
                        {item.decisionReason}
                    </p>
                ) : null}
            </TimelineDescription>
        </TimelineItem>
    )
}

function RoundCopyList({
    copies,
}: {
    copies: readonly UnifiedAuditTrailCopyItem[]
}) {
    if (copies.length === 0) return null
    return (
        <div className="space-y-1.5 rounded-md border border-border/70 bg-muted/25 px-3 py-2">
            <p className="inline-flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                <MailIcon aria-hidden="true" className="size-3.5 shrink-0" />
                <span>本轮抄送</span>
                <span className="num">({copies.length})</span>
            </p>
            <ul className="divide-y divide-border/50">
                {copies.map((copy) => {
                    const copiedAt = displayUnixSeconds(copy.copiedAt)
                    const readAt = displayUnixSeconds(copy.readAt)
                    return (
                        <li
                            key={copy.id}
                            className="flex flex-wrap items-baseline justify-between gap-x-2 gap-y-0.5 py-1.5 text-xs first:pt-0.5 last:pb-0"
                        >
                            <span className="min-w-0 break-words text-foreground">
                                {copy.recipientName}
                                <span className="text-muted-foreground">
                                    {" "}
                                    · {COPY_TRIGGER_LABELS[copy.trigger]}
                                </span>
                            </span>
                            <span className="flex shrink-0 items-center gap-1.5 text-muted-foreground">
                                {copiedAt ? (
                                    <time
                                        dateTime={copiedAt.dateTime}
                                        className="num"
                                    >
                                        {copiedAt.label}
                                    </time>
                                ) : null}
                                <span>·</span>
                                {copy.delivered ? (
                                    readAt ? (
                                        <span>
                                            已读{" "}
                                            <time
                                                dateTime={readAt.dateTime}
                                                className="num"
                                            >
                                                {readAt.label}
                                            </time>
                                        </span>
                                    ) : (
                                        <span>未读</span>
                                    )
                                ) : (
                                    <span className="text-warning-soft-foreground">
                                        {copy.undeliveredReason || "未送达"}
                                    </span>
                                )}
                            </span>
                        </li>
                    )
                })}
            </ul>
        </div>
    )
}

/**
 * 统一倒序审计轨迹（右侧上下文轨 / 抽屉通用原语）：
 * - 分段页签「审批与抄送」：倒序聚合多轮执行历史与轮次关联抄送记录，全部使用追加式加载更多；
 * - 分段页签「单据全动态」：融合任务动态与内联「当前任务 / 销售全链路」切换，彻底消除抽屉套娃。
 */
export function UnifiedAuditTrail({
    instanceId,
    historyItems,
    historyHasMore,
    historyLoadingMore,
    onLoadMoreHistory,
    copyItems,
    showCopyHistory = true,
    timelineTarget,
    currentDocument,
    salesOrders,
    isApprovalContext = false,
    defaultTab = "approval",
    activeTab,
    onActiveTabChange,
    highlightedExecutionId,
    idPrefix = "unified-audit-trail",
    className,
}: UnifiedAuditTrailProps) {
    const hasApprovalTab = Boolean(instanceId || historyItems || copyItems)
    const hasTimelineTab = Boolean(
        timelineTarget || (salesOrders && salesOrders.length > 0),
    )

    const initialTab: AuditTrailTab =
        defaultTab === "timeline" && hasTimelineTab
            ? "timeline"
            : hasApprovalTab
              ? "approval"
              : "timeline"

    const [internalTab, setInternalTab] = useState<AuditTrailTab>(initialTab)
    const currentTab = activeTab ?? internalTab
    const setTab = (next: AuditTrailTab) => {
        if (activeTab === undefined) setInternalTab(next)
        onActiveTabChange?.(next)
    }

    const [timelineScope, setTimelineScope] = useState<"task" | "sales">(
        timelineTarget ? "task" : "sales",
    )
    const [selectedSalesId, setSelectedSalesId] = useState<string | null>(null)
    const [salesCategory, setSalesCategory] = useState<
        TimelineCategory | undefined
    >()
    const [includeSalesDetails, setIncludeSalesDetails] = useState(false)

    const executionRefs = useRef(new Map<string, HTMLLIElement>())
    const registerExecutionRef = (
        id: string,
        element: HTMLLIElement | null,
    ) => {
        if (element) executionRefs.current.set(id, element)
        else executionRefs.current.delete(id)
    }

    const shouldQueryHistory = Boolean(instanceId) && historyItems === undefined
    const historyQuery = useApprovalHistoryInfiniteQuery(
        { instanceId: instanceId ?? "", order: "desc" },
        shouldQueryHistory,
    )

    const profile = useAccountProfileQuery().data
    const shouldQueryCopies =
        Boolean(instanceId) &&
        copyItems === undefined &&
        showCopyHistory &&
        Boolean(profile)
    const copyQuery = useInfiniteQuery({
        queryKey: [
            "approval",
            "copy-history",
            instanceId,
            profile?.userid,
            profile?.permissions,
        ],
        queryFn: ({ pageParam }) =>
            fetchCopyHistoryPage(instanceId!, pageParam),
        initialPageParam: undefined as string | undefined,
        getNextPageParam: (page) => page.next_cursor ?? undefined,
        enabled: shouldQueryCopies,
        staleTime: 0,
    })

    const resolvedExecutions = useMemo<readonly ApprovalHistoryItem[]>(() => {
        if (historyItems !== undefined) return historyItems
        return historyQuery.data?.pages.flatMap((page) => page.items) ?? []
    }, [historyItems, historyQuery.data])

    const resolvedCopies = useMemo<readonly UnifiedAuditTrailCopyItem[]>(() => {
        if (copyItems !== undefined) return copyItems
        return (
            copyQuery.data?.pages.flatMap((page) =>
                page.items.map((row) => ({
                    id: row.id,
                    recipientName: row.recipient_name,
                    trigger: row.trigger,
                    copiedAt: row.copied_at,
                    readAt: row.read_at,
                    delivered: row.delivered,
                    undeliveredReason: row.undelivered_reason,
                    roundNo: row.round_no,
                })),
            ) ?? []
        )
    }, [copyItems, copyQuery.data])

    const rounds = useMemo(
        () =>
            buildReverseChronologicalRounds(resolvedExecutions, resolvedCopies),
        [resolvedExecutions, resolvedCopies],
    )

    const resolvedHistoryHasMore =
        historyItems !== undefined
            ? Boolean(historyHasMore)
            : Boolean(historyQuery.hasNextPage)
    const resolvedHistoryLoadingMore =
        historyItems !== undefined
            ? Boolean(historyLoadingMore)
            : historyQuery.isFetchingNextPage
    const handleLoadMoreHistory = () => {
        if (historyItems !== undefined) {
            onLoadMoreHistory?.()
        } else {
            void historyQuery.fetchNextPage()
        }
    }

    const timelineTabActive = currentTab === "timeline"
    const taskTimelineQuery = useTimelineQuery(
        timelineTarget ?? { kind: "task", id: "" },
        { pageSize: 15 },
        Boolean(timelineTarget) && timelineTabActive,
    )

    const availableSalesOrders = useMemo<readonly TimelineSource[]>(() => {
        const merged = [
            ...(salesOrders ?? []),
            ...(taskTimelineQuery.data?.pages[0]?.salesOrders ?? []),
        ]
        const seen = new Set<string>()
        const deduped: TimelineSource[] = []
        for (const item of merged) {
            if (!item.id || seen.has(item.id)) continue
            seen.add(item.id)
            deduped.push(item)
        }
        return deduped
    }, [salesOrders, taskTimelineQuery.data])

    const activeSalesOrder =
        availableSalesOrders.find((item) => item.id === selectedSalesId) ??
        availableSalesOrders[0] ??
        null

    const effectiveTimelineScope: "task" | "sales" =
        timelineScope === "sales" && activeSalesOrder
            ? "sales"
            : timelineTarget
              ? "task"
              : "sales"

    const salesTimelineQuery = useTimelineQuery(
        { kind: "sales", id: activeSalesOrder?.id ?? "" },
        {
            category: salesCategory,
            includeDetails: includeSalesDetails,
            pageSize: 20,
        },
        timelineTabActive &&
            effectiveTimelineScope === "sales" &&
            Boolean(activeSalesOrder?.id),
    )

    useEffect(() => {
        if (!highlightedExecutionId) return
        if (currentTab !== "approval") return
        const target = executionRefs.current.get(highlightedExecutionId)
        target?.scrollIntoView({ behavior: "smooth", block: "nearest" })
    }, [highlightedExecutionId, currentTab, rounds.length])

    const approvalPanel = (
        <div className="min-w-0 space-y-4">
            {shouldQueryHistory && historyQuery.isPending ? (
                <p role="status" className="text-xs text-muted-foreground">
                    正在读取审批与抄送轨迹…
                </p>
            ) : shouldQueryHistory &&
              historyQuery.isError &&
              rounds.length === 0 ? (
                <div role="alert" className="space-y-2 text-body-compact">
                    <p className="text-destructive">
                        {getErrorMessage(
                            historyQuery.error,
                            "审批记录读取失败，请重试",
                        )}
                    </p>
                    <Button
                        id={`${idPrefix}-history-retry`}
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => void historyQuery.refetch()}
                    >
                        重试
                    </Button>
                </div>
            ) : rounds.length === 0 ? (
                <p className="py-4 text-xs text-muted-foreground">
                    暂无审批与抄送记录
                </p>
            ) : (
                <div className="space-y-5">
                    {rounds.map((round) => (
                        <section
                            key={round.roundNo}
                            aria-label={displayRound(round.roundNo)}
                            className="space-y-3"
                        >
                            <header className="flex items-center justify-between gap-2 border-b border-border/60 pb-1.5">
                                <h3 className="num text-xs font-semibold text-muted-foreground">
                                    {displayRound(round.roundNo)}
                                </h3>
                                <span className="num text-tiny text-muted-foreground">
                                    {round.executions.length} 条审批
                                    {round.copies.length > 0
                                        ? ` · ${round.copies.length} 条抄送`
                                        : ""}
                                </span>
                            </header>
                            {round.executions.length > 0 ? (
                                <Timeline>
                                    {round.executions.map((item) => (
                                        <AuditExecutionEntry
                                            key={item.executionId}
                                            item={item}
                                            highlighted={
                                                highlightedExecutionId ===
                                                item.executionId
                                            }
                                            registerRef={registerExecutionRef}
                                        />
                                    ))}
                                </Timeline>
                            ) : null}
                            <RoundCopyList copies={round.copies} />
                        </section>
                    ))}
                </div>
            )}

            {shouldQueryHistory && historyQuery.isError && rounds.length > 0 ? (
                <div role="alert" className="flex items-center gap-2 text-xs">
                    <span className="text-destructive">
                        {getErrorMessage(
                            historyQuery.error,
                            "更多审批记录读取失败",
                        )}
                    </span>
                    <Button
                        id={`${idPrefix}-history-error-retry`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        onClick={() => void historyQuery.refetch()}
                    >
                        重试
                    </Button>
                </div>
            ) : null}

            {shouldQueryCopies && copyQuery.isError ? (
                <div role="alert" className="flex items-center gap-2 text-xs">
                    <span className="text-destructive">
                        {getErrorMessage(copyQuery.error, "抄送记录读取失败")}
                    </span>
                    <Button
                        id={`${idPrefix}-copy-retry`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        onClick={() => void copyQuery.refetch()}
                    >
                        重试抄送记录
                    </Button>
                </div>
            ) : null}

            <div className="flex flex-wrap gap-2 empty:hidden">
                {resolvedHistoryHasMore &&
                (historyItems === undefined || Boolean(onLoadMoreHistory)) ? (
                    <LoadingButton
                        id={`${idPrefix}-history-load-more`}
                        type="button"
                        variant="outline"
                        size="sm"
                        loading={resolvedHistoryLoadingMore}
                        onClick={handleLoadMoreHistory}
                    >
                        {resolvedHistoryLoadingMore ? "加载中…" : "加载更多"}
                    </LoadingButton>
                ) : null}
                {shouldQueryCopies && copyQuery.hasNextPage ? (
                    <LoadingButton
                        id={`${idPrefix}-copy-load-more`}
                        type="button"
                        variant="outline"
                        size="sm"
                        loading={copyQuery.isFetchingNextPage}
                        onClick={() => void copyQuery.fetchNextPage()}
                    >
                        {copyQuery.isFetchingNextPage
                            ? "加载中…"
                            : "加载更多抄送"}
                    </LoadingButton>
                ) : null}
            </div>
        </div>
    )

    const taskEvents =
        taskTimelineQuery.data?.pages.flatMap((page) => page.items) ?? []
    const salesEvents =
        salesTimelineQuery.data?.pages.flatMap((page) => page.items) ?? []

    const timelinePanel = (
        <div className="min-w-0 space-y-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
                {timelineTarget && availableSalesOrders.length > 0 ? (
                    <div
                        role="group"
                        aria-label="动态范围切换"
                        className="inline-flex items-center rounded-md border border-border bg-muted/40 p-0.5"
                    >
                        <Button
                            id={`${idPrefix}-scope-task`}
                            type="button"
                            size="xs"
                            variant={
                                effectiveTimelineScope === "task"
                                    ? "secondary"
                                    : "ghost"
                            }
                            aria-pressed={effectiveTimelineScope === "task"}
                            className={cn(
                                "h-control-xs px-2 text-xs",
                                effectiveTimelineScope === "task" &&
                                    "bg-card text-foreground shadow-2xs",
                            )}
                            onClick={() => setTimelineScope("task")}
                        >
                            当前任务
                        </Button>
                        <Button
                            id={`${idPrefix}-scope-sales`}
                            type="button"
                            size="xs"
                            variant={
                                effectiveTimelineScope === "sales"
                                    ? "secondary"
                                    : "ghost"
                            }
                            aria-pressed={effectiveTimelineScope === "sales"}
                            className={cn(
                                "h-control-xs px-2 text-xs",
                                effectiveTimelineScope === "sales" &&
                                    "bg-card text-foreground shadow-2xs",
                            )}
                            onClick={() => setTimelineScope("sales")}
                        >
                            销售全链路
                        </Button>
                    </div>
                ) : (
                    <span className="text-xs font-medium text-muted-foreground">
                        {effectiveTimelineScope === "sales"
                            ? "销售全链路"
                            : "当前任务动态"}
                    </span>
                )}
                <Button
                    id={`${idPrefix}-timeline-refresh`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    disabled={
                        effectiveTimelineScope === "sales"
                            ? salesTimelineQuery.isFetching
                            : taskTimelineQuery.isFetching
                    }
                    onClick={() => {
                        if (effectiveTimelineScope === "sales") {
                            void salesTimelineQuery.refetch()
                        } else {
                            void taskTimelineQuery.refetch()
                        }
                    }}
                >
                    <RefreshCwIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    刷新
                </Button>
            </div>

            {effectiveTimelineScope === "task" ? (
                <div className="space-y-3">
                    {isApprovalContext ? (
                        <p className="text-xs text-muted-foreground">
                            以下为单据最新动态；审批请以本次提交时的资料为准。
                        </p>
                    ) : null}
                    {taskTimelineQuery.isPending ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            正在读取单据动态…
                        </p>
                    ) : taskTimelineQuery.isError ? (
                        <div
                            role="alert"
                            className="space-y-2 text-body-compact"
                        >
                            <p className="text-destructive">
                                {getErrorMessage(
                                    taskTimelineQuery.error,
                                    "单据动态暂时无法读取，请重试。",
                                )}
                            </p>
                            <Button
                                id={`${idPrefix}-task-timeline-retry`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => void taskTimelineQuery.refetch()}
                            >
                                重试
                            </Button>
                        </div>
                    ) : (
                        <>
                            <TimelineEvents
                                events={taskEvents}
                                idPrefix={`${idPrefix}-task-events`}
                                groupByDate
                                currentDocument={currentDocument}
                            />
                            {taskTimelineQuery.hasNextPage ? (
                                <LoadingButton
                                    id={`${idPrefix}-task-timeline-load-more`}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    loading={
                                        taskTimelineQuery.isFetchingNextPage
                                    }
                                    onClick={() =>
                                        void taskTimelineQuery.fetchNextPage()
                                    }
                                >
                                    {taskTimelineQuery.isFetchingNextPage
                                        ? "加载中…"
                                        : "加载更多"}
                                </LoadingButton>
                            ) : null}
                        </>
                    )}
                </div>
            ) : (
                <div className="space-y-3">
                    {availableSalesOrders.length > 1 ? (
                        <div className="flex flex-wrap gap-1.5">
                            {availableSalesOrders.map((order) => {
                                const selected =
                                    order.id === activeSalesOrder?.id
                                return (
                                    <Button
                                        key={order.id}
                                        id={`${idPrefix}-sales-order-${toAutomationIdSegment(order.id)}`}
                                        type="button"
                                        size="xs"
                                        variant={
                                            selected ? "secondary" : "outline"
                                        }
                                        aria-pressed={selected}
                                        className="num"
                                        onClick={() =>
                                            setSelectedSalesId(order.id)
                                        }
                                    >
                                        {order.number || "关联销售单"}
                                    </Button>
                                )
                            })}
                        </div>
                    ) : activeSalesOrder?.number ? (
                        <p className="num text-xs text-muted-foreground">
                            销售单号：{activeSalesOrder.number}
                        </p>
                    ) : null}

                    <div className="flex flex-wrap items-center gap-2.5">
                        <OptionCombobox
                            id={`${idPrefix}-sales-category`}
                            options={[
                                { value: "all", label: "全部环节" },
                                ...timelineCategories,
                            ]}
                            value={salesCategory ?? "all"}
                            onValueChange={(value) =>
                                setSalesCategory(
                                    !value || value === "all"
                                        ? undefined
                                        : (value as TimelineCategory),
                                )
                            }
                            allowClear={false}
                            size="sm"
                            aria-label="业务环节"
                            className="w-32"
                            inputClassName="w-32"
                        />
                        <label
                            htmlFor={`${idPrefix}-sales-details`}
                            className="inline-flex cursor-pointer items-center gap-1.5 text-xs text-muted-foreground"
                        >
                            <NativeCheckbox
                                id={`${idPrefix}-sales-details`}
                                checked={includeSalesDetails}
                                onChange={(event) =>
                                    setIncludeSalesDetails(event.target.checked)
                                }
                            />
                            包含草稿修改
                        </label>
                    </div>

                    {salesTimelineQuery.isPending ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            正在读取销售全链路…
                        </p>
                    ) : salesTimelineQuery.isError ? (
                        <div
                            role="alert"
                            className="space-y-2 text-body-compact"
                        >
                            <p className="text-destructive">
                                {getErrorMessage(
                                    salesTimelineQuery.error,
                                    "销售全链路读取失败，请重试。",
                                )}
                            </p>
                            <Button
                                id={`${idPrefix}-sales-timeline-retry`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() =>
                                    void salesTimelineQuery.refetch()
                                }
                            >
                                重试
                            </Button>
                        </div>
                    ) : (
                        <>
                            <TimelineEvents
                                events={salesEvents}
                                idPrefix={`${idPrefix}-sales-events`}
                                groupByDate
                                currentDocument={currentDocument}
                            />
                            {salesTimelineQuery.hasNextPage ? (
                                <LoadingButton
                                    id={`${idPrefix}-sales-timeline-load-more`}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    loading={
                                        salesTimelineQuery.isFetchingNextPage
                                    }
                                    onClick={() =>
                                        void salesTimelineQuery.fetchNextPage()
                                    }
                                >
                                    {salesTimelineQuery.isFetchingNextPage
                                        ? "加载中…"
                                        : "加载更多"}
                                </LoadingButton>
                            ) : null}
                            {salesTimelineQuery.data?.pages[0]?.notice ? (
                                <p className="border-t border-border/60 pt-2.5 text-xs text-muted-foreground">
                                    {salesTimelineQuery.data.pages[0].notice}
                                </p>
                            ) : null}
                        </>
                    )}
                </div>
            )}
        </div>
    )

    if (hasApprovalTab && hasTimelineTab) {
        return (
            <Tabs
                value={currentTab}
                onValueChange={(value) => {
                    if (value === "approval" || value === "timeline") {
                        setTab(value)
                    }
                }}
                className={cn("min-w-0 gap-3", className)}
            >
                <TabsList className="w-full">
                    <TabsTrigger value="approval" idPrefix={idPrefix}>
                        审批与抄送
                    </TabsTrigger>
                    <TabsTrigger value="timeline" idPrefix={idPrefix}>
                        单据全动态
                    </TabsTrigger>
                </TabsList>
                <TabsContent value="approval">{approvalPanel}</TabsContent>
                <TabsContent value="timeline">{timelinePanel}</TabsContent>
            </Tabs>
        )
    }

    return (
        <div
            data-slot="unified-audit-trail"
            className={cn("min-w-0", className)}
        >
            {hasApprovalTab ? approvalPanel : timelinePanel}
        </div>
    )
}
