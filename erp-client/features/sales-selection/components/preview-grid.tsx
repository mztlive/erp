/**
 * 陈列预览网格：销售发布前核对陈列项。
 * 单品平铺，套餐按档分组；封面只读后端字段，不拼图。
 */

"use client"

import * as React from "react"
import {
    AlertCircleIcon,
    PackageIcon,
    RefreshCwIcon,
    Trash2Icon,
    TableIcon,
    LayoutGridIcon,
} from "lucide-react"

import { SnapshotImage } from "./snapshot-image"
import { PriceAdjustment, PriceComparison } from "./price-comparison"
import type { UpdateSelectionPricesInput } from "../api/books"
import { PreviewTable } from "./preview-table"
import {
    BusinessEmptyState,
    BusinessTableFrame,
    DetailWorkspaceSubTabs,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DisplayItem } from "@/features/sales-selection/types"

const previewGridClassName =
    "grid grid-cols-[repeat(auto-fill,minmax(0,min(100%,16rem)))] gap-3.5"

/**
 * 单张陈列卡片，视觉对齐公司商品池画廊卡。
 */
const PreviewCard = ({
    item,
    onDelete,
    onEditPrice,
    canDelete,
    pending,
    deleting,
}: {
    item: DisplayItem
    onDelete?: (item: DisplayItem) => void
    onEditPrice?: (item: DisplayItem) => void
    canDelete: boolean
    pending: boolean
    deleting: boolean
}) => {
    const cardKey = toAutomationIdSegment(item.item_id)
    const members = item.kind === "PACKAGE" ? (item.members ?? []) : []
    const extraMembers = Math.max(0, members.length - 3)
    const unit = item.kind === "PACKAGE" ? "套" : item.unit

    return (
        <article className="min-w-0">
            <Card
                size="sm"
                className="group relative flex h-full flex-col justify-between gap-0 overflow-hidden border border-border/70 bg-card p-0 transition-all hover:border-border hover:shadow-xs"
            >
                <div>
                    <div className="relative aspect-square w-full overflow-hidden bg-muted/60">
                        {item.cover_image ? (
                            <SnapshotImage
                                path={item.cover_image}
                                alt={item.name}
                            />
                        ) : (
                            <div className="flex h-full flex-col items-center justify-center gap-1.5 text-muted-foreground">
                                <PackageIcon
                                    className="size-8 stroke-[1.5] text-muted-foreground/60"
                                    aria-hidden="true"
                                />
                                <span className="text-xs">暂无图片</span>
                            </div>
                        )}
                        {item.missing_image ? (
                            <div className="absolute top-2 left-2 shadow-xs">
                                <StatusBadge tone="warning" label="缺图" />
                            </div>
                        ) : null}
                    </div>

                    <CardHeader className="gap-1.5 p-3 pb-0">
                        <div className="flex flex-wrap items-center gap-1.5">
                            <MoneyValue
                                display="exact"
                                className="text-body-compact font-bold text-foreground"
                                value={item.price_gross || item.price}
                            />
                            <span className="text-xs text-muted-foreground">
                                含税
                            </span>
                            {item.markup_percent != null ? (
                                <PriceAdjustment
                                    markup_percent={item.markup_percent}
                                />
                            ) : null}
                        </div>
                        <PriceComparison
                            reference_prices={item.reference_prices}
                            original_price={item.original_price}
                            showAdjustment={false}
                        />
                        <div className="flex items-start justify-between gap-2">
                            <CardTitle
                                className="min-w-0 flex-1 line-clamp-2 text-xs leading-snug font-medium text-foreground"
                                title={item.name}
                            >
                                {item.name}
                            </CardTitle>
                            {unit ? (
                                <span className="shrink-0 text-xs text-muted-foreground">
                                    单位：{unit}
                                </span>
                            ) : null}
                        </div>
                    </CardHeader>

                    <CardContent className="flex flex-col gap-1 p-3 pt-1.5 text-xs text-muted-foreground">
                        {item.spec_label ? (
                            <p
                                className="truncate text-xs"
                                title={item.spec_label}
                            >
                                {item.spec_label}
                            </p>
                        ) : null}
                        {members.length > 0 ? (
                            <ul className="flex flex-col gap-0.5 border-t border-border/50 pt-1.5 text-xs">
                                {members.slice(0, 3).map((member) => (
                                    <li
                                        key={member.sku_id}
                                        className="truncate"
                                        title={`${member.name} · ${member.unit}`}
                                    >
                                        · {member.name} ({member.unit})
                                    </li>
                                ))}
                                {extraMembers > 0 ? (
                                    <li className="num pt-0.5 text-xs font-medium text-muted-foreground">
                                        另有 {extraMembers} 件商品
                                    </li>
                                ) : null}
                            </ul>
                        ) : null}
                    </CardContent>
                </div>

                {onEditPrice || (canDelete && onDelete) ? (
                    <div className="flex flex-wrap gap-1 border-t border-border/50 p-2 pt-1.5">
                        {onEditPrice ? (
                            <Button
                                id={`sales-selection-preview-${cardKey}-price`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                className="flex-1"
                                disabled={pending}
                                onClick={() => onEditPrice(item)}
                            >
                                {item.kind === "PACKAGE"
                                    ? "编辑商品售价"
                                    : "编辑售价"}
                            </Button>
                        ) : null}
                        {canDelete && onDelete ? (
                            <LoadingButton
                                id={`sales-selection-preview-${cardKey}-delete`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                className={`flex-1 justify-center ${detailWorkspaceStyles.quietDangerButton}`}
                                disabled={pending}
                                loading={deleting}
                                onClick={() => onDelete(item)}
                            >
                                <Trash2Icon
                                    className="size-3.5"
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                删除该项
                            </LoadingButton>
                        ) : null}
                    </div>
                ) : null}
            </Card>
        </article>
    )
}

export type PreviewTierConfig = {
    tier_id: string
    name: string
    target_amount: string
    tolerance: string
    sku_count: number
}

/**
 * 陈列预览网格。
 * @param items 后端陈列结果
 * @param selectionForm 选品形态（套餐按档分组）
 * @param tiers 档位设定明细
 * @param onRegenerateTier 待发布态单档重生成回调
 * @param onDelete 待发布态删减回调
 */
export const PreviewGrid = ({
    items,
    selectionForm,
    tiers,
    onRegenerateTier,
    onDelete,
    onEditPrice,
    priceEditor,
    pending = false,
    savingSkuIds,
    deletingItemId,
    regeneratingTierIds,
    emptyState,
    summary,
    actions,
}: {
    items: readonly DisplayItem[]
    selectionForm: "SINGLE_SKU" | "PACKAGE"
    tiers?: readonly PreviewTierConfig[]
    onRegenerateTier?: (tierId: string, tierName: string) => void
    onDelete?: (item: DisplayItem) => void
    priceEditor?: {
        batchId: string
        version: number
        onSubmit: (input: UpdateSelectionPricesInput) => Promise<unknown>
    }
    onEditPrice?: (item: DisplayItem) => void
    pending?: boolean
    savingSkuIds?: ReadonlySet<string>
    deletingItemId?: string
    regeneratingTierIds?: readonly string[]
    emptyState?: { title: string; description: string }
    summary?: React.ReactNode
    actions?: React.ReactNode
}) => {
    const canDelete = typeof onDelete === "function"
    const [tierFilter, setTierFilter] = React.useState<string>("all")
    const [onlyMissingImage, setOnlyMissingImage] = React.useState(false)
    const [layout, setLayout] = React.useState<"gallery" | "table">("gallery")

    // 过滤缺图项
    const filteredItems = React.useMemo(() => {
        if (!onlyMissingImage) return items
        return items.filter((item) => item.missing_image)
    }, [items, onlyMissingImage])

    const missingImageCount = React.useMemo(
        () => items.filter((item) => item.missing_image).length,
        [items],
    )

    const groups = React.useMemo(() => {
        if (selectionForm !== "PACKAGE") return null
        const map = new Map<string, DisplayItem[]>()
        for (const item of filteredItems) {
            const key = item.tier_name ?? item.tier_id ?? "未分组"
            const bucket = map.get(key) ?? []
            bucket.push(item)
            map.set(key, bucket)
        }
        return [...map.entries()]
    }, [filteredItems, selectionForm])

    const tierNames = React.useMemo(() => {
        if (selectionForm !== "PACKAGE") return []
        const names = new Set<string>()
        for (const item of items) {
            const key = item.tier_name ?? item.tier_id ?? "未分组"
            names.add(key)
        }
        return Array.from(names)
    }, [items, selectionForm])

    if (items.length === 0) {
        return (
            <div className="space-y-2">
                {summary}
                <BusinessEmptyState
                    kind="no-data"
                    title={emptyState?.title ?? "暂无可预览陈列"}
                    description={
                        emptyState?.description ??
                        "开始准备后，系统生成的商品陈列将显示在这里。"
                    }
                />
            </div>
        )
    }

    // 过滤特定档位
    const visibleGroups = groups
        ? tierFilter === "all"
            ? groups
            : groups.filter(([tierName]) => tierName === tierFilter)
        : null

    const tableItems = visibleGroups
        ? visibleGroups.flatMap(([, group]) => group)
        : filteredItems

    const viewToggle = (
        <DetailWorkspaceSubTabs
            ariaLabel="陈列视图"
            idPrefix="selection-preview-layout"
            className="pt-0"
            value={layout}
            onValueChange={(next) =>
                setLayout(next === "table" ? "table" : "gallery")
            }
            items={[
                {
                    id: "table",
                    label: (
                        <span className="inline-flex items-center gap-1.5">
                            <TableIcon
                                className="size-3.5"
                                aria-hidden="true"
                            />
                            表格
                        </span>
                    ),
                },
                {
                    id: "gallery",
                    label: (
                        <span className="inline-flex items-center gap-1.5">
                            <LayoutGridIcon
                                className="size-3.5"
                                aria-hidden="true"
                            />
                            卡片
                        </span>
                    ),
                },
            ]}
        />
    )

    const tierTabItems =
        selectionForm === "PACKAGE" && tierNames.length > 1
            ? [
                  {
                      id: "all",
                      label: (
                          <span>
                              全部档位{" "}
                              <span className="num">({items.length})</span>
                          </span>
                      ),
                  },
                  ...tierNames.map((name) => {
                      const count = items.filter(
                          (i) => (i.tier_name ?? i.tier_id) === name,
                      ).length
                      return {
                          id: name,
                          label: (
                              <span>
                                  {name} <span className="num">({count})</span>
                              </span>
                          ),
                      }
                  }),
              ]
            : []

    return (
        <BusinessTableFrame
            title="陈列明细"
            headingLevel="h3"
            className="gap-2"
            tableActions={
                <>
                    {actions}
                    {viewToggle}
                </>
            }
            selectionBar={
                <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5">
                    {summary}
                    {!summary || tableItems.length !== items.length ? (
                        <span className="text-xs text-muted-foreground">
                            共 <span className="num">{tableItems.length}</span>{" "}
                            项
                            {tableItems.length !== items.length ? (
                                <>
                                    {" "}
                                    / 全部{" "}
                                    <span className="num">
                                        {items.length}
                                    </span>{" "}
                                    项
                                </>
                            ) : null}
                        </span>
                    ) : null}
                </div>
            }
            toolbar={
                tierTabItems.length > 0 || missingImageCount > 0 ? (
                    <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/60 pb-3">
                        <div className="min-w-0">
                            {tierTabItems.length > 0 ? (
                                <DetailWorkspaceSubTabs
                                    ariaLabel="按档位筛选"
                                    idPrefix="selection-preview-tier"
                                    className="pt-0"
                                    value={tierFilter}
                                    onValueChange={setTierFilter}
                                    items={tierTabItems}
                                />
                            ) : null}
                        </div>

                        {/* 缺图项快捷筛选 */}
                        {missingImageCount > 0 ? (
                            <Button
                                id="selection-preview-missing-images"
                                type="button"
                                size="xs"
                                variant={
                                    onlyMissingImage ? "destructive" : "outline"
                                }
                                onClick={() =>
                                    setOnlyMissingImage(!onlyMissingImage)
                                }
                                className="h-7 text-xs"
                            >
                                <AlertCircleIcon
                                    className="mr-1 size-3.5"
                                    aria-hidden="true"
                                />
                                {onlyMissingImage ? (
                                    "显示全部"
                                ) : (
                                    <span>
                                        仅看缺图{" "}
                                        <span className="num">
                                            ({missingImageCount})
                                        </span>
                                    </span>
                                )}
                            </Button>
                        ) : null}
                    </div>
                ) : undefined
            }
            table={
                layout === "table" ? (
                    <PreviewTable
                        priceEditor={priceEditor}
                        items={tableItems}
                        pending={pending}
                        savingSkuIds={savingSkuIds}
                        deletingItemId={deletingItemId}
                        onDelete={onDelete}
                        onEditPrice={onEditPrice}
                        onRegenerateTier={onRegenerateTier}
                        regeneratingTierIds={regeneratingTierIds}
                    />
                ) : (
                    <>
                        {/* 单品形态平铺 */}
                        {!visibleGroups ? (
                            filteredItems.length === 0 ? (
                                <BusinessEmptyState
                                    kind="no-data"
                                    title="当前筛选无结果"
                                    description="没有符合条件的陈列项。"
                                />
                            ) : (
                                <div className={previewGridClassName}>
                                    {filteredItems.map((item) => (
                                        <PreviewCard
                                            key={item.item_id}
                                            item={item}
                                            onDelete={onDelete}
                                            onEditPrice={onEditPrice}
                                            canDelete={canDelete}
                                            pending={pending}
                                            deleting={
                                                deletingItemId === item.item_id
                                            }
                                        />
                                    ))}
                                </div>
                            )
                        ) : null}

                        {/* 套餐形态按档分组 */}
                        {visibleGroups ? (
                            visibleGroups.length === 0 ? (
                                <BusinessEmptyState
                                    kind="no-data"
                                    title="当前筛选无结果"
                                    description="该档位下暂无可显示的陈列项。"
                                />
                            ) : (
                                <div className="flex flex-col gap-8">
                                    {visibleGroups.map(
                                        ([tierName, tierItems]) => {
                                            const tierConfig = tiers?.find(
                                                (t) =>
                                                    t.name === tierName ||
                                                    t.tier_id === tierName,
                                            )
                                            return (
                                                <section
                                                    key={tierName}
                                                    aria-label={tierName}
                                                    className="flex flex-col gap-3.5"
                                                >
                                                    <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/60 pb-2">
                                                        <div className="flex flex-wrap items-baseline gap-2.5">
                                                            <h3 className="font-semibold text-foreground">
                                                                {tierName}
                                                            </h3>
                                                            <span className="num text-xs text-muted-foreground">
                                                                {
                                                                    tierItems.length
                                                                }{" "}
                                                                套
                                                            </span>
                                                            {tierConfig ? (
                                                                <span className="num text-xs text-muted-foreground">
                                                                    · 目标 ¥
                                                                    {
                                                                        tierConfig.target_amount
                                                                    }{" "}
                                                                    (±¥
                                                                    {
                                                                        tierConfig.tolerance
                                                                    }
                                                                    ) · 每套{" "}
                                                                    {
                                                                        tierConfig.sku_count
                                                                    }{" "}
                                                                    件
                                                                </span>
                                                            ) : null}
                                                        </div>

                                                        {onRegenerateTier &&
                                                        tierConfig ? (
                                                            <LoadingButton
                                                                id={`selection-regenerate-inline-${tierConfig.tier_id}`}
                                                                type="button"
                                                                size="xs"
                                                                variant="ghost"
                                                                className={
                                                                    detailWorkspaceStyles.subtleButton
                                                                }
                                                                disabled={
                                                                    pending
                                                                }
                                                                loading={regeneratingTierIds?.includes(
                                                                    tierConfig.tier_id,
                                                                )}
                                                                onClick={() =>
                                                                    onRegenerateTier(
                                                                        tierConfig.tier_id,
                                                                        tierConfig.name,
                                                                    )
                                                                }
                                                            >
                                                                <RefreshCwIcon
                                                                    className="mr-1 size-3"
                                                                    aria-hidden="true"
                                                                />
                                                                重新生成此档
                                                            </LoadingButton>
                                                        ) : null}
                                                    </div>

                                                    <div
                                                        className={
                                                            previewGridClassName
                                                        }
                                                    >
                                                        {tierItems.map(
                                                            (item) => (
                                                                <PreviewCard
                                                                    key={
                                                                        item.item_id
                                                                    }
                                                                    item={item}
                                                                    onDelete={
                                                                        onDelete
                                                                    }
                                                                    onEditPrice={
                                                                        onEditPrice
                                                                    }
                                                                    canDelete={
                                                                        canDelete
                                                                    }
                                                                    pending={
                                                                        pending
                                                                    }
                                                                    deleting={
                                                                        deletingItemId ===
                                                                        item.item_id
                                                                    }
                                                                />
                                                            ),
                                                        )}
                                                    </div>
                                                </section>
                                            )
                                        },
                                    )}
                                </div>
                            )
                        ) : null}
                    </>
                )
            }
        />
    )
}
