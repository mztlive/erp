"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"

/** 列表筛选 / 分页写入 URL（刷新、后退、分享一致）。 */
export function useListUrl() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const patchUrl = React.useCallback(
        (patch: Record<string, string | null>) => {
            const next = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "") next.delete(key)
                else next.set(key, value)
            }
            const qs = next.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    const pageParamRaw = Number(searchParams.get("page"))
    const pageParamIndex =
        Number.isFinite(pageParamRaw) && pageParamRaw > 0
            ? Math.max(0, Math.floor(pageParamRaw) - 1)
            : 0

    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: pageParamIndex,
        pageSize: 20,
    })

    const resetPagination = React.useCallback(() => {
        setPagination((current) =>
            current.pageIndex === 0 ? current : { ...current, pageIndex: 0 },
        )
    }, [])

    React.useEffect(() => {
        setPagination((current) => ({
            ...current,
            pageIndex: pageParamIndex,
        }))
    }, [pageParamIndex])

    const changePagination = React.useCallback(
        (next: PaginationState) => {
            setPagination(next)
            const page = next.pageIndex + 1
            patchUrl({ page: page > 1 ? String(page) : null })
        },
        [patchUrl],
    )

    const q = searchParams.get("q") ?? ""

    return {
        searchParams,
        patchUrl,
        q,
        pagination,
        setPagination,
        resetPagination,
        changePagination,
    }
}

/** 搜索草稿：URL 回填时保护正在输入的焦点。 */
export function useSearchDraft(
    q: string,
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const [searchDraft, setSearchDraft] = React.useState(q)

    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(q)
        }
    }, [q, searchInputRef])

    return { searchDraft, setSearchDraft }
}
