"use client"

import type { ColumnDef } from "@/components/business/data-table"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    ChevronDownIcon,
    ChevronRightIcon,
    SlidersHorizontalIcon,
} from "lucide-react"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import type { SourcingBatchSelectionProps } from "./create-sourcing/types"

/** 摘要优先展示实际参与的来源；暂停商品仍可核对保留的方案。 */
function sourceFacts(row: SourcingEditorRow) {
    const selected = row.allocations.filter(({ line }) => line.selected)
    return (selected.length ? selected : row.allocations).map(({ line }) => ({
        line,
        option: findSourcingOption(row.product, line.basisId),
    }))
}

function singleValue(values: string[], multiple: string) {
    const unique = [...new Set(values)]
    return unique.length === 1 ? unique[0] : unique.length ? multiple : "—"
}

export function sourcingProductColumns({
    selectedProductIds,
    onToggleProducts,
    expandedIds,
    onToggleExpanded,
    onEdit,
}: SourcingBatchSelectionProps & {
    expandedIds: ReadonlySet<string>
    onToggleExpanded: (id: string) => void
    onEdit: (id: string) => void
}): ColumnDef<SourcingEditorRow, unknown>[] {
    return [
        {
            id: "participation",
            header: "分配",
            cell: ({ row: { original: row } }) => (
                <NativeCheckbox
                    id={`sourcing-list-${toAutomationIdSegment(row.product.salesOrderLineId)}-select`}
                    aria-label={`本次分配 ${row.product.itemName}`}
                    checked={selectedProductIds.has(
                        row.product.salesOrderLineId,
                    )}
                    onCheckedChange={(checked) =>
                        onToggleProducts(
                            [row.product.salesOrderLineId],
                            checked,
                        )
                    }
                />
            ),
            enableHiding: false,
            enableSorting: false,
            enablePinning: false,
            enableResizing: false,
            meta: { label: "分配", role: "selection", align: "center" },
        },
        {
            id: "product",
            header: "商品 / 规格",
            enableHiding: false,
            cell: ({ row: { original: row } }) => (
                <div className="min-w-40 max-w-64 whitespace-normal">
                    <span
                        className="line-clamp-1 font-medium"
                        title={row.product.itemName}
                    >
                        {row.product.itemName}
                    </span>
                    <span
                        className="block truncate text-xs text-muted-foreground"
                        title={row.product.itemSku}
                    >
                        {row.product.itemSku || ""}
                        {row.product.deliveryAllocations.length
                            ? ` · ${row.product.deliveryAllocations.length} 个地址`
                            : ""}
                    </span>
                </div>
            ),
            meta: { label: "商品 / 规格", width: "reference" },
        },
        {
            id: "demand",
            header: "待分配量",
            cell: ({ row: { original: row } }) => (
                <QuantityValue
                    value={row.product.remainingQuantity}
                    unit={row.product.unit}
                />
            ),
            meta: { align: "end", width: "quantity" },
        },
        {
            id: "quantity",
            header: "本次分配量",
            cell: ({ row: { original: row } }) =>
                row.selected || row.partiallySelected ? (
                    <QuantityValue
                        value={row.quantity}
                        unit={row.product.unit}
                    />
                ) : (
                    "—"
                ),
            meta: { align: "end", width: "quantity" },
        },
        {
            id: "supplier",
            header: "供应商 / 出库仓",
            cell: ({ row: { original: row } }) => {
                const id = row.product.salesOrderLineId
                if (row.allocations.length > 1)
                    return (
                        <Button
                            id={`sourcing-list-${toAutomationIdSegment(id)}-expand`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            aria-expanded={expandedIds.has(id)}
                            aria-controls={`sourcing-list-${toAutomationIdSegment(id)}-sources`}
                            onClick={() => onToggleExpanded(id)}
                        >
                            {expandedIds.has(id) ? (
                                <ChevronDownIcon />
                            ) : (
                                <ChevronRightIcon />
                            )}
                            {row.allocations.length} 个供给来源
                        </Button>
                    )
                const option = sourceFacts(row)[0]?.option
                const name =
                    option?.sourceType === "EXISTING_STOCK"
                        ? option.warehouseName || option.supplierName
                        : option?.supplierName || "尚未选源"
                return (
                    <span className="block max-w-64 truncate" title={name}>
                        {name}
                    </span>
                )
            },
            meta: { width: "flex" },
        },
        {
            id: "route",
            header: "履约方式",
            cell: ({ row: { original: row } }) =>
                singleValue(
                    sourceFacts(row).map(({ option }) =>
                        option ? sourcingRouteLabel(option) : "尚未选源",
                    ),
                    "多种方式",
                ),
            meta: { width: "status" },
        },
        {
            id: "price",
            header: "含税单价",
            cell: ({ row: { original: row } }) => {
                const facts = sourceFacts(row)
                const value = singleValue(
                    facts.map(({ option }) =>
                        !option
                            ? "—"
                            : option.sourceType === "EXISTING_STOCK"
                              ? "库存核算"
                              : option.unitCostGross,
                    ),
                    "多个价格",
                )
                return ["—", "库存核算", "多个价格"].includes(value) ? (
                    value
                ) : (
                    <MoneyValue value={value} display="exact" />
                )
            },
            meta: { align: "end", width: "amount" },
        },
        {
            id: "delivery",
            header: "交付日期",
            cell: ({ row: { original: row } }) =>
                singleValue(
                    sourceFacts(row).map(({ line, option }) =>
                        option?.sourceType === "EXISTING_STOCK"
                            ? "库存出库"
                            : line.expectedDeliveryDate || "待填写",
                    ),
                    "多个日期",
                ),
            meta: { numeric: true, width: "status" },
        },
        {
            id: "tax",
            header: "进项税率",
            cell: ({ row: { original: row } }) => {
                const value = singleValue(
                    sourceFacts(row).map(({ option }) =>
                        option?.sourceType === "PURCHASE"
                            ? option.inputTaxRate
                            : "—",
                    ),
                    "多个税率",
                )
                return ["—", "多个税率"].includes(value) ? (
                    value
                ) : (
                    <RateValue
                        value={multiplyFixed(value, "100", {
                            leftMaxScale: 6,
                            rightMaxScale: 0,
                            outputScale: 2,
                        })}
                        precision={2}
                    />
                )
            },
            meta: { align: "end", width: "rate" },
        },
        {
            id: "payment",
            header: "付款条件",
            cell: ({ row: { original: row } }) =>
                singleValue(
                    sourceFacts(row).map(({ option }) =>
                        option?.sourceType === "PURCHASE"
                            ? option.paymentTermLabel
                            : "—",
                    ),
                    "多种条件",
                ),
            meta: { width: "status" },
        },
        {
            id: "status",
            header: "方案状态",
            cell: ({ row: { original: row } }) => (
                <div className="max-w-44">
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
                    {row.issues.length ? (
                        <p
                            className="mt-0.5 truncate text-xs text-destructive"
                            title={row.issues.join("；")}
                        >
                            {row.issues[0]}
                        </p>
                    ) : row.status === "部分分配" ? (
                        <p className="mt-0.5 text-xs text-muted-foreground">
                            剩余{" "}
                            <QuantityValue
                                value={row.remainingQuantity}
                                unit={row.product.unit}
                            />
                        </p>
                    ) : null}
                </div>
            ),
            meta: { width: "status" },
        },
        {
            id: "actions",
            header: "操作",
            enableHiding: false,
            cell: ({ row: { original: row } }) => (
                <Button
                    id={`sourcing-list-${toAutomationIdSegment(row.product.salesOrderLineId)}-edit`}
                    type="button"
                    variant="outline"
                    size="sm"
                    aria-label={`调整 ${row.product.itemName}`}
                    onClick={() => onEdit(row.product.salesOrderLineId)}
                >
                    <SlidersHorizontalIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    调整
                </Button>
            ),
            meta: { width: "status", align: "end" },
        },
    ]
}
