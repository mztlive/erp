"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
    ArrowRightIcon,
    ChevronDownIcon,
    Clock3Icon,
    RefreshCwIcon,
    TriangleAlertIcon,
    UserIcon,
    UsersIcon,
} from "lucide-react"

import {
    AsyncSectionState,
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    MetricFilterItem,
    MetricStrip,
    PageActions,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
    WorkTaskItem,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardAction,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Skeleton } from "@/components/ui/skeleton"
import { cn } from "@/lib/utils"
import {
    buildProcessHref,
    buildViewHref,
    buildWarningHref,
    resolveWorkspaceHref,
} from "@/features/workspace/destination"
import {
    deriveProjectionFreshness,
    deriveWorkItemsFreshness,
    greetingForNow,
} from "@/features/workspace/freshness"
import { useWorkspaceDashboardQuery } from "@/features/workspace/queries"
import {
    buildGroupAllHref,
    buildTaskQueueHref,
    buildWorkspaceSearchParams,
    filterSummaryFor,
    metricKeyFromUrlState,
    parseWorkspaceSearchParams,
    toTodayWorkspaceQuery,
    urlStateFromMetricKey,
    type WorkspaceUrlState,
} from "@/features/workspace/url-state"
import { actionLabelForWorkItemType, goToWorkspaceLabel } from "@/lib/ui-text"
import type {
    WorkspaceMetricKey,
    WorkspaceTaskGroup,
    WorkspaceWorkItem,
} from "@/features/workspace/types"

const VIEWER_TIMEZONE = "Asia/Shanghai"

/** 焦点还原走 sessionStorage，不落地址栏（内部 ID 禁止进 URL）。 */
const HOME_FOCUS_SESSION_KEY = "workspace-home.focus"

function responsiblePartyLabel(item: WorkspaceWorkItem): string {
    if (item.ownerUserLabel) {
        return `${item.ownerRoleLabel} · ${item.ownerUserLabel}`
    }
    return `${item.ownerRoleLabel} · 待认领`
}

function processBlocker(item: WorkspaceWorkItem): string | undefined {
    return item.actionBlockers.find((b) => b.action === "PROCESS")?.message
}

function canProcess(item: WorkspaceWorkItem): boolean {
    return (
        item.allowedActions.includes("PROCESS") &&
        !item.actionBlockers.some((b) => b.action === "PROCESS")
    )
}

function canView(item: WorkspaceWorkItem): boolean {
    return item.allowedActions.includes("VIEW")
}

function WorkspaceHomeSkeleton() {
    return (
        <PageScaffold>
            <div className="space-y-2">
                <Skeleton className="h-8 w-64" />
                <Skeleton className="h-4 w-96 max-w-full" />
            </div>
            <div className="grid gap-2 sm:grid-cols-2 sm:gap-3 lg:grid-cols-4">
                {Array.from({ length: 4 }).map((_, index) => (
                    <Skeleton key={index} className="h-20 rounded-lg" />
                ))}
            </div>
            <div className="grid min-w-0 gap-3 md:gap-4 xl:grid-cols-[minmax(0,3fr)_minmax(18rem,2fr)]">
                <Skeleton className="min-h-80 w-full rounded-lg" />
                <div className="space-y-3 md:space-y-4">
                    <Skeleton className="h-40 w-full rounded-lg" />
                    <Skeleton className="h-32 w-full rounded-lg" />
                </div>
            </div>
        </PageScaffold>
    )
}

