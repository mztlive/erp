"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { approvalKeys } from "@/features/approval-workflow/queries"
import { queryKeyRoots } from "@/lib/query-key-roots"

import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    useWorkspaceDashboardQuery,
    useWorkspaceSelectedTaskQuery,
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
    React.useEffect(() => {
        setSearchDraft(urlState.query ?? "")
    }, [urlState.query])
    const [autoNext, setAutoNext] = React.useState(false)
    const enteredFrom = React.useRef<string | null>(null)
    const [completionAnnouncement, setCompletionAnnouncement] = React.useState({
        sequence: 0,
        text: "",
    })

    const activeMetric = metricKeyFromUrlState(urlState)
    const hasActiveFilter = Boolean(
        urlState.due ||
        urlState.blocked ||
        urlState.family ||
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

    const openQueue = React.useCallback(
        (workItemType?: string, due?: "today" | "overdue") => {
            const next: WorkspaceUrlState = {
                view: "inbox",
                sort: "priority_due",
                queue: true,
                workItemType,
                due,
            }
            router.push(`${pathname}${buildWorkspaceSearchParams(next)}`, {
                scroll: false,
            })
            setSearchDraft("")
        },
        [pathname, router],
    )

    const onPageChange = React.useCallback(
        (page: number) => {
            replaceUrl({ ...urlState, page, currentWorkItemId: undefined })
        },
        [replaceUrl, urlState],
    )

    const returnState = { ...urlState, currentWorkItemId: undefined }
    const returnHref = `${pathname}${buildWorkspaceSearchParams(returnState)}`
    const backFromDetail = React.useCallback(() => {
        if (enteredFrom.current === returnHref) router.back()
        else router.replace(returnHref, { scroll: false })
        enteredFrom.current = null
    }, [returnHref, router])

    const onSelectTask = React.useCallback(
        (item: WorkspaceWorkItem) => {
            const href = `${pathname}${buildWorkspaceSearchParams({ ...urlState, currentWorkItemId: item.workItemId })}`
            if (urlState.currentWorkItemId)
                router.replace(href, { scroll: false })
            else {
                enteredFrom.current = `${pathname}${buildWorkspaceSearchParams(urlState)}`
                router.push(href, { scroll: false })
            }
        },
        [pathname, router, urlState],
    )

    const selectNextAfter = React.useCallback(
        (completedWorkItemId: string, preferredWorkItemId?: string) => {
            const items = view?.items ?? []
            const index = items.findIndex(
                (item) => item.workItemId === completedWorkItemId,
            )
            const next = preferredWorkItemId
                ? items.find((item) => item.workItemId === preferredWorkItemId)
                : (items[index + 1] ?? items[index - 1])
            const target =
                next ??
                items.find((item) => item.workItemId !== completedWorkItemId)
            if (autoNext && target)
                replaceUrl({
                    ...urlState,
                    currentWorkItemId: target.workItemId,
                })
            else backFromDetail()
            return autoNext ? target : undefined
        },
        [replaceUrl, urlState, view?.items, autoNext, backFromDetail],
    )

    /**
     * 决策应用后的收尾：切换到下一条并失效工作台列表查询。
     * 选中项只写 URL，不进入列表查询，避免把当前条提到队首。
     */
    const applyDecisionAfter = React.useCallback(
        (completedWorkItemId: string, preferredWorkItemId?: string) => {
            const next = selectNextAfter(
                completedWorkItemId,
                preferredWorkItemId,
            )
            setCompletionAnnouncement((current) => ({
                sequence: current.sequence + 1,
                text: next
                    ? `任务已完成，已切换到${next.objectTitle}`
                    : "任务已完成，已返回原列表",
            }))
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
                workItemType: undefined,
                page: 1,
                currentWorkItemId: undefined,
            })
        },
        [replaceUrl, urlState],
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
                currentWorkItemId: undefined,
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
                currentWorkItemId: undefined,
            })
        },
        [replaceUrl, urlState],
    )

    const applySearch = React.useCallback(() => {
        replaceUrl({
            ...urlState,
            query: searchDraft.trim() || undefined,
            page: 1,
            currentWorkItemId: undefined,
        })
    }, [replaceUrl, searchDraft, urlState])

    const clearSearch = React.useCallback(() => {
        setSearchDraft("")
        replaceUrl({
            ...urlState,
            query: undefined,
            page: 1,
            currentWorkItemId: undefined,
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

    const listedSelection = view?.items.find(
        (item) => item.workItemId === urlState.currentWorkItemId,
    )
    const detailQuery = useWorkspaceSelectedTaskQuery(
        accountProfileQuery.isSuccess &&
            view?.access === "allowed" &&
            urlState.view !== "started" &&
            !listedSelection
            ? urlState.currentWorkItemId
            : undefined,
        accountProfileQuery.data,
    )
    const selected = listedSelection ?? detailQuery.data

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
        autoNext,
        setAutoNext,
        backFromDetail,
        detailQuery,
        completionAnnouncement,
        selected,
        onMetricClick,
        clearFilters,
        openQueue,
        onPageChange,
        onSelectTask,
        selectNextAfter,
        applyDecisionAfter,
        onFamilyChange,
        onQueueFiltersChange,
        onSortChange,
        applySearch,
        clearSearch,
        refresh,
    }
}
