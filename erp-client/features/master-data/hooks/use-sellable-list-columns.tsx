"use client"

import * as React from "react"
import type { ColumnDef, SortingFn } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    QuantityValue,
    TableEmptyCell,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import { SellableItemThumbnail } from "@/features/master-data/components/list/sellable-item-thumbnail"
import type { MasterDataListItem } from "@/features/master-data/types"
import { compareDecimal } from "@/lib/fixed-decimal"

/** 按金额或数量允许精度比较十进制值；缺值排在最后。 */
function decimalSortingFn(maxScale: number): SortingFn<MasterDataListItem> {
    return (rowA, rowB, columnId) => {
        const left = String(rowA.getValue(columnId) ?? "")
        const right = String(rowB.getValue(columnId) ?? "")
        if (!left && !right) return 0
        if (!left) return 1
        if (!right) return -1
        return compareDecimal(left, right, maxScale)
    }
}

const moneySortingFn = decimalSortingFn(2)
const quantitySortingFn = decimalSortingFn(6)

export function useSellableListColumns() {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            {
                id: "name",
                accessorKey: "name",
                header: "商品名称 · 规格",
                // 身份列吸收整行余量，其余列锁在声明档位，避免所有列被平均拉伸
                meta: { label: "商品名称 · 规格", width: "flex" },
                enableSorting: false,
                cell: ({ row }) => {
                    const sellable = row.original.sellableItem
                    const { name, stableNo } = row.original
                    const specification =
                        sellable?.specificationLabel !== "无规格"
                            ? sellable?.specificationLabel
                            : undefined
                    const secondary = specification
                        ? `${stableNo} · ${specification}`
                        : stableNo
                    return (
                        <div className="flex min-w-0 items-center gap-2">
                            <SellableItemThumbnail
                                assetId={sellable?.mainImageAssetId}
                                label={name}
                                className="size-8 shrink-0 rounded-md"
                            />
                            <TableIdentityCell
                                primary={name}
                                primaryTitle={name}
                                secondary={secondary}
                                secondaryTitle={secondary}
                                numericSecondary
                                className="flex-1"
                            />
                            {sellable?.deliveryFreightExtra ? (
                                <StatusBadge
                                    tone="warning"
                                    label="需另付运费"
                                    className="shrink-0"
                                    title="所选地区内有地方只能集采并另付运费"
                                />
                            ) : null}
                        </div>
                    )
                },
            },
            {
                id: "productNo",
                accessorFn: (row) => row.sellableItem?.productNo ?? "",
                header: "SPU 编号",
                meta: { label: "SPU 编号", width: "default" },
                enableSorting: false,
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.sellableItem?.productNo}
                        numeric
                        clamp={1}
                    />
                ),
            },
            ...(
                [
                    {
                        id: "price",
                        key: "salesVisiblePriceGross",
                        label: "一件代发价（含税）",
                    },
                    {
                        id: "bulkPriceGross",
                        key: "bulkPriceGross",
                        label: "集采价（含税）",
                    },
                    {
                        id: "marketPrice",
                        key: "marketPrice",
                        label: "市场价（含税）",
                    },
                ] as const
            ).map<ColumnDef<MasterDataListItem>>(({ id, key, label }) => ({
                id,
                accessorFn: (row) => row.sellableItem?.[key] ?? "",
                header: label,
                meta: { label, width: "amount", align: "end", numeric: true },
                sortingFn: moneySortingFn,
                cell: ({ row }) => {
                    const value = row.original.sellableItem?.[key]
                    return value ? (
                        <MoneyValue display="exact" value={value} />
                    ) : (
                        <TableEmptyCell />
                    )
                },
            })),
            {
                id: "bulkMinQuantity",
                accessorFn: (row) => row.sellableItem?.bulkMinQuantity ?? "",
                header: "集采起订量",
                meta: {
                    label: "集采起订量",
                    width: "quantity",
                    align: "end",
                    numeric: true,
                },
                sortingFn: quantitySortingFn,
                cell: ({ row }) => {
                    const item = row.original.sellableItem
                    return item?.bulkMinQuantity ? (
                        <QuantityValue
                            value={item.bulkMinQuantity}
                            unit={item.baseUnit}
                        />
                    ) : (
                        <TableEmptyCell />
                    )
                },
            },
            {
                id: "supplierCodes",
                accessorFn: (row) =>
                    row.sellableItem?.supplierCodes?.join("、") ?? "",
                header: "供应商编号",
                meta: { label: "供应商编号", width: "default" },
                enableSorting: false,
                cell: ({ getValue }) => (
                    <TableTextCell
                        value={getValue<string>()}
                        numeric
                        clamp={2}
                        className="break-all"
                    />
                ),
            },
            {
                id: "supplierCount",
                accessorFn: (row) => row.sellableItem?.supplierCount ?? 0,
                header: "供应保障",
                meta: {
                    label: "供应保障",
                    width: "status",
                },
                cell: ({ row }) => {
                    const count = row.original.sellableItem?.supplierCount
                    if (count == null) return <TableEmptyCell />
                    if (count === 0) {
                        return (
                            <BusinessStatusBadge
                                context="list"
                                tone="neutral"
                                label="暂无供应商"
                            />
                        )
                    }
                    if (count === 1) {
                        return (
                            <>
                                <BusinessStatusBadge
                                    context="list"
                                    tone="warning"
                                    label="单一供应商"
                                    title="断供后无法替换"
                                />
                                <span className="sr-only">
                                    ，断供后无法替换
                                </span>
                            </>
                        )
                    }
                    return (
                        <BusinessStatusBadge
                            context="list"
                            appearance="quiet"
                            tone="success"
                            label={`${count} 家可供`}
                            className="num"
                        />
                    )
                },
            },
        ],
        [],
    )
}
