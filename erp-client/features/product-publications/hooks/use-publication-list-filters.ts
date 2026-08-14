"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"

import { parseMetric } from "@/features/product-publications/lib/parse-metric"

/**
 * 列表页筛选 / 分页 / 搜索草稿状态。
 * URL 是筛选参数的唯一事实来源；pageSize 与搜索草稿仅存本地。
 */
export function usePublicationListFilters() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const qParam = searchParams.get("q") ?? ""
    const skuId = searchParams.get("skuId") ?? undefined
    const supplierOfferingRevisionId =
        searchParams.get("supplierOfferingRevisionId") ?? undefined
    const mallId = searchParams.get("mall") ?? undefined
    const publicationStatus = searchParams.get("publicationStatus") ?? "all"
    const deliveryStatus = searchParams.get("deliveryStatus") ?? "all"
    const metric = parseMetric(searchParams.get("metric"))

    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const pageFromUrl = Math.max(
        1,
        Number(searchParams.get("page") ?? "1") || 1,
    )
    // 本地记录：pageSize 仅影响查询页大小（沿用页面默认 20，不写 URL）
    const [pageSize, setPageSize] = React.useState(20)

    React.useEffect(() => {
        // URL is source of truth when filters/metrics change outside the search box；
        // 用户正在输入（焦点在搜索框）时不得用 URL 旧值覆盖草稿
        const el = searchInputRef.current
        if (el && document.activeElement === el) return
        setSearchInput(qParam)
    }, [qParam])

    const replaceParams = React.useCallback(
        (patch: Record<string, string | undefined>) => {
            const sp = new URLSearchParams(searchParams.toString())
            for (const [k, v] of Object.entries(patch)) {
                if (!v || v === "all") sp.delete(k)
                else sp.set(k, v)
            }
            sp.delete("page")
            const qs = sp.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname)
        },
        [pathname, router, searchParams],
    )

    // P3：搜索 300ms 防抖自动写 URL（replace），Enter 兜底，`/` 聚焦
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput.trim() === qParam) return
            replaceParams({ q: searchInput.trim() || undefined })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- replaceParams 以当前 URL 快照为准
    }, [searchInput])

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

    const commitSearch = React.useCallback(() => {
        replaceParams({ q: searchInput.trim() || undefined })
    }, [replaceParams, searchInput])

    // P4：清搜索词 + 全部筛选参数 + page 回 1；保留排序/视图/导航上下文等（本页无此类参数，语义等价全清）
    const clearFilters = React.useCallback(() => {
        setSearchInput("")
        const sp = new URLSearchParams(searchParams.toString())
        for (const k of [
            "q",
            "skuId",
            "supplierOfferingRevisionId",
            "mall",
            "publicationStatus",
            "deliveryStatus",
            "metric",
            "page",
        ]) {
            sp.delete(k)
        }
        const qs = sp.toString()
        router.replace(qs ? `${pathname}?${qs}` : pathname)
    }, [pathname, router, searchParams])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            setPageSize(next.pageSize)
            const sp = new URLSearchParams(searchParams.toString())
            if (next.pageIndex <= 0) sp.delete("page")
            else sp.set("page", String(next.pageIndex + 1))
            const qs = sp.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname)
        },
        [pathname, router, searchParams],
    )

    const hasActiveFilters = Boolean(
        qParam ||
            mallId ||
            skuId ||
            supplierOfferingRevisionId ||
            publicationStatus !== "all" ||
            deliveryStatus !== "all" ||
            metric !== "all",
    )

    return {
        qParam,
        skuId,
        supplierOfferingRevisionId,
        mallId,
        publicationStatus,
        deliveryStatus,
        metric,
        page: pageFromUrl,
        pageSize,
        searchInput,
        setSearchInput,
        searchInputRef,
        replaceParams,
        commitSearch,
        clearFilters,
        handlePaginationChange,
        hasActiveFilters,
    }
}
