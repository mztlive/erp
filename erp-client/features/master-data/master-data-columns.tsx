"use client"

import * as React from "react"
import { BanIcon, HistoryIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { masterDataCopy } from "@/features/master-data/copy"
import { formatEffectiveRange } from "@/features/master-data/filter"
import {
    DisabledActionHint,
    productSkuPriceRange,
} from "@/features/master-data/master-data-list-presentation"
import type {
    MasterDataListItem,
    MasterDataResource,
    ProductListSkuSummary,
} from "@/features/master-data/types"

type MasterDataColumnsInput = {
    isProductResource: boolean
    isSupplierResource: boolean
    isBrandResource: boolean
    isUnitOfMeasureResource: boolean
    isVoucherCategoryResource: boolean
    isSellableResource: boolean
    canUpdateProductListing: boolean
    currentSupplySkuIds: ReadonlySet<string>
    lastFocusedRowId: React.MutableRefObject<string | null>
    productSkusByProduct: ReadonlyMap<string, readonly ProductListSkuSummary[]>
    productSkusPending: boolean
    productSkusError: boolean
    productListingPending: boolean
    productListingProductId: string | undefined
    resource: MasterDataResource
    rows: readonly MasterDataListItem[]
    showEffectiveColumn: boolean
    supplierOfferingsPending: boolean
    supplierOfferingsError: boolean
    onUpdateProductListing: (
        item: MasterDataListItem,
        listed: boolean,
    ) => Promise<void>
    onSupplyProduct: (item: MasterDataListItem) => void
    onReviseTarget: (item: MasterDataListItem) => void
    onDisableTarget: (item: MasterDataListItem) => void
    onPreview: (stableId: string) => void
    onNavigate: (href: string) => void
}

function useMasterDataColumns({
    isProductResource,
    isSupplierResource,
    isBrandResource,
    isUnitOfMeasureResource,
    isVoucherCategoryResource,
    isSellableResource,
    canUpdateProductListing,
    currentSupplySkuIds,
    lastFocusedRowId,
    productSkusByProduct,
    productSkusPending,
    productSkusError,
    productListingPending,
    productListingProductId,
    resource,
    rows,
    showEffectiveColumn,
    supplierOfferingsPending,
    supplierOfferingsError,
    onUpdateProductListing,
    onSupplyProduct,
    onReviseTarget,
    onDisableTarget,
    onPreview,
    onNavigate,
}: MasterDataColumnsInput) {
    const columns = React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            ...(!isSellableResource && !isSupplierResource
                ? [
                      {
                          id: "stableNo",
                          accessorKey: "stableNo",
                          header: masterDataCopy.colStableNo,
                          meta: {
                              label: masterDataCopy.colStableNo,
                              width: "default" as const,
                          },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => (
                              <span className="num text-sm">
                                  {row.original.stableNo}
                              </span>
                          ),
                      } satisfies ColumnDef<MasterDataListItem>,
                  ]
                : []),
            {
                id: "name",
                accessorKey: "name",
                header: isSellableResource
                    ? "商品名称 · 规格"
                    : masterDataCopy.colName,
                meta: {
                    label: isSellableResource
                        ? "商品名称 · 规格"
                        : masterDataCopy.colName,
                },
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
                            {sellable ? (
                                <div className="truncate text-xs text-muted-foreground">
                                    SKU 编号：
                                    <span className="num">
                                        {row.original.stableNo}
                                    </span>
                                </div>
                            ) : row.original.keyFacts[0] ? (
                                <div className="truncate text-xs text-muted-foreground">
                                    {row.original.keyFacts[0].label}：
                                    {row.original.keyFacts[0].value}
                                </div>
                            ) : null}
                        </div>
                    )
                },
            },
            ...(isSellableResource
                ? [
                      {
                          id: "productNo",
                          header: "SPU 编号",
                          meta: {
                              label: "SPU 编号",
                              width: "default" as const,
                          },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => (
                              <span className="num text-sm">
                                  {row.original.sellableItem?.productNo ?? "—"}
                              </span>
                          ),
                      } satisfies ColumnDef<MasterDataListItem>,
                      {
                          id: "price",
                          header: "销售价",
                          meta: { label: "销售价", width: "amount" as const },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => (
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
                      } satisfies ColumnDef<MasterDataListItem>,
                      {
                          id: "marketPrice",
                          header: "市场价",
                          meta: { label: "市场价", width: "amount" as const },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => {
                              const marketPrice =
                                  row.original.sellableItem?.marketPrice
                              if (!marketPrice) {
                                  return (
                                      <span className="text-sm text-muted-foreground">
                                          —
                                      </span>
                                  )
                              }
                              return <MoneyValue value={marketPrice} />
                          },
                      } satisfies ColumnDef<MasterDataListItem>,
                      {
                          id: "supplyRegions",
                          header: "可供区域",
                          meta: { label: "可供区域" },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => {
                              const regions =
                                  row.original.sellableItem?.supplyRegions ?? []
                              const label =
                                  regions.length > 0
                                      ? regions.join("、")
                                      : "未标注"
                              return (
                                  <span
                                      className="line-clamp-2 max-w-64 text-sm"
                                      title={label}
                                  >
                                      {label}
                                  </span>
                              )
                          },
                      } satisfies ColumnDef<MasterDataListItem>,
                      {
                          id: "supplierCount",
                          header: "有效供应商",
                          meta: {
                              label: "有效供应商",
                              width: "status" as const,
                          },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => (
                              <Badge variant="outline">
                                  <span className="num">
                                      {row.original.sellableItem
                                          ?.supplierCount ?? 0}
                                  </span>{" "}
                                  家
                              </Badge>
                          ),
                      } satisfies ColumnDef<MasterDataListItem>,
                  ]
                : [
                      {
                          id: "revisionNo",
                          header: masterDataCopy.colVersion,
                          meta: {
                              label: masterDataCopy.colVersion,
                              width: "amount" as const,
                          },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => (
                              <span className="num text-sm">
                                  v{row.original.revisionNo}
                              </span>
                          ),
                      } satisfies ColumnDef<MasterDataListItem>,
                      {
                          id: "lifecycle",
                          header: masterDataCopy.colLifecycle,
                          meta: { label: masterDataCopy.colLifecycle },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => (
                              <div className="flex flex-col gap-1">
                                  <BusinessStatusBadge
                                      context="list"
                                      label={row.original.lifecycleStatusLabel}
                                      tone={row.original.lifecycleTone}
                                  />
                                  {row.original.scheduledLifecycleLabel ? (
                                      <span className="text-tiny text-muted-foreground">
                                          {row.original.scheduledLifecycleLabel}
                                      </span>
                                  ) : null}
                              </div>
                          ),
                      } satisfies ColumnDef<MasterDataListItem>,
                  ]),
            ...(isProductResource
                ? [
                      {
                          id: "skuPriceRange",
                          header: "SKU 售价",
                          meta: { label: "SKU 售价", width: "amount" as const },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => (
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
                      } satisfies ColumnDef<MasterDataListItem>,
                      {
                          id: "skuCount",
                          header: "SKU 数量",
                          meta: { label: "SKU 数量", width: "amount" as const },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => (
                              <span className="num text-sm">
                                  {row.original.skuCount ?? 0} 个
                              </span>
                          ),
                      } satisfies ColumnDef<MasterDataListItem>,
                      {
                          id: "supply",
                          header: "供给",
                          meta: { label: "供给", width: "status" as const },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => {
                              const item = row.original
                              const productSkus =
                                  productSkusByProduct.get(item.stableId) ?? []
                              const suppliedSkuCount = productSkus.filter(
                                  (sku) => currentSupplySkuIds.has(sku.skuId),
                              ).length
                              const skuDataPending = productSkusPending
                              const skuDataFailed = productSkusError
                              const offeringPending =
                                  productSkus.length > 0 &&
                                  supplierOfferingsPending
                              const offeringFailed =
                                  productSkus.length > 0 &&
                                  supplierOfferingsError
                              const statusLabel = skuDataPending
                                  ? "读取中…"
                                  : skuDataFailed || offeringFailed
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
                                          lastFocusedRowId.current =
                                              item.stableId
                                          onSupplyProduct(item)
                                      }}
                                  >
                                      <Badge
                                          variant={
                                              suppliedSkuCount > 0 &&
                                              !skuDataPending &&
                                              !offeringPending &&
                                              !skuDataFailed &&
                                              !offeringFailed
                                                  ? "success"
                                                  : "outline"
                                          }
                                      >
                                          {offeringPending
                                              ? "读取中…"
                                              : statusLabel}
                                      </Badge>
                                      {!skuDataPending &&
                                      !skuDataFailed &&
                                      !offeringPending &&
                                      !offeringFailed &&
                                      productSkus.length > 0 ? (
                                          <span className="num text-xs text-muted-foreground">
                                              {suppliedSkuCount}/
                                              {productSkus.length} SKU
                                          </span>
                                      ) : null}
                                  </Button>
                              )
                          },
                      } satisfies ColumnDef<MasterDataListItem>,
                      {
                          id: "listing",
                          header: "上架状态",
                          meta: { label: "上架状态" },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => {
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
                                              (item.lifecycleStatus !==
                                                  "ENABLED" &&
                                                  inherited === "UNLISTED") ||
                                              (item.skuCount ?? 0) === 0
                                          }
                                          onCheckedChange={(checked) =>
                                              void onUpdateProductListing(
                                                  item,
                                                  checked,
                                              )
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
                      } satisfies ColumnDef<MasterDataListItem>,
                  ]
                : []),
            ...(!isSellableResource
                ? [
                      {
                          id: "revisionTiming",
                          header: masterDataCopy.colVersionState,
                          meta: { label: masterDataCopy.colVersionState },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => (
                              <Badge
                                  variant={
                                      row.original.revisionTiming === "FUTURE"
                                          ? "warning"
                                          : "secondary"
                                  }
                              >
                                  {row.original.revisionTimingLabel}
                              </Badge>
                          ),
                      } satisfies ColumnDef<MasterDataListItem>,
                  ]
                : []),
            ...(showEffectiveColumn
                ? [
                      {
                          id: "period",
                          header: masterDataCopy.colEffective,
                          meta: {
                              label: masterDataCopy.colEffective,
                          },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) => (
                              <span className="num text-xs">
                                  {formatEffectiveRange(
                                      row.original.effectiveFrom,
                                      row.original.effectiveTo,
                                  )}
                              </span>
                          ),
                      } satisfies ColumnDef<MasterDataListItem>,
                  ]
                : []),
            ...(rows.some((r) => r.primaryBlocker)
                ? [
                      {
                          id: "blocker",
                          header: masterDataCopy.colBlocker,
                          meta: { label: masterDataCopy.colBlocker },
                          cell: ({
                              row,
                          }: {
                              row: { original: MasterDataListItem }
                          }) =>
                              row.original.primaryBlocker ? (
                                  <span className="text-xs text-destructive">
                                      {row.original.primaryBlocker}
                                  </span>
                              ) : (
                                  <span className="text-xs text-muted-foreground">
                                      —
                                  </span>
                              ),
                      } satisfies ColumnDef<MasterDataListItem>,
                  ]
                : []),
            ...(!isSellableResource
                ? [
                      {
                          id: "actions",
                          header: masterDataCopy.colActions,
                          meta: { label: masterDataCopy.colActions },
                          cell: ({ row }) => {
                              const item = row.original
                              const canRevise =
                                  item.allowedActions.includes(
                                      "CREATE_REVISION",
                                  )
                              const canDisable =
                                  item.allowedActions.includes("DISABLE")
                              const reviseBlocker = item.actionBlockers.find(
                                  (b) => b.action === "CREATE_REVISION",
                              )
                              const disableBlocker = item.actionBlockers.find(
                                  (b) => b.action === "DISABLE",
                              )
                              // 卡券类目：仅原地编辑。
                              if (isVoucherCategoryResource) {
                                  return (
                                      <div className="flex flex-wrap gap-1">
                                          <DisabledActionHint
                                              message={reviseBlocker?.message}
                                          >
                                              <Button
                                                  type="button"
                                                  size="xs"
                                                  variant="ghost"
                                                  disabled={!canRevise}
                                                  title={reviseBlocker?.message}
                                                  onClick={(e) => {
                                                      e.stopPropagation()
                                                      lastFocusedRowId.current =
                                                          item.stableId
                                                      onReviseTarget(item)
                                                  }}
                                              >
                                                  <HistoryIcon
                                                      data-icon="inline-start"
                                                      aria-hidden
                                                  />
                                                  {masterDataCopy.actionUpdate}
                                              </Button>
                                          </DisabledActionHint>
                                      </div>
                                  )
                              }
                              // 商品点击行进入详情；品牌 / 计量单位点击行打开更新 Dialog。操作列均仅保留「停用」。
                              if (
                                  isProductResource ||
                                  isBrandResource ||
                                  isUnitOfMeasureResource
                              ) {
                                  return (
                                      <div className="flex flex-wrap gap-1">
                                          <DisabledActionHint
                                              message={disableBlocker?.message}
                                          >
                                              <Button
                                                  type="button"
                                                  size="xs"
                                                  variant="ghost"
                                                  disabled={!canDisable}
                                                  title={
                                                      disableBlocker?.message
                                                  }
                                                  onClick={(e) => {
                                                      e.stopPropagation()
                                                      lastFocusedRowId.current =
                                                          item.stableId
                                                      onDisableTarget(item)
                                                  }}
                                              >
                                                  <BanIcon
                                                      data-icon="inline-start"
                                                      aria-hidden
                                                  />
                                                  {masterDataCopy.actionDisable}
                                              </Button>
                                          </DisabledActionHint>
                                      </div>
                                  )
                              }
                              return (
                                  <div className="flex flex-wrap gap-1">
                                      <Button
                                          type="button"
                                          size="xs"
                                          variant="ghost"
                                          onClick={(e) => {
                                              e.stopPropagation()
                                              lastFocusedRowId.current =
                                                  item.stableId
                                              if (isSupplierResource) {
                                                  onNavigate(
                                                      `/master-data/${resource}/${item.stableId}?section=overview`,
                                                  )
                                              } else {
                                                  onPreview(item.stableId)
                                              }
                                          }}
                                      >
                                          {masterDataCopy.actionView}
                                      </Button>
                                      <DisabledActionHint
                                          message={reviseBlocker?.message}
                                      >
                                          <Button
                                              type="button"
                                              size="xs"
                                              variant="ghost"
                                              disabled={!canRevise}
                                              title={reviseBlocker?.message}
                                              onClick={(e) => {
                                                  e.stopPropagation()
                                                  if (isSupplierResource) {
                                                      // 详情页即编辑，与「查看」同一路由
                                                      onNavigate(
                                                          `/master-data/${resource}/${item.stableId}?section=overview`,
                                                      )
                                                  } else {
                                                      onReviseTarget(item)
                                                  }
                                              }}
                                          >
                                              <HistoryIcon
                                                  data-icon="inline-start"
                                                  aria-hidden
                                              />
                                              {masterDataCopy.actionUpdate}
                                          </Button>
                                      </DisabledActionHint>
                                      <DisabledActionHint
                                          message={disableBlocker?.message}
                                      >
                                          <Button
                                              type="button"
                                              size="xs"
                                              variant="ghost"
                                              disabled={!canDisable}
                                              title={disableBlocker?.message}
                                              onClick={(e) => {
                                                  e.stopPropagation()
                                                  onDisableTarget(item)
                                              }}
                                          >
                                              <BanIcon
                                                  data-icon="inline-start"
                                                  aria-hidden
                                              />
                                              {masterDataCopy.actionDisable}
                                          </Button>
                                      </DisabledActionHint>
                                  </div>
                              )
                          },
                      } satisfies ColumnDef<MasterDataListItem>,
                  ]
                : []),
        ],
        [
            isProductResource,
            isSupplierResource,
            isBrandResource,
            isUnitOfMeasureResource,
            isVoucherCategoryResource,
            isSellableResource,
            canUpdateProductListing,
            currentSupplySkuIds,
            lastFocusedRowId,
            productSkusByProduct,
            productSkusError,
            productSkusPending,
            productListingPending,
            productListingProductId,
            onDisableTarget,
            onPreview,
            onReviseTarget,
            onSupplyProduct,
            resource,
            onNavigate,
            rows,
            showEffectiveColumn,
            supplierOfferingsError,
            supplierOfferingsPending,
            onUpdateProductListing,
        ],
    )

    return columns
}

export { useMasterDataColumns }
