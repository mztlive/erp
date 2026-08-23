"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    useCreateFromBasisMutation,
    useCreationBasesQuery,
    usePurchaseOrderExportDataQuery,
    usePurchaseOrdersQuery,
} from "@/features/purchase-orders/hooks/queries"
import { usePurchaseOrdersListFilters } from "@/features/purchase-orders/hooks/use-purchase-orders-list-filters"
import { usePurchaseOrdersListKeyboard } from "@/features/purchase-orders/hooks/use-purchase-orders-list-keyboard"
import { buildPurchaseOrdersCsv } from "@/features/purchase-orders/lib/purchase-orders-list-helpers"

export type PurchaseOrdersActionResult = {
    status: "succeeded" | "failed" | "unknown"
    title: string
    description: string
    reference?: string
}

/**
 * 采购单列表页控制器：URL 状态、查询接线、筛选状态模型（Applied/Draft/UI）、
 * 键盘导航、建单弹框与导出/建单动作的状态逻辑。
 */
export function usePurchaseOrdersListController() {
    const router = useRouter()
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const filters = usePurchaseOrdersListFilters(searchInputRef)
    const {
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
        salesOrderFromUrl,
        createFromSales,
    } = filters

    const [createOpen, setCreateOpen] = React.useState(false)

    const listQuery = usePurchaseOrdersQuery(listQueryInput)
    const exportQuery = usePurchaseOrderExportDataQuery(listQueryInput)
    const basesQuery = useCreationBasesQuery({
        enabled: createOpen || Boolean(basisFromUrl) || createFromSales,
    })
    const createMutation = useCreateFromBasisMutation()

    const pageRows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data],
    )
    const total = listQuery.data?.total ?? 0

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

    const openBases = React.useMemo(() => {
        const open = basesQuery.data?.filter((b) => !b.consumed) ?? []
        if (!salesOrderFromUrl) return open
        return open.filter((b) => b.salesOrderId === salesOrderFromUrl)
    }, [basesQuery.data, salesOrderFromUrl])

    React.useEffect(() => {
        if (!basisFromUrl && !createFromSales) return
        // 显式建单动作或指定创建依据时打开 Dialog；关系筛选本身只展示列表。
        setCreateOpen(true)
        if (basisFromUrl) setSelectedBasisId(basisFromUrl)
    }, [basisFromUrl, createFromSales])

    React.useEffect(() => {
        if (!createOpen || basisFromUrl) return
        if (openBases.some((basis) => basis.basisId === selectedBasisId)) {
            return
        }
        const first = openBases[0]?.basisId
        setSelectedBasisId(first ?? "")
    }, [basisFromUrl, createOpen, openBases, selectedBasisId])

    const handleCreate = async () => {
        const basis = openBases.find((b) => b.basisId === selectedBasisId)
        if (!basis) return
        const result = await createMutation.mutateAsync({
            basisId: basis.basisId,
            idempotencyKey: `create-basis-${basis.basisId}-${Date.now()}`,
        })
        if (result.status === "succeeded") {
            setCreateOpen(false)
            setActionResult({
                status: "succeeded",
                title: "已创建采购草稿",
                description: `${result.data.draftLabel} · 已绑定审批流程，尚未形成待办。已按采购创建依据带入销售单 ${basis?.salesOrderNo ?? "—"} 与供应商 ${basis?.supplierName ?? "—"}。`,
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
        searchInputRef,
        filters: {
            searchDraft: filters.searchDraft,
            setSearchDraft: filters.setSearchDraft,
            statusDraft: filters.statusDraft,
            setStatusDraft: filters.setStatusDraft,
            panelOpen: filters.panelOpen,
            setPanelOpen: filters.setPanelOpen,
            hasActiveFilters: filters.hasActiveFilters,
            hasStructuredFilters: filters.hasStructuredFilters,
            appliedChips: filters.appliedChips,
            removeFilter: filters.removeFilter,
            applyFilters: filters.applyFilters,
            resetMoreFilters: filters.resetMoreFilters,
            clearAllFilters: filters.clearAllFilters,
        },
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
        salesOrderFromUrl,
        // 交互状态
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
