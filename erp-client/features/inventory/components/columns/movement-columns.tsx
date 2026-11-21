"use client"

import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { ExternalLinkIcon } from "lucide-react"

import {
    QuantityValue,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { WarehouseSkuIdentity } from "@/features/inventory/components/columns/warehouse-sku-cell"
import type { StockMovementRow } from "@/features/inventory/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"

export function buildMovementColumns(): ColumnDef<StockMovementRow>[] {
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
            id: "type",
            header: "流水类型",
            meta: { label: "流水类型", width: "default" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={row.original.movementTypeLabel}
                    secondary={
                        row.original.direction === "increase" ? "增加" : "减少"
                    }
                />
            ),
        },
        {
            id: "qty",
            header: "数量",
            meta: {
                label: "数量",
                width: "quantity",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <QuantityValue
                    value={row.original.quantity}
                    unit={row.original.baseUnit}
                />
            ),
        },
        {
            id: "occurred",
            header: "发生 / 记录",
            meta: { label: "发生 / 记录", width: "amount", numeric: true },
            cell: ({ row }) => (
                <div className="num text-muted-foreground">
                    <div>
                        发生 {formatDateTime(row.original.occurredAt, "full")}
                    </div>
                    <div>
                        记录 {formatDateTime(row.original.recordedAt, "full")}
                    </div>
                </div>
            ),
        },
        {
            id: "source",
            header: "来源单据",
            meta: { label: "来源单据", width: "default", align: "end" },
            cell: ({ row }) =>
                row.original.sourceHref ? (
                    <Button
                        id={`inventory-ledger-movement-row-${toAutomationIdSegment(row.original.movementId)}-source`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="h-7 max-w-full px-2 text-body-compact font-normal text-muted-foreground group-hover/row:text-primary group-focus-within/row:text-primary"
                        onClick={(event) => event.stopPropagation()}
                        render={
                            <Link
                                href={row.original.sourceHref}
                                aria-label={`查看来源 ${row.original.sourceDocumentNo}`}
                            />
                        }
                    >
                        <span
                            className="num truncate"
                            title={row.original.sourceDocumentNo}
                        >
                            {row.original.sourceDocumentNo}
                        </span>
                        <ExternalLinkIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                ) : (
                    <TableTextCell
                        value={row.original.sourceDocumentNo}
                        clamp={1}
                        numeric
                    />
                ),
        },
        {
            id: "recorder",
            header: "记录人",
            meta: { label: "记录人", width: "default" },
            cell: ({ row }) => (
                <TableTextCell value={row.original.recordedByLabel} clamp={1} />
            ),
        },
    ]
}
