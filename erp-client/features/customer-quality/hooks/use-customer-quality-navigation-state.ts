"use client"

import * as React from "react"
import type { PaginationState, SortingState } from "@tanstack/react-table"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { buildReturnTo } from "../lib/links"

export type CustomerQualityPatch = (
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean; scroll?: boolean },
) => void

export function useCustomerQualityNavigationState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const sort = searchParams.get("sort") ?? "salesGrossAmount:desc"
    const pageFromUrl = Math.max(
        1,
        Number(searchParams.get("page") ?? "1") || 1,
    )
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: pageFromUrl - 1,
        pageSize: 20,
    })

    function patchUrl(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean; scroll?: boolean },
    ) {
        // P1/P2/P6：筛选变更恒 replace 且不跳动（scroll:false）；page 入 URL，任何筛选变更回第 1 页
        patchSearchParams(
            { router, pathname, searchParams },
            { ...patch, page: null },
            { replace: true, scroll: false, ...options },
        )
        setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
    }

    const resetPage = React.useCallback(() => {
        setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
    }, [])

    // P6：分页写 URL（page），URL 回读同步本地分页
    React.useEffect(() => {
        setPagination((p) =>
            p.pageIndex === pageFromUrl - 1
                ? p
                : { ...p, pageIndex: pageFromUrl - 1 },
        )
    }, [pageFromUrl])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            setPagination(next)
            const sp = new URLSearchParams(searchParams.toString())
            if (next.pageIndex <= 0) sp.delete("page")
            else sp.set("page", String(next.pageIndex + 1))
            const qs = sp.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname)
        },
        [pathname, router, searchParams],
    )

    const returnTo = React.useMemo(
        () =>
            buildReturnTo(
                pathname,
                new URLSearchParams(searchParams.toString()),
            ),
        [pathname, searchParams],
    )

    // 排序状态与顶部「排序」下拉同源（URL sort）；表头点击回写同一参数。
    const tableSorting = React.useMemo<SortingState>(() => {
        const [id, dir] = sort.split(":")
        return [{ id, desc: dir === "desc" }]
    }, [sort])

    const handleTableSortingChange = React.useCallback(
        (next: SortingState) => {
            const nextSort = next[0]
            patchUrl({
                sort: nextSort
                    ? `${nextSort.id}:${nextSort.desc ? "desc" : "asc"}`
                    : "salesGrossAmount:desc",
            })
            resetPage()
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [sort],
    )

    return {
        sort,
        pagination,
        setPagination,
        patchUrl,
        resetPage,
        handlePaginationChange,
        tableSorting,
        handleTableSortingChange,
        returnTo,
    }
}
