"use client"

import { RefreshCwIcon } from "lucide-react"

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
import { Separator } from "@/components/ui/separator"
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
        refresh,
    } = useWorkspaceHome()

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
    const emptyTitle = hasActiveFilter
        ? "当前筛选没有待办"
        : "当前没有待处理事项"
    const emptyDescription = hasActiveFilter
        ? "可清除筛选后回到待我处理。"
        : "新任务到达后会出现在这里。"
    const emptyAction = hasActiveFilter ? (
        <Button type="button" variant="secondary" onClick={clearFilters}>
            回到待我处理
        </Button>
    ) : undefined

    const queueToolbar = (
        <WorkspaceQueueToolbar
            urlState={urlState}
            searchDraft={searchDraft}
            onSearchDraftChange={setSearchDraft}
            onSortChange={onSortChange}
            onSearch={applySearch}
        />
    )

    const detail = selected ? (
        <WorkspaceTaskDetail
            item={selected}
            grantedPermissions={accountProfileQuery.data?.permissions ?? []}
            onDecisionApplied={(_view, workItemId) => {
                applyDecisionAfter(workItemId)
            }}
            onTaskCompleted={applyDecisionAfter}
        />
    ) : (
        <div className="flex flex-1 items-center justify-center p-8">
            <BusinessEmptyState
                kind="no-tasks"
                title="选择一条待办开始处理"
                description="从左侧队列选中任务后，可在此核对并提交决定。"
                className="bg-transparent ring-0"
            />
        </div>
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
                    className={cn(
                        "flex min-h-0 flex-col",
                        items.length === 0
                            ? "flex-1 p-3"
                            : "w-full lg:w-80 lg:shrink-0 xl:w-96",
                    )}
                    aria-label={filterLabel}
                >
                    <header
                        className={cn(
                            "flex flex-col gap-2",
                            items.length === 0
                                ? "max-w-xl pb-3"
                                : "border-b border-grid px-3 pt-3 pb-3",
                        )}
                    >
                        <p className="sr-only" aria-live="polite">
                            {filterLabel} {view.total} 项
                        </p>
                        <WorkspaceFamilyNav
                            urlState={urlState}
                            counts={view.familyCounts}
                            onFamilyChange={onFamilyChange}
                        />
                        {queueToolbar}
                    </header>
                    {items.length === 0 ? (
                        <div className="flex flex-1 items-center justify-center p-6">
                            <BusinessEmptyState
                                kind={hasActiveFilter ? "filter" : "no-tasks"}
                                title={emptyTitle}
                                description={emptyDescription}
                                action={emptyAction}
                                className="bg-transparent ring-0"
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
                {items.length > 0 ? (
                    <>
                        <Separator
                            orientation="vertical"
                            className="hidden lg:block"
                        />
                        <div className="hidden min-h-0 min-w-0 flex-1 lg:flex lg:flex-col lg:p-5">
                            {detail}
                        </div>
                    </>
                ) : null}
            </div>

            <Sheet
                open={narrowDetailOpen && Boolean(selected)}
                onOpenChange={setNarrowDetailOpen}
                onOpenChangeComplete={setNarrowDetailSettledOpen}
            >
                <SheetContent
                    side="right"
                    size="detail"
                    className="w-full p-0 sm:max-w-lg"
                >
                    <SheetTitle className="sr-only">
                        {selected?.objectTitle ?? "任务详情"}
                    </SheetTitle>
                    <SheetDescription className="sr-only">
                        当前待办的摘要与处理动作
                    </SheetDescription>
                    {selected ? (
                        <WorkspaceTaskDetail
                            item={selected}
                            grantedPermissions={
                                accountProfileQuery.data?.permissions ?? []
                            }
                            onDecisionApplied={(_view, workItemId) => {
                                applyDecisionAfter(workItemId)
                                setNarrowDetailOpen(false)
                            }}
                            onTaskCompleted={(workItemId) => {
                                applyDecisionAfter(workItemId)
                                setNarrowDetailOpen(false)
                            }}
                        />
                    ) : null}
                </SheetContent>
            </Sheet>
        </PageScaffold>
    )
}
