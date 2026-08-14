"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    computeContractMetrics,
    contractMetricLabel,
    filterContracts,
    type ContractMetricFilter,
} from "@/features/contracts/lib/filter-contracts"
import { sortRows } from "@/features/contracts/lib/contract-list-sort"
import {
    contractsUrlCodec,
    type ContractsUrlState,
} from "@/features/contracts/lib/contracts-url-state"
import type { ContractListRow } from "@/features/contracts/types"

/**
 * 合同列表 URL-first 状态：筛选/分页/排序全部写 URL，浏览器后退与刷新一致。
 * 派生筛选、排序、分页与指标；搜索走 300ms 防抖 + Enter 兜底。
 */
export function useContractsList(
    rows: readonly ContractListRow[] | undefined,
) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const allRows = React.useMemo(() => rows ?? [], [rows])

    const url = React.useMemo(
        () => contractsUrlCodec.parse(searchParams),
        [searchParams],
    )
    const { q, metric, page, pageSize, sort, dir, customerId } = url

    const [searchDraft, setSearchDraft] = React.useState(q ?? "")

    const pushUrl = React.useCallback(
        (patch: Partial<ContractsUrlState>) => {
            const next = { ...url, ...patch }
            router.replace(`${pathname}${contractsUrlCodec.build(next)}`, {
                scroll: false,
            })
        },
        [pathname, router, url],
    )

    React.useEffect(() => {
        setSearchDraft(q ?? "")
    }, [q])

    // 防抖即时搜索（300ms）+ Enter 兜底；写 URL 并回第 1 页。
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchDraft.trim() === (q ?? "")) return
            pushUrl({ q: searchDraft.trim() || undefined, page: 1 })
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- pushUrl 以当前 URL 快照为准
    }, [searchDraft])

    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            if (
                target &&
                (target.tagName === "INPUT" ||
                    target.tagName === "TEXTAREA" ||
                    target.tagName === "SELECT" ||
                    target.isContentEditable)
            ) {
                return
            }
            if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
                event.preventDefault()
                document
                    .querySelector<HTMLInputElement>(
                        '[data-slot="contracts-search"]',
                    )
                    ?.focus()
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [])

    const filtered = React.useMemo(() => {
        let rowsFiltered = filterContracts(allRows, {
            search: q ?? "",
            metricKey: metric,
            statusFilter: "all",
        })
        if (customerId) {
            rowsFiltered = rowsFiltered.filter(
                (r) => r.customer.customerId === customerId,
            )
        }
        return rowsFiltered
    }, [allRows, customerId, metric, q])

    const sorting = React.useMemo<SortingState>(
        () => (sort ? [{ id: sort, desc: dir === "desc" }] : []),
        [dir, sort],
    )

    const sorted = React.useMemo(
        () => sortRows(filtered, sorting),
        [filtered, sorting],
    )

    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: Math.max(0, page - 1), pageSize }),
        [page, pageSize],
    )

    const pageRows = React.useMemo(() => {
        const start = pagination.pageIndex * pagination.pageSize
        return sorted.slice(start, start + pagination.pageSize)
    }, [pagination.pageIndex, pagination.pageSize, sorted])

    const metrics = React.useMemo(
        () => computeContractMetrics(allRows),
        [allRows],
    )

    /** 客户锁定来自 URL customerId：界面给出可移除 chip 与清除入口。 */
    const lockedCustomer = React.useMemo(() => {
        if (!customerId) return null
        return (
            allRows.find((r) => r.customer.customerId === customerId)
                ?.customer ?? null
        )
    }, [allRows, customerId])

    const filterSnapshotLabel = React.useMemo(() => {
        const parts = [
            `指标=${contractMetricLabel(metric)}`,
            (q ?? "").trim() ? `搜索=${(q ?? "").trim()}` : "搜索=空",
            lockedCustomer ? `客户=${lockedCustomer.displayName}` : null,
        ].filter(Boolean)
        return parts.join(" · ")
    }, [lockedCustomer, metric, q])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            pushUrl({ page: next.pageIndex + 1, pageSize: next.pageSize })
        },
        [pushUrl],
    )

    const handleSearchCommit = React.useCallback(
        (value: string) => {
            setSearchDraft(value)
            pushUrl({ q: value.trim() || undefined, page: 1 })
        },
        [pushUrl],
    )

    const handleMetricChange = React.useCallback(
        (next: ContractMetricFilter) => {
            pushUrl({ metric: next, page: 1 })
        },
        [pushUrl],
    )

    const handleSortingChange = React.useCallback(
        (next: SortingState) => {
            const head = next[0]
            pushUrl({
                sort: head?.id,
                dir: head ? (head.desc ? "desc" : "asc") : undefined,
                page: 1,
            })
        },
        [pushUrl],
    )

    const handleClearCustomerLock = React.useCallback(() => {
        pushUrl({ customerId: undefined })
    }, [pushUrl])

    /** P4：清 q + 全部筛选参数（含 customerId 锁定）+ 分页回 1；保留排序。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        pushUrl({
            q: undefined,
            metric: "all",
            customerId: undefined,
            page: 1,
        })
    }, [pushUrl])

    const isFiltered =
        (q ?? "").trim() !== "" || metric !== "all" || Boolean(customerId)

    return {
        url,
        q,
        metric,
        page,
        pageSize,
        sort,
        dir,
        customerId,
        searchDraft,
        setSearchDraft,
        filtered,
        sorting,
        sorted,
        pagination,
        pageRows,
        metrics,
        lockedCustomer,
        filterSnapshotLabel,
        isFiltered,
        handlePaginationChange,
        handleSearchCommit,
        handleMetricChange,
        handleSortingChange,
        handleClearCustomerLock,
        clearAllFilters,
    }
}
