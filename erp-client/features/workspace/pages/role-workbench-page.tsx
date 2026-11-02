"use client"

import { useEffect, useRef, useState } from "react"
import Link from "next/link"
import { Input } from "@/components/ui/input"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import {
    Table,
    TableBody,
    TableHeader,
    TableHead,
    TableRow,
    TableCell,
    TableCaption,
} from "@/components/ui/table"
import { compareDecimal, divideFixed } from "@/lib/fixed-decimal"
import { useQueryClient } from "@tanstack/react-query"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { useSearchParams } from "next/navigation"
import { useRouter } from "@/lib/navigation"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { AccountProfile } from "@/features/auth/api"
import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
    MoneyValue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { ROLE_WORKBENCHES, roleWorkbench } from "@/lib/role-workbenches"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useWorkspaceOverview } from "../hooks/use-workspace-overview"
import { useRoleWorkbench } from "../hooks/use-role-workbench"
import {
    WorkspaceOverviewMetrics,
    overviewValue,
} from "../components/workspace-overview"
import { WorkspaceOverviewDetail } from "../components/workspace-overview-detail"
import { TYPE_META } from "../api/work-item-meta"
import { mapWorkspaceWorkItem } from "../api/dashboard"
import type { OverviewMetric } from "../lib/overview"
import type { PeriodPreset } from "@/features/actual-profit-loss/types"
import { savePreference } from "./role-workbench-entry"

export function RoleWorkbenchPage({ id }: { id: string }) {
    const query = useAccountProfileQuery()
    const dates = useSearchParams()
    const from = dates.get("from"),
        to = dates.get("to")
    const invalid =
        (from !== null || to !== null) &&
        (!from ||
            !to ||
            from > to ||
            ![from, to].every((date) => {
                if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return false
                const value = new Date(`${date}T00:00:00Z`)
                return (
                    !Number.isNaN(value.getTime()) &&
                    value.toISOString().slice(0, 10) === date
                )
            }))
    if (invalid)
        return (
            <PageScaffold>
                <PageHeader title="统计期间无效" />
                <Link id="role-period-reset" href={`/workspace/roles/${id}`}>
                    清除期间筛选
                </Link>
            </PageScaffold>
        )
    if (query.isError)
        return (
            <PageScaffold>
                <BusinessFailureState
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </PageScaffold>
        )
    if (!query.data)
        return (
            <PageScaffold>
                <p role="status">正在读取岗位配置…</p>
            </PageScaffold>
        )
    if (!roleWorkbench(id) || !query.data.workbenches?.includes(id))
        return (
            <PageScaffold>
                <PageHeader title="无权访问此工作台" />
                <p>岗位未配置或核心权限已变更。</p>
                <Link id="workbench-denied-return" href="/workspace">
                    返回可用工作台
                </Link>
            </PageScaffold>
        )
    return (
        <RoleWorkbenchContent
            key={`${id}:${query.data.userid}:${query.data.policy_version}:${query.data.organization_version}:${from}:${to}`}
            id={id}
            profile={query.data}
        />
    )
}

