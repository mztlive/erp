"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"

import {
    parseLatency,
    parseMetric,
    parseRecon,
    parseSource,
} from "@/features/execution-projections/lib/url-state"
import type {
    ExecutionProjectionMetricKey,
    LatencyBand,
    ProjectionSource,
    ReconciliationStatus,
} from "@/features/execution-projections/types"

/** 列表页每页默认条数（URL 无 size 参数时生效；恢复该值时参数从 URL 移除） */
const DEFAULT_PAGE_SIZE = 8

export type ExecutionProjectionUrlState = {
    q: string
    mallId: string
    deliveryStatus: string
    source: ProjectionSource | "all"
    latency: LatencyBand | "all"
    reconciliation: ReconciliationStatus | "all"
    metric: ExecutionProjectionMetricKey | "all"
    projectionId: string | undefined
    revisionId: string | undefined
    page: number
    pageSize: number
    hasActiveFilters: boolean
    replaceParams: (patch: Record<string, string | null | undefined>) => void
    setPageState: (next: PaginationState) => void
    clearFilters: () => void
}

/**
 * W23 列表页 URL 参数状态：解析 searchParams、提供受控回写。
 * URL 参数与界面控件一一对应（AGENTS.md 第 5 节）。
 */
export function useExecutionProjectionUrlState(): ExecutionProjectionUrlState {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const q = searchParams.get("q") ?? ""
    const mallId = searchParams.get("mall") ?? "all"
    const deliveryStatus = searchParams.get("deliveryStatus") ?? "all"
    const source = parseSource(searchParams.get("source"))
    const latency = parseLatency(searchParams.get("latency"))
    const reconciliation = parseRecon(searchParams.get("reconciliation"))
    const metric = parseMetric(searchParams.get("metric"))
    const projectionId = searchParams.get("projectionId") ?? undefined
    const revisionId = searchParams.get("revision") ?? undefined
    const page = Math.max(1, Number(searchParams.get("page") ?? "1") || 1)
    const pageSize = Math.max(
        1,
        Math.min(
            50,
            Number(searchParams.get("size") ?? String(DEFAULT_PAGE_SIZE)) ||
                DEFAULT_PAGE_SIZE,
        ),
    )

    const replaceParams = React.useCallback(
        (patch: Record<string, string | null | undefined>) => {
            const next = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "" || value === "all")
                    next.delete(key)
                else next.set(key, value)
            }
            const qs = next.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    const setPageState = React.useCallback(
        (next: PaginationState) => {
            const sp = new URLSearchParams(searchParams.toString())
            if (next.pageIndex <= 0) sp.delete("page")
            else sp.set("page", String(next.pageIndex + 1))
            if (next.pageSize === DEFAULT_PAGE_SIZE) sp.delete("size")
            else sp.set("size", String(next.pageSize))
            const qs = sp.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    const hasActiveFilters = Boolean(
        q ||
        mallId !== "all" ||
        deliveryStatus !== "all" ||
        source !== "all" ||
        latency !== "all" ||
        reconciliation !== "all" ||
        metric !== "all",
    )

    // P4：清搜索词 + 全部筛选参数 + 分页回 1（保留视图/排序/导航上下文参数）
    const clearFilters = React.useCallback(() => {
        replaceParams({
            q: null,
            mall: null,
            deliveryStatus: null,
            source: null,
            latency: null,
            reconciliation: null,
            metric: null,
            page: null,
        })
    }, [replaceParams])

    return {
        q,
        mallId,
        deliveryStatus,
        source,
        latency,
        reconciliation,
        metric,
        projectionId,
        revisionId,
        page,
        pageSize,
        hasActiveFilters,
        replaceParams,
        setPageState,
        clearFilters,
    }
}
