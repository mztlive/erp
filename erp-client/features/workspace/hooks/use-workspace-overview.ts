"use client"

import { useQueries, useQuery } from "@tanstack/react-query"
import type { AccountProfile } from "@/features/auth/api"
import type { PeriodPreset } from "@/features/actual-profit-loss/types"
import { hasPermission } from "@/lib/permissions"
import { fetchOverviewData, fetchSalesCountTrend } from "../api/overview"
import {
    availableOverviewViews,
    overviewMetrics,
    overviewPeriod,
    type OverviewView,
} from "../lib/overview"
import { queryKeyRoots } from "@/lib/query-key-roots"

/** 账号、授权版本、模板与期间均参与缓存隔离；队列筛选不改变业务口径。 */
export function overviewIdentity(profile?: AccountProfile) {
    return [
        profile?.userid,
        profile?.policy_version,
        profile?.organization_version,
        profile ? [...profile.permissions].sort() : [],
        profile ? [...profile.role_ids].sort() : [],
        profile?.workbenches ?? [],
    ]
}

export function useWorkspaceOverview(
    profile: AccountProfile | undefined,
    requestedView?: OverviewView,
    preset: PeriodPreset = "month-to-date",
    loadTrend = true,
    custom?: { from: string; to: string },
) {
    const views = availableOverviewViews(profile)
    const view =
        requestedView && views.includes(requestedView)
            ? requestedView
            : views[0]
    const metrics =
        profile && view ? overviewMetrics(view, preset, profile, custom) : []
    const identity = overviewIdentity(profile)
    const sources = [
        ...new Map(
            metrics.map((metric) => [
                JSON.stringify(metric.source),
                metric.source,
            ]),
        ).values(),
    ]
    const sourceQueries = useQueries({
        queries: sources.map((source) => ({
            queryKey: [
                ...queryKeyRoots.workspaceHome,
                "overview",
                identity,
                source,
            ],
            queryFn: () => fetchOverviewData(source, 1, undefined, 1),
            staleTime: 60_000,
            refetchInterval: 300_000,
            retry: false,
            meta: { roleWorkbench: true },
        })),
    })
    const queries = metrics.map(
        (metric) =>
            sourceQueries[
                sources.findIndex(
                    (source) =>
                        JSON.stringify(source) ===
                        JSON.stringify(metric.source),
                )
            ]!,
    )
    const period = overviewPeriod(preset, custom)
    const trend = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "overview-trend",
            identity,
            period.to,
        ],
        queryFn: () => fetchSalesCountTrend(period.to),
        enabled:
            loadTrend &&
            view === "sales" &&
            Boolean(
                profile &&
                hasPermission(profile.permissions, "sales_order:list"),
            ),
        staleTime: 300_000,
        retry: false,
        meta: { roleWorkbench: true },
    })
    return { views, view, metrics, queries, trend, period, preset }
}
