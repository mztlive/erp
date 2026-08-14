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
import { RecentPanel } from "@/features/workspace/components/recent-panel"
import { ScopeSwitcher } from "@/features/workspace/components/scope-switcher"
import { TaskListCard } from "@/features/workspace/components/task-list-card"
import { WarningsPanel } from "@/features/workspace/components/warnings-panel"
import { WorkspaceHomeSkeleton } from "@/features/workspace/components/workspace-home-skeleton"
import { useWorkspaceHome } from "@/features/workspace/hooks/use-workspace-home"
import {
    deriveProjectionFreshness,
    deriveWorkItemsFreshness,
    greetingForNow,
} from "@/features/workspace/lib/freshness"
import {
    buildTaskQueueHref,
    filterSummaryFor,
} from "@/features/workspace/lib/url-state"

export function WorkspaceHomePage() {
    const {
        urlState,
        view,
        accountProfileQuery,
        dashboardQuery,
        refreshing,
        focusedStableNumber,
        activeMetric,
        hasActiveFilter,
        onMetricClick,
        clearFilters,
        onOpenTask,
        refresh,
        onScopeChange,
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
                        <ScopeSwitcher
                            scope={urlState.scope}
                            onScopeChange={onScopeChange}
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
                <TaskListCard
                    view={view}
                    urlState={urlState}
                    filterLabel={filterLabel}
                    visibleCount={visibleCount}
                    hasActiveFilter={hasActiveFilter}
                    taskQueueHref={taskQueueHref}
                    asyncStatus={asyncStatus}
                    hasRefreshError={dashboardQuery.isError}
                    focusStableNumber={focusedStableNumber ?? undefined}
                    onOpenTask={onOpenTask}
                    clearFilters={clearFilters}
                    refresh={refresh}
                />

                <div className="space-y-3 md:space-y-4">
                    <WarningsPanel warnings={view.warnings} />
                    <RecentPanel recent={view.recent} />
                </div>
            </div>
        </PageScaffold>
    )
}
