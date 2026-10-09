"use client"

import type { ReactNode } from "react"
import { QuantityValue } from "@/components/business"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SourcingBatchSelectionProps } from "./create-sourcing/types"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"

export function SourcingProductList({
    rows,
    toolbar,
    activeId,
    onSelect,
    selectedProductIds,
    onToggleProducts,
}: SourcingBatchSelectionProps & {
    toolbar: ReactNode
    rows: SourcingEditorRow[]
    activeId?: string
    onSelect: (id: string) => void
}) {
    const selectedCount = rows.filter((row) =>
        selectedProductIds.has(row.product.salesOrderLineId),
    ).length
    return (
        <TableToolbarScope>
            <TableToolbar className="shrink-0 px-4 py-2" actions={toolbar}>
                <span className="text-xs text-muted-foreground">
                    显示 {rows.length} 行 · 批量已选 {selectedProductIds.size}{" "}
                    行
                </span>
            </TableToolbar>
            <div
                className="min-h-0 flex-1 overflow-auto"
                data-slot="sourcing-product-list"
            >
                <Table data-density="compact">
                    <TableHeader>
                        <TableRow>
                            <TableHead className="w-10">
                                <NativeCheckbox
                                    id="sourcing-list-select-all"
                                    aria-label="选择当前显示的全部商品"
                                    checked={
                                        rows.length > 0 &&
                                        selectedCount === rows.length
                                    }
                                    indeterminate={
                                        selectedCount > 0 &&
                                        selectedCount < rows.length
                                    }
                                    disabled={!rows.length}
                                    onCheckedChange={(checked) =>
                                        onToggleProducts(
                                            rows.map(
                                                (row) =>
                                                    row.product
                                                        .salesOrderLineId,
                                            ),
                                            checked,
                                        )
                                    }
                                />
                            </TableHead>
                            <TableHead>商品</TableHead>
                            <TableHead data-align="end">需求</TableHead>
                            <TableHead data-align="end">分配</TableHead>
                            <TableHead>履约方式</TableHead>
                            <TableHead>状态</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {rows.map((row) => {
                            const id = toAutomationIdSegment(
                                row.product.salesOrderLineId,
                            )
                            return (
                                <TableRow
                                    key={id}
                                    className="h-12"
                                    data-state={
                                        activeId ===
                                        row.product.salesOrderLineId
                                            ? "selected"
                                            : undefined
                                    }
                                >
                                    <TableCell className="py-2">
                                        <NativeCheckbox
                                            id={`sourcing-list-${id}-select`}
                                            aria-label={`批量选择 ${row.product.itemName}`}
                                            checked={selectedProductIds.has(
                                                row.product.salesOrderLineId,
                                            )}
                                            onCheckedChange={(checked) =>
                                                onToggleProducts(
                                                    [
                                                        row.product
                                                            .salesOrderLineId,
                                                    ],
                                                    checked,
                                                )
                                            }
                                        />
                                    </TableCell>
                                    <TableCell className="min-w-36 max-w-60 whitespace-normal py-2">
                                        <button
                                            id={`sourcing-list-${id}-edit`}
                                            type="button"
                                            className="w-full rounded-sm text-left text-sm font-medium outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring"
                                            aria-label={`调整 ${row.product.itemName}`}
                                            aria-current={
                                                activeId ===
                                                row.product.salesOrderLineId
                                                    ? "true"
                                                    : undefined
                                            }
                                            onClick={() =>
                                                onSelect(
                                                    row.product
                                                        .salesOrderLineId,
                                                )
                                            }
                                        >
                                            {row.product.itemName}
                                        </button>
                                        {row.allocations.length > 1 ? (
                                            <p className="mt-0.5 text-xs text-muted-foreground">
                                                {row.allocations.length} 个来源
                                            </p>
                                        ) : null}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="py-2"
                                    >
                                        <QuantityValue
                                            value={
                                                row.product.remainingQuantity
                                            }
                                            unit={row.product.unit}
                                        />
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="py-2"
                                    >
                                        {row.quantity === "—" ? (
                                            "—"
                                        ) : (
                                            <QuantityValue
                                                value={row.quantity}
                                                unit={row.product.unit}
                                            />
                                        )}
                                    </TableCell>
                                    <TableCell className="max-w-32 whitespace-normal py-2 text-xs">
                                        {row.route}
                                    </TableCell>
                                    <TableCell className="py-2">
                                        <StatusBadge
                                            label={row.status}
                                            tone={
                                                row.needsAttention
                                                    ? "warning"
                                                    : row.status === "已就绪"
                                                      ? "success"
                                                      : "neutral"
                                            }
                                        />
                                    </TableCell>
                                </TableRow>
                            )
                        })}
                    </TableBody>
                </Table>
                {!rows.length ? (
                    <p className="p-6 text-center text-sm text-muted-foreground">
                        没有符合条件的商品，可清除搜索或关闭“仅看待调整”。
                    </p>
                ) : null}
            </div>
        </TableToolbarScope>
    )
}
