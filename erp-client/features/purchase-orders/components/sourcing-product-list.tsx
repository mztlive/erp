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
import { SourcingParticipationControl } from "./sourcing-participation-control"
import type {
    SourcingParticipationProps,
    SourcingBatchSelectionProps,
} from "./create-sourcing/types"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"

export function SourcingProductList({
    rows,
    toolbar,
    activeId,
    onSelect,
    selectedProductIds,
    onToggleProducts,
    onSetParticipation,
}: SourcingBatchSelectionProps &
    SourcingParticipationProps & {
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
                            <TableHead data-align="end">本次数量</TableHead>
                            <TableHead>本次处理</TableHead>
                            <TableHead>方案状态</TableHead>
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
                                    id={`sourcing-list-${id}-row`}
                                    tabIndex={0}
                                    aria-label={`查看 ${row.product.itemName} 的供给方案`}
                                    className="h-12 cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
                                    onClick={(event) => {
                                        if (
                                            event.target instanceof Element &&
                                            event.target.closest(
                                                'button, input, label, a, select, textarea, [role="switch"], [role="checkbox"]',
                                            )
                                        )
                                            return
                                        onSelect(row.product.salesOrderLineId)
                                    }}
                                    onKeyDown={(event) => {
                                        if (
                                            event.target !== event.currentTarget
                                        )
                                            return
                                        if (
                                            event.key === "Enter" ||
                                            event.key === " "
                                        ) {
                                            event.preventDefault()
                                            onSelect(
                                                row.product.salesOrderLineId,
                                            )
                                        }
                                    }}
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
                                        <p className="mt-0.5 text-xs text-muted-foreground">
                                            {row.status === "暂不分配"
                                                ? "方案已保留"
                                                : row.route}
                                        </p>
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
                                        {row.status === "暂不分配" ||
                                        row.quantity === "—" ? (
                                            "—"
                                        ) : (
                                            <QuantityValue
                                                value={row.quantity}
                                                unit={row.product.unit}
                                            />
                                        )}
                                    </TableCell>
                                    <TableCell className="py-2">
                                        <SourcingParticipationControl
                                            idPrefix={`sourcing-list-${id}`}
                                            itemName={row.product.itemName}
                                            included={
                                                row.selected ||
                                                row.partiallySelected
                                            }
                                            onChange={(included) =>
                                                onSetParticipation(
                                                    [
                                                        row.product
                                                            .salesOrderLineId,
                                                    ],
                                                    included,
                                                )
                                            }
                                        />
                                    </TableCell>
                                    <TableCell className="py-2">
                                        <StatusBadge
                                            label={
                                                row.status === "暂不分配"
                                                    ? "本次不提交"
                                                    : row.status
                                            }
                                            tone={
                                                row.needsAttention
                                                    ? "warning"
                                                    : row.status === "已就绪"
                                                      ? "success"
                                                      : "neutral"
                                            }
                                        />
                                        {row.status === "部分分配" ? (
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                剩余{" "}
                                                <QuantityValue
                                                    value={
                                                        row.remainingQuantity
                                                    }
                                                    unit={row.product.unit}
                                                />
                                            </p>
                                        ) : null}
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
