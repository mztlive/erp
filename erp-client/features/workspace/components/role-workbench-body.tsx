"use client"

import Link from "next/link"
import { ArrowUpRightIcon, BarChart3Icon, ClockIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import {
    Table,
    TableBody,
    TableCaption,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { AccountProfile } from "@/features/auth/api"
import { cn } from "@/lib/utils"
import { TYPE_META } from "../api/work-item-meta"
import type { useRoleWorkbench } from "../hooks/use-role-workbench"
import { WORKBENCH_PRESENTATION } from "../lib/workbench-presentation"
import { RoleWorkbenchActivity } from "./role-workbench-activity"
import { RoleWorkbenchUrgent } from "./role-workbench-urgent"
import { RoleWorkbenchBalances } from "./role-workbench-balances"
import type { OverviewMetric } from "../lib/overview"
import { overviewValue } from "./workspace-overview"
import {
    WorkbenchDataInfo,
    workbenchTime,
    workbenchUpdatedAt,
} from "./workbench-data-info"

type Role = ReturnType<typeof useRoleWorkbench>
const focus = "rounded-md focus-visible:outline-2 focus-visible:outline-primary"

function stageBarTone(key: string): string {
    const normalized = key.toLowerCase()
    if (normalized === "stage-effective") return "bg-primary"
    if (normalized === "stage-pending_review") return "bg-warning"
    if (normalized === "stage-partial" || normalized === "stage-completed")
        return "bg-success"
    if (normalized === "stage-voided") return "bg-muted-foreground/35"
    return "bg-primary/45"
}

export function RoleWorkbenchBody({
    role,
    profile,
    returnTo,
    onSelect,
}: {
    role: Role
    profile: AccountProfile
    returnTo: string
    onSelect: (metric: OverviewMetric) => void
}) {
    return (
        <div className="grid flex-1 items-start gap-x-6 gap-y-0 lg:grid-cols-[minmax(0,1fr)_20rem] xl:grid-cols-[minmax(0,1fr)_22rem] 2xl:grid-cols-[minmax(0,1fr)_24rem]">
            <div
                className="min-w-0 divide-y divide-border/60"
                aria-label="岗位业务分析"
            >
                {role.template.id === "WB-04" ? (
                    <RoleWorkbenchBalances role={role} onSelect={onSelect} />
                ) : (
                    <RoleWorkbenchStages role={role} onSelect={onSelect} />
                )}
                <RoleWorkbenchActivity
                    workbenchId={role.template.id}
                    profile={profile}
                />
            </div>
            <div
                className="min-w-0 divide-y divide-border/60 border-t border-border/60 lg:self-stretch lg:border-t-0 lg:border-l lg:border-border/60 lg:pl-6"
                aria-label="本人待办与时限"
            >
                <RoleWorkbenchUrgent urgent={role.urgent} returnTo={returnTo} />
                <RoleWorkbenchTiming role={role} />
            </div>
        </div>
    )
}

function RoleWorkbenchTiming({ role }: { role: Role }) {
    const presentation = WORKBENCH_PRESENTATION[role.template.id]
    return (
        <section
            className="min-w-0 py-5"
            aria-labelledby={`${presentation.prefix}-workbench-timing-title`}
        >
            <div className="flex items-center justify-between gap-2 pb-3">
                <div className="flex items-center gap-2">
                    <span
                        className="flex size-6 shrink-0 items-center justify-center rounded-md bg-warning-soft text-warning-soft-foreground"
                        aria-hidden="true"
                    >
                        <ClockIcon className="size-3.5" />
                    </span>
                    <div>
                        <h2
                            id={`${presentation.prefix}-workbench-timing-title`}
                            className="text-section-title font-semibold tracking-tight text-foreground"
                        >
                            {presentation.timingTitle}
                        </h2>
                        <p className="text-xs text-muted-foreground">
                            本人可处理 · 截至当前
                        </p>
                    </div>
                </div>
                <WorkbenchDataInfo
                    id={`${presentation.prefix}-workbench-timing-info`}
                    title={presentation.timingTitle}
                >
                    <p>
                        单位：项。今日到期与已超期可能重叠，不相加；未设置到期时间的任务不计入这两列。
                    </p>
                    <p>
                        统计数量来自当前岗位对应类型的全部待办任务，不是紧要事项的数量。
                    </p>
                    {role.template.tasks.map((type, index) => (
                        <p key={type}>
                            {TYPE_META[type]?.label}：
                            {role.tasks[index]?.statsError
                                ? "读取失败"
                                : workbenchTime(
                                      role.tasks[index]?.stats?.as_of,
                                      true,
                                  )}
                        </p>
                    ))}
                </WorkbenchDataInfo>
            </div>
            <div className="border-y border-border/60">
                <Table>
                    <TableCaption className="sr-only">
                        本人待办任务数量与时限，单位：项
                    </TableCaption>
                    <TableHeader>
                        <TableRow className="hover:bg-transparent">
                            <TableHead className="pl-0">类型</TableHead>
                            <TableHead className="w-15 px-1 text-center">
                                待处理
                            </TableHead>
                            <TableHead className="w-15 px-1 text-center">
                                今日到期
                            </TableHead>
                            <TableHead className="w-15 px-1 pr-0 text-center">
                                已超期
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {role.template.tasks.map((type, index) => {
                            const query = role.tasks[index]!
                            const stats = query.statsError
                                ? undefined
                                : query.stats
                            return (
                                <TableRow key={type}>
                                    <TableCell className="pl-0 text-xs">
                                        <Link
                                            id={`role-task-type-${type.toLowerCase()}`}
                                            className={cn(
                                                focus,
                                                "font-medium text-foreground hover:text-primary hover:underline",
                                            )}
                                            href={`/workspace/tasks?type=${type}`}
                                        >
                                            {TYPE_META[type]?.label}
                                        </Link>
                                    </TableCell>
                                    {query.statsError ? (
                                        <TableCell colSpan={3} className="pr-0">
                                            <Button
                                                id={`role-task-retry-${type.toLowerCase()}`}
                                                variant="ghost"
                                                size="xs"
                                                onClick={() =>
                                                    void query.refetch()
                                                }
                                            >
                                                读取失败，重试
                                            </Button>
                                        </TableCell>
                                    ) : (
                                        [
                                            stats?.assigned,
                                            stats?.due_today,
                                            stats?.overdue,
                                        ].map((count, column) => (
                                            <TableCell
                                                key={column}
                                                className={cn(
                                                    "px-1 text-center tabular-nums",
                                                    column === 2 && "pr-0",
                                                )}
                                            >
                                                {query.statsPending ? (
                                                    <Skeleton className="mx-auto h-4 w-5" />
                                                ) : count == null ? (
                                                    <span
                                                        className="text-muted-foreground"
                                                        aria-label="数量暂不可用"
                                                    >
                                                        —
                                                    </span>
                                                ) : (
                                                    <Link
                                                        id={`role-task-count-${type.toLowerCase()}-${column}`}
                                                        aria-label={`${TYPE_META[type]?.label}，${["待处理", "今日到期", "已超期"][column]} ${count} 项`}
                                                        href={`/workspace/tasks?type=${type}${column === 1 ? "&due=today" : column === 2 ? "&due=overdue" : ""}`}
                                                        className={cn(
                                                            focus,
                                                            "num inline-flex min-w-7 items-center justify-center rounded-sm px-2 py-0.5 text-xs transition-colors",
                                                            count === 0
                                                                ? "text-muted-foreground/70 hover:bg-muted"
                                                                : column === 2
                                                                  ? "bg-destructive-soft font-semibold text-destructive-soft-foreground hover:bg-destructive/15"
                                                                  : column === 1
                                                                    ? "bg-warning-soft font-semibold text-warning-soft-foreground hover:bg-warning/15"
                                                                    : "bg-info-soft font-semibold text-info-soft-foreground hover:bg-primary/15",
                                                        )}
                                                    >
                                                        {count}
                                                    </Link>
                                                )}
                                            </TableCell>
                                        ))
                                    )}
                                </TableRow>
                            )
                        })}
                    </TableBody>
                </Table>
            </div>
            <p className="pt-2.5 text-xs text-muted-foreground">
                {workbenchUpdatedAt(
                    role.tasks.map((query) =>
                        query.statsError ? undefined : query.stats?.as_of,
                    ),
                )}
            </p>
        </section>
    )
}

function RoleWorkbenchStages({
    role,
    onSelect,
}: {
    role: Role
    onSelect: (metric: OverviewMetric) => void
}) {
    const presentation = WORKBENCH_PRESENTATION[role.template.id]
    const counts = role.stages.map((query) =>
        query.isPending || query.isError || query.data?.noScope
            ? 0
            : (query.data?.count ?? 0),
    )
    const total = counts.reduce((sum, count) => sum + count, 0)
    const allReady = role.stages.every(
        (query) => !query.isPending && !query.isError && !query.data?.noScope,
    )

    return (
        <section
            className="min-w-0 py-5"
            aria-labelledby={`${presentation.prefix}-workbench-stages-title`}
        >
            <div className="flex flex-wrap items-center justify-between gap-2 pb-3.5">
                <div className="flex items-center gap-2">
                    <span
                        className="flex size-6 shrink-0 items-center justify-center rounded-md bg-info-soft text-info-soft-foreground"
                        aria-hidden="true"
                    >
                        <BarChart3Icon className="size-3.5" />
                    </span>
                    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                        <h2
                            id={`${presentation.prefix}-workbench-stages-title`}
                            className="text-section-title font-semibold tracking-tight text-foreground"
                        >
                            {presentation.stagesTitle}
                        </h2>
                        <span className="text-xs text-muted-foreground">
                            本人负责 · 截至当前
                        </span>
                    </div>
                </div>
                <div className="flex items-center gap-2">
                    {allReady ? (
                        <span className="num rounded-sm bg-muted/75 px-2 py-0.5 text-xs font-medium text-muted-foreground">
                            共 {total} 单
                        </span>
                    ) : null}
                    <span className="hidden text-xs text-muted-foreground sm:inline">
                        {role.stages.some((query) => query.isFetching)
                            ? "更新中…"
                            : workbenchUpdatedAt(
                                  role.stages.map((query) =>
                                      query.isError
                                          ? undefined
                                          : query.data?.asOf,
                                  ),
                              )}
                    </span>
                    <WorkbenchDataInfo
                        id={`${presentation.prefix}-workbench-stages-info`}
                        title={presentation.stagesTitle}
                    >
                        <p>展示各阶段单据的当前数量分布，点击阶段查看明细。</p>
                        {role.metrics.map((item, index) => (
                            <div key={item.key}>
                                <p className="font-medium text-foreground">
                                    {item.label}
                                </p>
                                <p>
                                    {role.stages[index]?.data?.scopeSummary ||
                                        `以${presentation.scopeLabel}为准`}
                                </p>
                                <p>
                                    {role.stages[index]?.isError
                                        ? "读取失败"
                                        : `更新于 ${workbenchTime(role.stages[index]?.data?.asOf, true)}`}
                                </p>
                            </div>
                        ))}
                    </WorkbenchDataInfo>
                </div>
            </div>

            <div className="space-y-3">
                <div
                    className="flex h-2 w-full gap-0.5 overflow-hidden rounded-full bg-muted/80"
                    aria-hidden="true"
                >
                    {total > 0 &&
                        role.metrics.map((item, index) => {
                            const count = counts[index] ?? 0
                            if (count <= 0) return null
                            return (
                                <span
                                    key={item.key}
                                    className={cn(
                                        "block h-full transition-[width] duration-300 first:rounded-l-full last:rounded-r-full",
                                        stageBarTone(item.key),
                                    )}
                                    style={{
                                        width: `${(count / total) * 100}%`,
                                    }}
                                />
                            )
                        })}
                </div>

                <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                    {role.metrics.map((item, index) => {
                        const query = role.stages[index]!
                        const value = query.isError
                            ? null
                            : overviewValue(item, query.data)
                        const ready =
                            !query.isPending &&
                            !query.isError &&
                            !query.data?.noScope &&
                            value != null
                        const numericCount = counts[index] ?? 0
                        const share =
                            ready && total > 0
                                ? Math.round((numericCount / total) * 100)
                                : null
                        return (
                            <button
                                key={item.key}
                                id={`role-stage-${item.key.toLowerCase()}`}
                                type="button"
                                onClick={() =>
                                    query.isError
                                        ? void query.refetch()
                                        : onSelect(item)
                                }
                                className={cn(
                                    focus,
                                    "group flex flex-col rounded-md bg-muted/35 px-3 py-2.5 text-left transition-colors hover:bg-muted/70",
                                )}
                            >
                                <span className="flex items-center justify-between gap-1.5 text-xs font-medium text-muted-foreground group-hover:text-primary">
                                    <span className="flex min-w-0 items-center gap-1.5">
                                        <span
                                            className={cn(
                                                "size-2 shrink-0 rounded-full",
                                                stageBarTone(item.key),
                                            )}
                                            aria-hidden="true"
                                        />
                                        <span className="truncate">
                                            {item.label}
                                        </span>
                                    </span>
                                    <ArrowUpRightIcon
                                        className="size-3 shrink-0 opacity-0 transition-opacity group-hover:opacity-100"
                                        aria-hidden="true"
                                    />
                                </span>
                                <span className="mt-1.5 flex min-h-7 items-baseline justify-between gap-2">
                                    {query.isPending ? (
                                        <Skeleton className="h-6 w-12" />
                                    ) : !ready ? (
                                        <span
                                            className={cn(
                                                "text-xs",
                                                query.isError
                                                    ? "text-destructive"
                                                    : "text-muted-foreground",
                                            )}
                                        >
                                            {query.isError
                                                ? "重试"
                                                : query.data?.noScope
                                                  ? "无权限"
                                                  : "部分数据"}
                                        </span>
                                    ) : (
                                        <>
                                            <span className="num flex items-baseline gap-1 text-value-compact font-semibold tracking-tight text-foreground">
                                                {value}
                                                <span className="text-xs font-normal text-muted-foreground">
                                                    单
                                                </span>
                                            </span>
                                            {share != null && (
                                                <span className="num text-xs text-muted-foreground">
                                                    {share}%
                                                </span>
                                            )}
                                        </>
                                    )}
                                </span>
                                {query.data?.limited && (
                                    <span className="mt-1 text-tiny text-muted-foreground">
                                        仅权限内部分数据
                                    </span>
                                )}
                            </button>
                        )
                    })}
                </div>
            </div>
        </section>
    )
}
