"use client"

import * as React from "react"

import {
    useListUrl,
    useSearchDraft,
} from "@/features/master-data/hooks/use-list-url"
import {
    lifecycleFilterLabel,
    revisionTimingFilterLabel,
} from "@/features/master-data/lib/copy"
import {
    parseLifecycleStatus,
    parseRevisionTiming,
} from "@/features/master-data/lib/list-filters"

/** 可被单独移除的已生效条件。 */
export type DictionaryFilterKey = "q" | "lifecycleStatus" | "revisionTiming"

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

    /** 所有已生效条件均可从 chip 单独撤销。 */
    const appliedChips = React.useMemo<readonly DictionaryAppliedChip[]>(() => {
        const chips: DictionaryAppliedChip[] = []
        if (q.trim()) {
            chips.push({ key: "q", label: `搜索：${q.trim()}` })
        }
        if (lifecycleStatus !== "all") {
            chips.push({
                key: "lifecycleStatus",
                label: `启停：${lifecycleFilterLabel(lifecycleStatus)}`,
            })
        }
        if (revisionTiming !== "all") {
            chips.push({
                key: "revisionTiming",
                label: `版本：${revisionTimingFilterLabel(revisionTiming)}`,
            })
        }
        return chips
    }, [lifecycleStatus, q, revisionTiming])

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
        setFilterPanelOpen(false)
    }, [
        lifecycleStatusDraft,
        patchUrl,
        resetPagination,
        revisionTimingDraft,
        searchDraft,
    ])

    /** 移除单个已生效条件；启停同时移除指标高亮参数。 */
    const removeFilter = React.useCallback(
        (key: DictionaryFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "lifecycleStatus") setLifecycleStatusDraft("all")
            if (key === "revisionTiming") setRevisionTimingDraft("all")
            patchUrl(
                key === "lifecycleStatus"
                    ? { lifecycleStatus: null, metricKey: null, page: null }
                    : { [key]: null, page: null },
            )
            resetPagination()
        },
        [patchUrl, resetPagination, setSearchDraft],
    )

    /** 仅清除「更多筛选」；保留关键词和快捷筛选（启停指标），并保持面板展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setRevisionTimingDraft("all")
        patchUrl({ revisionTiming: null, page: null })
        resetPagination()
    }, [patchUrl, resetPagination])

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
        lifecycleStatusDraft,
        setLifecycleStatusDraft,
        revisionTimingDraft,
        setRevisionTimingDraft,
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
