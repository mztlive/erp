"use client"

import * as React from "react"
import type { CellContext, ColumnDef } from "@tanstack/react-table"
import {
    DataTable,
    DetailInlineTable,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { StatusBadge } from "@/components/ui/status-badge"
import { compareDecimal } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DisplayItem } from "../types"
import { PriceAdjustment } from "./price-comparison"
import {
    InlinePriceEditor,
    type InlinePriceEditorContext,
} from "./inline-price-editor"
import { SnapshotImage } from "./snapshot-image"

export type PreviewTableProps = {
    items: readonly DisplayItem[]
    priceEditor?: InlinePriceEditorContext
    pending: boolean
    savingSkuIds?: ReadonlySet<string>
    deletingItemId?: string
    onEditPrice?: (item: DisplayItem) => void
    onDelete?: (item: DisplayItem) => void
    onRegenerateTier?: (tierId: string, tierName: string) => void
    regeneratingTierIds?: readonly string[]
}

const PriceCellContext = React.createContext<{
    pending: boolean
    savingSkuIds?: ReadonlySet<string>
    priceEditor?: InlinePriceEditorContext
}>({ pending: false })

function CustomerPriceCell({ row }: CellContext<DisplayItem, unknown>) {
    const { pending, savingSkuIds, priceEditor } =
        React.useContext(PriceCellContext)
    const item = row.original
    const products =
        item.kind === "PACKAGE"
            ? item.members
            : item.sku_id
              ? [
                    {
                        sku_id: item.sku_id,
                        name: item.name,
                        price: item.price,
                    },
                ]
              : []
    return priceEditor && products.length ? (
        <div className="grid gap-2">
            {item.kind === "PACKAGE" ? (
                <MoneyValue value={item.price} display="exact" />
            ) : null}
            {products.map((product) => (
                <div key={product.sku_id}>
                    {item.kind === "PACKAGE" ? (
                        <p className="mb-1 text-xs text-muted-foreground">
                            {product.name}
                        </p>
                    ) : null}
                    <InlinePriceEditor
                        id={`selection-table-${toAutomationIdSegment(item.item_id)}-${toAutomationIdSegment(product.sku_id)}-price-input`}
                        skuId={product.sku_id}
                        name={product.name}
                        price={product.price}
                        disabled={pending}
                        saving={savingSkuIds?.has(product.sku_id) ?? false}
                        context={priceEditor}
                    />
                </div>
            ))}
        </div>
    ) : (
        <MoneyValue value={item.price} display="exact" />
    )
}

