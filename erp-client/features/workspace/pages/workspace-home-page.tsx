"use client"

import { RefreshCwIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    MetricFilterItem,
    MetricStrip,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Sheet, SheetContent } from "@/components/ui/sheet"
import { WorkspaceFilterBar } from "@/features/workspace/components/workspace-filter-bar"
import { WorkspaceHomeSkeleton } from "@/features/workspace/components/workspace-home-skeleton"
import { WorkspaceTaskDetail } from "@/features/workspace/components/workspace-task-detail"
import { WorkspaceTaskList } from "@/features/workspace/components/workspace-task-list"
import { useWorkspaceHome } from "@/features/workspace/hooks/use-workspace-home"
import {
    deriveProjectionFreshness,
    deriveWorkItemsFreshness,
    greetingForNow,
} from "@/features/workspace/lib/freshness"
import { filterSummaryFor } from "@/features/workspace/lib/url-state"

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
    const filterLabel = filterSummaryFor(activeMetric)
    const metrics = view.metrics.filter((metric) => metric.visible)
    const items = view.items
    const detail = selected ? (
        <WorkspaceTaskDetail
            item={selected}
            onDecisionApplied={(_view, workItemId) => {
                applyDecisionAfter(workItemId)
            }}
        />
    ) : (
        <BusinessEmptyState
            kind="no-tasks"
            title={hasActiveFilter ? "当前筛选没有待办" : "当前没有待处理事项"}
            description={
                hasActiveFilter
                    ? "可清除筛选后回到待我处理。"
                    : "新任务到达后会出现在左侧列表。"
            }
            action={
                hasActiveFilter ? (
                    <Button
                        type="button"
                        variant="outline"
                        onClick={clearFilters}
                    >
                        回到待我处理
                    </Button>
                ) : undefined
            }
        />
    )

    return (
        <PageScaffold>
            <PageHeader
                title={greetingForNow(view.viewer.displayName)}
                description={`${view.viewer.activeRoleLabel}工作台`}
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

            <WorkspaceFilterBar
                urlState={urlState}
                searchDraft={searchDraft}
                onSearchDraftChange={setSearchDraft}
                onFamilyChange={onFamilyChange}
                onSortChange={onSortChange}
                onSearch={applySearch}
            />

            <div className="hidden min-h-[32rem] gap-4 lg:grid lg:grid-cols-[minmax(18rem,38%)_minmax(0,62%)]">
                <section
                    className="overflow-hidden rounded-lg border border-border"
                    aria-labelledby="workspace-task-main-title"
                >
                    <header className="border-b border-grid px-3 py-2">
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
                    {items.length === 0 ? (
                        <div className="p-4">
                            <BusinessEmptyState
                                kind="no-tasks"
                                title={
                                    hasActiveFilter
                                        ? "当前筛选没有待办"
                                        : "当前没有待处理事项"
                                }
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
                <div className="rounded-lg border border-border p-4">
                    {detail}
                </div>
            </div>

            <div className="lg:hidden">
                <section className="overflow-hidden rounded-lg border border-border">
                    <header className="border-b border-grid px-3 py-2">
                        <h2 className="text-sm font-medium">{filterLabel}</h2>
                    </header>
                    <WorkspaceTaskList
                        items={items}
                        selectedWorkItemId={selected?.workItemId}
                        onSelect={onSelectTask}
                    />
                </section>
                <Sheet
                    open={narrowDetailOpen && Boolean(selected)}
                    onOpenChange={setNarrowDetailOpen}
                >
                    <SheetContent side="right" className="w-full sm:max-w-lg">
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
            </div>
        </PageScaffold>
    )
}
