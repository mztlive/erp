"use client"

import { useEffect, useMemo, useState, type ReactNode } from "react"
import { SearchIcon, TriangleAlertIcon, XIcon } from "lucide-react"
import type { PaginationState } from "@/components/business/data-table"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { buildSourcingEditorRows } from "../lib/sourcing/editor-rows"
import { summarizeRowPricing } from "../lib/sourcing/pricing"
import type { SourcingLineInput } from "../lib/sourcing/types"
import type {
    SourcingBatchSelectionProps,
    SourcingEditorProps,
    SourcingParticipationProps,
} from "./create-sourcing/types"
import { SourcingProductAdjustDialog } from "./sourcing-product-adjust-dialog"
import { SourcingProductList } from "./sourcing-product-list"

type StatusFilter = "all" | "pending" | "included" | "excluded" | "inverted"

export type SourcingFocusRequest = Readonly<{
    salesOrderLineId: string
    openDialog?: boolean
    section?: "compare" | "addresses"
    token: number
}>

/**
 * 统一供给分配主工作区（工作台与独立建单页共用）：
 * 单行工具条整合筛选/搜索/批量辅助，表内直接高频编辑，复杂多源/多地址走全宽精调弹窗。
 */
export function PurchaseOrderCreateSourcingWorkspace({
    toolbarActions,
    selectionBar,
    checkedProductIds,
    onToggleChecked,
    onClearChecked,
    onSetParticipation,
    onSyncWarehouseToOthers,
    onRestoreProductLines,
    onVisibleIdsChange,
    focusRequest,
    scrollable = true,
    ...props
}: SourcingEditorProps &
    SourcingBatchSelectionProps &
    SourcingParticipationProps & {
        toolbarActions: ReactNode
        selectionBar?: ReactNode
        onSyncWarehouseToOthers: (
            warehouseId: string,
            warehouseName: string,
            excludeSalesOrderLineId: string,
        ) => void
        onRestoreProductLines: (
            salesOrderLineId: string,
            snapshot: SourcingLineInput[],
        ) => void
        onVisibleIdsChange?: (ids: readonly string[]) => void
        focusRequest?: SourcingFocusRequest | null
        scrollable?: boolean
    }) {
    const { form, order } = props
    const [query, setQuery] = useState("")
    const [status, setStatus] = useState<StatusFilter>("all")
    const [pagination, setPagination] = useState<PaginationState>({
        pageIndex: 0,
        pageSize: 50,
    })
    const [editingState, setEditingState] = useState<{
        id: string
        section?: "compare" | "addresses"
    } | null>(null)
    const [highlightedId, setHighlightedId] = useState<string | null>(null)
    const [returnFocusId, setReturnFocusId] = useState<string>()
    const [navigationIds, setNavigationIds] = useState<string[]>([])

    const rows = useMemo(
        () => buildSourcingEditorRows(order, form.state.values.lines),
        [order, form.state.values.lines],
    )

    const invertedIds = useMemo(
        () =>
            new Set(
                rows
                    .filter(
                        (row) =>
                            (row.selected || row.partiallySelected) &&
                            summarizeRowPricing(row).hasInvertedSource,
                    )
                    .map((row) => row.product.salesOrderLineId),
            ),
        [rows],
    )

    const counts = useMemo(
        () => ({
            all: rows.length,
            pending: rows.filter((row) => row.needsAttention).length,
            included: rows.filter(
                (row) => row.selected || row.partiallySelected,
            ).length,
            excluded: rows.filter(
                (row) => !row.selected && !row.partiallySelected,
            ).length,
            inverted: invertedIds.size,
        }),
        [invertedIds, rows],
    )

    const visible = useMemo(() => {
        const keyword = query.trim().toLocaleLowerCase()
        return rows.filter((row) => {
            if (status === "pending" && !row.needsAttention) return false
            if (
                status === "included" &&
                !row.selected &&
                !row.partiallySelected
            )
                return false
            if (
                status === "excluded" &&
                (row.selected || row.partiallySelected)
            )
                return false
            if (
                status === "inverted" &&
                !invertedIds.has(row.product.salesOrderLineId)
            )
                return false
            if (!keyword) return true
            const supplierNames = row.allocations
                .map(({ line }) => {
                    const opt = findSourcingOption(row.product, line.basisId)
                    return `${opt?.supplierName ?? ""} ${opt?.warehouseName ?? ""}`
                })
                .join(" ")
            return `${row.product.itemName} ${row.product.itemSku ?? ""} ${supplierNames}`
                .toLocaleLowerCase()
                .includes(keyword)
        })
    }, [invertedIds, query, rows, status])

    useEffect(() => {
        onVisibleIdsChange?.(visible.map((row) => row.product.salesOrderLineId))
    }, [onVisibleIdsChange, visible])

    // 响应底栏「点击定位待完善」或预览弹窗「回改」的外部定位请求
    useEffect(() => {
        if (!focusRequest) return
        const targetId = focusRequest.salesOrderLineId
        setQuery("")
        setStatus("all")
        setHighlightedId(targetId)
        const targetIndex = rows.findIndex(
            (row) => row.product.salesOrderLineId === targetId,
        )
        if (targetIndex >= 0) {
            setPagination((current) => ({
                ...current,
                pageIndex: Math.floor(targetIndex / current.pageSize),
            }))
        }
        if (focusRequest.openDialog) {
            setNavigationIds(rows.map((row) => row.product.salesOrderLineId))
            setEditingState({ id: targetId, section: focusRequest.section })
        }
    }, [focusRequest, rows])

    const changeQuery = (value: string) => {
        setQuery(value)
        setPagination((current) => ({ ...current, pageIndex: 0 }))
    }

    const openEditor = (id: string, section?: "compare" | "addresses") => {
        setReturnFocusId(`sourcing-list-${toAutomationIdSegment(id)}-edit`)
        setNavigationIds(visible.map((row) => row.product.salesOrderLineId))
        setHighlightedId(id)
        setEditingState({ id, section })
    }

    const showParticipationFilters =
        counts.excluded > 0 || status === "included" || status === "excluded"

    const filterItems: readonly {
        value: StatusFilter
        label: string
        count: number
        tone?: "warning" | "destructive"
    }[] = [
        { value: "all", label: "全部", count: counts.all },
        ...(counts.pending > 0 || status === "pending"
            ? [
                  {
                      value: "pending" as const,
                      label: "待调整",
                      count: counts.pending,
                      tone: "warning" as const,
                  },
              ]
            : []),
        ...(counts.inverted > 0 || status === "inverted"
            ? [
                  {
                      value: "inverted" as const,
                      label: "价格倒挂",
                      count: counts.inverted,
                      tone: "destructive" as const,
                  },
              ]
            : []),
        ...(showParticipationFilters
            ? [
                  {
                      value: "included" as const,
                      label: "本次分配",
                      count: counts.included,
                  },
                  {
                      value: "excluded" as const,
                      label: "暂不分配",
                      count: counts.excluded,
                  },
              ]
            : []),
    ]

    const toolbarLeft = (
        <div className="flex flex-wrap items-center gap-2">
            <div
                role="group"
                aria-label="商品方案状态"
                className="flex flex-wrap items-center gap-1"
            >
                {filterItems.map((item) => {
                    const active = status === item.value
                    return (
                        <Button
                            key={item.value}
                            id={`sourcing-products-filter-${item.value}`}
                            type="button"
                            size="xs"
                            variant={active ? "secondary" : "ghost"}
                            aria-pressed={active}
                            className={
                                item.tone === "destructive"
                                    ? "text-destructive hover:text-destructive"
                                    : item.tone === "warning"
                                      ? "text-warning-soft-foreground hover:text-warning-soft-foreground"
                                      : undefined
                            }
                            onClick={() => {
                                setStatus(item.value)
                                setPagination((current) => ({
                                    ...current,
                                    pageIndex: 0,
                                }))
                            }}
                        >
                            {item.tone === "destructive" ? (
                                <TriangleAlertIcon aria-hidden="true" />
                            ) : null}
                            {item.label}{" "}
                            <span
                                className={
                                    item.tone
                                        ? "num font-medium"
                                        : "num text-muted-foreground"
                                }
                            >
                                {item.count}
                            </span>
                        </Button>
                    )
                })}
            </div>

            <div className="relative w-48 sm:w-56">
                <SearchIcon
                    aria-hidden="true"
                    className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                />
                <Input
                    id="sourcing-products-search"
                    size="sm"
                    aria-label="搜索商品名称、规格或供应商"
                    placeholder="搜商品、规格或供应商"
                    className="pr-7 pl-8"
                    value={query}
                    onChange={(event) => changeQuery(event.target.value)}
                />
                {query ? (
                    <Button
                        id="sourcing-products-search-clear"
                        type="button"
                        size="icon-xs"
                        variant="ghost"
                        aria-label="清除商品搜索"
                        className="absolute top-1/2 right-0.5 -translate-y-1/2"
                        onClick={() => changeQuery("")}
                    >
                        <XIcon />
                    </Button>
                ) : null}
            </div>
        </div>
    )

    return (
        <section
            aria-label="供给商品列表"
            className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
        >
            <SourcingProductList
                form={form}
                order={order}
                rows={visible}
                toolbarLeft={toolbarLeft}
                toolbarActions={toolbarActions}
                selectionBar={selectionBar}
                checkedProductIds={checkedProductIds}
                onToggleChecked={onToggleChecked}
                onClearChecked={onClearChecked}
                onSyncWarehouseToOthers={onSyncWarehouseToOthers}
                pagination={pagination}
                onPaginationChange={setPagination}
                activeId={editingState?.id ?? highlightedId}
                scrollable={scrollable}
                onSelect={openEditor}
            />
            <SourcingProductAdjustDialog
                {...props}
                productId={editingState?.id ?? null}
                initialFocusSection={editingState?.section ?? null}
                onClose={() => setEditingState(null)}
                onSetParticipation={onSetParticipation}
                onRestoreProductLines={onRestoreProductLines}
                navigationIds={navigationIds}
                onNavigate={(nextId) => {
                    setHighlightedId(nextId)
                    setEditingState({ id: nextId })
                }}
                returnFocusId={returnFocusId}
            />
        </section>
    )
}
