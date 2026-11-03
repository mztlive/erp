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
import { WorkspaceSearchPanel } from "./workspace-search-panel"
import { WorkspaceTaskList } from "./workspace-task-list"
import { buildWorkspaceSearchParams } from "../lib/url-state"
import type { useWorkspaceHome } from "../hooks/use-workspace-home"
import type { TodayWorkspaceView, WorkspaceWorkItem } from "../types"
import { hasPermission } from "@/lib/permissions"
import { WorkspaceSortMenu } from "./workspace-sort-menu"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { WorkspaceTypeNav } from "./workspace-type-nav"
import { useWorkspaceQueueCounts } from "../hooks/use-workspace-queue-counts"
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
                error={accountProfileQuery.error ?? dashboardQuery.error}
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
            <section aria-label="我的事项" className="space-y-4">
                <h2 className="text-sm font-medium">我的事项</h2>
                <BusinessEmptyState
                    kind="no-tasks"
                    title="当前角色没有待办入口"
                    description="可返回工作台，或查看抄送给你的审批。"
                    action={
                        accountProfileQuery.data.account_kind === "admin" ? (
                            <Button
                                id="workspace-copy-link"
                                variant="outline"
                                render={<Link href="/workspace/copies" />}
                            >
                                <MailIcon aria-hidden="true" />
                                抄送给我
                            </Button>
                        ) : undefined
                    }
                />
            </section>
        )
    if (!view)
        return (
            <p role="status" className="p-5 text-sm text-muted-foreground">
                正在读取待办事项…
            </p>
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
    const options = view.filterOptions ?? {}
    const types = queueQuickTypes(options.item_types ?? [])
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
            ? options.item_types?.find(
                  (option) => option.value === selectedTypes[0],
              )
            : undefined
    const title = selectedLabel
        ? queueTypeLabel(selectedLabel)
        : selectedTypes.length
          ? "已选事项"
          : "全部事项"
    return (
        <div className="flex min-h-128 min-w-0 flex-1 flex-col gap-3">
            <section
                aria-label="我的事项"
                className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-xl border border-border/60 bg-card"
            >
                <TableToolbarScope>
                    <WorkspaceTypeNav
                        state={urlState}
                        options={options}
                        types={types}
                        counts={counts.counts}
                        onChange={(item_types) =>
                            applyFilters(urlState.query ?? "", {
                                ...filters,
                                item_types,
                            })
                        }
                    />
                    <section
                        aria-label="队列查询条件"
                        className="shrink-0 border-b border-border/60 px-4 py-3"
                    >
                        <WorkspaceSearchPanel
                            state={urlState}
                            options={options}
                            loading={refreshing}
                            failed={dashboardQuery.isError}
                            onApply={applyFilters}
                            onReset={clearFilters}
                            onScopeChange={home.onMetricClick}
                        />
                    </section>
                    <TableToolbar
                        isFrameToolbar
                        className="shrink-0 px-4"
                        actions={
                            !started && (
                                <WorkspaceSortMenu
                                    value={urlState.sort}
                                    onValueChange={onSortChange}
                                />
                            )
                        }
                    >
                        <div className="flex flex-wrap items-center gap-3">
                            <h2
                                id="workspace-queue-heading"
                                tabIndex={-1}
                                className="text-sm font-medium"
                            >
                                {title}
                                <span
                                    role="status"
                                    className="ml-2 text-xs font-normal text-muted-foreground"
                                >
                                    {refreshing
                                        ? "更新中…"
                                        : `共 ${view.total} 项`}
                                </span>
                            </h2>
                            {!started && !history && (
                                <div
                                    aria-label="待办期限快捷筛选"
                                    className="flex flex-wrap items-center gap-1"
                                >
                                    {(
                                        [undefined, "overdue", "today"] as const
                                    ).map((due) => (
                                        <Button
                                            key={due ?? "all"}
                                            id={`workspace-queue-due-${due ?? "all"}`}
                                            size="sm"
                                            variant={
                                                filters.due === due
                                                    ? "secondary"
                                                    : "ghost"
                                            }
                                            aria-pressed={filters.due === due}
                                            onClick={() =>
                                                applyFilters(
                                                    urlState.query ?? "",
                                                    {
                                                        ...filters,
                                                        due,
                                                        ...(due
                                                            ? {
                                                                  date_field:
                                                                      undefined,
                                                                  date_from:
                                                                      undefined,
                                                                  date_to:
                                                                      undefined,
                                                                  undated:
                                                                      undefined,
                                                              }
                                                            : {}),
                                                    },
                                                )
                                            }
                                        >
                                            {due === "overdue" ? (
                                                <ClockAlertIcon aria-hidden="true" />
                                            ) : due === "today" ? (
                                                <CalendarClockIcon aria-hidden="true" />
                                            ) : (
                                                <ListFilterIcon aria-hidden="true" />
                                            )}
                                            {due === "overdue"
                                                ? "已超期"
                                                : due === "today"
                                                  ? "今日到期"
                                                  : "全部"}
                                            <span
                                                className={
                                                    due === "overdue"
                                                        ? "num text-destructive"
                                                        : "num"
                                                }
                                            >
                                                {counts.counts[
                                                    due ?? "due-all"
                                                ] ?? "—"}
                                            </span>
                                        </Button>
                                    ))}
                                </div>
                            )}
                            {counts.failed && (
                                <Button
                                    id="workspace-queue-count-retry"
                                    variant="ghost"
                                    size="sm"
                                    onClick={counts.retry}
                                >
                                    数量加载失败，重试
                                </Button>
                            )}
                        </div>
                    </TableToolbar>
                    {dashboardQuery.isError && (
                        <div className="shrink-0 px-4 py-3">
                            <BusinessFailureState
                                error={dashboardQuery.error}
                                title="工作台更新失败"
                                description="当前显示上次加载的结果，请刷新后重试。"
                                onRetry={refresh}
                            />
                        </div>
                    )}
                    <section
                        data-slot="workspace-queue"
                        aria-label="任务队列"
                        className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
                    >
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
                            emptyState={
                                <BusinessEmptyState
                                    kind="no-tasks"
                                    title={
                                        hasActiveFilter
                                            ? "没有符合条件的事项"
                                            : history
                                              ? "暂无处理记录"
                                              : "当前没有待处理事项"
                                    }
                                    description={
                                        hasActiveFilter
                                            ? "可调整条件或清除筛选。"
                                            : "新事项到达后会显示在这里。"
                                    }
                                />
                            }
                        />
                    </section>
                </TableToolbarScope>
            </section>
        </div>
    )
}
