"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { productSkuPriceRange } from "@/features/master-data/lib/list-filters"
import {
    blockerColumn,
    disableOnlyActionsColumn,
    lifecycleColumn,
    nameColumn,
    revisionNoColumn,
    revisionTimingColumn,
    stableNoColumn,
} from "@/features/master-data/components/list/list-column-primitives"
import type {
    MasterDataListItem,
    ProductListSkuSummary,
} from "@/features/master-data/types"

export function useProductListColumns({
    canUpdateProductListing,
    currentSupplySkuIds,
    lastFocusedRowId,
    productSkusByProduct,
    productSkusPending,
    productSkusError,
    productListingPending,
    productListingProductId,
    rows,
    supplierOfferingsPending,
    supplierOfferingsError,
    onUpdateProductListing,
    onSupplyProduct,
    onDisableTarget,
}: {
    canUpdateProductListing: boolean
    currentSupplySkuIds: ReadonlySet<string>
    lastFocusedRowId: React.MutableRefObject<string | null>
    productSkusByProduct: ReadonlyMap<string, readonly ProductListSkuSummary[]>
    productSkusPending: boolean
    productSkusError: boolean
    productListingPending: boolean
    productListingProductId: string | undefined
    rows: readonly MasterDataListItem[]
    supplierOfferingsPending: boolean
    supplierOfferingsError: boolean
    onUpdateProductListing: (
        item: MasterDataListItem,
        listed: boolean,
    ) => Promise<void>
    onSupplyProduct: (item: MasterDataListItem) => void
    onDisableTarget: (item: MasterDataListItem) => void
}) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            stableNoColumn(),
            nameColumn(),
            revisionNoColumn(),
            lifecycleColumn(),
            {
                id: "skuNames",
                header: "SKU 名称",
                meta: { label: "SKU 名称" },
                cell: ({ row }) => {
                    const skus =
                        productSkusByProduct.get(row.original.stableId) ?? []
                    if (productSkusPending) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                读取中…
                            </span>
                        )
                    }
                    if (productSkusError) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                暂不可查
                            </span>
                        )
                    }
                    if (skus.length === 0) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                —
                            </span>
                        )
                    }
                    const names = skus
                        .map((sku) => sku.skuName.trim())
                        .filter(Boolean)
                    const label = names.length > 0 ? names.join("、") : "—"
                    return (
                        <span
                            className="line-clamp-2 max-w-56 text-sm"
                            title={label}
                        >
                            {label}
                        </span>
                    )
                },
            },
            {
                id: "skuPriceRange",
                header: "SKU 售价",
                meta: { label: "SKU 售价", width: "amount" },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {productSkusPending
                            ? "读取中…"
                            : productSkusError
                              ? "暂不可查"
                              : productSkuPriceRange(
                                    productSkusByProduct.get(
                                        row.original.stableId,
                                    ) ?? [],
                                )}
                    </span>
                ),
            },
            {
                id: "skuCount",
                header: "SKU 数量",
                meta: { label: "SKU 数量", width: "amount" },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.skuCount ?? 0} 个
                    </span>
                ),
            },
            {
                id: "supply",
                header: "供给",
                meta: { label: "供给", width: "status" },
                cell: ({ row }) => {
                    const item = row.original
                    const productSkus =
                        productSkusByProduct.get(item.stableId) ?? []
                    const suppliedSkuCount = productSkus.filter((sku) =>
                        currentSupplySkuIds.has(sku.skuId),
                    ).length
                    const offeringPending =
                        productSkus.length > 0 && supplierOfferingsPending
                    const offeringFailed =
                        productSkus.length > 0 && supplierOfferingsError
                    const statusLabel = productSkusPending
                        ? "读取中…"
                        : productSkusError || offeringFailed
                          ? "暂不可查"
                          : suppliedSkuCount > 0
                            ? "有供给"
                            : "无供给"
                    return (
                        <Button
                            type="button"
                            size="xs"
                            variant="ghost"
                            className="h-auto gap-1.5 px-1 py-0.5"
                            aria-label={`${item.name}供给详情：${statusLabel}`}
                            onClick={(event) => {
                                event.stopPropagation()
                                lastFocusedRowId.current = item.stableId
                                onSupplyProduct(item)
                            }}
                        >
                            <Badge
                                variant={
                                    suppliedSkuCount > 0 &&
                                    !productSkusPending &&
                                    !offeringPending &&
                                    !productSkusError &&
                                    !offeringFailed
                                        ? "success"
                                        : "outline"
                                }
                            >
                                {offeringPending ? "读取中…" : statusLabel}
                            </Badge>
                            {!productSkusPending &&
                            !productSkusError &&
                            !offeringPending &&
                            !offeringFailed &&
                            productSkus.length > 0 ? (
                                <span className="num text-xs text-muted-foreground">
                                    {suppliedSkuCount}/{productSkus.length} SKU
                                </span>
                            ) : null}
                        </Button>
                    )
                },
            },
            {
                id: "listing",
                header: "上架状态",
                meta: { label: "上架状态" },
                cell: ({ row }) => {
                    const item = row.original
                    const inherited = item.listingStatus ?? "UNLISTED"
                    const pending =
                        productListingPending &&
                        productListingProductId === item.stableId
                    const label =
                        inherited === "LISTED"
                            ? "已上架"
                            : inherited === "PARTIALLY_LISTED"
                              ? "部分上架"
                              : "已下架"
                    return (
                        <div className="flex items-center gap-2">
                            <Switch
                                size="sm"
                                checked={inherited === "LISTED"}
                                disabled={
                                    pending ||
                                    !canUpdateProductListing ||
                                    (item.lifecycleStatus !== "ENABLED" &&
                                        inherited === "UNLISTED") ||
                                    (item.skuCount ?? 0) === 0
                                }
                                onCheckedChange={(checked) =>
                                    void onUpdateProductListing(item, checked)
                                }
                                aria-label={`${item.name}整组上架状态`}
                            />
                            <span className="whitespace-nowrap text-xs text-muted-foreground">
                                {pending
                                    ? "更新中…"
                                    : `${label} ${item.listedSkuCount ?? 0}/${item.skuCount ?? 0}`}
                            </span>
                        </div>
                    )
                },
            },
            revisionTimingColumn(),
            ...blockerColumn(rows),
            disableOnlyActionsColumn({
                lastFocusedRowId,
                onDisableTarget,
            }),
        ],
        [
            canUpdateProductListing,
            currentSupplySkuIds,
            lastFocusedRowId,
            onDisableTarget,
            onSupplyProduct,
            onUpdateProductListing,
            productListingPending,
            productListingProductId,
            productSkusByProduct,
            productSkusError,
            productSkusPending,
            rows,
            supplierOfferingsError,
            supplierOfferingsPending,
        ],
    )
}
