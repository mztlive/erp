"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import {
    workspaceQueueLocation,
    workspaceQueueParams,
} from "@/lib/workspace-queue-routes"
import { workspaceHost } from "@/lib/workspace-tabs"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { approvalKeys } from "@/features/approval-workflow/queries"
import {
    hasWorkspaceFilters,
    type WorkspaceFilters,
} from "@/lib/workspace-search"
import { queryKeyRoots } from "@/lib/query-key-roots"
import type { OverviewView } from "../lib/overview"
import type { PeriodPreset } from "@/features/actual-profit-loss/types"
import { hasPermission } from "@/lib/permissions"

import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    useWorkspaceDashboardQuery,
    workspaceHomeKeys,
} from "@/features/workspace/hooks/queries"
import {
    buildWorkspaceSearchParams,
    metricKeyFromUrlState,
    parseWorkspaceSearchParams,
    toTodayWorkspaceQuery,
    urlStateFromMetricKey,
    type WorkspaceUrlState,
} from "@/features/workspace/lib/url-state"
import type {
    WorkspaceMetricKey,
    WorkspaceSort,
    WorkspaceWorkItem,
} from "@/features/workspace/types"

const VIEWER_TIMEZONE = "Asia/Shanghai"
/**
 * W01 队列状态：URL 筛选、指标切换与独立详情导航。
 */
export function useWorkspaceHome() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const queryClient = useQueryClient()

    const urlState = React.useMemo(
        () =>
            parseWorkspaceSearchParams(
                workspaceQueueParams(
                    pathname,
                    new URLSearchParams(searchParams),
                ),
            ),
        [pathname, searchParams],
    )

    const queryInput = React.useMemo(
        () => toTodayWorkspaceQuery(urlState, VIEWER_TIMEZONE),
        [urlState],
    )

    const accountProfileQuery = useAccountProfileQuery()
    const dashboardQuery = useWorkspaceDashboardQuery(
        queryInput,
        accountProfileQuery.data,
        Boolean(
            accountProfileQuery.data &&
            hasPermission(
                accountProfileQuery.data.permissions,
                urlState.view === "started"
                    ? "approval_instance:read"
                    : "work_item:list",
            ),
        ),
    )
    const view = dashboardQuery.data
    const refreshing =
        (accountProfileQuery.isFetching || dashboardQuery.isFetching) &&
        !dashboardQuery.isPending &&
        !!view

    const activeMetric = metricKeyFromUrlState(urlState)
    const hasActiveFilter = Boolean(
        hasWorkspaceFilters(urlState.filters) ||
        urlState.due ||
        urlState.blocked ||
        urlState.workItemType ||
        urlState.query ||
        urlState.handlerUserIds ||
        urlState.salesOrderIds ||
        urlState.purchaseOrderIds,
    )

    const replaceUrl = React.useCallback(
        (next: WorkspaceUrlState) => {
            const qs = buildWorkspaceSearchParams(next)
            router.replace(
                workspaceQueueLocation(next.view, new URLSearchParams(qs)),
                { scroll: false },
            )
        },
        [router],
    )

    const viewStates = React.useRef<Partial<Record<string, WorkspaceUrlState>>>(
        {},
    )
    const onMetricClick = React.useCallback(
        (key: WorkspaceMetricKey) => {
            viewStates.current[urlState.view] = urlState
            const target = urlStateFromMetricKey(key, {
                view: urlState.view,
                sort: "priority_due",
            })
            const saved = viewStates.current[target.view]
            const next = {
                ...target,
                ...saved,
                query: urlState.query,
                page: 1,
                scopeVersion: undefined,
                queue: true,
                overviewView: urlState.overviewView,
                overviewPeriod: urlState.overviewPeriod,
            }
            if (key === "overdue" || key === "blocked")
                Object.assign(next, target, {
                    overviewView: urlState.overviewView,
                    overviewPeriod: urlState.overviewPeriod,
                    query: undefined,
                    filters:
                        key === "overdue"
                            ? { due: "overdue" }
                            : { blocked: true },
                })
            router.push(
                workspaceQueueLocation(
                    next.view,
                    new URLSearchParams(buildWorkspaceSearchParams(next)),
                ),
                {
                    scroll: false,
                },
            )
        },
        [router, urlState],
    )

    const applyFilters = React.useCallback(
        (query: string, filters: WorkspaceFilters) => {
            replaceUrl({
                view: urlState.view,
                sort: urlState.sort,
                query: query.trim() || undefined,
                filters,
                page: 1,
                queue: true,
                overviewView: urlState.overviewView,
                overviewPeriod: urlState.overviewPeriod,
            })
        },
        [
            replaceUrl,
            urlState.view,
            urlState.sort,
            urlState.overviewView,
            urlState.overviewPeriod,
        ],
    )

    const clearFilters = React.useCallback(() => {
        replaceUrl({
            view: urlState.view,
            sort: "priority_due",
            queue: true,
            overviewView: urlState.overviewView,
            overviewPeriod: urlState.overviewPeriod,
        })
    }, [
        replaceUrl,
        urlState.view,
        urlState.overviewView,
        urlState.overviewPeriod,
    ])

    const onOverviewChange = React.useCallback(
        (overviewView: OverviewView, overviewPeriod: PeriodPreset) => {
            replaceUrl({ ...urlState, overviewView, overviewPeriod })
        },
        [replaceUrl, urlState],
    )

    const onPageChange = React.useCallback(
        (page: number) => {
            replaceUrl({
                ...urlState,
                page,
                scopeVersion: page > 1 ? view?.scopeVersion : undefined,
            })
        },
        [replaceUrl, urlState, view?.scopeVersion],
    )

    const onSelectTask = React.useCallback(
        (item: WorkspaceWorkItem | string) => {
            const returnTo = workspaceQueueLocation(
                urlState.view,
                new URLSearchParams(buildWorkspaceSearchParams(urlState)),
            )
            router.push(
                buildWorkspaceDetailHref(
                    typeof item !== "string" &&
                        item.workItemType === "APPROVAL_INSTANCE"
                        ? "approvals"
                        : "tasks",
                    typeof item === "string" ? item : item.workItemId,
                    { returnTo },
                ),
            )
        },
        [router, urlState],
    )

    const onSortChange = React.useCallback(
        (sort: WorkspaceSort) => {
            replaceUrl({
                ...urlState,
                sort,
                page: 1,
                scopeVersion: undefined,
            })
        },
        [replaceUrl, urlState],
    )

    const refresh = React.useCallback(() => {
        if ((urlState.page ?? 1) > 1)
            replaceUrl({ ...urlState, page: 1, scopeVersion: undefined })
        void accountProfileQuery.refetch().then((profileResult) => {
            if (!profileResult.isSuccess) return
            workspaceHost()?.changed(window)
            for (const queryKey of [
                workspaceHomeKeys.all,
                ["workspace-filter-options"],
                approvalKeys.all,
                queryKeyRoots.fulfillmentOperations,
                queryKeyRoots.supplierPayables,
            ]) {
                void queryClient.invalidateQueries({ queryKey })
            }
        })
    }, [accountProfileQuery, queryClient, urlState, replaceUrl])

    return {
        urlState,
        view,
        accountProfileQuery,
        dashboardQuery,
        refreshing,
        activeMetric,
        hasActiveFilter,
        onMetricClick,
        onOverviewChange,
        clearFilters,
        onPageChange,
        onSelectTask,
        onSortChange,
        applyFilters,
        refresh,
    }
}
