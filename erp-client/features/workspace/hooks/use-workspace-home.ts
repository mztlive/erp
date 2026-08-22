"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

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
            // 窄屏才打开详情 Sheet；桌面端详情已内联展示，弹出模态框会遮挡并
            // 把背景（桌面详情区）置为 aria-hidden。
            setNarrowDetailOpen(
                typeof window !== "undefined" &&
                    typeof window.matchMedia === "function" &&
                    window.matchMedia("(max-width: 1023.98px)").matches,
            )
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

    /**
     * 决策应用后的收尾：切换到下一条并失效工作台列表查询。
     * 列表查询键随 URL(currentWorkItemId) 变化；若直接 refetch 旧键，
     * 请求会携带已完成任务的 current_work_item_id 被后端 404，列表不更新。
     * 失效后由键回退后的活动查询重新拉取（不受 staleTime 影响）。
     */
    const applyDecisionAfter = React.useCallback(
        (completedWorkItemId: string) => {
            selectNextAfter(completedWorkItemId)
            void queryClient.invalidateQueries({
                queryKey: workspaceHomeKeys.all,
            })
        },
        [queryClient, selectNextAfter],
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
        applyDecisionAfter,
        onFamilyChange,
        onSortChange,
        applySearch,
        refresh,
    }
}
