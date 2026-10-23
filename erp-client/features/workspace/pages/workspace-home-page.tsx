"use client"

import { useSearchParams } from "next/navigation"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { ApprovalCopiesPage } from "@/features/approval-copies/page"
import { RefreshCwIcon } from "lucide-react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    WorkspaceQueueScopeNav,
    WorkspaceQueueToolbar,
} from "../components/workspace-filter-bar"
import { workspaceCategoryLabel } from "../lib/category-label"
import { WorkspaceHomeSkeleton } from "../components/workspace-home-skeleton"
import { WorkspaceQueueTypeNav } from "../components/workspace-queue-type-nav"
import { WorkspaceQueueFilters } from "../components/workspace-queue-filters"
import { WorkspaceTaskList } from "../components/workspace-task-list"
import { useWorkspaceHome } from "../hooks/use-workspace-home"
import { deriveWorkItemsFreshness } from "../lib/freshness"
import { filterSummaryFor, buildWorkspaceSearchParams } from "../lib/url-state"

export function WorkspaceHomePage() {
    const params = useSearchParams()
    const profile = useAccountProfileQuery().data
    if (
        params.get("view") === "copied" ||
        (profile?.account_kind === "admin" &&
            !hasPermission(profile.permissions, "work_item:list"))
    )
        return <ApprovalCopiesPage />
    return <WorkspaceTaskHome />
}
function WorkspaceTaskHome() {
    return <WorkspaceHomeView home={useWorkspaceHome()} />
}

