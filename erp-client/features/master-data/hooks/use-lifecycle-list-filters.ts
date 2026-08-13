"use client"

import * as React from "react"

import {
    useListUrl,
    useSearchDraft,
} from "@/features/master-data/hooks/use-list-url"
import {
    parseLifecycleStatus,
    parseRevisionTiming,
} from "@/features/master-data/lib/list-filters"

/**
 * 字典 / 仓库列表：搜索 + 启停 + 版本。
 * URL 为唯一事实源。
 */
export function useLifecycleListFilters(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const {
        searchParams,
        patchUrl,
        q,
        pagination,
        setPagination,
        resetPagination,
        changePagination,
    } = useListUrl()
    const { searchDraft, setSearchDraft } = useSearchDraft(q, searchInputRef)

    const lifecycleStatus = parseLifecycleStatus(
        searchParams.get("lifecycleStatus"),
    )
    const revisionTiming = parseRevisionTiming(
        searchParams.get("revisionTiming"),
    )
    const metricKey = searchParams.get("metricKey") ?? "all"
    const hasStructuredListFilters =
        lifecycleStatus !== "all" || revisionTiming !== "all"

    const [filterPanelOpen, setFilterPanelOpen] = React.useState(
        hasStructuredListFilters,
    )
    const [lifecycleStatusDraft, setLifecycleStatusDraft] =
        React.useState(lifecycleStatus)
    const [revisionTimingDraft, setRevisionTimingDraft] =
        React.useState(revisionTiming)

    const commitSearch = React.useCallback(() => {
        const next = searchDraft.trim()
        if (next === q.trim()) return
        patchUrl({ q: next || null, page: null })
        resetPagination()
    }, [patchUrl, q, resetPagination, searchDraft])

    const changeLifecycle = React.useCallback(
        (next: "enabled" | "disabled" | "all") => {
            if (next === lifecycleStatus) return
            patchUrl({
                lifecycleStatus: next === "all" ? null : next,
                metricKey: next === "all" ? null : next,
                page: null,
            })
            resetPagination()
        },
        [lifecycleStatus, patchUrl, resetPagination],
    )

    const applyListFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || null,
            lifecycleStatus:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            metricKey:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            revisionTiming:
                revisionTimingDraft === "all" ? null : revisionTimingDraft,
            page: null,
        })
        resetPagination()
    }, [
        lifecycleStatusDraft,
        patchUrl,
        resetPagination,
        revisionTimingDraft,
        searchDraft,
    ])

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setLifecycleStatusDraft("all")
        setRevisionTimingDraft("all")
        setFilterPanelOpen(false)
        patchUrl({
            q: null,
            lifecycleStatus: null,
            metricKey: null,
            revisionTiming: null,
            page: null,
        })
        resetPagination()
    }, [patchUrl, resetPagination, setSearchDraft])

    React.useEffect(() => {
        setLifecycleStatusDraft(lifecycleStatus)
        setRevisionTimingDraft(revisionTiming)
        setFilterPanelOpen(hasStructuredListFilters)
    }, [hasStructuredListFilters, lifecycleStatus, revisionTiming])

    return {
        q,
        lifecycleStatus,
        revisionTiming,
        metricKey,
        hasStructuredListFilters,
        searchDraft,
        setSearchDraft,
        filterPanelOpen,
        setFilterPanelOpen,
        lifecycleStatusDraft,
        setLifecycleStatusDraft,
        revisionTimingDraft,
        setRevisionTimingDraft,
        pagination,
        setPagination,
        changePagination,
        patchUrl,
        changeLifecycle,
        commitSearch,
        applyListFilters,
        clearAllFilters,
    }
}