export function PreviewTable({
    items,
    priceEditor,
    pending,
    savingSkuIds,
    deletingItemId,
    onEditPrice,
    onDelete,
    onRegenerateTier,
    regeneratingTierIds,
}: PreviewTableProps) {
    const columns = React.useMemo<ColumnDef<DisplayItem>[]>(
        () => [
            {
                id: "product",
                accessorKey: "name",
                header: "商品 / 套餐",
                meta: { label: "商品 / 套餐", width: "flex" },
                cell: ({ row }) => (
                    <div className="flex min-w-0 items-center gap-3">
                        <div className="size-12 shrink-0 overflow-hidden rounded-md bg-muted">
                            <SnapshotImage
                                path={row.original.cover_image}
                                alt={row.original.name}
                            />
                        </div>
                        <div className="min-w-0 space-y-1 whitespace-normal">
                            <p className="line-clamp-2 font-medium">
                                {row.original.name}
                            </p>
                            <p className="line-clamp-2 text-xs text-muted-foreground">
                                {row.original.spec_label ||
                                    (row.original.kind === "PACKAGE" ? (
                                        <>
                                            <span className="num">
                                                {row.original.members.length}
                                            </span>{" "}
                                            件商品
                                        </>
                                    ) : (
                                        "—"
                                    ))}
                            </p>
                            {row.original.missing_image ? (
                                <StatusBadge tone="warning" label="缺图" />
                            ) : null}
                        </div>
                    </div>
                ),
            },
            {
                id: "tier",
                accessorKey: "tier_name",
                header: "档位",
                meta: { label: "档位", width: "reference" },
                cell: ({ row }) => row.original.tier_name || "—",
            },
            {
                id: "unit",
                accessorKey: "unit",
                header: "单位",
                meta: { label: "单位", width: "quantity" },
                cell: ({ row }) =>
                    row.original.kind === "PACKAGE"
                        ? "套"
                        : row.original.unit || "—",
            },
            ...(
                [
                    ["bulk_price_gross", "集采价"],
                    ["dropship_price_gross", "一件代发价"],
                    ["market_price", "市场价"],
                ] as const
            ).map(([field, label]): ColumnDef<DisplayItem> => ({
                id: field,
                accessorFn: (item) =>
                    item.reference_prices?.[field] ??
                    (field === "bulk_price_gross" ? item.original_price : null),
                header: label,
                meta: { label, width: "amount", numeric: true, align: "end" },
                sortingFn: (a, b) =>
                    compareDecimal(
                        a.getValue<string | null>(field) ?? "0",
                        b.getValue<string | null>(field) ?? "0",
                        2,
                    ),
                cell: ({ getValue }) => {
                    const price = getValue<string | null>()
                    return price != null ? (
                        <MoneyValue value={price} display="exact" />
                    ) : (
                        <span className="text-muted-foreground">未记录</span>
                    )
                },
            })),
            {
                id: "price",
                accessorKey: "price",
                header: "客户售价",
                meta: {
                    label: "客户售价",
                    width: "amount",
                    numeric: true,
                    align: "end",
                },
                sortingFn: (a, b) =>
                    compareDecimal(a.original.price, b.original.price, 2),
                cell: CustomerPriceCell,
            },
            {
                id: "markup_percent",
                accessorKey: "markup_percent",
                header: "加价比例",
                meta: {
                    label: "加价比例",
                    width: "rate",
                    numeric: true,
                    align: "end",
                },
                sortingFn: (a, b) =>
                    compareDecimal(
                        a.original.markup_percent ?? "0",
                        b.original.markup_percent ?? "0",
                        2,
                    ),
                cell: ({ row }) => (
                    <PriceAdjustment
                        markup_percent={row.original.markup_percent}
                    />
                ),
            },
            ...(onEditPrice || onDelete || onRegenerateTier
                ? [
                      {
                          id: "actions",
                          header: "操作",
                          enableHiding: false,
                          enableSorting: false,
                          meta: { label: "操作", width: "reference" as const },
                          cell: ({
                              row,
                          }: {
                              row: { original: DisplayItem }
                          }) => {
                              const item = row.original
                              const key = toAutomationIdSegment(item.item_id)
                              return (
                                  <div className="flex flex-wrap items-center gap-1">
                                      {onEditPrice && !priceEditor ? (
                                          <Button
                                              id={`selection-table-${key}-price`}
                                              variant="ghost"
                                              size="xs"
                                              disabled={pending}
                                              onClick={() => onEditPrice(item)}
                                          >
                                              编辑售价
                                          </Button>
                                      ) : null}
                                      {onDelete ? (
                                          <LoadingButton
                                              id={`selection-table-${key}-delete`}
                                              variant="ghost"
                                              size="xs"
                                              className={
                                                  detailWorkspaceStyles.quietDangerButton
                                              }
                                              disabled={pending}
                                              loading={
                                                  deletingItemId ===
                                                  item.item_id
                                              }
                                              onClick={() => onDelete(item)}
                                          >
                                              删除
                                          </LoadingButton>
                                      ) : null}
                                      {onRegenerateTier && item.tier_id ? (
                                          <LoadingButton
                                              id={`selection-table-${key}-regenerate`}
                                              variant="ghost"
                                              size="xs"
                                              className={
                                                  detailWorkspaceStyles.quietButton
                                              }
                                              disabled={pending}
                                              loading={regeneratingTierIds?.includes(
                                                  item.tier_id,
                                              )}
                                              onClick={() =>
                                                  onRegenerateTier(
                                                      item.tier_id!,
                                                      item.tier_name ??
                                                          item.tier_id!,
                                                  )
                                              }
                                          >
                                              重新生成此档
                                          </LoadingButton>
                                      ) : null}
                                  </div>
                              )
                          },
                      },
                  ]
                : []),
        ],
        [
            priceEditor,
            pending,
            deletingItemId,
            onEditPrice,
            onDelete,
            onRegenerateTier,
            regeneratingTierIds,
        ],
    )

    return (
        <PriceCellContext.Provider
            value={{ pending, savingSkuIds, priceEditor }}
        >
            <DetailInlineTable frame="none" className="space-y-2">
                <p className="text-xs text-muted-foreground">
                    价格均含税
                    {priceEditor
                        ? "，修改客户售价后按回车或离开输入框保存。"
                        : ""}
                </p>
                <DataTable
                    id="selection-preview-table"
                    className="min-w-0 [&_[data-slot=data-table-surface]]:rounded-none [&_[data-slot=data-table-surface]]:border-x-0"
                    data={[...items]}
                    columns={columns}
                    getRowId={(item) => item.item_id}
                    rowLabel={(item) => item.name}
                    rowCount={items.length}
                    caption="选品册商品与客户售价对比（价格均含税）"
                    defaultColumnPinning={{ left: ["product"] }}
                    manualSorting={false}
                    manualPagination={false}
                    manualFiltering={false}
                    showToolbarSummary={false}
                    defaultColumnVisibility={{
                        tier: items.some((item) => item.kind === "PACKAGE"),
                    }}
                    emptyTitle="当前筛选无结果"
                    emptyDescription="调整档位或缺图筛选后再试。"
                />
            </DetailInlineTable>
        </PriceCellContext.Provider>
    )
}