/** 工作台只承载队列；任务与审批由独立详情路由承载。 */
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
        searchDraft,
        setSearchDraft,
        onMetricClick,
        clearFilters,
        onSelectTask,
        onQueueFiltersChange,
        onSortChange,
        applySearch,
        onPageChange,
        clearSearch,
        refresh,
    } = home
    if (accountProfileQuery.isError || (dashboardQuery.isError && !view))
        return (
            <PageScaffold>
                <BusinessFailureState
                    error={accountProfileQuery.error ?? dashboardQuery.error}
                    action={
                        <Button id="workspace-home-retry" onClick={refresh}>
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    if (!view) return <WorkspaceHomeSkeleton />
    if (view.access !== "allowed")
        return (
            <PageScaffold>
                <BusinessFailureState
                    kind="permission"
                    title={
                        view.access === "forbidden"
                            ? "无工作台权限"
                            : "当前角色无数据范围"
                    }
                    description="请联系管理员确认工作台权限与数据范围。"
                />
            </PageScaffold>
        )
    const started = urlState.view === "started"
    const freshness = deriveWorkItemsFreshness(view.freshness, { refreshing })
    const filterText = [
        urlState.workItemType &&
            workspaceCategoryLabel(urlState.workItemType, started),
        urlState.query && `关键词：${urlState.query}`,
        urlState.due === "today" && "今日到期",
        urlState.due === "overdue" && "已超期",
        urlState.blocked && "处理受阻",
        urlState.handlerUserIds && "已限定处理人",
        urlState.salesOrderIds && "已限定来源销售单",
        urlState.purchaseOrderIds && "已限定来源采购单",
    ]
        .filter(Boolean)
        .join(" · ")
    return (
        <PageScaffold density="compact" className="min-h-0 gap-4">
            <PageHeader
                title={
                    started
                        ? "我发起的审批"
                        : urlState.workItemType
                          ? workspaceCategoryLabel(urlState.workItemType)
                          : filterSummaryFor(activeMetric)
                }
                actions={
                    <div className="flex flex-wrap items-center gap-3">
                        <DataFreshness
                            updatedAt={freshness.updatedAtLabel}
                            dateTime={freshness.dateTime}
                            state={freshness.state}
                            statusLabel={freshness.statusLabel}
                        />
                        <Button
                            id="workspace-home-refresh"
                            variant="ghost"
                            size="sm"
                            disabled={refreshing}
                            onClick={refresh}
                        >
                            <RefreshCwIcon />
                            刷新
                        </Button>
                    </div>
                }
            />
            <div className="flex flex-wrap items-center gap-4">
                <WorkspaceQueueScopeNav
                    metrics={view.metrics}
                    activeMetric={activeMetric}
                    onMetricClick={onMetricClick}
                />
            </div>
            <section
                aria-label="队列查询条件"
                className="space-y-3 rounded-xl border border-border/60 bg-muted/15 p-4"
            >
                <WorkspaceQueueToolbar
                    urlState={urlState}
                    searchDraft={searchDraft}
                    onSearchDraftChange={setSearchDraft}
                    onSortChange={onSortChange}
                    onSearch={applySearch}
                    onClearSearch={clearSearch}
                    showSort={!started}
                    loading={refreshing}
                    resultCount={view.total}
                    resultNoun={started ? "条审批" : "条待办"}
                    filters={
                        !started ? (
                            <WorkspaceQueueFilters
                                state={urlState}
                                onChange={onQueueFiltersChange}
                            />
                        ) : undefined
                    }
                />
                <WorkspaceQueueTypeNav
                    state={urlState}
                    counts={view.workItemTypeCounts}
                    total={
                        view.metrics.find(
                            (metric) =>
                                metric.key === (started ? "started" : "inbox"),
                        )?.count
                    }
                    onChange={(workItemType) =>
                        onQueueFiltersChange({ workItemType })
                    }
                />
                {hasActiveFilter && (
                    <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                        <span>{filterText || "已限定任务分类"}</span>
                        <Button
                            id="workspace-queue-reset-filters"
                            variant="ghost"
                            size="xs"
                            onClick={clearFilters}
                        >
                            清除筛选
                        </Button>
                    </div>
                )}
            </section>
            {dashboardQuery.isError && (
                <BusinessFailureState
                    error={dashboardQuery.error}
                    title="工作台更新失败"
                    description="当前显示上次加载的结果，请刷新后重试。"
                />
            )}
            <section
                data-slot="workspace-queue"
                aria-label="任务队列"
                className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-border/60 bg-card"
            >
                {view.items.length ? (
                    <WorkspaceTaskList
                        items={view.items}
                        onSelect={onSelectTask}
                        tracking={started}
                        workItemType={urlState.workItemType}
                        scrollKey={buildWorkspaceSearchParams(urlState)}
                    />
                ) : (
                    <BusinessEmptyState
                        kind="no-tasks"
                        title={
                            hasActiveFilter
                                ? "没有符合条件的事项"
                                : "当前没有待处理事项"
                        }
                        description={
                            hasActiveFilter
                                ? "可调整条件或清除筛选。"
                                : "新事项到达后会显示在这里。"
                        }
                    />
                )}
                <footer className="flex shrink-0 items-center justify-between gap-3 border-t px-5 py-3 text-xs text-muted-foreground">
                    <span>
                        共 {view.total} {started ? "条审批" : "条待办"}
                    </span>
                    {!started &&
                        (view.total > 50 || (urlState.page ?? 1) > 1) && (
                            <div className="flex items-center gap-2">
                                <Button
                                    id="workspace-queue-page-previous"
                                    size="xs"
                                    variant="ghost"
                                    disabled={
                                        (urlState.page ?? 1) <= 1 || refreshing
                                    }
                                    onClick={() =>
                                        onPageChange((urlState.page ?? 1) - 1)
                                    }
                                >
                                    上一页
                                </Button>
                                <span>第 {urlState.page ?? 1} 页</span>
                                <Button
                                    id="workspace-queue-page-next"
                                    size="xs"
                                    variant="ghost"
                                    disabled={
                                        (urlState.page ?? 1) * 50 >=
                                            view.total || refreshing
                                    }
                                    onClick={() =>
                                        onPageChange((urlState.page ?? 1) + 1)
                                    }
                                >
                                    下一页
                                </Button>
                            </div>
                        )}
                </footer>
            </section>
        </PageScaffold>
    )
}
