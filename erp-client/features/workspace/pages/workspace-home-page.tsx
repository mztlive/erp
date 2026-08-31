"use client"

import * as React from "react"
import { Maximize2Icon, Minimize2Icon, RefreshCwIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    PageActions,
    PageHeader,
    PageScaffold,
    WorkspaceTaskPane,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import {
    Sheet,
    SheetContent,
    SheetDescription,
    SheetTitle,
} from "@/components/ui/sheet"
import {
    WorkspaceFamilyNav,
    WorkspaceQueueScopeNav,
    WorkspaceQueueToolbar,
} from "@/features/workspace/components/workspace-filter-bar"
import { WorkspaceHomeSkeleton } from "@/features/workspace/components/workspace-home-skeleton"
import { WorkspacePaneActionsProvider } from "@/features/workspace/components/workspace-pane-actions"
import { WorkspaceTaskDetail } from "@/features/workspace/components/workspace-task-detail"
import { WorkspaceTaskList } from "@/features/workspace/components/workspace-task-list"
import { useWorkspaceHome } from "@/features/workspace/hooks/use-workspace-home"
import { deriveWorkItemsFreshness } from "@/features/workspace/lib/freshness"
import { filterSummaryFor } from "@/features/workspace/lib/url-state"
import { cn } from "@/lib/utils"

/**
 * 工作台：页头切换待办口径，队列与作业面左右分栏，审批在右侧连续提交。
 */
export function WorkspaceHomePage() {
    const {
        urlState,
        view,
        accountProfileQuery,
        dashboardQuery,
        refreshing,
        activeMetric,
        hasActiveFilter,
        searchDraft,
        setSearchDraft,
        narrowDetailOpen,
        setNarrowDetailOpen,
        setNarrowDetailSettledOpen,
        completionAnnouncement,
        selected,
        onMetricClick,
        clearFilters,
        onSelectTask,
        applyDecisionAfter,
        onFamilyChange,
        onSortChange,
        applySearch,
        clearSearch,
        refresh,
    } = useWorkspaceHome()
    const [detailFullscreen, setDetailFullscreen] = React.useState(false)

    React.useEffect(() => {
        if (!selected) setDetailFullscreen(false)
    }, [selected])

    React.useEffect(() => {
        if (!detailFullscreen) return
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key === "Escape") setDetailFullscreen(false)
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [detailFullscreen])

    if ((accountProfileQuery.isPending || dashboardQuery.isPending) && !view) {
        return <WorkspaceHomeSkeleton />
    }

    if (accountProfileQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    error={accountProfileQuery.error}
                    action={
                        <Button
                            id="workspace-home-profile-retry"
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

    if (dashboardQuery.isError && !view) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    error={dashboardQuery.error}
                    action={
                        <Button
                            id="workspace-home-dashboard-retry"
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

    if (!view) return <WorkspaceHomeSkeleton />

    if (view.access === "forbidden") {
        return (
            <PageScaffold>
                <BusinessFailureState
                    kind="permission"
                    title="无工作台权限"
                    description="当前账号没有工作台模块权限。入口应已隐藏；若通过链接直接访问，请联系管理员开通权限。"
                />
            </PageScaffold>
        )
    }

    if (view.access === "no_data_scope") {
        return (
            <PageScaffold>
                <PageHeader
                    title="我的工作台"
                    description="当前角色无数据范围"
                />
                <BusinessEmptyState
                    kind="no-scope"
                    title="当前角色无数据范围"
                    description="你可以进入此页面，但当前权限范围内没有可查看的任务与指标。系统不会展示虚假的 0 指标。"
                />
            </PageScaffold>
        )
    }

    const workItemsFreshness = deriveWorkItemsFreshness(view.freshness, {
        refreshing,
    })
    const filterLabel = filterSummaryFor(activeMetric)
    const metrics = view.metrics.filter((metric) => metric.visible)
    const items = view.items
    const startedView = urlState.view === "started"
    const startedHasQuery = startedView && Boolean(urlState.query)
    const hasEffectiveFilter =
        startedHasQuery || (!startedView && hasActiveFilter)
    const emptyTitle = startedHasQuery
        ? "没有匹配的审批"
        : startedView
          ? "还没有我发起的审批"
          : hasEffectiveFilter
            ? "当前筛选没有待办"
            : "当前没有待处理事项"
    const emptyDescription = startedHasQuery
        ? "可清除关键词后查看全部我发起的审批。"
        : startedView
          ? "你发起的审批会在这里持续显示当前节点、审批人和处理状态。"
          : hasEffectiveFilter
            ? "可清除筛选后回到待我处理。"
            : "新任务到达后会出现在这里。"
    const emptyAction = startedHasQuery ? (
        <Button
            id="workspace-home-clear-search"
            type="button"
            variant="secondary"
            onClick={clearSearch}
        >
            清除搜索
        </Button>
    ) : hasEffectiveFilter ? (
        <Button
            id="workspace-home-clear-filters"
            type="button"
            variant="secondary"
            onClick={clearFilters}
        >
            回到待我处理
        </Button>
    ) : undefined

    const queueToolbar = startedView ? (
        <div className="flex flex-col gap-2">
            <div className="flex items-baseline justify-between gap-3 py-1">
                <h2 className="text-sm font-medium">我发起的审批</h2>
                <span className="text-xs text-muted-foreground">
                    {view.total.toLocaleString("zh-CN")} 条
                </span>
            </div>
            <WorkspaceQueueToolbar
                urlState={urlState}
                searchDraft={searchDraft}
                onSearchDraftChange={setSearchDraft}
                onSortChange={onSortChange}
                onSearch={applySearch}
                showSort={false}
                searchAriaLabel="搜索我发起的审批"
            />
        </div>
    ) : (
        <>
            <WorkspaceFamilyNav
                urlState={urlState}
                counts={view.familyCounts}
                onFamilyChange={onFamilyChange}
            />
            <WorkspaceQueueToolbar
                urlState={urlState}
                searchDraft={searchDraft}
                onSearchDraftChange={setSearchDraft}
                onSortChange={onSortChange}
                onSearch={applySearch}
            />
        </>
    )

    const paneActions = selected ? (
        <WorkspaceDetailFullscreenButton
            expanded={detailFullscreen}
            onToggle={() => setDetailFullscreen((current) => !current)}
        />
    ) : null

    const detail = selected ? (
        <WorkspacePaneActionsProvider actions={paneActions}>
            <WorkspaceTaskDetail
                item={selected}
                grantedPermissions={accountProfileQuery.data?.permissions ?? []}
                onDecisionApplied={(commandView, workItemId) => {
                    applyDecisionAfter(
                        workItemId,
                        commandView.nextOpenTask?.workItemId,
                    )
                }}
                onTaskCompleted={applyDecisionAfter}
            />
        </WorkspacePaneActionsProvider>
    ) : (
        <WorkspaceTaskPane
            header={
                <div className="flex min-w-0 flex-col gap-2">
                    <h2 className="text-xl font-semibold tracking-tight">
                        任务处理
                    </h2>
                    <p className="text-sm text-muted-foreground">
                        选中左侧待办后，可在此核对并提交决定。
                    </p>
                </div>
            }
            aria-label="任务处理"
        >
            <div className="flex min-h-full items-center justify-center p-8">
                <BusinessEmptyState
                    kind="no-tasks"
                    title="在此处理任务"
                    description="选中左侧待办后，可在此核对并提交决定。"
                    className="bg-transparent ring-0"
                />
            </div>
        </WorkspaceTaskPane>
    )

    return (
        <PageScaffold className="min-h-0" density="compact">
            <p
                key={completionAnnouncement.sequence}
                className="sr-only"
                role="status"
                aria-live="polite"
                aria-atomic="true"
            >
                {completionAnnouncement.text}
            </p>
            <PageHeader
                title="我的工作台"
                metadata={
                    <DataFreshness
                        updatedAt={workItemsFreshness.updatedAtLabel}
                        dateTime={workItemsFreshness.dateTime}
                        state={workItemsFreshness.state}
                        statusLabel={workItemsFreshness.statusLabel}
                    />
                }
                actions={
                    <div className="flex flex-wrap items-center gap-2">
                        <WorkspaceQueueScopeNav
                            metrics={metrics}
                            activeMetric={activeMetric}
                            onMetricClick={onMetricClick}
                        />
                        <PageActions
                            actions={[
                                {
                                    actionKey: "refresh",
                                    id: "workspace-home-refresh",
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

            <div
                className={cn(
                    surfacePanelClassName,
                    "flex min-h-0 flex-1 flex-col overflow-hidden lg:flex-row",
                )}
            >
                <section
                    data-slot="workspace-queue"
                    className={cn(
                        "flex min-h-0 w-full flex-col lg:w-80 lg:shrink-0 xl:w-96",
                        detailFullscreen && "hidden",
                    )}
                    aria-label={filterLabel}
                >
                    <header className="flex flex-col gap-2 border-b border-grid px-3 pt-3 pb-3">
                        <p className="sr-only" aria-live="polite">
                            {filterLabel} {view.total} 项
                        </p>
                        {queueToolbar}
                    </header>
                    {items.length === 0 ? (
                        <div className="flex min-h-0 flex-1 items-center justify-center px-4 py-8">
                            <BusinessEmptyState
                                kind={
                                    hasEffectiveFilter ? "filter" : "no-tasks"
                                }
                                title={emptyTitle}
                                description={emptyDescription}
                                action={emptyAction}
                                className="bg-transparent p-0 ring-0"
                            />
                        </div>
                    ) : (
                        <WorkspaceTaskList
                            items={items}
                            selectedWorkItemId={selected?.workItemId}
                            onSelect={onSelectTask}
                        />
                    )}
                </section>
                <Separator
                    orientation="vertical"
                    className={cn(
                        "hidden lg:block",
                        detailFullscreen && "lg:hidden",
                    )}
                />
                <section
                    data-slot="workspace-detail"
                    aria-label="任务处理"
                    className="hidden min-h-0 min-w-0 flex-1 lg:flex lg:flex-col"
                >
                    {detail}
                </section>
            </div>

            <Sheet
                open={narrowDetailOpen && Boolean(selected)}
                onOpenChange={setNarrowDetailOpen}
                onOpenChangeComplete={setNarrowDetailSettledOpen}
            >
                <SheetContent
                    side="right"
                    size="detail"
                    closeButtonId="workspace-detail-sheet-close"
                    className="w-full p-0 sm:max-w-lg"
                >
                    <SheetTitle className="sr-only">
                        {selected?.objectTitle ?? "任务详情"}
                    </SheetTitle>
                    <SheetDescription className="sr-only">
                        当前待办的摘要与处理动作
                    </SheetDescription>
                    {selected ? (
                        <WorkspacePaneActionsProvider actions={paneActions}>
                            <WorkspaceTaskDetail
                                item={selected}
                                grantedPermissions={
                                    accountProfileQuery.data?.permissions ?? []
                                }
                                onDecisionApplied={(
                                    commandView,
                                    workItemId,
                                ) => {
                                    applyDecisionAfter(
                                        workItemId,
                                        commandView.nextOpenTask?.workItemId,
                                    )
                                    setNarrowDetailOpen(false)
                                }}
                                onTaskCompleted={(
                                    workItemId,
                                    preferredWorkItemId,
                                ) => {
                                    applyDecisionAfter(
                                        workItemId,
                                        preferredWorkItemId,
                                    )
                                    setNarrowDetailOpen(false)
                                }}
                            />
                        </WorkspacePaneActionsProvider>
                    ) : null}
                </SheetContent>
            </Sheet>
        </PageScaffold>
    )
}

/** 右侧作业面全屏：收起左列队列，Esc 退出。 */
function WorkspaceDetailFullscreenButton({
    expanded,
    onToggle,
}: {
    expanded: boolean
    onToggle: () => void
}) {
    const label = expanded ? "退出全屏" : "全屏处理"
    return (
        <Tooltip>
            <TooltipTrigger
                id="workspace-detail-fullscreen-trigger"
                render={
                    <Button
                        id="workspace-detail-fullscreen-trigger"
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label={label}
                        aria-pressed={expanded}
                        data-testid="workspace-detail-fullscreen"
                        onClick={onToggle}
                    />
                }
            >
                {expanded ? (
                    <Minimize2Icon aria-hidden="true" />
                ) : (
                    <Maximize2Icon aria-hidden="true" />
                )}
            </TooltipTrigger>
            <TooltipContent>{label}</TooltipContent>
        </Tooltip>
    )
}
