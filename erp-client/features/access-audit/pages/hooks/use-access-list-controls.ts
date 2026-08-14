"use client"

import * as React from "react"

export type DebouncedAuditFilters = {
    actorId?: string
    traceId?: string
    objectType?: string
    objectId?: string
}

type AccessListControlsInput = {
    qParam: string
    patchUrl: (
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean },
    ) => void
    /** 筛选 URL 修补：统一回到第一页，避免筛选变窄后空态与总数并存。 */
    patchFilterUrl: (
        patch: Record<string, string | null | undefined>,
    ) => void
    resetPaginationToFirstPage: () => void
}

/** 列表级输入状态：搜索（含防抖）与高级筛选（含防抖）。 */
function useAccessListControls({
    qParam,
    patchUrl,
    patchFilterUrl,
    resetPaginationToFirstPage,
}: AccessListControlsInput) {
    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    React.useEffect(() => {
        setSearchInput(qParam)
    }, [qParam])

    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput === qParam) return
            patchUrl(
                {
                    q: searchInput.trim() || null,
                    page: null,
                },
                { replace: true },
            )
            resetPaginationToFirstPage()
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [searchInput])

    // 高级筛选输入防抖：不逐键发请求
    const [debouncedFilters, setDebouncedFilters] =
        React.useState<DebouncedAuditFilters>({})
    const lastPatchedFilters = React.useRef("")
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            const next = debouncedFilters
            const key = JSON.stringify(next)
            if (key === lastPatchedFilters.current) return
            lastPatchedFilters.current = key
            patchFilterUrl({
                actorId: next.actorId?.trim() || null,
                traceId: next.traceId?.trim() || null,
                objectType: next.objectType?.trim() || null,
                objectId: next.objectId?.trim() || null,
            })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [debouncedFilters])

    return {
        searchInput,
        searchInputRef,
        setSearchInput,
        debouncedFilters,
        setDebouncedFilters,
    }
}

export { useAccessListControls }
