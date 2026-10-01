"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { productSkuPriceRange } from "@/features/master-data/lib/list-filters"
import {
    blockerColumn,
    lifecycleColumn,
    productActionsColumn,
    nameColumn,
    revisionNoColumn,
    revisionTimingColumn,
    stableNoColumn,
} from "@/features/master-data/components/list/list-column-primitives"
import type {
    MasterDataListItem,
    ProductListSkuSummary,
} from "@/features/master-data/types"

function ProductSupplyStatus({
    pending,
    failed,
    skuCount,
    suppliedSkuCount,
}: {
    pending: boolean
    failed: boolean
    skuCount: number
    suppliedSkuCount: number
}) {
    if (pending || failed || skuCount === 0 || suppliedSkuCount === 0) {
        const label = pending
            ? "读取中…"
            : failed
              ? "暂不可查"
              : skuCount === 0
                ? "—"
                : "未覆盖"
        return <span className="text-sm text-muted-foreground">{label}</span>
    }
    if (suppliedSkuCount === skuCount) {
        return (
            <BusinessStatusBadge context="list" label="已覆盖" tone="success" />
        )
    }
    return (
        <BusinessStatusBadge
            context="list"
            label={`${suppliedSkuCount}/${skuCount}`}
            tone="warning"
        />
    )
}

function productSupplyLabel(
    pending: boolean,
    failed: boolean,
    skuCount: number,
    suppliedSkuCount: number,
): string {
    if (pending) return "读取中…"
    if (failed) return "暂不可查"
    if (skuCount === 0) return "没有启用中的规格"
    if (suppliedSkuCount === 0) return "未覆盖"
    if (suppliedSkuCount === skuCount) return "已覆盖"
    return `${suppliedSkuCount}/${skuCount}`
}

function productSupplyTitle(
    pending: boolean,
    failed: boolean,
    skuCount: number,
    suppliedSkuCount: number,
): string | undefined {
    if (pending || failed) return undefined
    if (skuCount === 0) return "请先新增或启用商品规格"
    if (suppliedSkuCount === 0) return "当前没有启用中的供给"
    if (suppliedSkuCount === skuCount) return `${skuCount} 个 SKU 均有供给`
    return `${suppliedSkuCount} 个 SKU 有供给，共 ${skuCount} 个`
}

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
    onAddSupply,
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
    onAddSupply: (item: MasterDataListItem) => void
    onDisableTarget: (item: MasterDataListItem) => void
}) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            stableNoColumn(),
            nameColumn({ showNumber: true }),
            {
                id: "maintainer",
                header: "维护人",
                meta: { label: "维护人" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.ownerName ?? "—"}
                    </span>
                ),
            },
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
                meta: {
                    label: "SKU 售价",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
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
                meta: {
                    label: "SKU 数量",
                    width: "quantity",
                    align: "end",
                    numeric: true,
                },
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
                    const pending = productSkusPending || offeringPending
                    const failed = productSkusError || offeringFailed
                    const statusLabel = productSupplyLabel(
                        pending,
                        failed,
                        productSkus.length,
                        suppliedSkuCount,
                    )
                    return (
                        <Button
                            id={`master-data-product-${toAutomationIdSegment(item.stableId)}-supply`}
                            type="button"
                            size="xs"
                            variant="ghost"
                            className="h-auto px-0 py-0.5 font-normal"
                            aria-label={`${item.name}供给详情：${statusLabel}`}
                            title={productSupplyTitle(
                                pending,
                                failed,
                                productSkus.length,
                                suppliedSkuCount,
                            )}
                            onClick={(event) => {
                                event.stopPropagation()
                                lastFocusedRowId.current = item.stableId
                                onSupplyProduct(item)
                            }}
                        >
                            <ProductSupplyStatus
                                pending={pending}
                                failed={failed}
                                skuCount={productSkus.length}
                                suppliedSkuCount={suppliedSkuCount}
                            />
                        </Button>
                    )
                },
            },
            {
                id: "listing",
                header: "上架状态",
                meta: { label: "上架状态", width: "status" },
                cell: ({ row }) => {
                    const item = row.original
                    const inherited = item.listingStatus ?? "UNLISTED"
                    const pending =
                        productListingPending &&
                        productListingProductId === item.stableId
                    const statusLabel = pending
                        ? "更新中…"
                        : inherited === "LISTED"
                          ? "已上架"
                          : inherited === "PARTIALLY_LISTED"
                            ? "部分上架"
                            : "已下架"
                    const accessibleName = `${item.name}整组上架状态：${statusLabel}`
                    return (
                        <Switch
                            nativeButton
                            render={
                                <button
                                    type="button"
                                    aria-label={accessibleName}
                                    title={statusLabel}
                                />
                            }
                            id={`master-data-product-${toAutomationIdSegment(item.stableId)}-listing`}
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
                            aria-label={accessibleName}
                            title={statusLabel}
                        />
                    )
                },
            },
            revisionTimingColumn(),
            ...blockerColumn(rows),
            productActionsColumn({
                lastFocusedRowId,
                onDisableTarget,
                onAddSupply,
                addSupplyDisabledReason: (item) => {
                    if (productSkusPending) return "正在读取商品规格"
                    if (productSkusError) return "规格读取失败，请重试"
                    const skuCount =
                        productSkusByProduct.get(item.stableId)?.length ?? 0
                    if (skuCount === 0) return "请先新增或启用商品规格"
                    return undefined
                },
            }),
        ],
        [
            canUpdateProductListing,
            currentSupplySkuIds,
            lastFocusedRowId,
            onAddSupply,
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
