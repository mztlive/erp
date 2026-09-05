"use client"

import * as React from "react"
import { TriangleAlertIcon } from "lucide-react"
import type { ColumnDef, SortingFn } from "@tanstack/react-table"

import { MoneyValue } from "@/components/business"
import type { MasterDataListItem } from "@/features/master-data/types"
import { compareDecimal } from "@/lib/fixed-decimal"

/** 金额按精确十进制比较；缺值排在最后。 */
const moneySortingFn: SortingFn<MasterDataListItem> = (
    rowA,
    rowB,
    columnId,
) => {
    const left = String(rowA.getValue(columnId) ?? "")
    const right = String(rowB.getValue(columnId) ?? "")
    if (!left && !right) return 0
    if (!left) return 1
    if (!right) return -1
    return compareDecimal(left, right, 2)
}

/** 可供区域最多展开 2 个，其余折成「+N」；完整值进 title。 */
function SupplyRegions({ regions }: { regions: readonly string[] }) {
    if (regions.length === 0) {
        return <span className="text-sm text-muted-foreground">未标注</span>
    }
    const shown = regions.slice(0, 2)
    const rest = regions.length - 2
    return (
        <div
            className="flex min-w-0 flex-wrap gap-x-2 gap-y-1 text-[13px] text-muted-foreground"
            title={regions.join("、")}
        >
            {shown.map((region) => (
                <span key={region}>{region}</span>
            ))}
            {rest > 0 ? <span className="num">+{rest}</span> : null}
        </div>
    )
}

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
                    return (
                        <div className="min-w-0 space-y-1.5">
                            <div
                                className="truncate text-sm font-medium text-foreground"
                                title={row.original.name}
                            >
                                {row.original.name}
                            </div>
                            <div className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
                                <span className="num shrink-0">
                                    {row.original.stableNo}
                                </span>
                                {sellable &&
                                sellable.specificationLabel !== "无规格" ? (
                                    <span
                                        className="truncate border-l border-border pl-2"
                                        title={sellable.specificationLabel}
                                    >
                                        {sellable.specificationLabel}
                                    </span>
                                ) : null}
                            </div>
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
                    <span className="num text-sm">
                        {row.original.sellableItem?.productNo ?? "—"}
                    </span>
                ),
            },
            {
                id: "price",
                accessorFn: (row) =>
                    row.sellableItem?.salesVisiblePriceGross ?? "",
                // 口径写在列头，不在每一行重复「含税」
                header: "销售价（含税）",
                meta: {
                    label: "销售价（含税）",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                sortingFn: moneySortingFn,
                cell: ({ row }) => (
                    <MoneyValue
                        className="font-semibold"
                        value={
                            row.original.sellableItem?.salesVisiblePriceGross
                        }
                    />
                ),
            },
            {
                id: "marketPrice",
                accessorFn: (row) => row.sellableItem?.marketPrice ?? "",
                header: "市场参考价",
                meta: {
                    label: "市场参考价",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                sortingFn: moneySortingFn,
                cell: ({ row }) => {
                    const marketPrice = row.original.sellableItem?.marketPrice
                    if (!marketPrice) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                —
                            </span>
                        )
                    }
                    return (
                        <MoneyValue
                            className="font-normal text-muted-foreground"
                            value={marketPrice}
                        />
                    )
                },
            },
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
                meta: {
                    label: "供应保障",
                    width: "status",
                },
                cell: ({ row }) => {
                    const count = row.original.sellableItem?.supplierCount ?? 0
                    // 全表唯一有决策价值的列：只有一家供货时断供就没得替换
                    const atRisk = count <= 1
                    return (
                        <span
                            className={
                                atRisk
                                    ? "inline-flex items-center gap-1.5 text-[13px] text-warning-soft-foreground"
                                    : "inline-flex items-center gap-1.5 text-[13px] text-muted-foreground"
                            }
                        >
                            {atRisk ? (
                                <TriangleAlertIcon
                                    className="size-3.5 shrink-0"
                                    aria-hidden="true"
                                />
                            ) : null}
                            {atRisk ? (
                                "单一供应商"
                            ) : (
                                <>
                                    <span className="num">{count}</span> 家可供
                                </>
                            )}
                            {atRisk ? (
                                <span className="sr-only">
                                    ，断供后无法替换
                                </span>
                            ) : null}
                        </span>
                    )
                },
            },
        ],
        [],
    )
}
