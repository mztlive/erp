"use client"

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import {
    DataTable,
    type PaginationState,
} from "@/components/business/data-table"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { NativeCheckbox } from "@/components/ui/checkbox"
import type { SourcingBatchSelectionProps } from "./create-sourcing/types"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { sourcingProductColumns } from "./sourcing-product-table-columns"
import { SourcingProductExpandedSources } from "./sourcing-product-expanded-sources"

/** 商品级表格只分页视图，分配选择与展开状态按稳定商品 ID 保留。 */
export function SourcingProductList({
    rows,
    toolbar,
    onSelect,
    selectedProductIds,
    onToggleProducts,
    pagination,
    onPaginationChange,
    totalCount,
    activeId,
}: SourcingBatchSelectionProps & {
    rows: SourcingEditorRow[]
    toolbar: ReactNode
    onSelect: (id: string) => void
    pagination: PaginationState
    onPaginationChange: (value: PaginationState) => void
    totalCount: number
    activeId?: string | null
}) {
    const [expandedIds, setExpandedIds] = useState<ReadonlySet<string>>(
        new Set(),
    )
    const surface = useRef<HTMLDivElement>(null)
    const pageIndex = Math.min(
        pagination.pageIndex,
        Math.max(0, Math.ceil(rows.length / pagination.pageSize) - 1),
    )
    const pageRows = rows.slice(
        pageIndex * pagination.pageSize,
        (pageIndex + 1) * pagination.pageSize,
    )
    const pageSelected = pageRows.filter((row) =>
        selectedProductIds.has(row.product.salesOrderLineId),
    ).length
    useEffect(() => {
        surface.current
            ?.querySelector('[data-slot="table-container"]')
            ?.scrollTo({ top: 0 })
    }, [pageIndex, pagination.pageSize])
    const columns = useMemo(
        () =>
            sourcingProductColumns({
                selectedProductIds,
                onToggleProducts,
                expandedIds,
                onEdit: onSelect,
                onToggleExpanded: (id) =>
                    setExpandedIds((current) => {
                        const next = new Set(current)
                        if (next.has(id)) next.delete(id)
                        else next.add(id)
                        return next
                    }),
            }),
        [selectedProductIds, onToggleProducts, expandedIds, onSelect],
    )
    return (
        <TableToolbarScope>
            <TableToolbar className="shrink-0 px-4 py-2" actions={toolbar}>
                <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                    <label
                        htmlFor="sourcing-list-select-all"
                        className="flex cursor-pointer items-center gap-2"
                    >
                        <NativeCheckbox
                            id="sourcing-list-select-all"
                            aria-label="全选当前页参与分配"
                            checked={
                                pageRows.length > 0 &&
                                pageSelected === pageRows.length
                            }
                            indeterminate={
                                pageSelected > 0 &&
                                pageSelected < pageRows.length
                            }
                            disabled={!pageRows.length}
                            onCheckedChange={(checked) =>
                                onToggleProducts(
                                    pageRows.map(
                                        (row) => row.product.salesOrderLineId,
                                    ),
                                    checked,
                                )
                            }
                        />
                        全选当前页 {pageRows.length} 行
                    </label>
                    <span>
                        筛选 {rows.length} / 共 {totalCount} 个商品 · 本次分配{" "}
                        {selectedProductIds.size} 个（全单）
                    </span>
                </div>
            </TableToolbar>
            <div
                ref={surface}
                className="flex min-h-0 flex-1 flex-col"
                data-slot="sourcing-product-list"
            >
                <DataTable
                    id="sourcing-products-table"
                    data={rows}
                    columns={columns}
                    getRowId={(row) => row.product.salesOrderLineId}
                    rowCount={rows.length}
                    rowLabel={(row) => row.product.itemName}
                    caption="供给分配商品表，全选仅作用当前页，分配选择保留至全单预览。"
                    manualPagination={false}
                    pagination={{ ...pagination, pageIndex }}
                    onPaginationChange={onPaginationChange}
                    defaultColumnVisibility={{ tax: false, payment: false }}
                    defaultColumnPinning={{
                        left: ["participation", "product"],
                        right: ["actions"],
                    }}
                    defaultColumnSizing={{
                        product: 200,
                        demand: 110,
                        quantity: 110,
                        supplier: 240,
                        route: 110,
                        price: 110,
                        delivery: 120,
                        status: 140,
                        actions: 76,
                    }}
                    onRowOpen={(row) => onSelect(row.product.salesOrderLineId)}
                    highlightedRowId={activeId ?? undefined}
                    renderExpandedRow={(row) =>
                        expandedIds.has(row.product.salesOrderLineId) &&
                        row.allocations.length > 1 ? (
                            <SourcingProductExpandedSources row={row} />
                        ) : null
                    }
                    density="compact"
                    scrollable
                    className="gap-0"
                    emptyTitle="没有符合条件的商品"
                    emptyDescription="请调整搜索条件或切换到全部商品。"
                />
            </div>
        </TableToolbarScope>
    )
}
