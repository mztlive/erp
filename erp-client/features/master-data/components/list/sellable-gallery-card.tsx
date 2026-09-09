"use client"

import { TriangleAlertIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { SellableItemThumbnail } from "@/features/master-data/components/list/sellable-item-thumbnail"
import { sellableSupplierLabel } from "@/features/master-data/lib/sellable-excel-rows"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

function SupplyRegions({ regions }: { regions: readonly string[] }) {
    if (regions.length === 0) {
        return <span className="text-xs text-muted-foreground">未标注</span>
    }
    const shown = regions.slice(0, 2)
    const rest = regions.length - 2
    return (
        <p className="text-xs text-muted-foreground" title={regions.join("、")}>
            {shown.join("、")}
            {rest > 0 ? <span className="num"> +{rest}</span> : null}
        </p>
    )
}

export function SellableGalleryCard({
    row,
    selected,
    highlighted,
    onToggle,
    onPreview,
}: {
    row: MasterDataListItem
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
            className="mb-3 break-inside-avoid outline-none"
        >
            <Card
                size="sm"
                className={cn(
                    "gap-0 py-0",
                    selected
                        ? "ring-2 ring-primary"
                        : highlighted
                          ? "ring-2 ring-foreground"
                          : undefined,
                )}
            >
                <div className="relative">
                    <div
                        className="absolute top-2 left-2 z-10 rounded-md bg-background/90 p-1"
                        onClick={(event) => event.stopPropagation()}
                        onKeyDown={(event) => event.stopPropagation()}
                    >
                        <Checkbox
                            id={`${prefix}-select`}
                            checked={selected}
                            onCheckedChange={(checked) =>
                                onToggle(checked === true)
                            }
                            aria-label={`选择 ${row.name}`}
                        />
                    </div>
                    <button
                        id={`${prefix}-image`}
                        type="button"
                        className="block w-full rounded-t-xl text-left"
                        onClick={onPreview}
                    >
                        <SellableItemThumbnail
                            assetId={item?.mainImageAssetId}
                            label={row.name}
                            className="rounded-t-xl"
                        />
                    </button>
                </div>
                <CardHeader className="gap-1.5 px-3 pt-3 pb-0">
                    <button
                        id={`${prefix}-preview`}
                        type="button"
                        className="flex flex-col gap-1.5 text-left"
                        onClick={onPreview}
                    >
                        <div className="flex items-baseline gap-1.5">
                            <MoneyValue
                                className="font-semibold [&>span:first-child]:text-lg"
                                value={item?.salesVisiblePriceGross}
                            />
                            <span className="text-xs text-muted-foreground">
                                含税
                            </span>
                        </div>
                        {item?.marketPrice ? (
                            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                                <span>市场参考价</span>
                                <MoneyValue
                                    className="[&>span:first-child]:text-xs [&>span:first-child]:font-normal [&>span:first-child]:text-muted-foreground"
                                    value={item.marketPrice}
                                />
                            </div>
                        ) : (
                            <span className="text-xs text-muted-foreground">
                                市场参考价 —
                            </span>
                        )}
                        <CardTitle
                            className="line-clamp-2 text-sm leading-5 font-medium"
                            title={row.name}
                        >
                            {row.name}
                        </CardTitle>
                    </button>
                </CardHeader>
                <CardContent className="flex flex-col gap-1 px-3 pt-2 pb-3 text-xs text-muted-foreground">
                    {spec ? (
                        <p className="truncate" title={spec}>
                            {spec}
                        </p>
                    ) : null}
                    <p>
                        <span className="num">{row.stableNo}</span>
                        {item?.productNo ? (
                            <>
                                <span aria-hidden="true"> · </span>
                                <span className="num">{item.productNo}</span>
                            </>
                        ) : null}
                    </p>
                    <SupplyRegions regions={item?.supplyRegions ?? []} />
                    <p
                        className={cn(
                            "inline-flex items-center gap-1",
                            atRisk
                                ? "text-warning-soft-foreground"
                                : undefined,
                        )}
                    >
                        {atRisk ? (
                            <TriangleAlertIcon
                                className="size-3.5 shrink-0"
                                aria-hidden="true"
                            />
                        ) : null}
                        {sellableSupplierLabel(supplierCount)}
                    </p>
                </CardContent>
            </Card>
        </article>
    )
}
