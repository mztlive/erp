"use client"

import * as React from "react"
import {
    Maximize2Icon,
    Minimize2Icon,
    RefreshCwIcon,
    XIcon,
} from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    PageActions,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
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
    // WorkspaceFamilyNav,
    WorkspaceQueueScopeNav,
    // WorkspaceQueueStatusNav,
    // WorkspaceQueueToolbar,
} from "@/features/workspace/components/workspace-filter-bar"
import { WorkspaceHomeSkeleton } from "@/features/workspace/components/workspace-home-skeleton"
import { WorkspacePaneActionsProvider } from "@/features/workspace/components/workspace-pane-actions"
import { WorkspaceTaskDetail } from "@/features/workspace/components/workspace-task-detail"
import { WorkspaceTaskList } from "@/features/workspace/components/workspace-task-list"
import { useWorkspaceHome } from "@/features/workspace/hooks/use-workspace-home"
import { deriveWorkItemsFreshness } from "@/features/workspace/lib/freshness"
import { filterSummaryFor } from "@/features/workspace/lib/url-state"
import { cn } from "@/lib/utils"
import { toAutomationIdSegment } from "@/lib/automation-id"

/**
 * 工作台：页头切换待办口径，队列与作业面左右分栏，审批在右侧连续提交。
 */
export function WorkspaceHomePage() {
    return <WorkspaceHomeView home={useWorkspaceHome()} />
}

