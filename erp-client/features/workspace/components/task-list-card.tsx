"use client"

import Link from "next/link"
import { ArrowRightIcon } from "lucide-react"

import {
    AsyncSectionState,
    BusinessEmptyState,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardAction,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { TaskGroupSection } from "@/features/workspace/components/task-group-section"
import { resolveWorkspaceHref } from "@/features/workspace/lib/destination"
import { buildGroupAllHref } from "@/features/workspace/lib/url-state"
import type { WorkspaceUrlState } from "@/features/workspace/lib/url-state"
import type {
    TodayWorkspaceView,
    WorkspaceWorkItem,
} from "@/features/workspace/types"
import { cn } from "@/lib/utils"

export function TaskListCard({
    view,
    urlState,
    filterLabel,
    visibleCount,
    hasActiveFilter,
    taskQueueHref,
    asyncStatus,
    hasRefreshError,
    focusStableNumber,
    onOpenTask,
    clearFilters,
    refresh,
}: {
    view: TodayWorkspaceView
    urlState: WorkspaceUrlState
    filterLabel: string
    visibleCount: number
    hasActiveFilter: boolean
    taskQueueHref: string
    asyncStatus: "error" | "refreshing" | "success"
    hasRefreshError: boolean
    focusStableNumber?: string
    onOpenTask: (item: WorkspaceWorkItem, intent: "PROCESS" | "VIEW") => void
    clearFilters: () => void
    refresh: () => void
}) {
    return (
        <Card size="sm" className={cn("min-w-0", surfacePanelClassName)}>
            <CardHeader className="rounded-t-lg border-b border-border/30">
                <CardTitle id="workspace-task-main-title">
                    {filterLabel}
                </CardTitle>
                <CardDescription aria-live="polite" aria-atomic="true">
                    {hasActiveFilter
                        ? `当前筛选「${filterLabel}」共 ${visibleCount} 项`
                        : `当前共 ${visibleCount} 项待办，按超期与截止时间展示`}
                </CardDescription>
                <CardAction className="flex flex-wrap gap-1">
                    {hasActiveFilter ? (
                        <Button
                            type="button"
                            variant="ghost"
                            size="xs"
                            onClick={clearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null}
                    {view.canOpenTaskQueue ? (
                        <Button
                            size="xs"
                            variant="secondary"
                            className="rounded-md shadow-none"
                            render={<Link href={taskQueueHref} />}
                        >
                            查看全部待办
                            <ArrowRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    ) : null}
                </CardAction>
            </CardHeader>
            <CardContent>
                <AsyncSectionState
                    status={asyncStatus}
                    refreshingLabel="正在刷新，仍显示上次结果"
                    error="工作台任务刷新失败，已保留上次成功内容。"
                    errorKind={hasRefreshError ? "projection" : "system"}
                    retryAction={
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={refresh}
                        >
                            重试
                        </Button>
                    }
                >
                    {visibleCount === 0 && !hasActiveFilter ? (
                        <BusinessEmptyState
                            kind="no-tasks"
                            title="当前没有待处理事项"
                            description="当前没有待处理事项。可查看最近打开记录，或进入其它业务模块。"
                            // 嵌在任务卡内：去掉空态自带描边/底，避免框中套框
                            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0 md:p-8"
                            action={
                                <div className="flex flex-wrap items-center justify-center gap-2">
                                    {/* secondary chip：有底有 hover，比 ghost 更像可点，比 outline 更轻 */}
                                    <Button
                                        size="sm"
                                        variant="secondary"
                                        className="rounded-lg shadow-none"
                                        render={
                                            <Link
                                                href={resolveWorkspaceHref(
                                                    "W05",
                                                )}
                                            />
                                        }
                                    >
                                        销售单
                                    </Button>
                                    <Button
                                        size="sm"
                                        variant="secondary"
                                        className="rounded-lg shadow-none"
                                        render={
                                            <Link
                                                href={resolveWorkspaceHref(
                                                    "W07",
                                                )}
                                            />
                                        }
                                    >
                                        采购确认
                                    </Button>
                                    <Button
                                        size="sm"
                                        variant="secondary"
                                        className="rounded-lg shadow-none"
                                        render={
                                            <Link
                                                href={resolveWorkspaceHref(
                                                    "W10",
                                                )}
                                            />
                                        }
                                    >
                                        库存台账
                                    </Button>
                                </div>
                            }
                        />
                    ) : null}

                    {visibleCount === 0 && hasActiveFilter ? (
                        <BusinessEmptyState
                            kind="filter"
                            title="当前筛选无结果"
                            description={`没有符合「${filterLabel}」的待办。可清除筛选查看全部任务。`}
                            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0 md:p-8"
                            action={
                                <Button
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    className="rounded-lg shadow-none"
                                    onClick={clearFilters}
                                >
                                    清除筛选
                                </Button>
                            }
                        />
                    ) : null}

                    {visibleCount > 0 ? (
                        <div
                            className="space-y-3"
                            role="list"
                            aria-labelledby="workspace-task-main-title"
                        >
                            {view.groups.map((group) => (
                                <div key={group.family} role="listitem">
                                    <TaskGroupSection
                                        group={group}
                                        scope={urlState.scope}
                                        focusStableNumber={focusStableNumber}
                                        onOpenTask={onOpenTask}
                                        groupAllHref={buildGroupAllHref(
                                            urlState,
                                            group.family,
                                        )}
                                    />
                                </div>
                            ))}
                        </div>
                    ) : null}
                </AsyncSectionState>
            </CardContent>
        </Card>
    )
}
