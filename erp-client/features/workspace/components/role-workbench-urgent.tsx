"use client"

import Link from "next/link"
import {
    ArrowUpRightIcon,
    CircleAlertIcon,
    CircleCheckIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { cn } from "@/lib/utils"
import { mapWorkspaceWorkItem } from "../api/dashboard"
import type { useWorkbenchUrgent } from "../hooks/use-workbench-urgent"
import { WorkbenchDataInfo, workbenchTime } from "./workbench-data-info"

export function RoleWorkbenchUrgent({
    urgent,
    returnTo,
}: {
    urgent: ReturnType<typeof useWorkbenchUrgent>
    returnTo: string
}) {
    return (
        <aside
            className="min-w-0 rounded-lg border border-border/90 bg-card shadow-2xs"
            aria-labelledby="role-workbench-urgent-title"
        >
            <div className="flex items-center justify-between gap-2 border-b border-grid/70 px-4 py-3">
                <div className="flex items-center gap-2.5">
                    <span
                        className={cn(
                            "flex size-7 shrink-0 items-center justify-center rounded-md",
                            urgent.items.length > 0
                                ? "bg-destructive-soft text-destructive-soft-foreground"
                                : "bg-success-soft text-success-soft-foreground",
                        )}
                        aria-hidden="true"
                    >
                        {urgent.items.length > 0 ? (
                            <CircleAlertIcon className="size-3.5" />
                        ) : (
                            <CircleCheckIcon className="size-3.5" />
                        )}
                    </span>
                    <div>
                        <h2
                            id="role-workbench-urgent-title"
                            className="flex items-center gap-2 text-section-title font-semibold text-foreground tracking-tight"
                        >
                            需优先处理
                            {urgent.items.length > 0 ? (
                                <span className="num rounded-full bg-destructive-soft px-1.5 text-tiny font-semibold leading-4.5 text-destructive-soft-foreground">
                                    {urgent.items.length}
                                </span>
                            ) : null}
                        </h2>
                        <p className="text-xs text-muted-foreground">
                            本人待办 · 含审批 · 最多 5 项
                        </p>
                    </div>
                </div>
                <WorkbenchDataInfo
                    id="role-workbench-urgent-info"
                    title="紧要事项筛选规则"
                >
                    <p>
                        从本人全部待处理且具备处理资格的待办中筛选，不随当前岗位和新增单据期间变化。
                    </p>
                    <p>
                        依次展示：已超期、今日到期、已标记为紧急或高优先级的事项；同一事项只显示一次。
                    </p>
                    <p>
                        同类按截止时间先后排列；未设置截止时间的优先级事项优先列出。
                    </p>
                    <p>
                        普通或低优先级且未到期的事项不展示。没有紧急待办不代表全部工作已完成。
                    </p>
                    <p>
                        仅展示最优先的 5
                        项；更多匹配事项及常规待办请进入待我处理。
                    </p>
                </WorkbenchDataInfo>
            </div>
            {urgent.isError && (
                <div
                    role="status"
                    className="mx-4 mt-3 rounded-lg bg-warning-soft px-3 py-2 text-xs text-warning-soft-foreground"
                >
                    <p>
                        {urgent.items.length
                            ? "部分紧要事项读取失败，当前列表可能不完整。"
                            : "紧要事项读取不完整，暂时无法确认是否有紧急待办。"}
                    </p>
                    <Button
                        id="role-workbench-urgent-retry"
                        variant="ghost"
                        size="xs"
                        className="mt-1"
                        onClick={() => void urgent.refetch()}
                    >
                        重新读取
                    </Button>
                </div>
            )}
            {urgent.isPending && (
                <div
                    role="status"
                    aria-label="正在读取紧要事项"
                    className="space-y-2 px-4 py-3"
                >
                    {urgent.items.length ? (
                        <p className="text-xs text-muted-foreground">
                            正在读取其他紧要事项…
                        </p>
                    ) : (
                        <>
                            <Skeleton className="h-4 w-3/4" />
                            <Skeleton className="h-4 w-1/2" />
                        </>
                    )}
                </div>
            )}
            {!urgent.isPending &&
                !urgent.isError &&
                urgent.items.length === 0 && (
                    <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
                        <span
                            className="flex size-9 items-center justify-center rounded-full bg-success-soft text-success-soft-foreground"
                            aria-hidden="true"
                        >
                            <CircleCheckIcon className="size-4.5" />
                        </span>
                        <p className="text-body-compact font-medium text-foreground">
                            当前无紧急待办
                        </p>
                        <p className="text-xs text-muted-foreground">
                            超期或高优先级事项将自动置顶显示
                        </p>
                    </div>
                )}
            {urgent.items.length > 0 && (
                <ul className="divide-y divide-grid">
                    {urgent.items.map(({ item, reason, rank }) => {
                        const task = mapWorkspaceWorkItem(item, "Asia/Shanghai")
                        return (
                            <li
                                key={item.id}
                                className="px-4 py-3 transition-colors hover:bg-row-hover"
                            >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <span
                                        className={cn(
                                            "rounded-sm px-1.5 py-0.5 text-tiny font-medium",
                                            rank === 0 || rank === 2
                                                ? "bg-destructive-soft text-destructive-soft-foreground"
                                                : rank === 1
                                                  ? "bg-warning-soft text-warning-soft-foreground"
                                                  : "bg-info-soft text-info-soft-foreground",
                                        )}
                                    >
                                        {reason}
                                    </span>
                                    <span className="text-xs text-muted-foreground">
                                        {task.workItemTypeLabel}
                                    </span>
                                </div>
                                <p className="mt-1.5 break-words text-body-compact font-medium text-foreground">
                                    {task.counterpartyName || task.objectTitle}
                                </p>
                                {task.counterpartyName && (
                                    <p className="mt-0.5 break-words text-xs text-muted-foreground">
                                        {task.objectTitle}
                                    </p>
                                )}
                                <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
                                    <span className="text-xs text-muted-foreground">
                                        {task.dueAt
                                            ? `截止 ${workbenchTime(task.dueAt, true)}`
                                            : "未设置截止时间"}
                                    </span>
                                    <Button
                                        id={`role-process-${toAutomationIdSegment(item.id)}`}
                                        variant="ghost"
                                        size="xs"
                                        className="text-primary hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
                                        render={
                                            <Link
                                                href={buildWorkspaceDetailHref(
                                                    "tasks",
                                                    item.id,
                                                    { returnTo },
                                                )}
                                            />
                                        }
                                    >
                                        查看待办
                                        <ArrowUpRightIcon aria-hidden="true" />
                                    </Button>
                                </div>
                            </li>
                        )
                    })}
                </ul>
            )}
            <div className="border-t border-grid px-4 py-2.5">
                <Link
                    id="role-workbench-urgent-all"
                    href="/workspace/tasks"
                    className="inline-flex items-center gap-1 rounded-md text-xs font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                >
                    查看全部待办
                    <ArrowUpRightIcon className="size-3.5" aria-hidden="true" />
                </Link>
            </div>
        </aside>
    )
}