function RoleWorkbenchContent({
    id,
    profile,
}: {
    id: string
    profile: AccountProfile
}) {
    const router = useRouter()
    const queryClient = useQueryClient()
    const params = useSearchParams()
    const requested = params.get("period")
    const preset: PeriodPreset =
        requested === "last-month" || requested === "quarter-to-date"
            ? requested
            : "month-to-date"
    const custom =
        params.get("from") && params.get("to")
            ? { from: params.get("from")!, to: params.get("to")! }
            : undefined
    const [range, setRange] = useState(custom ?? { from: "", to: "" })
    const validRange =
        /^\d{4}-\d{2}-\d{2}$/.test(range.from) &&
        /^\d{4}-\d{2}-\d{2}$/.test(range.to) &&
        range.from <= range.to &&
        [range.from, range.to].every((date) => {
            const value = new Date(`${date}T00:00:00Z`)
            return (
                !Number.isNaN(value.getTime()) &&
                value.toISOString().slice(0, 10) === date
            )
        })
    const role = useRoleWorkbench(profile, id)
    const overview = useWorkspaceOverview(
        profile,
        role.template.view,
        preset,
        false,
        custom,
    )
    const [metric, setMetric] = useState<OverviewMetric | null>(null)
    const [open, setOpen] = useState(false)
    const [saved, setSaved] = useState<boolean | null>(null)
    const focus = useRef<HTMLElement | null>(null)
    useEffect(() => {
        savePreference(profile.userid, id)
    }, [profile.userid, id])
    const select = (next: OverviewMetric) => {
        focus.current = document.activeElement as HTMLElement
        setMetric(next)
        setOpen(true)
    }
    const refresh = () => {
        void queryClient.invalidateQueries({
            queryKey: queryKeyRoots.workspaceHome,
        })
    }
    const periodQuery = custom
        ? `from=${custom.from}&to=${custom.to}`
        : `period=${preset}`
    const href = `/workspace/roles/${id}?${periodQuery}`
    return (
        <PageScaffold className="gap-5 overflow-auto">
            <PageHeader
                title={role.template.name}
                description={role.template.description}
                actions={
                    <div className="flex flex-wrap gap-2">
                        <label className="text-sm">
                            切换岗位
                            <NativeSelect
                                id="role-workbench-switch"
                                className="ml-2 rounded border bg-background p-2"
                                value={id}
                                onChange={(event) =>
                                    router.push(
                                        `/workspace/roles/${event.target.value}?${periodQuery}`,
                                    )
                                }
                            >
                                {ROLE_WORKBENCHES.filter((item) =>
                                    profile.workbenches?.includes(item.id),
                                ).map((item) => (
                                    <NativeSelectOption
                                        key={item.id}
                                        value={item.id}
                                    >
                                        {item.name}
                                    </NativeSelectOption>
                                ))}
                            </NativeSelect>
                        </label>
                        <Button
                            id="role-workbench-default"
                            variant="outline"
                            onClick={() => {
                                setSaved(
                                    savePreference(profile.userid, id, true),
                                )
                            }}
                        >
                            {saved === true
                                ? "已设为默认"
                                : saved === false
                                  ? "保存失败，重试"
                                  : "设为默认首页"}
                        </Button>
                        <Button
                            id="role-workbench-refresh"
                            variant="outline"
                            onClick={refresh}
                        >
                            刷新
                        </Button>
                        <Link
                            id="role-workbench-all-todos"
                            className="self-center text-sm underline"
                            href="/workspace?queue=1"
                        >
                            我的全部待办
                        </Link>
                    </div>
                }
            />
            <p className="text-sm text-muted-foreground">
                业务统计按各模块的授权范围展示；待办仅含本人具备处理资格的开放任务。时区：Asia/Shanghai。
            </p>
            {id === "WB-01" && (
                <label className="text-sm">
                    新增单据期间
                    <NativeSelect
                        id="role-workbench-period"
                        className="ml-2 rounded border bg-background p-2"
                        value={preset}
                        onChange={(event) =>
                            router.replace(
                                `/workspace/roles/${id}?period=${event.target.value}`,
                            )
                        }
                    >
                        <NativeSelectOption value="month-to-date">
                            本月迄今
                        </NativeSelectOption>
                        <NativeSelectOption value="last-month">
                            上月
                        </NativeSelectOption>
                        <NativeSelectOption value="quarter-to-date">
                            本季迄今
                        </NativeSelectOption>
                    </NativeSelect>
                    <span className="ml-3 text-muted-foreground">
                        {overview.period.from} 至 {overview.period.to}
                        ；当前余额与任务不随期间变化
                    </span>
                </label>
            )}
            {id === "WB-01" && (
                <div className="flex flex-wrap items-end gap-3">
                    <label className="text-sm" htmlFor="role-period-from">
                        自定义开始日
                        <Input
                            id="role-period-from"
                            type="date"
                            value={range.from}
                            onChange={(event) =>
                                setRange({ ...range, from: event.target.value })
                            }
                        />
                    </label>
                    <label className="text-sm" htmlFor="role-period-to">
                        结束日
                        <Input
                            id="role-period-to"
                            type="date"
                            value={range.to}
                            onChange={(event) =>
                                setRange({ ...range, to: event.target.value })
                            }
                        />
                    </label>
                    <Button
                        id="role-period-apply"
                        disabled={!validRange}
                        onClick={() =>
                            router.replace(
                                `/workspace/roles/${id}?from=${range.from}&to=${range.to}`,
                            )
                        }
                    >
                        应用期间
                    </Button>
                </div>
            )}
            <WorkspaceOverviewMetrics overview={overview} onSelect={select} />
            <div
                className={
                    id === "WB-04"
                        ? "grid gap-5 xl:grid-cols-[1fr_2fr]"
                        : "grid gap-5 xl:grid-cols-2"
                }
            >
                <section className="min-w-0 rounded-xl border p-5">
                    <h2 className="font-medium">
                        {id === "WB-01"
                            ? "销售单阶段分布"
                            : id === "WB-03"
                              ? "采购执行阶段"
                              : "待分配资金"}
                    </h2>
                    <p className="my-2 text-xs text-muted-foreground">
                        截至当前 ·{" "}
                        {id === "WB-04"
                            ? "收款与付款分开展示，不相减"
                            : "按单据当前状态计数；所列状态互斥，不代表历史转化率"}
                    </p>
                    <Table className="w-full text-sm">
                        <TableCaption className="sr-only">
                            当前业务分布数据表
                        </TableCaption>
                        <TableBody>
                            {role.metrics.map((item, index) => {
                                const query = role.stages[index]!
                                const value = query.isError
                                    ? null
                                    : overviewValue(item, query.data)
                                const max = Math.max(
                                    1,
                                    ...role.stages.map((result) =>
                                        result.isError || result.data?.noScope
                                            ? 0
                                            : (result.data?.count ?? 0),
                                    ),
                                )
                                return (
                                    <TableRow
                                        key={item.key}
                                        className="border-b"
                                    >
                                        <TableHead className="py-3 text-left font-normal">
                                            <button
                                                id={`role-stage-${item.key.toLowerCase()}`}
                                                className="underline underline-offset-4"
                                                onClick={() => select(item)}
                                            >
                                                {item.label}
                                            </button>
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                {query.data?.scopeSummary ||
                                                    "服务端授权范围"}{" "}
                                                ·{" "}
                                                {query.data?.asOf ||
                                                    "更新时间待返回"}
                                            </p>
                                        </TableHead>
                                        <TableCell className="w-1/2 py-3 text-right">
                                            {query.isPending ? (
                                                "读取中…"
                                            ) : query.isError ? (
                                                <Button
                                                    id={`role-stage-retry-${item.key.toLowerCase()}`}
                                                    variant="ghost"
                                                    onClick={() =>
                                                        void query.refetch()
                                                    }
                                                >
                                                    读取失败，重试
                                                </Button>
                                            ) : query.data?.noScope ? (
                                                "当前未配置可查看的数据范围"
                                            ) : value == null ? (
                                                "当前口径受限"
                                            ) : item.field === "count" ? (
                                                <>
                                                    <span>{value} 单</span>
                                                    <div
                                                        aria-hidden
                                                        className="mt-1 h-2 rounded bg-muted"
                                                    >
                                                        <div
                                                            className="h-2 rounded bg-primary"
                                                            style={{
                                                                width: `${((query.data?.count ?? 0) / max) * 100}%`,
                                                            }}
                                                        />
                                                    </div>
                                                </>
                                            ) : (
                                                <>
                                                    <MoneyValue
                                                        value={String(value)}
                                                    />
                                                    <div
                                                        aria-hidden
                                                        className="mt-1 h-2 rounded bg-muted"
                                                    >
                                                        <div
                                                            className="h-2 rounded bg-primary origin-left"
                                                            style={{
                                                                transform: `scaleX(${divideFixed(
                                                                    String(
                                                                        value,
                                                                    ),
                                                                    role.stages.reduce(
                                                                        (
                                                                            maximum,
                                                                            result,
                                                                        ) =>
                                                                            result
                                                                                .data
                                                                                ?.balance &&
                                                                            compareDecimal(
                                                                                result
                                                                                    .data
                                                                                    .balance,
                                                                                maximum,
                                                                                2,
                                                                            ) >
                                                                                0
                                                                                ? result
                                                                                      .data
                                                                                      .balance
                                                                                : maximum,
                                                                        "1",
                                                                    ),
                                                                    {
                                                                        numeratorMaxScale: 2,
                                                                        denominatorMaxScale: 2,
                                                                        outputScale: 4,
                                                                    },
                                                                )})`,
                                                            }}
                                                        />
                                                    </div>
                                                </>
                                            )}
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </Table>
                </section>
                <section className="min-w-0 rounded-xl border p-5">
                    <h2 className="font-medium">
                        {id === "WB-04"
                            ? "付款执行时限"
                            : id === "WB-03"
                              ? "采购任务时限"
                              : "客户跟进任务时限"}
                    </h2>
                    <p className="my-2 text-xs text-muted-foreground">
                        本人可处理任务 · 项 ·
                        截至当前；今日到期与已超期可能重叠，不相加。没有到期时间不计入两列。
                    </p>
                    <Table className="w-full text-sm">
                        <TableCaption className="sr-only">
                            任务数量与时限数据表
                        </TableCaption>
                        <TableHeader>
                            <TableRow>
                                <TableHead className="text-left">
                                    类型
                                </TableHead>
                                <TableHead>待处理</TableHead>
                                <TableHead>今日到期</TableHead>
                                <TableHead>已超期</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {role.template.tasks.map((type, index) => {
                                const query = role.tasks[index]!
                                const stats = query.statsError
                                    ? undefined
                                    : query.stats
                                return (
                                    <TableRow key={type} className="border-b">
                                        <TableHead className="py-3 text-left font-normal">
                                            <Link
                                                id={`role-task-type-${type.toLowerCase()}`}
                                                href={`/workspace?queue=1&type=${type}`}
                                            >
                                                {TYPE_META[type]?.label}
                                            </Link>
                                            <p className="text-xs text-muted-foreground">
                                                {stats &&
                                                    new Date(
                                                        stats.as_of * 1000,
                                                    ).toLocaleString("zh-CN", {
                                                        timeZone:
                                                            "Asia/Shanghai",
                                                    })}
                                            </p>
                                        </TableHead>
                                        {query.statsError ? (
                                            <TableCell colSpan={3}>
                                                <Button
                                                    id={`role-task-retry-${type.toLowerCase()}`}
                                                    variant="ghost"
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
                                                    className="px-2 text-center"
                                                >
                                                    {count === undefined ? (
                                                        "—"
                                                    ) : (
                                                        <Link
                                                            id={`role-task-count-${type.toLowerCase()}-${column}`}
                                                            href={`/workspace?queue=1&type=${type}${column === 1 ? "&due=today" : column === 2 ? "&due=overdue" : ""}`}
                                                        >
                                                            {count}
                                                        </Link>
                                                    )}
                                                    <div
                                                        aria-hidden
                                                        className="mt-1 h-2 rounded bg-muted"
                                                    >
                                                        <div
                                                            className="h-2 rounded bg-primary"
                                                            style={{
                                                                width: `${((count ?? 0) / Math.max(1, stats?.assigned ?? 0)) * 100}%`,
                                                            }}
                                                        />
                                                    </div>
                                                </TableCell>
                                            ))
                                        )}
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </Table>
                </section>
            </div>
            <section className="rounded-xl border p-5">
                <h2 className="font-medium">
                    {id === "WB-04"
                        ? "已批准待执行及退货款处理"
                        : id === "WB-03"
                          ? "优先处理采购事项"
                          : "优先跟进客户事项"}
                </h2>
                <p className="my-2 text-xs text-muted-foreground">
                    各类型按优先级与到期时间显示前 5
                    项；其他审批及兼岗任务请查看我的全部待办。
                </p>
                <div className="grid gap-4 lg:grid-cols-2">
                    {role.template.tasks.map((type, index) => {
                        const query = role.tasks[index]!
                        return (
                            <section key={type}>
                                <h3 className="my-2 text-sm font-medium">
                                    {TYPE_META[type]?.label}
                                </h3>
                                {query.isPending ? (
                                    <p role="status">读取中…</p>
                                ) : query.isError ? (
                                    <Button
                                        id={`role-list-retry-${type.toLowerCase()}`}
                                        variant="ghost"
                                        onClick={() => void query.refetch()}
                                    >
                                        清单读取失败，重试
                                    </Button>
                                ) : !query.data?.page.items.length ? (
                                    <p className="text-sm text-muted-foreground">
                                        当前范围暂无待办
                                    </p>
                                ) : (
                                    query.data.page.items.map((dto) => {
                                        const task = mapWorkspaceWorkItem(
                                            dto,
                                            "Asia/Shanghai",
                                        )
                                        return (
                                            <div
                                                key={dto.id}
                                                className="flex items-center justify-between gap-3 border-b py-3 text-sm"
                                            >
                                                <div className="min-w-0">
                                                    <p className="truncate">
                                                        {task.objectTitle}
                                                    </p>
                                                    <p className="text-xs text-muted-foreground">
                                                        {task.nextActionHint} ·{" "}
                                                        {task.dueAtLabel}
                                                    </p>
                                                </div>
                                                <Link
                                                    id={`role-process-${toAutomationIdSegment(dto.id)}`}
                                                    className="shrink-0 underline"
                                                    href={buildWorkspaceDetailHref(
                                                        "tasks",
                                                        dto.id,
                                                        {
                                                            returnTo: href,
                                                        },
                                                    )}
                                                >
                                                    查看待办
                                                </Link>
                                            </div>
                                        )
                                    })
                                )}
                            </section>
                        )
                    })}
                </div>
            </section>
            <nav aria-label="岗位常用操作" className="flex flex-wrap gap-4">
                {role.template.operations.map((operation) => (
                    <Link
                        id={`role-operation-${toAutomationIdSegment(operation.href)}`}
                        key={operation.href}
                        className="text-sm underline"
                        href={operation.href}
                    >
                        {operation.label}
                    </Link>
                ))}
            </nav>
            <WorkspaceOverviewDetail
                open={open}
                metric={metric}
                profile={profile}
                onClose={() => setOpen(false)}
                onClosed={() => {
                    setMetric(null)
                    focus.current?.focus()
                }}
                onTask={(taskId) =>
                    router.push(
                        buildWorkspaceDetailHref("tasks", taskId, {
                            returnTo: href,
                        }),
                    )
                }
            />
        </PageScaffold>
    )
}
