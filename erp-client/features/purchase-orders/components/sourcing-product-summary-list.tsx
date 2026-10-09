"use client"

import { useState, type ReactNode } from "react"
import { ChevronRightIcon } from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import {
    TableToolbar,
    TableToolbarScope,
} from "@/components/business/table-toolbar"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { FULFILLMENT_RESPONSIBILITY_LABEL } from "../types"
import type {
    SourcingEditorProps,
    SourcingBatchSelectionProps,
    SourcingParticipationProps,
} from "./create-sourcing/types"
import { SourcingParticipationControl } from "./sourcing-participation-control"
import { SourcingProductAdjustDialog } from "./sourcing-product-adjust-dialog"

/** 窄屏行直接展示供给摘要，整行点击进入编辑弹窗。 */
export function SourcingProductSummaryList({
    rows,
    toolbar,
    selectedProductIds,
    onToggleProducts,
    onSetParticipation,
    ...props
}: SourcingEditorProps &
    SourcingBatchSelectionProps &
    SourcingParticipationProps & {
        rows: SourcingEditorRow[]
        toolbar: ReactNode
    }) {
    const [editingId, setEditingId] = useState<string | null>(null)
    const visibleSelectedCount = rows.filter((row) =>
        selectedProductIds.has(row.product.salesOrderLineId),
    ).length
    return (
        <TableToolbarScope>
            <TableToolbar
                className="shrink-0 border-b border-border px-4 py-2"
                actions={toolbar}
            >
                <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                    <label
                        htmlFor="sourcing-summary-select-all"
                        className="flex cursor-pointer items-center gap-2"
                    >
                        <NativeCheckbox
                            id="sourcing-summary-select-all"
                            aria-label="选择当前显示的全部商品"
                            checked={
                                rows.length > 0 &&
                                visibleSelectedCount === rows.length
                            }
                            indeterminate={
                                visibleSelectedCount > 0 &&
                                visibleSelectedCount < rows.length
                            }
                            disabled={!rows.length}
                            onCheckedChange={(checked) =>
                                onToggleProducts(
                                    rows.map(
                                        (row) => row.product.salesOrderLineId,
                                    ),
                                    checked,
                                )
                            }
                        />
                        全选当前 {rows.length} 行
                    </label>
                    <span>批量已选 {selectedProductIds.size} 行</span>
                </div>
            </TableToolbar>
            <div
                className="min-h-0 flex-1 overflow-auto"
                data-slot="sourcing-summary-list"
            >
                {rows.map((row) => {
                    const key = row.product.salesOrderLineId
                    const id = `sourcing-summary-${toAutomationIdSegment(key)}`
                    return (
                        <div
                            key={key}
                            className="flex items-start border-b border-border"
                        >
                            <div className="shrink-0 pt-5 pl-4">
                                <NativeCheckbox
                                    id={`${id}-select`}
                                    aria-label={`批量选择 ${row.product.itemName}`}
                                    checked={selectedProductIds.has(key)}
                                    onCheckedChange={(checked) =>
                                        onToggleProducts([key], checked)
                                    }
                                />
                            </div>
                            <div className="min-w-0 flex-1">
                                <div className="flex flex-wrap items-center justify-between gap-2 px-4 pt-4">
                                    <span className="text-base font-semibold">
                                        {row.product.itemName}
                                    </span>
                                    <SourcingParticipationControl
                                        idPrefix={id}
                                        itemName={row.product.itemName}
                                        included={
                                            row.selected ||
                                            row.partiallySelected
                                        }
                                        onChange={(included) =>
                                            onSetParticipation([key], included)
                                        }
                                    />
                                </div>
                                <button
                                    id={`${id}-adjust`}
                                    type="button"
                                    aria-label={`调整 ${row.product.itemName}`}
                                    aria-haspopup="dialog"
                                    className="flex w-full min-w-0 items-center gap-3 px-4 py-3 text-left outline-none hover:bg-muted/30 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
                                    onClick={() => setEditingId(key)}
                                >
                                    <span className="min-w-0 flex-1 space-y-1.5">
                                        <span className="flex flex-wrap items-center gap-2">
                                            <span className="text-xs text-muted-foreground">
                                                供给方案
                                            </span>
                                            <StatusBadge
                                                label={row.status}
                                                tone={
                                                    row.needsAttention
                                                        ? "warning"
                                                        : row.status ===
                                                            "已就绪"
                                                          ? "success"
                                                          : "neutral"
                                                }
                                            />
                                        </span>
                                        <span className="block divide-y divide-border/60">
                                            {row.allocations.map(({ line }) => {
                                                const option =
                                                    findSourcingOption(
                                                        row.product,
                                                        line.basisId,
                                                    )
                                                const stock =
                                                    option?.sourceType ===
                                                    "EXISTING_STOCK"
                                                return (
                                                    <span
                                                        key={line.rowKey}
                                                        className="block space-y-1.5 not-first:pt-2 not-last:pb-2"
                                                    >
                                                        <span className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 text-sm">
                                                            <span className="min-w-0 break-words">
                                                                {stock
                                                                    ? "出库仓库："
                                                                    : "供应商："}
                                                                {stock
                                                                    ? option.warehouseName ||
                                                                      option.supplierName
                                                                    : option?.supplierName ||
                                                                      "尚未选择"}
                                                            </span>
                                                            {option &&
                                                            !stock ? (
                                                                <span className="shrink-0">
                                                                    含税单价{" "}
                                                                    <MoneyValue
                                                                        value={
                                                                            option.unitCostGross
                                                                        }
                                                                    />
                                                                </span>
                                                            ) : null}
                                                        </span>
                                                        <span className="block text-sm text-muted-foreground">
                                                            {option
                                                                ? stock
                                                                    ? "现有库存"
                                                                    : FULFILLMENT_RESPONSIBILITY_LABEL[
                                                                          option
                                                                              .fulfillmentResponsibility
                                                                      ]
                                                                : "未选来源"}
                                                            {" · "}
                                                            {line.quantity ? (
                                                                <QuantityValue
                                                                    value={
                                                                        line.quantity
                                                                    }
                                                                    unit={
                                                                        row
                                                                            .product
                                                                            .unit
                                                                    }
                                                                />
                                                            ) : (
                                                                "数量待填写"
                                                            )}
                                                            {!stock &&
                                                            line.expectedDeliveryDate
                                                                ? ` · ${line.expectedDeliveryDate} 交付`
                                                                : ""}
                                                            {!line.selected
                                                                ? " · 暂不分配"
                                                                : ""}
                                                        </span>
                                                        {option && !stock ? (
                                                            <span className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
                                                                <span>
                                                                    进项税率{" "}
                                                                    <RateValue
                                                                        value={multiplyFixed(
                                                                            option.inputTaxRate,
                                                                            "100",
                                                                            {
                                                                                leftMaxScale: 6,
                                                                                rightMaxScale: 0,
                                                                                outputScale: 2,
                                                                            },
                                                                        )}
                                                                        precision={
                                                                            2
                                                                        }
                                                                    />
                                                                </span>
                                                                <span>
                                                                    {
                                                                        option.paymentTermLabel
                                                                    }
                                                                </span>
                                                                {option.fulfillmentResponsibility ===
                                                                "WAREHOUSE" ? (
                                                                    <span>
                                                                        入库目标仓：
                                                                        {line.targetWarehouseName ||
                                                                            "待选择"}
                                                                    </span>
                                                                ) : null}
                                                            </span>
                                                        ) : stock ? (
                                                            <span className="block text-xs text-muted-foreground">
                                                                成本按库存核算
                                                            </span>
                                                        ) : null}
                                                    </span>
                                                )
                                            })}
                                        </span>
                                        {row.status === "部分分配" ? (
                                            <span className="block text-xs text-muted-foreground">
                                                本次{" "}
                                                <QuantityValue
                                                    value={row.quantity}
                                                    unit={row.product.unit}
                                                />{" "}
                                                · 剩余{" "}
                                                <QuantityValue
                                                    value={
                                                        row.remainingQuantity
                                                    }
                                                    unit={row.product.unit}
                                                />
                                            </span>
                                        ) : null}
                                        {row.issues.length ? (
                                            <span className="block text-xs text-destructive">
                                                {row.issues.join("；")}
                                            </span>
                                        ) : null}
                                    </span>
                                    <ChevronRightIcon
                                        aria-hidden="true"
                                        className="size-4 shrink-0 text-muted-foreground"
                                    />
                                </button>
                            </div>
                        </div>
                    )
                })}
                {!rows.length ? (
                    <p className="px-4 py-8 text-center text-sm text-muted-foreground">
                        没有符合条件的商品，可清除搜索或关闭“仅看待调整”。
                    </p>
                ) : null}
            </div>
            <SourcingProductAdjustDialog
                {...props}
                onSetParticipation={onSetParticipation}
                productId={editingId}
                onClose={() => setEditingId(null)}
            />
        </TableToolbarScope>
    )
}
