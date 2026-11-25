"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    QuantityValue,
    TableAsyncCell,
    TableEmptyCell,
    TableSwitchCell,
    TableTextCell,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { PRODUCT_SOURCE_FIELD_LABELS } from "../api/types-product-source"
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
    return (
        <TableAsyncCell pending={pending} failed={failed}>
            {skuCount === 0 ? (
                <TableEmptyCell />
            ) : suppliedSkuCount === 0 ? (
                <BusinessStatusBadge
                    context="list"
                    label="未覆盖"
                    tone="neutral"
                />
            ) : suppliedSkuCount === skuCount ? (
                <BusinessStatusBadge
                    context="list"
                    appearance="quiet"
                    label="已覆盖"
                    tone="success"
                />
            ) : (
                <BusinessStatusBadge
                    context="list"
                    label={`${suppliedSkuCount}/${skuCount}`}
                    tone="warning"
                    className="num"
                />
            )}
        </TableAsyncCell>
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
    canReadOfferings,
    canReadSkus,
    currentSupplySkuIds,
    lastFocusedRowId,
    productSkusByProduct,
    productSkusPending,
    productSkusError,
    productListingPending,
    productListingProductId,
    batchListingPending = false,
    rows,
    supplierOfferingsPending,
    supplierOfferingsError,
    onUpdateProductListing,
    onSupplyProduct,
    onSourceDiff,
    onAddSupply,
    onDisableTarget,
}: {
    canUpdateProductListing: boolean
    canReadOfferings: boolean
    canReadSkus: boolean
    currentSupplySkuIds: ReadonlySet<string>
    lastFocusedRowId: React.MutableRefObject<string | null>
    productSkusByProduct: ReadonlyMap<string, readonly ProductListSkuSummary[]>
    productSkusPending: boolean
    productSkusError: boolean
    productListingPending: boolean
    productListingProductId: string | undefined
    batchListingPending?: boolean
    rows: readonly MasterDataListItem[]
    supplierOfferingsPending: boolean
    supplierOfferingsError: boolean
    onUpdateProductListing: (
        item: MasterDataListItem,
        listed: boolean,
    ) => Promise<void>
    onSupplyProduct: (item: MasterDataListItem) => void
    onSourceDiff?: (item: MasterDataListItem) => void
    onAddSupply?: (item: MasterDataListItem) => void
    onDisableTarget?: (item: MasterDataListItem) => void
}) {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(() => {
        const hasSourceDiffRows = rows.some((row) =>
            Boolean(row.sourceDiffCount),
        )
        return [
            stableNoColumn(),
            nameColumn({ showNumber: true }),
            {
                id: "maintainer",
                accessorKey: "ownerName",
                header: "维护人",
                enableSorting: true,
                meta: { label: "维护人" },
                cell: ({ row }) => (
                    <TableTextCell value={row.original.ownerName} />
                ),
            },
            revisionNoColumn(),
            lifecycleColumn(),
            ...(hasSourceDiffRows
                ? ([
                      {
                          id: "sourceDiff",
                          header: "资料更新",
                          enableSorting: false,
                          meta: { label: "资料更新" },
                          cell: ({ row }) => {
                              const item = row.original
                              if (!item.sourceDiffCount)
                                  return <TableEmptyCell />
                              const summary = (
                                  <>
                                      <BusinessStatusBadge
                                          context="list"
                                          tone="warning"
                                          label={`资料更新待确认 · ${item.sourceDiffCount} 项`}
                                          className="num"
                                      />
                                      <span className="line-clamp-2 w-full text-xs whitespace-normal text-muted-foreground">
                                          {[
                                              ...new Set(
                                                  item.sourceDiffFields ?? [],
                                              ),
                                          ]
                                              .map(
                                                  (field) =>
                                                      PRODUCT_SOURCE_FIELD_LABELS[
                                                          field
                                                      ],
                                              )
                                              .join("、")}
                                      </span>
                                  </>
                              )
                              if (!onSourceDiff)
                                  return (
                                      <div className="flex flex-col items-start gap-1">
                                          {summary}
                                      </div>
                                  )
                              return (
                                  <Button
                                      id={`master-data-product-${toAutomationIdSegment(item.stableId)}-source-diff`}
                                      type="button"
                                      variant="ghost"
                                      size="sm"
                                      className="h-auto max-w-48 flex-col items-start gap-1 px-0 text-left"
                                      onClick={(event) => {
                                          event.stopPropagation()
                                          onSourceDiff(item)
                                      }}
                                  >
                                      {summary}
                                  </Button>
                              )
                          },
                      },
                  ] satisfies ColumnDef<MasterDataListItem>[])
                : []),
            ...(canReadSkus
                ? ([
                      {
                          id: "skuNames",
                          header: "SKU 名称",
                          enableSorting: false,
                          meta: { label: "SKU 名称" },
                          cell: ({ row }) => {
                              const skus =
                                  productSkusByProduct.get(
                                      row.original.stableId,
                                  ) ?? []
                              const names = skus
                                  .map((sku) => sku.skuName.trim())
                                  .filter(Boolean)
                              return (
                                  <TableAsyncCell
                                      pending={productSkusPending}
                                      failed={productSkusError}
                                  >
                                      <TableTextCell
                                          value={names.join("、")}
                                          clamp={2}
                                      />
                                  </TableAsyncCell>
                              )
                          },
                      },
                  ] satisfies ColumnDef<MasterDataListItem>[])
                : []),
            ...(canReadSkus
                ? ([
                      {
                          id: "skuPriceRange",
                          header: "SKU 售价",
                          enableSorting: false,
                          meta: {
                              label: "SKU 售价",
                              width: "amount",
                              align: "end",
                              numeric: true,
                          },
                          cell: ({ row }) => {
                              const priceRange = productSkuPriceRange(
                                  productSkusByProduct.get(
                                      row.original.stableId,
                                  ) ?? [],
                              )
                              return (
                                  <TableAsyncCell
                                      pending={productSkusPending}
                                      failed={productSkusError}
                                  >
                                      {priceRange === "未填写" ? (
                                          <TableEmptyCell
                                              variant="label"
                                              label="未填写"
                                          />
                                      ) : (
                                          <span className="num font-medium text-foreground">
                                              {priceRange}
                                          </span>
                                      )}
                                  </TableAsyncCell>
                              )
                          },
                      },
                  ] satisfies ColumnDef<MasterDataListItem>[])
                : []),
            {
                id: "skuCount",
                accessorKey: "skuCount",
                header: "SKU 数量",
                enableSorting: true,
                meta: {
                    label: "SKU 数量",
                    width: "quantity",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <QuantityValue
                        value={String(row.original.skuCount ?? 0)}
                        unit="个"
                    />
                ),
            },
            ...(canReadOfferings
                ? ([
                      {
                          id: "supply",
                          header: "供给",
                          enableSorting: false,
                          meta: { label: "供给", width: "status" },
                          cell: ({ row }) => {
                              const item = row.original
                              const productSkus =
                                  productSkusByProduct.get(item.stableId) ?? []
                              const suppliedSkuCount = productSkus.filter(
                                  (sku) => currentSupplySkuIds.has(sku.skuId),
                              ).length
                              const offeringPending =
                                  productSkus.length > 0 &&
                                  supplierOfferingsPending
                              const offeringFailed =
                                  productSkus.length > 0 &&
                                  supplierOfferingsError
                              const pending =
                                  productSkusPending || offeringPending
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
                                          lastFocusedRowId.current =
                                              item.stableId
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
                  ] satisfies ColumnDef<MasterDataListItem>[])
                : []),
            {
                id: "listing",
                header: "上架状态",
                enableSorting: false,
                meta: { label: "上架状态", width: "status" },
                cell: ({ row }) => {
                    const item = row.original
                    const inherited = item.listingStatus ?? "UNLISTED"
                    const pending =
                        productListingPending &&
                        productListingProductId === item.stableId
                    const isPartiallyListed = inherited === "PARTIALLY_LISTED"
                    const statusLabel = pending
                        ? "更新中…"
                        : inherited === "LISTED"
                          ? "已上架"
                          : isPartiallyListed
                            ? "部分上架"
                            : "已下架"
                    const statusTone =
                        inherited === "LISTED"
                            ? "success"
                            : isPartiallyListed
                              ? "warning"
                              : "neutral"
                    const disabledReason = pending
                        ? "正在更新上架状态"
                        : batchListingPending
                          ? "正在批量上架"
                          : item.lifecycleStatus !== "ENABLED" &&
                              inherited === "UNLISTED"
                            ? "请先启用商品"
                            : (item.skuCount ?? 0) === 0
                              ? "请先新增或启用商品规格"
                              : undefined
                    const switchTitle =
                        disabledReason ??
                        (isPartiallyListed
                            ? "部分上架 · 开启可补齐全部启用 SKU 上架"
                            : statusLabel)
                    const switchControl = (
                        <TableSwitchCell
                            id={`master-data-product-${toAutomationIdSegment(item.stableId)}-listing`}
                            canEdit={canUpdateProductListing}
                            checked={inherited === "LISTED"}
                            disabled={disabledReason != null}
                            statusLabel={statusLabel}
                            statusTone={statusTone}
                            accessibleName={`${item.name}整组上架状态：${statusLabel}`}
                            title={switchTitle}
                            onCheckedChange={(checked) => {
                                lastFocusedRowId.current = item.stableId
                                void onUpdateProductListing(item, checked)
                            }}
                        />
                    )
                    const switchWithHint =
                        canUpdateProductListing && disabledReason ? (
                            <span
                                className="inline-flex"
                                title={disabledReason}
                            >
                                {switchControl}
                            </span>
                        ) : (
                            switchControl
                        )
                    if (!canUpdateProductListing || !isPartiallyListed) {
                        return switchWithHint
                    }
                    return (
                        <div className="inline-flex items-center gap-1.5">
                            {switchWithHint}
                            <span
                                className="inline-flex items-center rounded-sm bg-warning-soft px-1.5 py-0.5 text-tiny font-medium text-warning-soft-foreground"
                                title="部分 SKU 已上架，开启开关可补齐全部启用 SKU 上架"
                            >
                                部分
                            </span>
                        </div>
                    )
                },
            },
            revisionTimingColumn(),
            ...blockerColumn(rows),
            ...(onDisableTarget || onAddSupply
                ? [
                      productActionsColumn({
                          lastFocusedRowId,
                          onDisableTarget,
                          onAddSupply,
                          addSupplyDisabledReason: (item) => {
                              if (productSkusPending) return "正在读取商品规格"
                              if (productSkusError)
                                  return "规格读取失败，请重试"
                              const skuCount =
                                  productSkusByProduct.get(item.stableId)
                                      ?.length ?? 0
                              if (skuCount === 0)
                                  return "请先新增或启用商品规格"
                              return undefined
                          },
                      }),
                  ]
                : []),
        ]
    }, [
        batchListingPending,
        canUpdateProductListing,
        canReadOfferings,
        canReadSkus,
        currentSupplySkuIds,
        lastFocusedRowId,
        onAddSupply,
        onDisableTarget,
        onSupplyProduct,
        onSourceDiff,
        onUpdateProductListing,
        productListingPending,
        productListingProductId,
        productSkusByProduct,
        productSkusError,
        productSkusPending,
        rows,
        supplierOfferingsError,
        supplierOfferingsPending,
    ])
}
