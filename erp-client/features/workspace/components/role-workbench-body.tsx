"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"
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
import { hasPermission } from "@/lib/permissions"
import type { AccountProfile } from "@/features/auth/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { TYPE_META } from "../api/work-item-meta"
import type { useRoleWorkbench } from "../hooks/use-role-workbench"
import { WORKBENCH_PRESENTATION } from "../lib/workbench-presentation"
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
const panel = "min-w-0 rounded-xl border border-border/70 bg-card"
const focus = "rounded-md focus-visible:outline-2 focus-visible:outline-ring"

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
    const operations = role.template.operations.filter((operation) =>
        hasPermission(profile.permissions, operation.permission),
    )
    return (
        <div className="grid items-start gap-5 xl:grid-cols-[minmax(0,2fr)_minmax(22rem,1fr)]">
            <div className="min-w-0 space-y-5" aria-label="岗位业务分析">
                <div className="grid items-start gap-5 2xl:grid-cols-2">
                    {role.template.id === "WB-04" ? (
                        <RoleWorkbenchBalances
                            role={role}
                            onSelect={onSelect}
                        />
                    ) : (
                        <RoleWorkbenchStages role={role} onSelect={onSelect} />
                    )}
                    <RoleWorkbenchTiming role={role} />
                </div>
                {operations.length > 0 && (
                    <nav
                        aria-label="岗位常用操作"
                        className="flex flex-wrap items-center gap-x-4 gap-y-2 px-1"
                    >
                        <span className="text-xs text-muted-foreground">
                            常用入口
                        </span>
                        {operations.map((operation) => (
                            <Link
                                key={operation.href}
                                id={`role-operation-${toAutomationIdSegment(operation.href)}`}
                                href={operation.href}
                                className={cn(
                                    focus,
                                    "inline-flex items-center gap-1 text-xs hover:underline",
                                )}
                            >
                                {operation.label}
                                <ArrowUpRightIcon
                                    className="size-3.5 text-muted-foreground"
                                    aria-hidden="true"
                                />
                            </Link>
                        ))}
                    </nav>
                )}
            </div>
            <RoleWorkbenchUrgent urgent={role.urgent} returnTo={returnTo} />
        </div>
    )
}

