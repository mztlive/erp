"use client"

import { TriangleAlertIcon } from "lucide-react"

import { MoneyValue, QuantityValue } from "@/components/business"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { SellableItemThumbnail } from "@/features/master-data/components/list/sellable-item-thumbnail"
import { sellableSupplierLabel } from "@/features/master-data/lib/sellable-excel-rows"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export function SellableGalleryCard({
    row,
    canSelect,
    selectionDisabled,
    selected,
    highlighted,
    onToggle,
    onPreview,
}: {
    row: MasterDataListItem
    canSelect: boolean
    selectionDisabled: boolean
    selected: boolean
    highlighted: boolean
    onToggle: (selected: boolean) => void
    onPreview: () => void
}) {
    const item = row.sellableItem
    const idSegment = toAutomationIdSegment(row.stableId)
    const prefix = `master-data-sellable-items-gallery-card-${idSegment}`
    const spec =
        item && item.specificationLabel !== "无规格"
            ? item.specificationLabel
            : undefined
    const supplierCount = item?.supplierCount ?? 0
    const atRisk = supplierCount <= 1

    return (
        <article
            id={prefix}
            data-row-id={row.stableId}
            tabIndex={-1}
            className="min-w-0 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary"
        >
            <Card
                size="sm"
                className={cn(
                    "h-full gap-0 py-0",
                    selected
                        ? "ring-2 ring-primary"
                        : highlighted
                          ? "ring-2 ring-foreground"
                          : undefined,
                )}
            >
                <div className="relative">
                    {canSelect ? (
                        <div className="absolute top-2 left-2 z-10 rounded-md bg-background/90 p-1">
                            <Checkbox
                                id={`${prefix}-select`}
                                checked={selected}
                                disabled={selectionDisabled}
                                onClick={(event) => event.stopPropagation()}
                                onCheckedChange={(checked) =>
                                    onToggle(checked === true)
                                }
                                aria-label={`选择 ${row.name}（${row.stableNo}）`}
                            />
                        </div>
                    ) : null}
                    <button
                        id={`${prefix}-image`}
                        type="button"
                        aria-label={`预览 ${row.name}（${row.stableNo}）`}
                        className="block w-full rounded-t-lg text-left"
                        onClick={onPreview}
                    >
                        <SellableItemThumbnail
                            assetId={item?.mainImageAssetId}
                            label={row.name}
                            className="aspect-[4/3] rounded-t-lg"
                        />
                    </button>
                </div>
                <CardHeader className="gap-1 px-2.5 pt-2 pb-0">
                    <button
                        id={`${prefix}-preview`}
                        type="button"
                        className="flex w-full min-w-0 flex-col gap-1 text-left"
                        onClick={onPreview}
                    >
                        <div className="flex flex-wrap items-baseline gap-x-1.5 gap-y-0.5">
                            <MoneyValue
                                display="exact"
                                className="text-body-compact font-semibold"
                                value={item?.salesVisiblePriceGross}
                            />
                            <span
                                className="text-xs text-muted-foreground"
                                title="一件代发价、集采价、市场价均为含税销售参考价"
                            >
                                一件代发·含税
                            </span>
                        </div>
                        <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span className="inline-flex min-w-0 items-baseline gap-1">
                                <span>集采</span>
                                <MoneyValue
                                    display="exact"
                                    className="text-xs font-medium"
                                    value={item?.bulkPriceGross}
                                />
                            </span>
                            <span className="inline-flex min-w-0 items-baseline gap-1">
                                <span>市场</span>
                                <MoneyValue
                                    display="exact"
                                    className="text-xs font-medium"
                                    value={item?.marketPrice}
                                />
                            </span>
                        </div>
                        <CardTitle
                            className="line-clamp-2 text-body-compact leading-snug font-semibold tracking-tight"
                            title={row.name}
                        >
                            {row.name}
                        </CardTitle>
                    </button>
                </CardHeader>
                <CardContent className="flex flex-1 flex-col gap-0.5 px-2.5 pt-1 pb-2.5 text-xs text-muted-foreground">
                    {spec ? (
                        <p className="truncate" title={spec}>
                            {spec}
                        </p>
                    ) : null}
                    {item?.deliveryFreightExtra ? (
                        <p className="truncate text-warning-soft-foreground">
                            所选地区部分需另付运费
                        </p>
                    ) : null}
                    <div className="mt-auto flex flex-wrap items-center justify-between gap-x-2 gap-y-0.5 pt-0.5">
                        <span title="集采起订量">
                            集采起订{" "}
                            {item?.bulkMinQuantity ? (
                                <QuantityValue
                                    value={item.bulkMinQuantity}
                                    unit={item.baseUnit}
                                />
                            ) : (
                                "—"
                            )}
                        </span>
                        <span
                            className={cn(
                                "inline-flex items-center gap-1",
                                atRisk
                                    ? "text-warning-soft-foreground"
                                    : undefined,
                            )}
                        >
                            {atRisk ? (
                                <TriangleAlertIcon
                                    className="size-3 shrink-0"
                                    aria-hidden="true"
                                />
                            ) : null}
                            {sellableSupplierLabel(supplierCount)}
                        </span>
                    </div>
                </CardContent>
            </Card>
        </article>
    )
}
