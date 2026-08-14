"use client"

import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { ExternalLinkIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { formatQty } from "@/features/inventory/components/presentation"
import type { StockMovementRow } from "@/features/inventory/types"
import { formatDateTime } from "@/lib/datetime"

export function buildMovementColumns(): ColumnDef<StockMovementRow>[] {
    return [
        {
            id: "identity",
            header: "仓库 / SKU",
            meta: { label: "仓库 / SKU", width: "reference" },
            cell: ({ row }) => (
                <div className="min-w-0 text-sm">
                    <div className="truncate font-medium">
                        {row.original.warehouseName}
                    </div>
                    <div className="truncate">
                        <span className="num">{row.original.skuCode}</span>
                        <span className="text-muted-foreground"> · </span>
                        {row.original.skuName}
                    </div>
                </div>
            ),
        },
        {
            id: "type",
            header: "流水类型",
            meta: { label: "流水类型", width: "default" },
            cell: ({ row }) => (
                <div className="text-sm">
                    <div>{row.original.movementTypeLabel}</div>
                    <div className="text-xs text-muted-foreground">
                        {row.original.direction === "increase"
                            ? "增加"
                            : "减少"}
                    </div>
                </div>
            ),
        },
        {
            id: "qty",
            header: "数量",
            meta: {
                label: "数量",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) =>
                formatQty(row.original.quantity, row.original.baseUnit),
        },
        {
            id: "occurred",
            header: "发生 / 记录",
            meta: { label: "时间", width: "default", numeric: true },
            cell: ({ row }) => (
                <div className="num text-xs text-muted-foreground">
                    <div>
                        发生{" "}
                        {formatDateTime(
                            row.original.occurredAt,
                            "full",
                            "passthrough",
                        )}
                    </div>
                    <div>
                        记录{" "}
                        {formatDateTime(
                            row.original.recordedAt,
                            "full",
                            "passthrough",
                        )}
                    </div>
                </div>
            ),
        },
        {
            id: "source",
            header: "来源单据",
            meta: { label: "来源单据", width: "default" },
            cell: ({ row }) =>
                row.original.sourceHref ? (
                    <Button
                        type="button"
                        variant="link"
                        size="xs"
                        className="h-auto px-0"
                        render={
                            <Link
                                href={row.original.sourceHref}
                                aria-label={`查看来源 ${row.original.sourceDocumentNo}`}
                            />
                        }
                    >
                        <span className="num">
                            {row.original.sourceDocumentNo}
                        </span>
                        <ExternalLinkIcon className="ml-1 size-3" aria-hidden />
                    </Button>
                ) : (
                    <span className="num text-sm">
                        {row.original.sourceDocumentNo}
                    </span>
                ),
        },
        {
            id: "recorder",
            header: "记录人",
            meta: { label: "记录人", width: "default" },
            cell: ({ row }) => (
                <span className="text-sm">{row.original.recordedByLabel}</span>
            ),
        },
    ]
}
