"use client"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { WorkspaceQueueScopeNav } from "./workspace-filter-bar"
import { WorkspaceSearchPanel } from "./workspace-search-panel"
import { WorkspaceTaskList } from "./workspace-task-list"
import { buildWorkspaceSearchParams } from "../lib/url-state"
import type { useWorkspaceHome } from "../hooks/use-workspace-home"
import type { WorkspaceWorkItem } from "../types"
import { hasPermission } from "@/lib/permissions"
import { CopyNavLink } from "@/features/approval-copies/page"
import { WorkspaceSortMenu } from "./workspace-sort-menu"

export function WorkspaceQueuePanel({
    home,
    onSelectTask,
}: {
    home: ReturnType<typeof useWorkspaceHome>
    onSelectTask: (item: WorkspaceWorkItem) => void
}) {
    const {
        view,
        accountProfileQuery,
        dashboardQuery,
        urlState,
        refreshing,
        activeMetric,
        hasActiveFilter,
        onMetricClick,
        clearFilters,
        onSortChange,
        applyFilters,
        onPageChange,
        refresh,
    } = home
    if (accountProfileQuery.isError || (dashboardQuery.isError && !view))
        return (
            <BusinessFailureState
                error={accountProfileQuery.error ?? dashboardQuery.error}
                onRetry={refresh}
            />
        )
    if (
        accountProfileQuery.data &&
        !hasPermission(accountProfileQuery.data.permissions, "work_item:list")
    )
        return (
            <section aria-label="我的事项" className="space-y-4">
                <h2 className="text-sm font-medium">我的事项</h2>
                <BusinessEmptyState
                    kind="no-tasks"
                    title="当前角色没有待办入口"
                    description="可继续查看上方业务指标，或查看抄送给你的审批。"
                    action={<CopyNavLink />}
                />
            </section>
        )
    if (!view)
        return (
            <p role="status" className="p-5 text-sm text-muted-foreground">
                正在读取待办事项…
            </p>
        )
    const started = urlState.view === "started"
    return (
        <section
            aria-label="我的事项"
            className="flex min-h-128 min-w-0 flex-1 flex-col overflow-hidden rounded-xl border border-border/60 bg-card"
        >
            <header className="flex shrink-0 flex-wrap items-start justify-between gap-2 px-4 pt-4 pb-2">
                <div className="flex min-w-0 items-center gap-2">
                    <h2
                        id="workspace-queue-heading"
                        tabIndex={-1}
                        className="text-sm font-medium"
                    >
                        我的事项
                    </h2>
                    <span
                        role="status"
                        className="text-xs tabular-nums text-muted-foreground"
                    >
                        {refreshing
                            ? "更新中…"
                            : `${view.total} ${started ? "条审批" : "条事项"}`}
                    </span>
                </div>
                {!started && (
                    <WorkspaceSortMenu
                        value={urlState.sort}
                        onValueChange={onSortChange}
                    />
                )}
            </header>
            <div className="shrink-0 border-b border-border/60 px-4 pb-2">
                <WorkspaceQueueScopeNav
                    metrics={view.metrics}
                    activeMetric={activeMetric}
                    onMetricClick={onMetricClick}
                />
            </div>
            <section
                aria-label="队列查询条件"
                className="shrink-0 border-b border-border/60 bg-muted/15 px-4 py-3"
            >
                <WorkspaceSearchPanel
                    state={urlState}
                    options={view.filterOptions ?? {}}
                    loading={refreshing}
                    failed={dashboardQuery.isError}
                    onApply={applyFilters}
                    onReset={clearFilters}
                />
            </section>
            {dashboardQuery.isError && (
                <div className="shrink-0 px-4 py-3">
                    <BusinessFailureState
                        error={dashboardQuery.error}
                        title="工作台更新失败"
                        description="当前显示上次加载的结果，请刷新后重试。"
                    />
                </div>
            )}
            <section
                data-slot="workspace-queue"
                aria-label="任务队列"
                className="flex min-h-0 flex-1 flex-col overflow-hidden"
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
                                : urlState.view === "history"
                                  ? "暂无处理记录"
                                  : "当前没有待处理事项"
                        }
                        description={
                            hasActiveFilter
                                ? "可调整条件或清除筛选。"
                                : "新事项到达后会显示在这里。"
                        }
                    />
                )}
                {(view.total > 50 || (urlState.page ?? 1) > 1) && (
                    <footer className="flex shrink-0 items-center justify-end gap-3 border-t px-4 py-3 text-xs text-muted-foreground">
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
                                    (urlState.page ?? 1) * 50 >= view.total ||
                                    refreshing
                                }
                                onClick={() =>
                                    onPageChange((urlState.page ?? 1) + 1)
                                }
                            >
                                下一页
                            </Button>
                        </div>
                    </footer>
                )}
            </section>
        </section>
    )
}
