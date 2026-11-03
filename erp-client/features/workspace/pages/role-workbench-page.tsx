"use client"

import { useEffect, useRef, useState } from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { useSearchParams } from "next/navigation"
import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
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
    SalesWorkbenchPeriod,
} from "../components/role-workbench-header"
import { RoleWorkbenchMetrics } from "../components/role-workbench-metrics"
import { RoleWorkbenchBody } from "../components/role-workbench-body"
import { WorkbenchDataInfo } from "../components/workbench-data-info"
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
    return (
        <PageScaffold
            density="compact"
            className="gap-5 overflow-auto bg-background"
        >
            <RoleWorkbenchHeader
                template={role.template}
                profile={profile}
                periodQuery={periodQuery}
                onRefresh={refresh}
                refreshing={
                    overview.queries.some((query) => query.isFetching) ||
                    role.stages.some((query) => query.isFetching) ||
                    role.urgent.isFetching ||
                    role.tasks.some((query) => query.statsFetching)
                }
            />
            <section aria-label="岗位业务概览" className="space-y-3">
                {id === "WB-01" ? (
                    <SalesWorkbenchPeriod overview={overview} custom={custom} />
                ) : (
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <div className="flex items-center gap-1.5">
                            <h2 className="text-sm font-semibold">业务概览</h2>
                            <WorkbenchDataInfo
                                id={`${presentation.prefix}-workbench-scope-info`}
                                title="业务概览"
                            >
                                <p>
                                    {id === "WB-03"
                                        ? "采购单统计叠加本人负责人条件；询价按本人开放询价责任及实际授权范围展示。"
                                        : "账款与资金余额分别按对应模块的授权范围展示，不代表本人名下金额。"}
                                </p>
                                <p>
                                    各项指标均为截至当前的数据，包含全部期间的当前存量。具体统计口径与范围见指标详情。
                                </p>
                                <p>
                                    待办只包含本人具备处理资格的开放任务；其他审批及兼岗事项请查看待我处理。
                                </p>
                            </WorkbenchDataInfo>
                        </div>
                        <span className="text-xs text-muted-foreground">
                            截至当前 · 全部期间
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
                onSelect={select}
            />
            {detail}
        </PageScaffold>
    )
}
