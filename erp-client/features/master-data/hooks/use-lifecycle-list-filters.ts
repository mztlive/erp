"use client"

import * as React from "react"

import {
    useListUrl,
    useSearchDraft,
} from "@/features/master-data/hooks/use-list-url"
import { revisionTimingFilterLabel } from "@/features/master-data/lib/copy"
import {
    parseLifecycleStatus,
    parseRevisionTiming,
} from "@/features/master-data/lib/list-filters"

/** 可被单独移除的已生效条件。 */
export type DictionaryFilterKey = "q" | "revisionTiming"

export type DictionaryAppliedChip = Readonly<{
    key: DictionaryFilterKey
    label: string
}>

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
    const metricKey = lifecycleStatus
    const hasStructuredListFilters =
        lifecycleStatus !== "all" || revisionTiming !== "all"

    const [filterPanelOpen, setFilterPanelOpen] = React.useState(
        hasStructuredListFilters,
    )
    const [revisionTimingDraft, setRevisionTimingDraft] =
        React.useState(revisionTiming)

    /** Tab 以外的已生效条件可从 chip 单独撤销。 */
    const appliedChips = React.useMemo<readonly DictionaryAppliedChip[]>(() => {
        const chips: DictionaryAppliedChip[] = []
        if (q.trim()) {
            chips.push({ key: "q", label: `搜索：${q.trim()}` })
        }
        if (revisionTiming !== "all") {
            chips.push({
                key: "revisionTiming",
                label: `版本：${revisionTimingFilterLabel(revisionTiming)}`,
            })
        }
        return chips
    }, [q, revisionTiming])

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
            revisionTiming:
                revisionTimingDraft === "all" ? null : revisionTimingDraft,
            page: null,
        })
        resetPagination()
        setFilterPanelOpen(false)
    }, [patchUrl, resetPagination, revisionTimingDraft, searchDraft])

    /** 移除单个普通筛选条件，保留当前启停 Tab。 */
    const removeFilter = React.useCallback(
        (key: DictionaryFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "revisionTiming") setRevisionTimingDraft("all")
            patchUrl({ [key]: null, page: null })
            resetPagination()
        },
        [patchUrl, resetPagination, setSearchDraft],
    )

    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        revisionTimingDraft !== revisionTiming

    /** 字典页无更多面板；保留草稿重置以免外部仍调用。 */
    const resetMoreFilters = React.useCallback(() => {
        setRevisionTimingDraft("all")
    }, [])

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
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
        setRevisionTimingDraft(revisionTiming)
    }, [lifecycleStatus, revisionTiming])

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
        revisionTimingDraft,
        setRevisionTimingDraft,
        hasPendingChanges,
        appliedChips,
        pagination,
        setPagination,
        changePagination,
        patchUrl,
        changeLifecycle,
        commitSearch,
        applyListFilters,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
    }
}
