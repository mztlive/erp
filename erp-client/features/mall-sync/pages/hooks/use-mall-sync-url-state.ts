"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import type { MallSyncViewName } from "@/features/mall-sync/types"
import {
    ALL_OBJECT_PARAMS,
    parseView,
    VIEW_OBJECT_PARAMS,
} from "@/features/mall-sync/lib/presentation"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"

export type PatchUrl = (
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean },
) => void

export type MallSyncUrlState = {
    view: MallSyncViewName
    q: string
    jobId?: string
    snapshotId?: string
    mappingTaskId?: string
    workItemId?: string
    differenceId?: string
    queueContextId: string
    searchInput: string
    setSearchInput: (value: string) => void
    searchInputRef: React.RefObject<HTMLInputElement | null>
    patchUrl: PatchUrl
    clearObjectParamsForView: (next: MallSyncViewName) => Record<string, null>
    hasActiveFilters: boolean
    clearAllFilters: () => void
    searchParams: ReturnType<typeof useSearchParams>
}

export function useMallSyncUrlState(): MallSyncUrlState {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const view = parseView(searchParams.get("view"))
    const q = searchParams.get("q") ?? ""
    const jobId = searchParams.get("jobId") ?? undefined
    const snapshotId = searchParams.get("snapshotId") ?? undefined
    const mappingTaskId = searchParams.get("mappingTaskId") ?? undefined
    const workItemId =
        searchParams.get("workItemId") ??
        searchParams.get("currentWorkItemId") ??
        undefined
    const differenceId = searchParams.get("differenceId") ?? undefined
    const queueContextId =
        searchParams.get("queueContextId") ?? "queue:W17:mall-sync"

    const [searchInput, setSearchInput] = React.useState(q)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    React.useEffect(() => {
        setSearchInput(q)
    }, [q])

    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput === q) return
            patchUrl({ q: searchInput.trim() || null }, { replace: true })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [searchInput])

    // / 聚焦搜索
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            ) {
                return
            }
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            if (
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    function patchUrl(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean },
    ) {
        patchSearchParams(
            { router, pathname, searchParams, view },
            patch,
            options,
        )
    }

    const clearObjectParamsForView = React.useCallback(
        (next: MallSyncViewName) => {
            const keep = new Set(VIEW_OBJECT_PARAMS[next])
            const patch: Record<string, null> = {}
            for (const key of ALL_OBJECT_PARAMS) {
                if (!keep.has(key)) patch[key] = null
            }
            return patch
        },
        [],
    )

    const hasActiveFilters = Boolean(
        q || jobId || snapshotId || mappingTaskId || workItemId || differenceId,
    )

    const clearAllFilters = () => {
        setSearchInput("")
        patchUrl(
            {
                q: null,
                jobId: null,
                snapshotId: null,
                mappingTaskId: null,
                workItemId: null,
                currentWorkItemId: null,
                differenceId: null,
            },
            { replace: true },
        )
    }

    return {
        view,
        q,
        jobId,
        snapshotId,
        mappingTaskId,
        workItemId,
        differenceId,
        queueContextId,
        searchInput,
        setSearchInput,
        searchInputRef,
        patchUrl,
        clearObjectParamsForView,
        hasActiveFilters,
        clearAllFilters,
        searchParams,
    }
}
