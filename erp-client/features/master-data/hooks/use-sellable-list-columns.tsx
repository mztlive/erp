"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { MasterDataListItem } from "@/features/master-data/types"

export function useSellableListColumns() {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            {
                id: "name",
                accessorKey: "name",
                header: "SKU 名称 · 规格",
                meta: { label: "SKU 名称 · 规格" },
                cell: ({ row }) => {
                    const sellable = row.original.sellableItem
                    return (
                        <div className="min-w-0">
                            <div className="truncate text-sm font-medium">
                                {row.original.name}
                                {sellable ? (
                                    <span className="text-muted-foreground">
                                        {" "}
                                        · {sellable.specificationLabel}
                                    </span>
                                ) : null}
                            </div>
                            <div className="truncate text-xs text-muted-foreground">
                                SKU 编号：
                                <span className="num">
                                    {row.original.stableNo}
                                </span>
                            </div>
                        </div>
                    )
                },
            },
            {
                id: "productNo",
                header: "SPU 编号",
                meta: { label: "SPU 编号", width: "default" },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.sellableItem?.productNo ?? "—"}
                    </span>
                ),
            },
            {
                id: "price",
                header: "销售价",
                meta: { label: "销售价", width: "amount" },
                cell: ({ row }) => (
                    <div className="flex flex-col gap-0.5">
                        <MoneyValue
                            value={
                                row.original.sellableItem
                                    ?.salesVisiblePriceGross
                            }
                        />
                        <span className="text-tiny text-muted-foreground">
                            含税
                        </span>
                    </div>
                ),
            },
            {
                id: "marketPrice",
                header: "市场价",
                meta: { label: "市场价", width: "amount" },
                cell: ({ row }) => {
                    const marketPrice = row.original.sellableItem?.marketPrice
                    if (!marketPrice) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                —
                            </span>
                        )
                    }
                    return <MoneyValue value={marketPrice} />
                },
            },
            {
                id: "supplyRegions",
                header: "可供区域",
                meta: { label: "可供区域" },
                cell: ({ row }) => {
                    const regions =
                        row.original.sellableItem?.supplyRegions ?? []
                    const label =
                        regions.length > 0 ? regions.join("、") : "未标注"
                    return (
                        <span
                            className="line-clamp-2 max-w-64 text-sm"
                            title={label}
                        >
                            {label}
                        </span>
                    )
                },
            },
            {
                id: "supplierCount",
                header: "有效供应商",
                meta: { label: "有效供应商", width: "status" },
                cell: ({ row }) => (
                    <Badge variant="outline">
                        <span className="num">
                            {row.original.sellableItem?.supplierCount ?? 0}
                        </span>{" "}
                        家
                    </Badge>
                ),
            },
        ],
        [],
    )
}
