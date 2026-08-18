"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { useWorkspaceDashboardQuery } from "@/features/workspace/hooks/queries"
import {
    buildWorkspaceSearchParams,
    metricKeyFromUrlState,
    parseWorkspaceSearchParams,
    toTodayWorkspaceQuery,
    urlStateFromMetricKey,
    type WorkspaceUrlState,
} from "@/features/workspace/lib/url-state"
import type {
    WorkspaceFamilyFilter,
    WorkspaceMetricKey,
    WorkspaceSort,
    WorkspaceWorkItem,
} from "@/features/workspace/types"

const VIEWER_TIMEZONE = "Asia/Shanghai"

/**
 * W01 页面状态：URL 筛选、指标切换、当前项与连续处理后选中下一条。
 */
export function useWorkspaceHome() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

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
    const [narrowDetailOpen, setNarrowDetailOpen] = React.useState(false)

    const activeMetric = metricKeyFromUrlState(urlState)
    const hasActiveFilter = Boolean(
        urlState.view !== "inbox" ||
        urlState.due ||
        urlState.blocked ||
        urlState.family ||
        urlState.workItemType ||
        urlState.query,
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
            replaceUrl(urlStateFromMetricKey(key, urlState))
            setNarrowDetailOpen(false)
        },
        [replaceUrl, urlState],
    )

    const clearFilters = React.useCallback(() => {
        replaceUrl({
            view: "inbox",
            sort: "priority_due",
        })
        setSearchDraft("")
        setNarrowDetailOpen(false)
    }, [replaceUrl])

    const onSelectTask = React.useCallback(
        (item: WorkspaceWorkItem) => {
            replaceUrl({
                ...urlState,
                currentWorkItemId: item.workItemId,
            })
            setNarrowDetailOpen(true)
        },
        [replaceUrl, urlState],
    )

    const selectNextAfter = React.useCallback(
        (completedWorkItemId: string) => {
            const items = view?.items ?? []
            const index = items.findIndex(
                (item) => item.workItemId === completedWorkItemId,
            )
            const next = items[index + 1] ?? items[index - 1]
            replaceUrl({
                ...urlState,
                currentWorkItemId: next?.workItemId,
            })
            if (!next) setNarrowDetailOpen(false)
        },
        [replaceUrl, urlState, view?.items],
    )

    const onFamilyChange = React.useCallback(
        (family?: WorkspaceFamilyFilter) => {
            replaceUrl({
                ...urlState,
                family,
                currentWorkItemId: undefined,
            })
        },
        [replaceUrl, urlState],
    )

    const onSortChange = React.useCallback(
        (sort: WorkspaceSort) => {
            replaceUrl({ ...urlState, sort })
        },
        [replaceUrl, urlState],
    )

    const applySearch = React.useCallback(() => {
        replaceUrl({
            ...urlState,
            query: searchDraft.trim() || undefined,
            currentWorkItemId: undefined,
        })
    }, [replaceUrl, searchDraft, urlState])

    const refresh = React.useCallback(() => {
        void accountProfileQuery.refetch().then((profileResult) => {
            if (profileResult.isSuccess) void dashboardQuery.refetch()
        })
    }, [accountProfileQuery, dashboardQuery])

    const selected =
        view?.items.find(
            (item) => item.workItemId === urlState.currentWorkItemId,
        ) ?? view?.items[0]

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
        narrowDetailOpen,
        setNarrowDetailOpen,
        selected,
        onMetricClick,
        clearFilters,
        onSelectTask,
        selectNextAfter,
        onFamilyChange,
        onSortChange,
        applySearch,
        refresh,
    }
}
