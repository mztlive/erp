"use client"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import Link from "next/link"
import {
    CalendarClockIcon,
    ClockAlertIcon,
    ListFilterIcon,
    MailIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { cn } from "@/lib/utils"
import { WorkspaceSearchPanel } from "./workspace-search-panel"
import { WorkspaceTaskList } from "./workspace-task-list"
import { buildWorkspaceSearchParams } from "../lib/url-state"
import type { useWorkspaceHome } from "../hooks/use-workspace-home"
import type { TodayWorkspaceView, WorkspaceWorkItem } from "../types"
import { hasPermission } from "@/lib/permissions"
import { WorkspaceSortMenu } from "./workspace-sort-menu"
import {
    ListWorkSurface,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { WorkspaceTypeNav } from "./workspace-type-nav"
import { useWorkspaceQueueCounts } from "../hooks/use-workspace-queue-counts"
import { workspaceQueueSubjects } from "../hooks/queries"
import { queueQuickTypes, queueTypeLabel } from "../lib/queue-presentation"

export function WorkspaceQueuePanel({
    home,
    onSelectTask,
}: {
    home: ReturnType<typeof useWorkspaceHome>
    onSelectTask: (item: WorkspaceWorkItem) => void
}) {
    const { view, accountProfileQuery, dashboardQuery, refresh } = home
    if (accountProfileQuery.isError || (dashboardQuery.isError && !view))
        return (
            <BusinessFailureState
                idPrefix="workspace-queue-load-failure"
                error={accountProfileQuery.error ?? dashboardQuery.error}
                readContext={{
                    subject: accountProfileQuery.isError
                        ? "账号资料"
                        : workspaceQueueSubjects[home.urlState.view],
                }}
                onRetry={refresh}
            />
        )
    if (
        accountProfileQuery.data &&
        !hasPermission(
            accountProfileQuery.data.permissions,
            home.urlState.view === "started"
                ? "approval_instance:read"
                : "work_item:list",
        )
    )
        return (
            <section aria-label="我的事项">
                <BusinessEmptyState
                    kind="no-tasks"
                    className={listWorkspaceEmptyStateClassName}
                    title="当前角色没有待办入口"
                    description="可返回工作台，或查看抄送给你的审批。"
                    action={
                        accountProfileQuery.data.account_kind === "admin" ? (
                            <Button
                                id="workspace-copy-link"
                                variant="outline"
                                render={<Link href="/workspace/copies" />}
                            >
                                <MailIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                抄送给我
                            </Button>
                        ) : undefined
                    }
                />
            </section>
        )
    if (!view)
        return (
            <div aria-busy="true" className="flex flex-col gap-3">
                <span role="status" className="sr-only">
                    正在读取事项…
                </span>
                <Skeleton className="h-10 w-full" />
                <Skeleton className="h-96 w-full rounded-none" />
            </div>
        )
    return (
        <WorkspaceQueueContent
            home={home}
            view={view}
            onSelectTask={onSelectTask}
        />
    )
}

/** 范围导航与查询表单分离，表格工具栏独占排序及列设置。 */
function WorkspaceQueueContent({
    home,
    view,
    onSelectTask,
}: {
    home: ReturnType<typeof useWorkspaceHome>
    view: TodayWorkspaceView
    onSelectTask: (item: WorkspaceWorkItem) => void
}) {
    const {
        accountProfileQuery,
        dashboardQuery,
        urlState,
        refreshing,
        hasActiveFilter,
        clearFilters,
        onSortChange,
        applyFilters,
        onPageChange,
        refresh,
    } = home
    const options = view.filterOptions
    const types = queueQuickTypes(options?.item_types ?? [])
    const counts = useWorkspaceQueueCounts(
        urlState,
        types,
        view.total,
        accountProfileQuery.data,
    )
    const filters = urlState.filters ?? {}
    const selectedTypes = filters.item_types ?? []
    const started = urlState.view === "started"
    const history = urlState.view === "history"
    const selectedLabel =
        selectedTypes.length === 1
            ? options?.item_types?.find(
                  (option) => option.value === selectedTypes[0],
              )
            : undefined
    const title = selectedLabel
        ? queueTypeLabel(selectedLabel)
        : selectedTypes.length
          ? "已选事项"
          : "全部事项"
    const quickFilters =
        (!started && !history) || counts.failure ? (
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                {!started && !history && (
                    <div
                        role="group"
                        aria-label="待办期限快捷筛选"
                        className="flex flex-wrap items-center gap-1"
                    >
                        {([undefined, "overdue", "today"] as const).map(
                            (due) => (
                                <Button
                                    key={due ?? "all"}
                                    id={`workspace-queue-due-${due ?? "all"}`}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    aria-pressed={filters.due === due}
                                    className={cn(
                                        listWorkspaceStyles.quietButton,
                                        filters.due === due &&
                                            "bg-sidebar-accent/70 font-medium text-sidebar-accent-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
                                    )}
                                    onClick={() =>
                                        applyFilters(urlState.query ?? "", {
                                            ...filters,
                                            due,
                                            ...(due
                                                ? {
                                                      date_field: undefined,
                                                      date_from: undefined,
                                                      date_to: undefined,
                                                      undated: undefined,
                                                  }
                                                : {}),
                                        })
                                    }
                                >
                                    {due === "overdue" ? (
                                        <ClockAlertIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                    ) : due === "today" ? (
                                        <CalendarClockIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                    ) : (
                                        <ListFilterIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                    )}
                                    {due === "overdue"
                                        ? "已超期"
                                        : due === "today"
                                          ? "今日到期"
                                          : "全部"}
                                    <span
                                        className={
                                            due === "overdue" &&
                                            (counts.counts.overdue ?? 0) > 0
                                                ? "num text-destructive"
                                                : "num"
                                        }
                                    >
                                        {counts.counts[due ?? "due-all"] ?? "—"}
                                    </span>
                                </Button>
                            ),
                        )}
                    </div>
                )}
                {counts.retry ? (
                    <Button
                        id="workspace-queue-count-retry"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={listWorkspaceStyles.quietButton}
                        onClick={counts.retry}
                    >
                        数量加载失败，重试
                    </Button>
                ) : counts.failure ? (
                    <p role="status" className="text-xs text-muted-foreground">
                        {counts.failure.title}
                    </p>
                ) : null}
            </div>
        ) : undefined
    return (
        <ListWorkSurface
            ariaLabel="我的事项"
            tableClassName="flex min-w-0 flex-col"
            toolbarClassName="[&>[data-slot=table-toolbar-inline-controls]]:self-start"
            views={
                <WorkspaceTypeNav
                    state={urlState}
                    options={options ?? {}}
                    types={types}
                    counts={counts.counts}
                    onChange={(item_types) =>
                        applyFilters(urlState.query ?? "", {
                            ...filters,
                            item_types,
                        })
                    }
                />
            }
            toolbar={
                <WorkspaceSearchPanel
                    state={urlState}
                    options={options ?? {}}
                    loading={refreshing}
                    failed={dashboardQuery.isError}
                    hasActiveFilters={hasActiveFilter}
                    quickFilters={quickFilters}
                    onApply={applyFilters}
                    onReset={clearFilters}
                    onScopeChange={home.onMetricClick}
                />
            }
            tableActions={
                !started && (
                    <WorkspaceSortMenu
                        value={urlState.sort}
                        onValueChange={onSortChange}
                    />
                )
            }
            table={
                <>
                    {dashboardQuery.isError && (
                        <div className="shrink-0 px-(--scaffold-px) py-workspace-toolbar-block">
                            <BusinessFailureState
                                idPrefix="workspace-queue-refresh-failure"
                                error={dashboardQuery.error}
                                title={`${workspaceQueueSubjects[urlState.view]}刷新失败`}
                                readContext={{
                                    subject:
                                        workspaceQueueSubjects[urlState.view],
                                    impact: "当前显示上次加载的列表。",
                                }}
                                onRetry={refresh}
                            />
                        </div>
                    )}
                    <section
                        data-slot="workspace-queue"
                        aria-label="任务队列"
                        className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
                    >
                        <h2
                            id="workspace-queue-heading"
                            tabIndex={-1}
                            className="sr-only"
                        >
                            {title}
                        </h2>
                        <WorkspaceTaskList
                            items={view.items}
                            onSelect={onSelectTask}
                            tracking={started}
                            history={history}
                            selectedTypes={selectedTypes}
                            scrollKey={buildWorkspaceSearchParams(urlState)}
                            total={view.total}
                            page={urlState.page ?? 1}
                            onPageChange={onPageChange}
                            loading={refreshing}
                            noScope={view.access === "no_data_scope"}
                            hasActiveFilters={hasActiveFilter}
                            onClearFilters={clearFilters}
                        />
                    </section>
                </>
            }
        />
    )
}
