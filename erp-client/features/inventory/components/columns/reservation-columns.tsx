"use client"

import { ArrowUpRightIcon } from "lucide-react"

import type { ColumnDef } from "@tanstack/react-table"

import {
    QuantityValue,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import { inventoryStatusColumn } from "@/features/inventory/components/columns/status-column"
import { WarehouseSkuIdentity } from "@/features/inventory/components/columns/warehouse-sku-cell"
import type { StockReservationRow } from "@/features/inventory/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function buildReservationColumns(): ColumnDef<StockReservationRow>[] {
    return [
        {
            id: "identity",
            header: "仓库 / SKU",
            meta: { label: "仓库 / SKU", width: "flex" },
            cell: ({ row }) => (
                <WarehouseSkuIdentity
                    warehouseName={row.original.warehouseName}
                    skuCode={row.original.skuCode}
                    skuName={row.original.skuName}
                />
            ),
        },
        {
            id: "sales",
            header: "销售单 / 明细",
            meta: { label: "销售单", width: "default" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={row.original.salesOrderNo || <TableEmptyCell />}
                    secondary={row.original.salesOrderLineLabel}
                    secondaryTitle={row.original.salesOrderLineLabel}
                    numericSecondary
                    className="num"
                />
            ),
        },
        {
            id: "qty",
            header: "建立 / 剩余",
            meta: {
                label: "数量",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <div className="whitespace-nowrap">
                    <QuantityValue
                        value={row.original.establishedQuantity}
                        unit=""
                    />
                    <span className="px-1 text-muted-foreground">/</span>
                    <QuantityValue
                        value={row.original.remainingQuantity}
                        unit={row.original.baseUnit}
                    />
                    <div className="num text-xs text-muted-foreground">
                        已消耗 {row.original.consumedQuantity} · 已释放{" "}
                        {row.original.releasedQuantity}
                    </div>
                </div>
            ),
        },
        inventoryStatusColumn<StockReservationRow>(),
        {
            id: "source",
            header: "入库来源",
            meta: { label: "入库来源", width: "default" },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.inboundSourceDocumentNo}
                    clamp={1}
                    numeric
                />
            ),
        },
        {
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "default", align: "end" },
            cell: ({ row }) => {
                // 明确不提供释放预占入口
                const href = row.original.fulfillmentHref
                if (!href) return <TableEmptyCell />
                const segment = toAutomationIdSegment(
                    row.original.reservationId,
                )
                const documentNo =
                    row.original.salesOrderNo.trim() ||
                    row.original.inboundSourceDocumentNo?.trim()
                return (
                    <TableRowActions
                        actions={[
                            {
                                id: `inventory-ledger-reservation-row-${segment}-fulfillment`,
                                label: "查看发货任务",
                                icon: ArrowUpRightIcon,
                                href,
                            },
                        ]}
                        moreId={`inventory-ledger-reservation-row-${segment}-more`}
                        moreLabel={
                            documentNo
                                ? `${documentNo} 更多操作`
                                : "预占 更多操作"
                        }
                    />
                )
            },
        },
    ]
}
