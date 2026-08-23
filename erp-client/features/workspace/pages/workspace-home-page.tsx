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
import {
    Sheet,
    SheetContent,
    SheetDescription,
    SheetTitle,
} from "@/components/ui/sheet"
import { WorkspaceFilterBar } from "@/features/workspace/components/workspace-filter-bar"
import { WorkspaceHomeSkeleton } from "@/features/workspace/components/workspace-home-skeleton"
import { WorkspaceTaskDetail } from "@/features/workspace/components/workspace-task-detail"
import { WorkspaceTaskList } from "@/features/workspace/components/workspace-task-list"
import { useWorkspaceHome } from "@/features/workspace/hooks/use-workspace-home"
import { deriveWorkItemsFreshness } from "@/features/workspace/lib/freshness"
import { filterSummaryFor } from "@/features/workspace/lib/url-state"
import { cn } from "@/lib/utils"

/**
 * 唯一工作台：指标筛选左列，审批决定在右侧详情连续提交。
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
        <Button type="button" variant="outline" onClick={clearFilters}>
            回到待我处理
        </Button>
    ) : undefined

    const detail = selected ? (
        <WorkspaceTaskDetail
            item={selected}
            onDecisionApplied={(_view, workItemId) => {
                applyDecisionAfter(workItemId)
            }}
        />
    ) : (
        <div className="flex flex-1 items-center justify-center p-6">
            <BusinessEmptyState
                kind="no-tasks"
                title="选择一条待办开始处理"
                description="从左侧列表选中任务后，可在此查看摘要并处理。"
                className="bg-transparent ring-0"
            />
        </div>
    )

    return (
        <PageScaffold className="min-h-0">
            <PageHeader
                title="我的工作台"
                description={`当前共 ${view.total} 项`}
                metadata={
                    <DataFreshness
                        updatedAt={workItemsFreshness.updatedAtLabel}
                        dateTime={workItemsFreshness.dateTime}
                        state={workItemsFreshness.state}
                        statusLabel={workItemsFreshness.statusLabel}
                    />
                }
                actions={
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
                }
            />

            <div
                className={cn(
                    surfacePanelClassName,
                    "flex min-h-0 flex-1 flex-col overflow-hidden",
                )}
            >
                <WorkspaceFilterBar
                    urlState={urlState}
                    metrics={metrics}
                    activeMetric={activeMetric}
                    searchDraft={searchDraft}
                    onSearchDraftChange={setSearchDraft}
                    onMetricClick={onMetricClick}
                    onFamilyChange={onFamilyChange}
                    onSortChange={onSortChange}
                    onSearch={applySearch}
                />

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
                    <div className="flex min-h-0 flex-1">
                        <section
                            className="flex min-h-0 w-full flex-col overflow-hidden lg:w-[min(24rem,38%)] lg:shrink-0 lg:border-r lg:border-border/30"
                            aria-labelledby="workspace-task-main-title"
                        >
                            <header className="border-b border-border/30 px-3 py-2">
                                <h2
                                    id="workspace-task-main-title"
                                    className="text-sm font-medium"
                                >
                                    {filterLabel}
                                </h2>
                                <p
                                    className="text-xs text-muted-foreground"
                                    aria-live="polite"
                                >
                                    当前共 {view.total} 项
                                </p>
                            </header>
                            <WorkspaceTaskList
                                items={items}
                                selectedWorkItemId={selected?.workItemId}
                                onSelect={onSelectTask}
                            />
                        </section>
                        <div className="hidden min-h-0 min-w-0 flex-1 lg:flex lg:flex-col">
                            {detail}
                        </div>
                    </div>
                )}
            </div>

            <Sheet
                open={narrowDetailOpen && Boolean(selected)}
                onOpenChange={setNarrowDetailOpen}
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
                            onDecisionApplied={(_view, workItemId) => {
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
