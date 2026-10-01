"use client"

import { useCallback, useMemo, useState } from "react"
import type { BackgroundJobListParams } from "../api"
import {
    BACKGROUND_JOB_FILTER_CHIP_FIELDS,
    EMPTY_BACKGROUND_JOB_FILTERS,
    backgroundJobFilterChips,
    type BackgroundJobFilterValues,
} from "../lib/filters"

/** 查询草稿、已应用条件与分页在同一处更新，保留显式查询的交互。 */
export function useBackgroundJobsFilters(
    isAdmin: boolean,
    currentUserId: string | undefined,
) {
    const [draft, setDraft] = useState(EMPTY_BACKGROUND_JOB_FILTERS)
    const [applied, setApplied] = useState(EMPTY_BACKGROUND_JOB_FILTERS)
    const [panelOpen, setPanelOpen] = useState(false)
    const [page, setPage] = useState(1)

    const changeDraft = useCallback(
        <K extends keyof BackgroundJobFilterValues>(
            field: K,
            value: BackgroundJobFilterValues[K],
        ) => {
            setDraft((current) => ({ ...current, [field]: value }))
        },
        [],
    )

    const apply = useCallback(() => {
        setApplied({ ...draft, jobNo: draft.jobNo.trim() })
        setPage(1)
        setPanelOpen(false)
    }, [draft])

    const resetMore = useCallback(() => {
        setDraft((current) => ({ ...current, domain: "", scope: "all" }))
    }, [])

    const cancelMore = useCallback(() => {
        setDraft((current) => ({
            ...current,
            domain: applied.domain,
            scope: applied.scope,
        }))
        setPanelOpen(false)
    }, [applied.domain, applied.scope])

    const toggleMore = useCallback(() => {
        if (panelOpen) cancelMore()
        else setPanelOpen(true)
    }, [panelOpen, cancelMore])

    const clearAll = useCallback(() => {
        setDraft(EMPTY_BACKGROUND_JOB_FILTERS)
        setApplied(EMPTY_BACKGROUND_JOB_FILTERS)
        setPanelOpen(false)
        setPage(1)
    }, [])

    const clearChip = useCallback((key: string) => {
        const field = Object.hasOwn(BACKGROUND_JOB_FILTER_CHIP_FIELDS, key)
            ? BACKGROUND_JOB_FILTER_CHIP_FIELDS[key]
            : undefined
        if (field) {
            const reset = (current: BackgroundJobFilterValues) => ({
                ...current,
                [field]: EMPTY_BACKGROUND_JOB_FILTERS[field],
            })
            setDraft(reset)
            setApplied(reset)
        }
        setPage(1)
    }, [])

    const listParams = useMemo<BackgroundJobListParams>(
        () => ({
            page,
            page_size: 20,
            job_no: applied.jobNo || undefined,
            status: applied.status === "all" ? undefined : applied.status,
            job_type: applied.jobType || undefined,
            domain_job_type: applied.domain || undefined,
            requested_by:
                isAdmin && applied.scope === "mine" ? currentUserId : undefined,
        }),
        [page, applied, isAdmin, currentUserId],
    )

    const hasActiveFilters =
        applied.jobNo !== "" ||
        applied.status !== "all" ||
        applied.jobType !== "" ||
        applied.domain !== "" ||
        (isAdmin && applied.scope !== "all")
    const hasPendingChanges =
        draft.jobNo.trim() !== applied.jobNo ||
        draft.status !== applied.status ||
        draft.jobType !== applied.jobType ||
        draft.domain !== applied.domain ||
        (isAdmin && draft.scope !== applied.scope)

    return {
        draft,
        applied,
        panelOpen,
        page,
        setPage,
        changeDraft,
        apply,
        resetMore,
        toggleMore,
        clearAll,
        clearChip,
        listParams,
        hasActiveFilters,
        hasPendingChanges,
        chips: backgroundJobFilterChips(applied, isAdmin),
        moreCount:
            Number(applied.domain !== "") +
            Number(isAdmin && applied.scope === "mine"),
    }
}

export type BackgroundJobsFilters = ReturnType<typeof useBackgroundJobsFilters>
