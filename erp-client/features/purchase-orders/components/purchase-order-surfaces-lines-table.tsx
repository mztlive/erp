"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    DataTable,
    MoneyValue,
    QuantityValue,
    RateValue,
    taxAmountToneClass,
} from "@/components/business"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import { multiplyFixed } from "@/lib/fixed-decimal"

export type PurchaseOrderLinesTableOrder = NonNullable<
    ReturnType<typeof usePurchaseOrderCenterQuery>["data"]
>

export type PurchaseOrderLineRow =
    PurchaseOrderLinesTableOrder["currentContent"]["lines"][number]

function lineTypeLabel(lineType: PurchaseOrderLineRow["lineType"]) {
    return lineType === "LOGISTICS_FEE" ? "物流费用" : "商品/服务"
}

function taxRatePercent(inputTaxRate: string) {
    return multiplyFixed(inputTaxRate, "100", {
        leftMaxScale: 6,
        rightMaxScale: 0,
        outputScale: 2,
    })
}

function MaskedAmount() {
    return <span className="text-sm text-muted-foreground">•••</span>
}

function buildPurchaseOrderLinesColumns(
    costMasked: boolean,
    summary: boolean,
): ColumnDef<PurchaseOrderLineRow>[] {
    return [
        {
            id: "item",
            accessorFn: (row) => row.itemName,
            header: "项目",
            meta: { label: "项目", width: summary ? "default" : "flex" },
            cell: ({ row }) => {
                const line = row.original
                return (
                    <div className="whitespace-normal">
                        <div className="font-medium">{line.itemName}</div>
                        {summary ? (
                            <div className="text-xs text-muted-foreground">
                                {lineTypeLabel(line.lineType)}
                            </div>
                        ) : null}
                        {line.procurementConfirmationLineId ? (
                            <div className="text-xs text-muted-foreground">
                                {line.salesAllocationLabel ??
                                    `确认分行 · ${line.itemName}`}
                            </div>
                        ) : null}
                    </div>
                )
            },
        },
        {
            id: "type",
            accessorFn: (row) => lineTypeLabel(row.lineType),
            header: "类型",
            meta: { label: "类型", width: "status" },
            cell: ({ row }) => (
                <span className="text-xs text-muted-foreground">
                    {lineTypeLabel(row.original.lineType)}
                </span>
            ),
        },
        {
            id: "quantity",
            accessorKey: "quantity",
            header: "数量",
            meta: {
                label: "数量",
                width: "quantity",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => {
                const line = row.original
                if (line.lineType === "LOGISTICS_FEE") return "—"
                return (
                    <QuantityValue
                        value={line.quantity ?? "0"}
                        unit={line.unit}
                    />
                )
            },
        },
        {
            id: "unitCost",
            accessorKey: "unitCostGross",
            header: "含税单价",
            meta: {
                label: "含税单价",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) =>
                costMasked ? (
                    <MaskedAmount />
                ) : (
                    <div>
                        <MoneyValue value={row.original.unitCostGross} />
                        {summary ? (
                            <div className="text-xs text-muted-foreground">
                                税率{" "}
                                <RateValue
                                    value={taxRatePercent(
                                        row.original.inputTaxRate,
                                    )}
                                    precision={2}
                                />
                            </div>
                        ) : null}
                    </div>
                ),
        },
        {
            id: "taxRate",
            accessorKey: "inputTaxRate",
            header: "税率",
            meta: {
                label: "税率",
                width: "rate",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <RateValue
                    value={taxRatePercent(row.original.inputTaxRate)}
                    precision={2}
                />
            ),
        },
        {
            id: "delivery",
            accessorKey: "expectedDeliveryDate",
            header: "交期",
            meta: {
                label: "交期",
                width: "default",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <span className="num text-[13px]">
                    {row.original.expectedDeliveryDate ?? "—"}
                </span>
            ),
        },
        {
            id: "gross",
            accessorKey: "grossAmount",
            header: summary ? "含税小计" : "行含税",
            meta: {
                label: "行含税",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) =>
                costMasked ? (
                    <MaskedAmount />
                ) : (
                    <div>
                        <MoneyValue
                            value={row.original.grossAmount}
                            className={taxAmountToneClass("行含税")}
                        />
                        {summary ? (
                            <div className="text-xs text-muted-foreground">
                                税额{" "}
                                <MoneyValue value={row.original.taxAmount} />
                            </div>
                        ) : null}
                    </div>
                ),
        },
        {
            id: "tax",
            accessorKey: "taxAmount",
            header: "税额",
            meta: {
                label: "税额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) =>
                costMasked ? (
                    <MaskedAmount />
                ) : (
                    <MoneyValue
                        value={row.original.taxAmount}
                        className={taxAmountToneClass("税额")}
                    />
                ),
        },
    ]
}

export function LinesTable({
    order,
    costMasked,
    summary = false,
}: {
    order: PurchaseOrderLinesTableOrder
    costMasked: boolean
    summary?: boolean
}) {
    const lines = order.currentContent.lines
    const columns = React.useMemo(
        () => buildPurchaseOrderLinesColumns(costMasked, summary),
        [costMasked, summary],
    )

    return (
        <DataTable
            id={`procurement-orders-detail-lines-table-${order.identity.purchaseOrderId}`}
            data={[...lines]}
            columns={columns}
            defaultColumnVisibility={
                summary ? { type: false, taxRate: false, tax: false } : {}
            }
            getRowId={(row) => row.lineId}
            rowCount={lines.length}
            rowLabel={(row) => row.itemName}
            caption="采购明细"
            layout="flush"
            density="compact"
            showPagination={false}
            showColumnVisibility={false}
            defaultColumnPinning={{ left: ["item"] }}
            emptyTitle="暂无采购明细"
        />
    )
}
