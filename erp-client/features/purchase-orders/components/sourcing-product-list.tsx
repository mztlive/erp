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
import { cn } from "@/lib/utils"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import type { SourcingSalesOrder } from "../lib/sourcing/types"
import type {
    SourcingBatchSelectionProps,
    SourcingParticipationProps,
} from "./create-sourcing/types"
import { SourcingProductExpandedSources } from "./sourcing-product-expanded-sources"
import { sourcingProductColumns } from "./sourcing-product-table-columns"

/**
 * 商品级供给分配表：首屏 7 列在主力 1200–1440px 视口下零横向滚动，
 * 单源行可直接行内比价换源、选目标仓、改数量与交期；左侧勾选框严格用于批量选区。
 */
export function SourcingProductList({
    form,
    order,
    rows,
    toolbarLeft,
    toolbarActions,
    selectionBar,
    onSelect,
    checkedProductIds,
    onToggleChecked,
    onSetParticipation,
    onSyncWarehouseToOthers,
    pagination,
    onPaginationChange,
    activeId,
    scrollable = true,
}: SourcingBatchSelectionProps &
    SourcingParticipationProps & {
        form: PurchaseOrderCreateFormApi
        order: SourcingSalesOrder
        rows: SourcingEditorRow[]
        toolbarLeft?: ReactNode
        toolbarActions?: ReactNode
        selectionBar?: ReactNode
        onSelect: (id: string, section?: "compare" | "addresses") => void
        onSyncWarehouseToOthers: (
            warehouseId: string,
            warehouseName: string,
            excludeSalesOrderLineId: string,
        ) => void
        pagination: PaginationState
        onPaginationChange: (value: PaginationState) => void
        activeId?: string | null
        scrollable?: boolean
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
    const pageRowIds = useMemo(
        () => pageRows.map((row) => row.product.salesOrderLineId),
        [pageRows],
    )

    useEffect(() => {
        surface.current
            ?.querySelector('[data-slot="table-container"]')
            ?.scrollTo({ top: 0 })
    }, [pageIndex, pagination.pageSize])

    const columns = useMemo(
        () =>
            sourcingProductColumns({
                form,
                order,
                pageRowIds,
                checkedProductIds,
                onToggleChecked,
                onSetParticipation,
                expandedIds,
                onEdit: onSelect,
                onSyncWarehouseToOthers,
                onToggleExpanded: (id) =>
                    setExpandedIds((current) => {
                        const next = new Set(current)
                        if (next.has(id)) next.delete(id)
                        else next.add(id)
                        return next
                    }),
            }),
        [
            form,
            order,
            pageRowIds,
            checkedProductIds,
            onToggleChecked,
            onSetParticipation,
            expandedIds,
            onSelect,
            onSyncWarehouseToOthers,
        ],
    )

    const needsPagination = rows.length > pagination.pageSize || pageIndex > 0

    const hasSelection = Boolean(selectionBar)

    return (
        <TableToolbarScope>
            {/* 单行上下文工具栏：未勾选展示筛选与快捷匹配；勾选后原地切换为批量操作条，避免两排按钮堆叠打架 */}
            <TableToolbar
                className={cn(
                    "min-h-12 shrink-0 border-b px-4 py-2 transition-colors",
                    hasSelection
                        ? "border-info-border/70 bg-info-soft/50 [&_[data-slot=table-toolbar-summary]]:flex-1 [&>div:has(>[data-slot=table-toolbar-column-settings])]:hidden"
                        : "border-border/60 bg-card",
                )}
                actions={hasSelection ? undefined : toolbarActions}
                isFrameToolbar
            >
                {hasSelection ? selectionBar : toolbarLeft}
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
                    caption="供给分配商品表：左侧复选框用于批量操作，右侧开关控制是否纳入本次分配，单源行支持直接修改来源、目标仓、数量与交期。"
                    manualPagination={false}
                    pagination={{ ...pagination, pageIndex }}
                    onPaginationChange={onPaginationChange}
                    showPagination={needsPagination}
                    defaultColumnVisibility={{
                        "sales-prices": false,
                        tax: false,
                        payment: false,
                    }}
                    defaultColumnPinning={{
                        left: ["batchSelect", "product"],
                        right: ["statusActions"],
                    }}
                    defaultColumnSizing={{
                        product: 205,
                        source: 255,
                        quantity: 148,
                        pricing: 168,
                        delivery: 158,
                        "sales-prices": 220,
                        tax: 88,
                        payment: 110,
                        statusActions: 196,
                    }}
                    highlightedRowId={activeId ?? undefined}
                    renderExpandedRow={(row) =>
                        expandedIds.has(row.product.salesOrderLineId) &&
                        row.allocations.length > 1 ? (
                            <SourcingProductExpandedSources row={row} />
                        ) : null
                    }
                    density="compact"
                    scrollable={scrollable}
                    showToolbarSummary={false}
                    className="gap-0 [--spacing-table-column-selection:3rem]"
                    emptyTitle="没有符合条件的商品"
                    emptyDescription="请调整搜索条件或切换到全部商品。"
                />
            </div>
        </TableToolbarScope>
    )
}
