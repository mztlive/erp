"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { writeW02FocusId } from "@/features/unified-task-queue/queue-url"
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
    WorkspaceMetricKey,
    WorkspaceWorkItem,
} from "@/features/workspace/types"

const VIEWER_TIMEZONE = "Asia/Shanghai"

/** 焦点还原走 sessionStorage，不落地址栏（内部 ID 禁止进 URL）。 */
const HOME_FOCUS_SESSION_KEY = "workspace-home.focus"

export type WorkspaceTaskIntent = "PROCESS" | "VIEW"

/**
 * W01 页面状态：URL 筛选解析、指标/范围切换、刷新编排与任务焦点还原。
 * 取数仍由 TanStack Query（useWorkspaceDashboardQuery / useAccountProfileQuery）承载。
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

    const [focusedStableNumber, setFocusedStableNumber] = React.useState<
        string | null
    >(null)

    const activeMetric = metricKeyFromUrlState(urlState)
    // scope 也是激活筛选：团队待处理无任务时走「当前筛选无结果」空态（D17）
    const hasActiveFilter = Boolean(
        urlState.scope === "team" || urlState.due || urlState.family,
    )

    const clearFocus = React.useCallback(() => {
        sessionStorage.removeItem(HOME_FOCUS_SESSION_KEY)
        setFocusedStableNumber(null)
    }, [])

    // 筛选/指标变更恒 replace，不膨胀历史（P2）；scope 默认值省略，URL 最小化
    const replaceUrl = React.useCallback(
        (next: WorkspaceUrlState) => {
            const qs = buildWorkspaceSearchParams(next)
            router.replace(`${pathname}${qs}`, { scroll: false })
        },
        [pathname, router],
    )

    const onMetricClick = React.useCallback(
        (key: WorkspaceMetricKey) => {
            clearFocus()
            replaceUrl(urlStateFromMetricKey(key, urlState))
        },
        [clearFocus, replaceUrl, urlState],
    )

    const clearFilters = React.useCallback(() => {
        clearFocus()
        replaceUrl({ scope: "mine" })
    }, [clearFocus, replaceUrl])

    const onOpenTask = React.useCallback(
        (item: WorkspaceWorkItem, intent: WorkspaceTaskIntent) => {
            // Persist focus so a return restores the row without putting the
            // internal task identity in the URL.
            sessionStorage.setItem(HOME_FOCUS_SESSION_KEY, item.stableNumber)
            if (intent === "PROCESS" && item.destinationWorkspaceId !== "W18") {
                writeW02FocusId(item.workItemId)
            }
        },
        [],
    )

    const refresh = React.useCallback(() => {
        void accountProfileQuery.refetch().then((profileResult) => {
            if (profileResult.isSuccess) void dashboardQuery.refetch()
        })
    }, [accountProfileQuery, dashboardQuery])

    const onScopeChange = React.useCallback(
        (scope: "mine" | "team") => {
            if (scope === urlState.scope) return
            clearFocus()
            replaceUrl({ ...urlState, scope })
        },
        [clearFocus, replaceUrl, urlState],
    )

    // Restore task focus after return from a target page. One-shot: the stored
    // focus is consumed here, so a plain refresh never re-scrolls unexpectedly.
    React.useEffect(() => {
        if (!view) return
        const stableNumber = sessionStorage.getItem(HOME_FOCUS_SESSION_KEY)
        if (!stableNumber) return
        const el = document.getElementById(`work-item-${stableNumber}`)
        sessionStorage.removeItem(HOME_FOCUS_SESSION_KEY)
        if (!el) return
        setFocusedStableNumber(stableNumber)
        el.scrollIntoView({ block: "nearest", behavior: "smooth" })
        if (el instanceof HTMLElement) {
            el.focus({ preventScroll: true })
        }
    }, [view])

    return {
        urlState,
        view,
        accountProfileQuery,
        dashboardQuery,
        refreshing,
        focusedStableNumber,
        activeMetric,
        hasActiveFilter,
        onMetricClick,
        clearFilters,
        onOpenTask,
        refresh,
        onScopeChange,
    }
}
