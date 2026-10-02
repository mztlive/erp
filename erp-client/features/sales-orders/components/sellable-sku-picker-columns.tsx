"use client"

import * as React from "react"
import { CircleCheckIcon, TriangleAlertIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"

import { MoneyValue, QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { SkuThumbnail } from "@/features/sales-orders/components/sku-thumbnail"
import type { MasterDataListItem } from "@/features/master-data/types"

const REFERENCE_PRICE_COLUMNS = [
    { id: "factoryPriceGross", label: "出厂价（含税）" },
    { id: "salesVisiblePriceGross", label: "一件代发价（含税）" },
    { id: "bulkPriceGross", label: "集采价（含税）" },
    { id: "marketPrice", label: "市场价（含税）" },
] as const

function SupplyRegions({ regions }: { regions: readonly string[] }) {
    if (regions.length === 0) {
        return <span className="text-sm text-muted-foreground">未标注</span>
    }
    const shown = regions.slice(0, 2)
    const rest = regions.length - 2
    return (
        <div
            className="flex min-w-0 flex-wrap gap-1"
            title={regions.join("、")}
        >
            {shown.map((region) => (
                <Badge key={region} variant="secondary">
                    {region}
                </Badge>
            ))}
            {rest > 0 ? <Badge variant="neutral">+{rest}</Badge> : null}
        </div>
    )
}

export function useSellableSkuPickerColumns() {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            {
                id: "name",
                accessorKey: "name",
                header: "商品",
                meta: { label: "商品", width: "flex" },
                enableSorting: false,
                cell: ({ row }) => {
                    const sellable = row.original.sellableItem
                    const label = sellable
                        ? `${row.original.name} · ${sellable.specificationLabel}`
                        : row.original.name
                    return (
                        <div className="flex min-w-0 items-center gap-3">
                            <SkuThumbnail
                                assetId={sellable?.mainImageAssetId}
                                label={label}
                            />
                            <div className="min-w-0">
                                <div className="truncate text-sm font-medium">
                                    {label}
                                </div>
                                <div className="truncate text-xs text-muted-foreground">
                                    SKU 编号：
                                    <span className="num">
                                        {row.original.stableNo}
                                    </span>
                                </div>
                                {sellable?.supplierCodes?.length ? (
                                    <div
                                        className="truncate text-xs text-muted-foreground"
                                        title={sellable.supplierCodes.join(
                                            "、",
                                        )}
                                    >
                                        供应商编号：
                                        <span className="num">
                                            {sellable.supplierCodes.join("、")}
                                        </span>
                                    </div>
                                ) : null}
                            </div>
                        </div>
                    )
                },
            },
            ...REFERENCE_PRICE_COLUMNS.map(
                ({ id, label }) =>
                    ({
                        id,
                        accessorFn: (row: MasterDataListItem) =>
                            row.sellableItem?.[id] ?? "",
                        header: label,
                        meta: {
                            label,
                            width: "amount" as const,
                            align: "end" as const,
                            numeric: true,
                        },
                        enableSorting: false,
                        cell: ({ row }) => (
                            <div className="space-y-1">
                                <MoneyValue
                                    value={row.original.sellableItem?.[id]}
                                />
                                {id === "bulkPriceGross" &&
                                row.original.sellableItem?.bulkMinQuantity ? (
                                    <div className="text-xs text-muted-foreground">
                                        起订{" "}
                                        <QuantityValue
                                            value={
                                                row.original.sellableItem
                                                    .bulkMinQuantity
                                            }
                                            unit={
                                                row.original.sellableItem
                                                    .baseUnit
                                            }
                                        />
                                    </div>
                                ) : null}
                            </div>
                        ),
                    }) satisfies ColumnDef<MasterDataListItem>,
            ),
            {
                id: "supplyRegions",
                header: "可供区域",
                meta: { label: "可供区域", width: "default" },
                enableSorting: false,
                cell: ({ row }) => (
                    <SupplyRegions
                        regions={row.original.sellableItem?.supplyRegions ?? []}
                    />
                ),
            },
            {
                id: "supplierCount",
                accessorFn: (row) => row.sellableItem?.supplierCount ?? 0,
                header: "供应保障",
                meta: { label: "供应保障", width: "status" },
                enableSorting: false,
                cell: ({ row }) => {
                    const count = row.original.sellableItem?.supplierCount ?? 0
                    const atRisk = count <= 1
                    return (
                        <Badge variant={atRisk ? "warning" : "success"}>
                            {atRisk ? (
                                <TriangleAlertIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                            ) : (
                                <CircleCheckIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                            )}
                            {atRisk ? (
                                "单一供应商"
                            ) : (
                                <>
                                    <span className="num">{count}</span> 家可供
                                </>
                            )}
                        </Badge>
                    )
                },
            },
        ],
        [],
    )
}
