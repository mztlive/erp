"use client"

import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { StockReservationRow } from "@/features/inventory/types"

export function buildReservationColumns(): ColumnDef<StockReservationRow>[] {
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
            id: "sales",
            header: "销售单 / 明细",
            meta: { label: "销售单", width: "default" },
            cell: ({ row }) => (
                <div className="text-sm">
                    <div className="num">{row.original.salesOrderNo}</div>
                    <div className="text-xs text-muted-foreground">
                        {row.original.salesOrderLineLabel}
                    </div>
                </div>
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
                <div className="text-end text-sm">
                    <div className="num">
                        {row.original.establishedQuantity} /{" "}
                        {row.original.remainingQuantity}
                        <span className="ml-1 text-xs text-muted-foreground">
                            {row.original.baseUnit}
                        </span>
                    </div>
                    <div className="text-xs text-muted-foreground">
                        已消耗 {row.original.consumedQuantity} · 已释放{" "}
                        {row.original.releasedQuantity}
                    </div>
                </div>
            ),
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={row.original.statusLabel}
                    tone={row.original.statusTone}
                />
            ),
        },
        {
            id: "source",
            header: "入库来源",
            meta: { label: "入库来源", width: "default" },
            cell: ({ row }) => (
                <span className="num text-sm">
                    {row.original.inboundSourceDocumentNo ?? "—"}
                </span>
            ),
        },
        {
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "default", align: "end" },
            cell: ({ row }) => (
                <div className="flex justify-end gap-1">
                    {row.original.fulfillmentHref ? (
                        <Button
                            type="button"
                            variant="outline"
                            size="xs"
                            render={
                                <Link href={row.original.fulfillmentHref} />
                            }
                        >
                            履约上下文
                        </Button>
                    ) : null}
                    {/* 明确不提供释放预占入口 */}
                </div>
            ),
        },
    ]
}