function RoleWorkbenchTiming({ role }: { role: Role }) {
    const presentation = WORKBENCH_PRESENTATION[role.template.id]
    return (
        <section
            className={panel}
            aria-labelledby={`${presentation.prefix}-workbench-timing-title`}
        >
            <div className="flex items-center justify-between px-4 pt-3">
                <div>
                    <h2
                        id={`${presentation.prefix}-workbench-timing-title`}
                        className="text-sm font-semibold"
                    >
                        {presentation.timingTitle}
                    </h2>
                    <p className="mt-1 text-xs text-muted-foreground">
                        本人可处理 · 截至当前
                    </p>
                </div>
                <WorkbenchDataInfo
                    id={`${presentation.prefix}-workbench-timing-info`}
                    title={presentation.timingTitle}
                >
                    <p>
                        单位：项。今日到期与已超期可能重叠，不相加；未设置到期时间的任务不计入这两列。
                    </p>
                    <p>
                        统计数量来自当前岗位对应类型的全部开放任务，不是紧要事项的数量。
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
            <div className="px-2 py-3">
                <Table>
                    <TableCaption className="sr-only">
                        本人开放任务数量与时限，单位：项
                    </TableCaption>
                    <TableHeader>
                        <TableRow>
                            <TableHead>类型</TableHead>
                            <TableHead className="text-center">
                                待处理
                            </TableHead>
                            <TableHead className="text-center">
                                今日到期
                            </TableHead>
                            <TableHead className="text-center">
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
                                    <TableCell className="text-xs">
                                        <Link
                                            id={`role-task-type-${type.toLowerCase()}`}
                                            className={cn(
                                                focus,
                                                "hover:underline",
                                            )}
                                            href={`/workspace/tasks?type=${type}`}
                                        >
                                            {TYPE_META[type]?.label}
                                        </Link>
                                    </TableCell>
                                    {query.statsError ? (
                                        <TableCell colSpan={3}>
                                            <Button
                                                id={`role-task-retry-${type.toLowerCase()}`}
                                                variant="ghost"
                                                size="sm"
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
                                                className="text-center tabular-nums"
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
                                                            "inline-flex min-w-7 justify-center px-1 py-1 hover:bg-muted",
                                                            count === 0
                                                                ? "text-muted-foreground"
                                                                : column === 2
                                                                  ? "font-semibold text-destructive"
                                                                  : column === 1
                                                                    ? "font-semibold text-warning-soft-foreground"
                                                                    : "font-semibold",
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
                <p className="px-2 pt-2 text-xs text-muted-foreground">
                    {workbenchUpdatedAt(
                        role.tasks.map((query) =>
                            query.statsError ? undefined : query.stats?.as_of,
                        ),
                    )}
                </p>
            </div>
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
    const max = Math.max(
        1,
        ...role.stages.map((query) =>
            query.isError || query.data?.noScope ? 0 : (query.data?.count ?? 0),
        ),
    )
    return (
        <section
            className={panel}
            aria-labelledby={`${presentation.prefix}-workbench-stages-title`}
        >
            <div className="flex items-center justify-between px-4 pt-3">
                <div>
                    <h2
                        id={`${presentation.prefix}-workbench-stages-title`}
                        className="text-sm font-semibold"
                    >
                        {presentation.stagesTitle}
                    </h2>
                    <p className="mt-1 text-xs text-muted-foreground">
                        本人负责 · 截至当前 · 单
                    </p>
                </div>
                <WorkbenchDataInfo
                    id={`${presentation.prefix}-workbench-stages-info`}
                    title={presentation.stagesTitle}
                >
                    <p>
                        按单据当前状态计数；所列状态互斥，不代表历史转化率。横条以当前最多的状态为基准。
                    </p>
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
            <div className="space-y-1 p-3">
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
                    return (
                        <div key={item.key}>
                            <button
                                id={`role-stage-${item.key.toLowerCase()}`}
                                type="button"
                                className={cn(
                                    focus,
                                    "flex w-full items-center gap-3 px-1 py-2 text-left text-xs hover:bg-muted/40",
                                )}
                                onClick={() =>
                                    query.isError
                                        ? void query.refetch()
                                        : onSelect(item)
                                }
                            >
                                <span className="w-12 shrink-0">
                                    {item.label}
                                </span>
                                {query.isPending ? (
                                    <Skeleton className="h-3 flex-1" />
                                ) : !ready ? (
                                    <span
                                        className={cn(
                                            "flex-1 text-right",
                                            query.isError
                                                ? "text-destructive"
                                                : "text-muted-foreground",
                                        )}
                                    >
                                        {query.isError
                                            ? "读取失败，点击重试"
                                            : query.data?.noScope
                                              ? "当前无数据范围"
                                              : "当前口径受限"}
                                    </span>
                                ) : (
                                    <>
                                        <span
                                            className="h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-muted"
                                            aria-hidden="true"
                                        >
                                            <span
                                                className={cn(
                                                    "block h-full rounded-full",
                                                    item.key ===
                                                        "stage-effective" ||
                                                        item.key ===
                                                            "stage-EFFECTIVE"
                                                        ? "bg-info"
                                                        : item.key ===
                                                                "stage-pending_review" ||
                                                            item.key ===
                                                                "stage-PENDING_REVIEW"
                                                          ? "bg-warning"
                                                          : "bg-foreground/30",
                                                )}
                                                style={{
                                                    width: `${((query.data?.count ?? 0) / max) * 100}%`,
                                                }}
                                            />
                                        </span>
                                        <span className="w-9 shrink-0 text-right tabular-nums">
                                            {value} 单
                                        </span>
                                    </>
                                )}
                            </button>
                            {query.data?.limited && (
                                <p className="px-1 pb-1 text-xs text-muted-foreground">
                                    {item.label} · 部分覆盖或权限受限
                                </p>
                            )}
                        </div>
                    )
                })}
                <p className="border-t border-grid px-1 pt-2 text-xs text-muted-foreground">
                    {role.stages.some((query) => query.isFetching)
                        ? "更新中…"
                        : workbenchUpdatedAt(
                              role.stages.map((query) =>
                                  query.isError ? undefined : query.data?.asOf,
                              ),
                          )}
                </p>
            </div>
        </section>
    )
}
