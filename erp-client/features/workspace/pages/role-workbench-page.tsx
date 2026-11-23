"use client"

import { useEffect, useRef, useState } from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { useSearchParams } from "next/navigation"
import {
    BusinessFailureState,
    PageScaffold,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { useRouter } from "@/lib/navigation"
import { roleWorkbench } from "@/lib/role-workbenches"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { AccountProfile } from "@/features/auth/api"
import type { PeriodPreset } from "@/features/actual-profit-loss/types"
import { useWorkspaceOverview } from "../hooks/use-workspace-overview"
import { useRoleWorkbench } from "../hooks/use-role-workbench"
import { WORKBENCH_PRESENTATION } from "../lib/workbench-presentation"
import type { OverviewMetric } from "../lib/overview"
import { WorkspaceOverviewDetail } from "../components/workspace-overview-detail"
import {
    RoleWorkbenchHeader,
    RoleWorkbenchPeriod,
} from "../components/role-workbench-header"
import { RoleWorkbenchMetrics } from "../components/role-workbench-metrics"
import { RoleWorkbenchBody } from "../components/role-workbench-body"
import {
    WorkbenchDataInfo,
    workbenchUpdatedAt,
} from "../components/workbench-data-info"
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
            <PageScaffold
                density="compact"
                className={`${detailWorkspaceStyles.page} min-h-0 overflow-auto`}
            >
                <header className="flex min-h-12 shrink-0 items-center justify-between gap-3 border-b border-border/60 bg-card py-2">
                    <h1 className={detailWorkspaceStyles.title}>
                        统计期间无效
                    </h1>
                </header>
                <div className="space-y-3 py-6">
                    <p className="text-body-compact text-muted-foreground">
                        地址中的起止日期格式不正确，或开始日晚于结束日。
                    </p>
                    <Button
                        id="role-period-reset"
                        variant="outline"
                        size="sm"
                        render={<Link href={`/workspace/roles/${id}`} />}
                    >
                        清除期间筛选
                    </Button>
                </div>
            </PageScaffold>
        )
    if (query.isError)
        return (
            <PageScaffold
                density="compact"
                className={`${detailWorkspaceStyles.page} min-h-0 overflow-auto`}
            >
                <BusinessFailureState
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </PageScaffold>
        )
    if (!query.data)
        return (
            <PageScaffold
                density="compact"
                className={`${detailWorkspaceStyles.page} min-h-0 overflow-auto`}
            >
                <p role="status" className="py-6 text-xs text-muted-foreground">
                    正在读取岗位配置…
                </p>
            </PageScaffold>
        )
    if (!roleWorkbench(id) || !query.data.workbenches?.includes(id))
        return (
            <PageScaffold
                density="compact"
                className={`${detailWorkspaceStyles.page} min-h-0 overflow-auto`}
            >
                <BusinessFailureState
                    kind="permission"
                    title="无权访问此工作台"
                    description="岗位未配置或核心权限已变更。"
                    action={
                        <Button
                            id="workbench-denied-return"
                            variant="outline"
                            size="sm"
                            render={<Link href="/workspace" />}
                        >
                            返回可用工作台
                        </Button>
                    }
                />
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

function scopeInfoSummary(id: string): string {
    if (id === "WB-02") {
        return "各项履约指标按本人待处理的履约责任及对应业务模块授权范围展示；点击卡片可直接查看待办并办理。"
    }
    if (id === "WB-03") {
        return "采购单统计叠加本人负责人条件；询价按本人待处理的询价责任及实际授权范围展示。"
    }
    return "账款与资金余额分别按对应模块的授权范围展示，不代表本人名下金额。"
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
    const detail = (
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
    )
    const presentation = WORKBENCH_PRESENTATION[role.template.id]
    const hasPeriodSelector = id === "WB-01" || id === "WB-05"

    return (
        <PageScaffold
            density="compact"
            className={`${detailWorkspaceStyles.page} min-h-0 overflow-auto`}
        >
            <RoleWorkbenchHeader
                template={role.template}
                profile={profile}
                periodQuery={periodQuery}
                onRefresh={refresh}
                urgentCount={role.urgent.items.length}
                refreshing={
                    overview.queries.some((query) => query.isFetching) ||
                    role.stages.some((query) => query.isFetching) ||
                    role.urgent.isFetching ||
                    role.tasks.some((query) => query.statsFetching)
                }
            />
            <section
                aria-label="岗位业务概览"
                className="space-y-2 border-b border-border/60 py-2.5"
            >
                {hasPeriodSelector ? (
                    <RoleWorkbenchPeriod
                        workbenchId={id}
                        overview={overview}
                        custom={custom}
                    />
                ) : (
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <div className="flex items-center gap-1.5">
                            <h2 className="text-section-title font-semibold tracking-tight">
                                业务概览
                            </h2>
                            <WorkbenchDataInfo
                                id={`${presentation.prefix}-workbench-scope-info`}
                                title="业务概览"
                            >
                                <p>{scopeInfoSummary(id)}</p>
                                <p>
                                    各项指标均为截至当前的数据，包含全部期间的当前存量。具体统计口径与范围见指标详情。
                                </p>
                                <p>
                                    待办只包含本人具备处理资格的待办任务；其他审批及兼岗事项请查看待我处理。
                                </p>
                            </WorkbenchDataInfo>
                        </div>
                        <span className="text-xs text-muted-foreground">
                            截至当前 · 全部期间 ·{" "}
                            {overview.queries.some(
                                (query) => query.isFetching && !query.isPending,
                            )
                                ? "更新中…"
                                : workbenchUpdatedAt(
                                      overview.queries.map((query) =>
                                          query.isError
                                              ? undefined
                                              : query.data?.asOf,
                                      ),
                                  )}
                        </span>
                    </div>
                )}
                <RoleWorkbenchMetrics
                    overview={overview}
                    onSelect={select}
                    idPrefix={presentation.prefix}
                />
            </section>
            <RoleWorkbenchBody
                role={role}
                profile={profile}
                returnTo={href}
                period={overview.period}
                onSelect={select}
            />
            {detail}
        </PageScaffold>
    )
}