function TaskGroupSection({
    group,
    focusStableNumber,
    onOpenTask,
    groupAllHref,
}: {
    group: WorkspaceTaskGroup
    focusStableNumber?: string
    onOpenTask: (item: WorkspaceWorkItem) => void
    groupAllHref: string
}) {
    const [open, setOpen] = React.useState(group.defaultExpanded)
    const headingId = `task-group-${group.family}`
    const previewLimit =
        group.pagePreviewLimit ??
        // TEMPORARY design fallback only — not an acceptance contract.
        5
    const previewItems = group.items.slice(0, previewLimit)
    const hasMore = group.total > previewItems.length

    return (
        <Collapsible open={open} onOpenChange={setOpen}>
            <div className="rounded-lg border">
                <CollapsibleTrigger
                    className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-sm font-medium hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    aria-expanded={open}
                    aria-controls={`${headingId}-panel`}
                    id={headingId}
                >
                    <span>
                        {group.label}
                        <span className="ml-2 text-muted-foreground num">
                            {group.total}
                        </span>
                    </span>
                    <ChevronDownIcon
                        aria-hidden="true"
                        className={
                            open
                                ? "size-4 shrink-0 rotate-180 transition"
                                : "size-4 shrink-0 transition"
                        }
                    />
                </CollapsibleTrigger>
                <CollapsibleContent
                    id={`${headingId}-panel`}
                    role="region"
                    aria-labelledby={headingId}
                >
                    <div className="space-y-2 border-t p-3">
                        {previewItems.map((item) => {
                            const processOk = canProcess(item)
                            const viewOk = canView(item)
                            const blocker = processBlocker(item)
                            const processHref = buildProcessHref(item)
                            const viewHref = buildViewHref(item)
                            const isFocused =
                                focusStableNumber === item.stableNumber

                            return (
                                <div
                                    key={item.workItemId}
                                    id={`work-item-${item.stableNumber}`}
                                    data-stable-number={item.stableNumber}
                                    tabIndex={isFocused ? -1 : undefined}
                                    className={
                                        isFocused
                                            ? "rounded-lg ring-2 ring-ring ring-offset-2"
                                            : undefined
                                    }
                                >
                                    <WorkTaskItem
                                        taskType={item.workItemTypeLabel}
                                        businessObject={item.objectTitle}
                                        counterparty={item.counterpartyName}
                                        enteredAt={item.enteredAtLabel}
                                        enteredDateTime={item.createdAt}
                                        enteredAtLabel="进入时间"
                                        dueAt={item.dueAtLabel}
                                        dueDateTime={item.dueAt}
                                        responsibleParty={responsiblePartyLabel(
                                            item,
                                        )}
                                        reason={item.reasonLabel}
                                        impact={item.impactSummary}
                                        status={{
                                            label: item.statusLabel,
                                            tone: item.statusTone,
                                        }}
                                        nextAction={
                                            <div className="flex flex-col items-end gap-1">
                                                {processOk ? (
                                                    <Button
                                                        size="sm"
                                                        variant={
                                                            item.statusTone ===
                                                            "destructive"
                                                                ? "default"
                                                                : "outline"
                                                        }
                                                        render={
                                                            <Link
                                                                href={
                                                                    processHref
                                                                }
                                                                onClick={() =>
                                                                    onOpenTask(
                                                                        item,
                                                                    )
                                                                }
                                                            />
                                                        }
                                                    >
                                                        {actionLabelForWorkItemType(
                                                            item.workItemTypeLabel,
                                                        )}
                                                        <ArrowRightIcon
                                                            data-icon="inline-end"
                                                            aria-hidden="true"
                                                        />
                                                    </Button>
                                                ) : (
                                                    <Button
                                                        size="sm"
                                                        variant="outline"
                                                        disabled
                                                        aria-disabled="true"
                                                    >
                                                        {actionLabelForWorkItemType(
                                                            item.workItemTypeLabel,
                                                        )}
                                                    </Button>
                                                )}
                                                {!processOk && viewOk ? (
                                                    <Button
                                                        size="xs"
                                                        variant="ghost"
                                                        render={
                                                            <Link
                                                                href={viewHref}
                                                                onClick={() =>
                                                                    onOpenTask(
                                                                        item,
                                                                    )
                                                                }
                                                            />
                                                        }
                                                    >
                                                        查看
                                                    </Button>
                                                ) : null}
                                                {blocker ? (
                                                    <span className="max-w-40 text-right text-xs text-muted-foreground">
                                                        {blocker}
                                                    </span>
                                                ) : null}
                                            </div>
                                        }
                                    />
                                </div>
                            )
                        })}
                        {hasMore ? (
                            <div className="flex justify-end pt-1">
                                <Button
                                    size="sm"
                                    variant="ghost"
                                    render={<Link href={groupAllHref} />}
                                >
                                    查看该组全部 {group.total} 条
                                    <ArrowRightIcon
                                        data-icon="inline-end"
                                        aria-hidden="true"
                                    />
                                </Button>
                            </div>
                        ) : null}
                    </div>
                </CollapsibleContent>
            </div>
        </Collapsible>
    )
}

