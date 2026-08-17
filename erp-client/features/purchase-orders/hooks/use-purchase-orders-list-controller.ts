"use client"

import * as React from "react"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    useCreateFromBasisMutation,
    useCreationBasesQuery,
    usePurchaseOrderExportDataQuery,
    usePurchaseOrdersQuery,
} from "@/features/purchase-orders/hooks/queries"
import { usePurchaseOrdersListKeyboard } from "@/features/purchase-orders/hooks/use-purchase-orders-list-keyboard"
import { usePurchaseOrdersListUrl } from "@/features/purchase-orders/hooks/use-purchase-orders-list-url"
import { buildPurchaseOrdersCsv } from "@/features/purchase-orders/lib/purchase-orders-list-helpers"

export type PurchaseOrdersActionResult = {
    status: "succeeded" | "failed" | "unknown"
    title: string
    description: string
    reference?: string
}

/**
 * 采购单列表页控制器：URL 状态、查询接线、搜索防抖、键盘导航、
 * 建单弹框与导出/建单动作的全部状态逻辑。
 */
export function usePurchaseOrdersListController() {
    const {
        router,
        url,
        pushUrl,
        listReturnHref,
        sortBy,
        sortDir,
        effectiveMetric,
        listQueryInput,
        search,
        statusFilter,
        metricKey,
        basisFromUrl,
    } = usePurchaseOrdersListUrl()

    const [createOpen, setCreateOpen] = React.useState(false)

    const listQuery = usePurchaseOrdersQuery(listQueryInput)
    const exportQuery = usePurchaseOrderExportDataQuery(listQueryInput)
    const basesQuery = useCreationBasesQuery({
        enabled: createOpen || Boolean(basisFromUrl),
    })
    const createMutation = useCreateFromBasisMutation()

    const pageRows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data],
    )
    const total = listQuery.data?.total ?? 0

    const [searchDraft, setSearchDraft] = React.useState(search)
    const [focusedIndex, setFocusedIndex] = React.useState(0)
    const [selectedBasisId, setSelectedBasisId] = React.useState<string>("")
    const [actionResult, setActionResult] =
        React.useState<PurchaseOrdersActionResult | null>(null)

    const rowRefs = React.useRef<Map<string, HTMLElement>>(new Map())

    const pagination = React.useMemo<PaginationState>(
        () => ({
            pageIndex: Math.max(0, url.page - 1),
            pageSize: url.pageSize,
        }),
        [url.page, url.pageSize],
    )

    const sorting = React.useMemo<SortingState>(
        () =>
            sortBy && sortDir ? [{ id: sortBy, desc: sortDir === "desc" }] : [],
        [sortBy, sortDir],
    )

    // P4：清除=清搜索/状态/指标筛选并回第 1 页，保留排序与视图参数；
    // 空态与工具栏常驻清除共用同一函数（D19）。
    const hasActiveFilters =
        Boolean(url.q) || statusFilter !== "all" || effectiveMetric !== "all"
    const clearFilters = React.useCallback(() => {
        pushUrl({ q: undefined, status: "all", metric: "all", page: 1 })
    }, [pushUrl])

    React.useEffect(() => {
        setSearchDraft(search)
    }, [search])

    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchDraft.trim() === (url.q ?? "")) return
            pushUrl({ q: searchDraft.trim() || undefined, page: 1 })
        }, 300)
        return () => globalThis.clearTimeout(handle)
    }, [pushUrl, searchDraft, url.q])

    React.useEffect(() => {
        setFocusedIndex(0)
    }, [metricKey, pageRows.length, search, statusFilter])

    React.useEffect(() => {
        const data = listQuery.data
        if (!data || data.page === url.page) return
        pushUrl({ page: data.page })
    }, [listQuery.data, pushUrl, url.page])

    // 键盘导航：仅列表可见且建单弹层未打开时生效；焦点行滚动到可视区。
    React.useEffect(() => {
        const focusedRow = pageRows[focusedIndex]
        if (!focusedRow) return
        rowRefs.current.get(focusedRow.purchaseOrderId)?.scrollIntoView({
            block: "nearest",
        })
    }, [focusedIndex, pageRows])

    const openDetail = React.useCallback(
        (purchaseOrderId: string) => {
            router.push(`/procurement/orders/${purchaseOrderId}`)
        },
        [router],
    )

    usePurchaseOrdersListKeyboard({
        pageRows,
        focusedIndex,
        createOpen,
        onFocusIndex: setFocusedIndex,
        onOpenDetail: openDetail,
    })

    const exportCsv = React.useCallback(async () => {
        const result = await exportQuery.refetch()
        const rows = result.data ?? []
        if (rows.length === 0) return
        const csv = buildPurchaseOrdersCsv(rows)
        const objectUrl = URL.createObjectURL(
            new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" }),
        )
        const anchor = document.createElement("a")
        anchor.href = objectUrl
        anchor.download = "采购单列表.csv"
        anchor.click()
        URL.revokeObjectURL(objectUrl)
        setActionResult({
            status: "succeeded",
            title: "导出已生成",
            description: `已下载当前筛选 ${rows.length} 条。`,
            reference: `EXPORT-${rows.length}`,
        })
    }, [exportQuery])

    const openBases = React.useMemo(
        () => basesQuery.data?.filter((b) => !b.consumed) ?? [],
        [basesQuery.data],
    )

    React.useEffect(() => {
        if (!basisFromUrl) return
        // W07/W05 携带创建依据：打开建单 Dialog，不要求 work_item
        setSelectedBasisId(basisFromUrl)
        setCreateOpen(true)
    }, [basisFromUrl])

    React.useEffect(() => {
        if (!createOpen || selectedBasisId || basisFromUrl) return
        const first = openBases[0]?.basisId
        if (first) setSelectedBasisId(first)
    }, [basisFromUrl, createOpen, openBases, selectedBasisId])

    const handleCreate = async () => {
        if (!selectedBasisId) return
        const basis = openBases.find((b) => b.basisId === selectedBasisId)
        const result = await createMutation.mutateAsync({
            basisId: selectedBasisId,
            idempotencyKey: `create-basis-${selectedBasisId}-${Date.now()}`,
        })
        if (result.status === "succeeded") {
            setCreateOpen(false)
            setActionResult({
                status: "succeeded",
                title: "已创建采购草稿",
                description: `${result.data.draftLabel} · 已使用采购二次确认创建依据（销售单 ${basis?.salesOrderNo ?? "—"} · ${basis?.supplierName ?? "—"}）。`,
                reference: result.reference,
            })
            router.push(
                `/procurement/orders/${result.data.purchaseOrderId}?mode=edit`,
            )
        } else if (result.status === "failed") {
            setActionResult({
                status: "failed",
                title: "建单失败",
                description: result.message,
            })
        }
    }

    const openCreateDialog = React.useCallback(() => {
        setSelectedBasisId(openBases[0]?.basisId ?? "")
        setCreateOpen(true)
    }, [openBases])

    return {
        // 查询与派生数据
        listQuery,
        exportQuery,
        basesQuery,
        createMutation,
        pageRows,
        total,
        pagination,
        sorting,
        // URL 状态
        url,
        pushUrl,
        listReturnHref,
        search,
        statusFilter,
        metricKey,
        effectiveMetric,
        basisFromUrl,
        hasActiveFilters,
        clearFilters,
        // 交互状态
        searchDraft,
        setSearchDraft,
        focusedIndex,
        setFocusedIndex,
        createOpen,
        setCreateOpen,
        selectedBasisId,
        setSelectedBasisId,
        actionResult,
        setActionResult,
        rowRefs,
        // 动作
        exportCsv,
        handleCreate,
        openBases,
        openCreateDialog,
        openDetail,
    }
}