/** 工作台视图独立消费页面状态，业务请求与处理命令仍由现有 hooks 管理。 */
export function WorkspaceHomeView({
    home,
}: {
    home: ReturnType<typeof useWorkspaceHome>
}) {
    const {
        urlState,
        view,
        accountProfileQuery,
        dashboardQuery,
        refreshing,
        activeMetric,
        hasActiveFilter,
        // searchDraft,
        // setSearchDraft,
        narrowDetailOpen,
        setNarrowDetailOpen,
        setNarrowDetailSettledOpen,
        completionAnnouncement,
        selected,
        onMetricClick,
        clearFilters,
        onSelectTask,
        applyDecisionAfter,
        // onFamilyChange,
        // onSortChange,
        // applySearch,
        clearSearch,
        refresh,
    } = home
    const [detailFullscreen, setDetailFullscreen] = React.useState(false)
    const [detailHidden, setDetailHidden] = React.useState(false)
    const detailItem = detailHidden ? undefined : selected
    const selectTask = (item: NonNullable<typeof selected>) => {
        setDetailHidden(false)
        onSelectTask(item)
    }
    const closeDetail = () => {
        setDetailHidden(true)
        setDetailFullscreen(false)
        setNarrowDetailOpen(false)
        if (selected)
            document
                .getElementById(
                    "workspace-task-" +
                        toAutomationIdSegment(selected.workItemId),
                )
                ?.focus()
    }

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
    const managedView = urlState.view === "managed"
    const startedHasQuery = startedView && Boolean(urlState.query)
    const hasEffectiveFilter =
        startedHasQuery || (!startedView && hasActiveFilter)
    const emptyTitle = startedHasQuery
        ? "没有匹配的审批"
        : startedView
          ? "还没有我发起的审批"
          : managedView && !hasEffectiveFilter
            ? "范围内没有待办"
            : hasEffectiveFilter
              ? "当前筛选没有待办"
              : "当前没有待处理事项"
    const emptyDescription = startedHasQuery
        ? "可清除关键词后查看全部我发起的审批。"
        : startedView
          ? "你发起的审批会在这里持续显示当前节点、审批人和处理状态。"
          : managedView && !hasEffectiveFilter
            ? "这里列出你权限范围内尚未完成的任务，不限于派给你本人处理的事项。"
            : hasEffectiveFilter
              ? managedView
                  ? "可清除筛选后回到范围内待办。"
                  : "可清除筛选后回到待我处理。"
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
            清除筛选
        </Button>
    ) : undefined

    // 暂停展示搜索、类型、超期和排序控件；恢复时一并取消相关导入与状态解构的注释。
    // const queueToolbar = (
    //     <WorkspaceQueueToolbar
    //         urlState={urlState}
    //         searchDraft={searchDraft}
    //         onSearchDraftChange={setSearchDraft}
    //         onSortChange={onSortChange}
    //         onSearch={applySearch}
    //         onClearSearch={clearSearch}
    //         showSort={!startedView}
    //         searchAriaLabel={startedView ? "搜索我发起的审批" : "搜索待办"}
    //         filters={
    //             !startedView ? (
    //                 <>
    //                     <WorkspaceFamilyNav
    //                         urlState={urlState}
    //                         counts={view.familyCounts}
    //                         onFamilyChange={onFamilyChange}
    //                     />
    //                     <WorkspaceQueueStatusNav
    //                         metrics={metrics}
    //                         activeMetric={activeMetric}
    //                         onMetricClick={onMetricClick}
    //                     />
    //                 </>
    //             ) : undefined
    //         }
    //     />
    // )

    const paneActions = detailItem ? (
        <>
            <WorkspaceDetailFullscreenButton
                expanded={detailFullscreen}
                onToggle={() => setDetailFullscreen((current) => !current)}
            />
            <Button
                id="workspace-detail-close"
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="关闭详情"
                onClick={closeDetail}
            >
                <XIcon aria-hidden="true" />
            </Button>
        </>
    ) : null

    const detail = detailItem ? (
        <WorkspacePaneActionsProvider actions={paneActions}>
            <WorkspaceTaskDetail
                item={detailItem}
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
    ) : null

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
                actions={
                    <div className="flex flex-wrap items-center gap-3">
                        <DataFreshness
                            updatedAt={workItemsFreshness.updatedAtLabel}
                            dateTime={workItemsFreshness.dateTime}
                            state={workItemsFreshness.state}
                            statusLabel={workItemsFreshness.statusLabel}
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

            <div className="border-b border-border pb-2">
                <WorkspaceQueueScopeNav
                    metrics={metrics}
                    activeMetric={activeMetric}
                    onMetricClick={onMetricClick}
                />
            </div>
            {/* 暂停展示筛选栏：{queueToolbar} */}
            {hasEffectiveFilter ? (
                <section aria-label="当前筛选" className="space-y-2">
                    <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                        <span>
                            {[
                                urlState.query
                                    ? "关键词：" + urlState.query
                                    : undefined,
                                !startedView && urlState.due === "today"
                                    ? "今日截止"
                                    : undefined,
                                !startedView && urlState.workItemType
                                    ? "已限定任务类型"
                                    : undefined,
                            ]
                                .filter(Boolean)
                                .join(" · ") || "筛选已生效"}
                        </span>
                        <Button
                            id="workspace-queue-reset-filters"
                            type="button"
                            variant="ghost"
                            size="xs"
                            onClick={startedView ? clearSearch : clearFilters}
                        >
                            清除筛选
                        </Button>
                    </div>
                </section>
            ) : null}
            <div
                className={cn(
                    surfacePanelClassName,
                    "flex min-h-0 flex-1 overflow-hidden border border-border/70",
                )}
            >
                <section
                    data-slot="workspace-queue"
                    className={cn(
                        "flex min-h-0 min-w-0 flex-1 flex-col",
                        detailFullscreen && "xl:hidden",
                        detailItem &&
                            !narrowDetailOpen &&
                            !detailFullscreen &&
                            "xl:w-[420px] xl:flex-none 2xl:w-[450px]",
                    )}
                    aria-label={filterLabel}
                >
                    {items.length === 0 ? (
                        <div className="flex min-h-72 flex-1 items-center justify-center px-6 py-12">
                            <BusinessEmptyState
                                kind={
                                    hasEffectiveFilter ? "filter" : "no-tasks"
                                }
                                title={emptyTitle}
                                description={emptyDescription}
                                action={emptyAction}
                                className="max-w-sm bg-transparent p-0 ring-0"
                            />
                        </div>
                    ) : (
                        <WorkspaceTaskList
                            items={items}
                            selectedWorkItemId={detailItem?.workItemId}
                            onSelect={selectTask}
                            tracking={startedView}
                        />
                    )}
                    <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-grid px-5 py-3 text-xs text-muted-foreground">
                        <span role="status">
                            {refreshing
                                ? "正在更新…"
                                : "共 " +
                                  view.total.toLocaleString("zh-CN") +
                                  (startedView ? " 条审批" : " 条待办")}
                        </span>
                        {view.total > items.length ? (
                            <span>已显示 {items.length} 条</span>
                        ) : null}
                    </footer>
                </section>
                {detailItem && !narrowDetailOpen ? (
                    <section
                        data-slot="workspace-detail"
                        aria-label={startedView ? "审批详情" : "任务详情"}
                        className={cn(
                            "hidden min-h-0 min-w-0 flex-col border-l border-grid bg-card xl:flex",
                            detailFullscreen ? "flex-1 border-l-0" : "flex-1",
                        )}
                    >
                        {detail}
                    </section>
                ) : null}
            </div>

            <Sheet
                open={narrowDetailOpen && Boolean(detailItem)}
                onOpenChange={setNarrowDetailOpen}
                onOpenChangeComplete={setNarrowDetailSettledOpen}
            >
                <SheetContent
                    side="right"
                    size="detail"
                    closeButtonId="workspace-detail-sheet-close"
                    className="w-full p-0 sm:max-w-lg [&_[data-slot=workspace-task-header]]:px-7 [&_[data-slot=workspace-task-header]]:pt-10 [&_[data-slot=workspace-task-header]]:pr-14 [&_[data-slot=workspace-task-header]_h2]:leading-8 [&_[data-slot=workspace-task-footer]]:px-7 [&_[data-slot=workspace-task-footer]]:py-4"
                >
                    <SheetTitle className="sr-only">
                        {selected?.objectTitle ?? "任务详情"}
                    </SheetTitle>
                    <SheetDescription className="sr-only">
                        {startedView
                            ? "当前审批的进度与处理记录"
                            : "当前待办的摘要与处理动作"}
                    </SheetDescription>
                    {detailItem ? (
                        <WorkspacePaneActionsProvider actions={null}>
                            <WorkspaceTaskDetail
                                item={detailItem}
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