export function WorkspaceHomePage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const urlState = React.useMemo(
        () => parseWorkspaceSearchParams(searchParams),
        [searchParams],
    )

    const queryInput = React.useMemo(
        () => toTodayWorkspaceQuery(urlState, VIEWER_TIMEZONE),
        [urlState],
    )

    const dashboardQuery = useWorkspaceDashboardQuery(queryInput)
    const view = dashboardQuery.data
    const refreshing =
        dashboardQuery.isFetching && !dashboardQuery.isPending && !!view

    const [focusedStableNumber, setFocusedStableNumber] = React.useState<
        string | null
    >(null)

    const activeMetric = metricKeyFromUrlState(urlState)
    // scope 也是激活筛选：团队待认领无任务时走「当前筛选无结果」空态（D17）
    const hasActiveFilter = Boolean(
        urlState.scope === "role_pool" || urlState.due || urlState.family,
    )

    // 筛选/指标变更恒 replace，不膨胀历史（P2）；scope 默认值省略，URL 最小化
    const replaceUrl = React.useCallback(
        (next: WorkspaceUrlState) => {
            const qs = buildWorkspaceSearchParams(next)
            router.replace(`${pathname}${qs}`, { scroll: false })
        },
        [pathname, router],
    )

    const onMetricClick = React.useCallback(
        (key: WorkspaceMetricKey) => {
            sessionStorage.removeItem(HOME_FOCUS_SESSION_KEY)
            setFocusedStableNumber(null)
            replaceUrl(urlStateFromMetricKey(key, urlState))
        },
        [replaceUrl, urlState],
    )

    const clearFilters = React.useCallback(() => {
        sessionStorage.removeItem(HOME_FOCUS_SESSION_KEY)
        setFocusedStableNumber(null)
        replaceUrl({
            scope: urlState.scope,
        })
    }, [replaceUrl, urlState.scope])

    const onOpenTask = React.useCallback((item: WorkspaceWorkItem) => {
        // Persist focus in sessionStorage so a return from the target page
        // restores the exact task row without leaking internal ids into the URL.
        sessionStorage.setItem(HOME_FOCUS_SESSION_KEY, item.stableNumber)
    }, [])

    const refresh = React.useCallback(() => {
        void dashboardQuery.refetch()
    }, [dashboardQuery])

    const onScopeChange = React.useCallback(
        (scope: "mine" | "role_pool") => {
            if (scope === urlState.scope) return
            sessionStorage.removeItem(HOME_FOCUS_SESSION_KEY)
            setFocusedStableNumber(null)
            replaceUrl({ ...urlState, scope })
        },
        [replaceUrl, urlState],
    )

    // Restore task focus after return from a target page. One-shot: the stored
    // focus is consumed here, so a plain refresh never re-scrolls unexpectedly.
    React.useEffect(() => {
        if (!view) return
        const stableNumber = sessionStorage.getItem(HOME_FOCUS_SESSION_KEY)
        if (!stableNumber) return
        const el = document.getElementById(`work-item-${stableNumber}`)
        sessionStorage.removeItem(HOME_FOCUS_SESSION_KEY)
        if (!el) return
        setFocusedStableNumber(stableNumber)
        el.scrollIntoView({ block: "nearest", behavior: "smooth" })
        if (el instanceof HTMLElement) {
            el.focus({ preventScroll: true })
        }
    }, [view])

    if (dashboardQuery.isPending && !view) {
        return <WorkspaceHomeSkeleton />
    }

    if (dashboardQuery.isError && !view) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    error={dashboardQuery.error}
                    action={
                        <Button
                            type="button"
                            variant="outline"
                            onClick={refresh}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!view) {
        return <WorkspaceHomeSkeleton />
    }

    if (view.access === "forbidden") {
        return (
            <PageScaffold>
                <BusinessFailureState
                    kind="permission"
                    title="无今日工作台权限"
                    description="当前账号没有今日工作台模块权限。入口应已隐藏；若通过链接直接访问，请联系管理员开通权限。"
                />
            </PageScaffold>
        )
    }

    if (view.access === "no_data_scope") {
        return (
            <PageScaffold>
                <PageHeader
                    title={greetingForNow(view.viewer.displayName)}
                    description={`${view.viewer.activeRoleLabel}工作台`}
                />
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色无数据范围"
                    description="你可以进入此页面，但当前权限范围内没有可查看的任务与指标。系统不会展示虚假的 0 指标。"
                />
            </PageScaffold>
        )
    }

    const projectionFreshness = deriveProjectionFreshness(view.freshness, {
        refreshing,
    })
    const workItemsFreshness = deriveWorkItemsFreshness(view.freshness, {
        refreshing,
    })

    const visibleCount = view.groups.reduce(
        (sum, group) => sum + group.total,
        0,
    )
    const filterLabel = filterSummaryFor(activeMetric, urlState.scope)
    const taskQueueHref = buildTaskQueueHref(urlState)
    const metrics = view.metrics.filter((metric) => metric.visible)

    const asyncStatus = dashboardQuery.isError
        ? "error"
        : refreshing
          ? "refreshing"
          : "success"

    return (
        <PageScaffold>
            <PageHeader
                title={greetingForNow(view.viewer.displayName)}
                description={`${view.viewer.activeRoleLabel}工作台 · 先处理超期项，再完成今日到期任务。`}
                metadata={
                    <div className="flex flex-col gap-1 sm:flex-row sm:flex-wrap sm:items-center sm:gap-x-4">
                        <DataFreshness
                            label="待办"
                            updatedAt={workItemsFreshness.updatedAtLabel}
                            dateTime={workItemsFreshness.dateTime}
                            state={workItemsFreshness.state}
                            statusLabel={workItemsFreshness.statusLabel}
                        />
                        <DataFreshness
                            label="工作台汇总"
                            updatedAt={projectionFreshness.updatedAtLabel}
                            dateTime={projectionFreshness.dateTime}
                            state={projectionFreshness.state}
                            statusLabel={projectionFreshness.statusLabel}
                        />
                    </div>
                }
                actions={
                    <div className="flex flex-wrap items-center justify-end gap-2">
                        {/*
              分段切换：轨道 + 图标 + 选中白底浮起 / 未选中弱字色，
              明确是「二选一控件」而不是静态标签。
            */}
                        <div
                            role="group"
                            aria-label="责任范围"
                            className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                        >
                            <button
                                type="button"
                                aria-pressed={urlState.scope === "mine"}
                                onClick={() => onScopeChange("mine")}
                                className={cn(
                                    "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    urlState.scope === "mine"
                                        ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                        : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                                )}
                            >
                                <UserIcon
                                    className="size-3.5 shrink-0"
                                    aria-hidden="true"
                                />
                                我的待办
                            </button>
                            <button
                                type="button"
                                aria-pressed={urlState.scope === "role_pool"}
                                onClick={() => onScopeChange("role_pool")}
                                className={cn(
                                    "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    urlState.scope === "role_pool"
                                        ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                        : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                                )}
                            >
                                <UsersIcon
                                    className="size-3.5 shrink-0"
                                    aria-hidden="true"
                                />
                                团队待认领
                            </button>
                        </div>
                        <PageActions
                            actions={[
                                {
                                    actionKey: "refresh",
                                    label: refreshing ? "刷新中" : "刷新",
                                    icon: RefreshCwIcon,
                                    variant: "ghost",
                                    disabled: refreshing,
                                    onClick: refresh,
                                    className:
                                        "text-muted-foreground hover:text-foreground",
                                },
                            ]}
                        />
                    </div>
                }
            />

            {/* 数据新鲜度由页头 DataFreshness 徽章统一表达；刷新失败由任务卡内
          AsyncSectionState 错误卡承载，避免同一故障两处噪音（P2-8/P2-10）。 */}

            <MetricStrip columns={4} aria-label="待办筛选">
                {metrics.map((metric) => (
                    <MetricFilterItem
                        key={metric.key}
                        label={metric.label}
                        value={metric.count}
                        detail={metric.detail}
                        active={activeMetric === metric.key}
                        onClick={() => onMetricClick(metric.key)}
                        status={
                            metric.tone === "destructive" ||
                            metric.tone === "warning"
                                ? { label: metric.label, tone: metric.tone }
                                : undefined
                        }
                    />
                ))}
            </MetricStrip>

            <div className="grid min-w-0 gap-3 md:gap-4 xl:grid-cols-[minmax(0,3fr)_minmax(18rem,2fr)]">
                <Card
                    size="sm"
                    className={cn("min-w-0", surfacePanelClassName)}
                >
                    <CardHeader className="rounded-t-lg border-b border-border/30">
                        <CardTitle id="workspace-task-main-title">
                            {filterLabel}
                        </CardTitle>
                        <CardDescription aria-live="polite" aria-atomic="true">
                            {hasActiveFilter
                                ? `当前筛选「${filterLabel}」共 ${visibleCount} 项`
                                : `当前共 ${visibleCount} 项待办，按超期与截止时间展示`}
                        </CardDescription>
                        <CardAction className="flex flex-wrap gap-1">
                            {hasActiveFilter ? (
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    onClick={clearFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : null}
                            {view.canOpenTaskQueue ? (
                                <Button
                                    size="xs"
                                    variant="secondary"
                                    className="rounded-md shadow-none"
                                    render={<Link href={taskQueueHref} />}
                                >
                                    查看全部待办
                                    <ArrowRightIcon
                                        data-icon="inline-end"
                                        aria-hidden="true"
                                    />
                                </Button>
                            ) : null}
                        </CardAction>
                    </CardHeader>
                    <CardContent>
                        <AsyncSectionState
                            status={asyncStatus}
                            refreshingLabel="正在刷新，仍显示上次结果"
                            error="工作台任务刷新失败，已保留上次成功内容。"
                            errorKind={
                                dashboardQuery.isError ? "projection" : "system"
                            }
                            retryAction={
                                <Button
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    onClick={refresh}
                                >
                                    重试
                                </Button>
                            }
                        >
                            {visibleCount === 0 && !hasActiveFilter ? (
                                <BusinessEmptyState
                                    kind="no-tasks"
                                    title="当前没有待处理事项"
                                    description="当前没有待处理事项。可查看最近打开记录，或进入其它业务模块。"
                                    // 嵌在任务卡内：去掉空态自带描边/底，避免框中套框
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0 md:p-8"
                                    action={
                                        <div className="flex flex-wrap items-center justify-center gap-2">
                                            {/* secondary chip：有底有 hover，比 ghost 更像可点，比 outline 更轻 */}
                                            <Button
                                                size="sm"
                                                variant="secondary"
                                                className="rounded-lg shadow-none"
                                                render={
                                                    <Link
                                                        href={resolveWorkspaceHref(
                                                            "W05",
                                                        )}
                                                    />
                                                }
                                            >
                                                销售单
                                            </Button>
                                            <Button
                                                size="sm"
                                                variant="secondary"
                                                className="rounded-lg shadow-none"
                                                render={
                                                    <Link
                                                        href={resolveWorkspaceHref(
                                                            "W07",
                                                        )}
                                                    />
                                                }
                                            >
                                                采购确认
                                            </Button>
                                            <Button
                                                size="sm"
                                                variant="secondary"
                                                className="rounded-lg shadow-none"
                                                render={
                                                    <Link
                                                        href={resolveWorkspaceHref(
                                                            "W10",
                                                        )}
                                                    />
                                                }
                                            >
                                                库存台账
                                            </Button>
                                        </div>
                                    }
                                />
                            ) : null}

                            {visibleCount === 0 && hasActiveFilter ? (
                                <BusinessEmptyState
                                    kind="filter"
                                    title="当前筛选无结果"
                                    description={`没有符合「${filterLabel}」的待办。可清除筛选查看全部任务。`}
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0 md:p-8"
                                    action={
                                        <Button
                                            type="button"
                                            variant="secondary"
                                            size="sm"
                                            className="rounded-lg shadow-none"
                                            onClick={clearFilters}
                                        >
                                            清除筛选
                                        </Button>
                                    }
                                />
                            ) : null}

                            {visibleCount > 0 ? (
                                <div
                                    className="space-y-3"
                                    role="list"
                                    aria-labelledby="workspace-task-main-title"
                                >
                                    {view.groups.map((group) => (
                                        <div key={group.family} role="listitem">
                                            <TaskGroupSection
                                                group={group}
                                                focusStableNumber={
                                                    focusedStableNumber ??
                                                    undefined
                                                }
                                                onOpenTask={onOpenTask}
                                                groupAllHref={buildGroupAllHref(
                                                    urlState,
                                                    group.family,
                                                )}
                                            />
                                        </div>
                                    ))}
                                </div>
                            ) : null}
                        </AsyncSectionState>
                    </CardContent>
                </Card>

                <div className="space-y-3 md:space-y-4">
                    <Card size="sm" className={surfacePanelClassName}>
                        <CardHeader className="rounded-t-lg border-b border-border/30">
                            <CardTitle>需要关注的预警</CardTitle>
                            <CardDescription>
                                只显示需要你关注的异常
                            </CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-2">
                            {view.warnings.length === 0 ? (
                                <p className="text-sm text-muted-foreground">
                                    当前没有需要关注的预警。
                                </p>
                            ) : (
                                view.warnings.map((warning) => (
                                    <Alert
                                        key={warning.warningId}
                                        variant={
                                            warning.severity === "destructive"
                                                ? "destructive"
                                                : warning.severity === "warning"
                                                  ? "warning"
                                                  : "default"
                                        }
                                    >
                                        {warning.severity === "destructive" ? (
                                            <TriangleAlertIcon aria-hidden="true" />
                                        ) : (
                                            <Clock3Icon aria-hidden="true" />
                                        )}
                                        <AlertTitle>{warning.title}</AlertTitle>
                                        <AlertDescription className="flex flex-col gap-2">
                                            <span>{warning.description}</span>
                                            <Button
                                                size="xs"
                                                variant="outline"
                                                className="w-fit"
                                                render={
                                                    <Link
                                                        href={buildWarningHref(
                                                            warning,
                                                        )}
                                                    />
                                                }
                                            >
                                                {goToWorkspaceLabel(
                                                    warning.destinationWorkspaceId,
                                                )}
                                                <ArrowRightIcon
                                                    data-icon="inline-end"
                                                    aria-hidden="true"
                                                />
                                            </Button>
                                        </AlertDescription>
                                    </Alert>
                                ))
                            )}
                        </CardContent>
                    </Card>

                    <Card size="sm" className={surfacePanelClassName}>
                        <CardHeader className="rounded-t-lg border-b border-border/30">
                            <CardTitle>最近打开</CardTitle>
                            <CardDescription>
                                快速回到上次处理的任务。
                            </CardDescription>
                        </CardHeader>
                        <CardContent>
                            {view.recent.length === 0 ? (
                                <p className="text-sm text-muted-foreground">
                                    暂无最近记录。
                                </p>
                            ) : (
                                <nav
                                    aria-label="最近打开的任务"
                                    className="space-y-1"
                                >
                                    {view.recent.map((item) => (
                                        <Button
                                            key={item.id}
                                            variant="ghost"
                                            className="w-full justify-between"
                                            render={<Link href={item.href} />}
                                        >
                                            <span className="truncate">
                                                {item.label}
                                            </span>
                                            <ArrowRightIcon aria-hidden="true" />
                                        </Button>
                                    ))}
                                </nav>
                            )}
                        </CardContent>
                    </Card>
                </div>
            </div>
        </PageScaffold>
    )
}
