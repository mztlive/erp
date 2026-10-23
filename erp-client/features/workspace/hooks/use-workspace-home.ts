"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { approvalKeys } from "@/features/approval-workflow/queries"
import { queryKeyRoots } from "@/lib/query-key-roots"

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
        () => parseWorkspaceSearchParams(searchParams),
        [searchParams],
    )

    const queryInput = React.useMemo(
        () => toTodayWorkspaceQuery(urlState, VIEWER_TIMEZONE),
        [urlState],
    )

    const accountProfileQuery = useAccountProfileQuery()
    const dashboardQuery = useWorkspaceDashboardQuery(
        queryInput,
        accountProfileQuery.data,
    )
    const view = dashboardQuery.data
    const refreshing =
        (accountProfileQuery.isFetching || dashboardQuery.isFetching) &&
        !dashboardQuery.isPending &&
        !!view

    const [searchDraft, setSearchDraft] = React.useState(urlState.query ?? "")
    React.useEffect(() => {
        setSearchDraft(urlState.query ?? "")
    }, [urlState.query])
    const activeMetric = metricKeyFromUrlState(urlState)
    const hasActiveFilter = Boolean(
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
            router.replace(`${pathname}${qs}`, { scroll: false })
        },
        [pathname, router],
    )

    const onMetricClick = React.useCallback(
        (key: WorkspaceMetricKey) => {
            const base = { view: urlState.view, sort: "priority_due" as const }
            router.push(
                `${pathname}${buildWorkspaceSearchParams({ ...urlStateFromMetricKey(key, base), queue: true })}`,
                { scroll: false },
            )
            setSearchDraft("")
        },
        [pathname, router, urlState],
    )

    const clearFilters = React.useCallback(() => {
        replaceUrl({
            view: urlState.view,
            sort: "priority_due",
            queue: true,
        })
        setSearchDraft("")
    }, [replaceUrl, urlState.view])

    const onPageChange = React.useCallback(
        (page: number) => {
            replaceUrl({ ...urlState, page })
        },
        [replaceUrl, urlState],
    )

    const onSelectTask = React.useCallback(
        (item: WorkspaceWorkItem) => {
            const returnTo = `${pathname}${buildWorkspaceSearchParams(urlState)}`
            router.push(
                buildWorkspaceDetailHref(
                    item.workItemType === "APPROVAL_INSTANCE"
                        ? "approvals"
                        : "tasks",
                    item.workItemId,
                    { returnTo },
                ),
            )
        },
        [pathname, router, urlState],
    )

    const onQueueFiltersChange = React.useCallback(
        (
            patch: Pick<WorkspaceUrlState, "workItemType" | "due" | "blocked">,
        ) => {
            replaceUrl({
                ...urlState,
                ...patch,
                queue: true,
                page: 1,
            })
        },
        [replaceUrl, urlState],
    )

    const onSortChange = React.useCallback(
        (sort: WorkspaceSort) => {
            replaceUrl({
                ...urlState,
                sort,
                page: 1,
            })
        },
        [replaceUrl, urlState],
    )

    const applySearch = React.useCallback(() => {
        replaceUrl({
            ...urlState,
            query: searchDraft.trim() || undefined,
            page: 1,
        })
    }, [replaceUrl, searchDraft, urlState])

    const clearSearch = React.useCallback(() => {
        setSearchDraft("")
        replaceUrl({
            ...urlState,
            query: undefined,
            page: 1,
        })
    }, [replaceUrl, urlState])

    const refresh = React.useCallback(() => {
        void accountProfileQuery.refetch().then((profileResult) => {
            if (!profileResult.isSuccess) return
            for (const queryKey of [
                workspaceHomeKeys.all,
                approvalKeys.all,
                queryKeyRoots.fulfillmentOperations,
                queryKeyRoots.supplierPayables,
            ]) {
                void queryClient.invalidateQueries({ queryKey })
            }
        })
    }, [accountProfileQuery, queryClient])

    return {
        urlState,
        view,
        accountProfileQuery,
        dashboardQuery,
        refreshing,
        activeMetric,
        hasActiveFilter,
        searchDraft,
        setSearchDraft,
        onMetricClick,
        clearFilters,
        onPageChange,
        onSelectTask,
        onQueueFiltersChange,
        onSortChange,
        applySearch,
        clearSearch,
        refresh,
    }
}
