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
import { PriceComparison } from "./price-comparison"
import { PreviewTable } from "./preview-table"
import {
    BusinessEmptyState,
    BusinessTableFrame,
    MoneyValue,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DisplayItem } from "@/features/sales-selection/types"
import { cn } from "@/lib/utils"

const previewGridClassName =
    "grid grid-cols-2 gap-3.5 sm:grid-cols-2 md:grid-cols-3 xl:grid-cols-3 2xl:grid-cols-4"

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
    onDelete?: (itemId: string) => void
    onEditPrice?: (item: DisplayItem) => void
    canDelete: boolean
    pending: boolean
    deleting: boolean
}) => {
    const cardKey = toAutomationIdSegment(item.item_id)
    const members = item.kind === "PACKAGE" ? (item.members ?? []) : []
    const extraMembers = Math.max(0, members.length - 3)

    return (
        <article className="min-w-0">
            <Card
                size="sm"
                className="group relative flex h-full flex-col justify-between overflow-hidden border border-border/70 bg-card p-0 transition-all hover:border-border hover:shadow-xs"
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
                                <span className="text-2xs">暂无图片</span>
                            </div>
                        )}
                        {item.missing_image ? (
                            <Badge
                                variant="warning"
                                className="absolute top-2 left-2 shadow-xs"
                            >
                                缺图
                            </Badge>
                        ) : null}
                    </div>

                    <CardHeader className="gap-1.5 p-3 pb-0">
                        <div className="flex items-baseline gap-1.5">
                            <MoneyValue
                                display="exact"
                                className="text-base font-bold text-foreground"
                                value={item.price_gross || item.price}
                            />
                            <span className="text-2xs text-muted-foreground">
                                含税
                            </span>
                        </div>
                        <PriceComparison
                            original_price={item.original_price}
                            markup_percent={item.markup_percent}
                        />
                        <CardTitle
                            className="line-clamp-2 text-xs leading-snug font-medium text-foreground"
                            title={item.name}
                        >
                            {item.name}
                        </CardTitle>
                    </CardHeader>

                    <CardContent className="flex flex-col gap-1 p-3 pt-1.5 text-xs text-muted-foreground">
                        {item.spec_label ? (
                            <p
                                className="truncate text-2xs"
                                title={item.spec_label}
                            >
                                {item.spec_label}
                            </p>
                        ) : null}
                        {members.length > 0 ? (
                            <ul className="flex flex-col gap-0.5 rounded-md bg-muted/30 p-1.5 text-2xs">
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
                                    <li className="num pt-0.5 text-2xs font-medium text-muted-foreground">
                                        另有 {extraMembers} 件商品
                                    </li>
                                ) : null}
                            </ul>
                        ) : item.unit ? (
                            <p className="text-2xs">单位：{item.unit}</p>
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
                                className="flex-1 justify-center text-destructive hover:bg-destructive/10 hover:text-destructive"
                                disabled={pending}
                                loading={deleting}
                                onClick={() => onDelete(item.item_id)}
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
    pending = false,
    deletingItemId,
    regeneratingTierIds,
}: {
    items: readonly DisplayItem[]
    selectionForm: "SINGLE_SKU" | "PACKAGE"
    tiers?: readonly PreviewTierConfig[]
    onRegenerateTier?: (tierId: string) => void
    onDelete?: (itemId: string) => void
    onEditPrice?: (item: DisplayItem) => void
    pending?: boolean
    deletingItemId?: string
    regeneratingTierIds?: readonly string[]
}) => {
    const canDelete = typeof onDelete === "function"
    const [tierFilter, setTierFilter] = React.useState<string>("ALL")
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
            <BusinessEmptyState
                kind="no-data"
                title="暂无可预览陈列"
                description="开始准备后，系统生成的商品陈列将显示在这里。"
            />
        )
    }

    // 过滤特定档位
    const visibleGroups = groups
        ? tierFilter === "ALL"
            ? groups
            : groups.filter(([tierName]) => tierName === tierFilter)
        : null

    const tableItems = visibleGroups
        ? visibleGroups.flatMap(([, group]) => group)
        : filteredItems
    const viewToggle = (
        <div
            role="group"
            aria-label="陈列视图"
            className="flex items-center gap-1"
        >
            <Button
                id="selection-preview-layout-table"
                variant={layout === "table" ? "secondary" : "ghost"}
                size="sm"
                aria-pressed={layout === "table"}
                onClick={() => setLayout("table")}
            >
                <TableIcon data-icon="inline-start" aria-hidden="true" />
                表格
            </Button>
            <Button
                id="selection-preview-layout-gallery"
                variant={layout === "gallery" ? "secondary" : "ghost"}
                size="sm"
                aria-pressed={layout === "gallery"}
                onClick={() => setLayout("gallery")}
            >
                <LayoutGridIcon data-icon="inline-start" aria-hidden="true" />
                卡片
            </Button>
        </div>
    )
    return (
        <BusinessTableFrame
            title="陈列预览"
            tableActions={viewToggle}
            selectionBar={
                <span className="text-xs text-muted-foreground">
                    共 {tableItems.length} 项
                    {tableItems.length !== items.length
                        ? ` / 全部 ${items.length} 项`
                        : ""}
                </span>
            }
            toolbar={
                (selectionForm === "PACKAGE" && tierNames.length > 1) ||
                missingImageCount > 0 ? (
                    <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/60 pb-3">
                        <div className="flex flex-wrap items-center gap-1.5">
                            {selectionForm === "PACKAGE" &&
                            tierNames.length > 1 ? (
                                <>
                                    <button
                                        id="selection-preview-tier-all"
                                        type="button"
                                        onClick={() => setTierFilter("ALL")}
                                        className={cn(
                                            "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                                            tierFilter === "ALL"
                                                ? "bg-primary text-primary-foreground shadow-2xs"
                                                : "bg-muted/50 text-muted-foreground hover:bg-muted hover:text-foreground",
                                        )}
                                    >
                                        全部档位 ({items.length})
                                    </button>
                                    {tierNames.map((name) => {
                                        const count = items.filter(
                                            (i) =>
                                                (i.tier_name ?? i.tier_id) ===
                                                name,
                                        ).length
                                        return (
                                            <button
                                                id={`selection-preview-tier-${toAutomationIdSegment(name)}`}
                                                key={name}
                                                type="button"
                                                onClick={() =>
                                                    setTierFilter(name)
                                                }
                                                className={cn(
                                                    "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                                                    tierFilter === name
                                                        ? "bg-primary text-primary-foreground shadow-2xs"
                                                        : "bg-muted/50 text-muted-foreground hover:bg-muted hover:text-foreground",
                                                )}
                                            >
                                                {name} ({count})
                                            </button>
                                        )
                                    })}
                                </>
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
                                {onlyMissingImage
                                    ? "显示全部"
                                    : `仅看缺图 (${missingImageCount})`}
                            </Button>
                        ) : null}
                    </div>
                ) : undefined
            }
            table={
                layout === "table" ? (
                    <PreviewTable
                        items={tableItems}
                        pending={pending}
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
                                                    <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border/60 bg-muted/30 px-3.5 py-2">
                                                        <div className="flex flex-wrap items-baseline gap-2">
                                                            <h3 className="text-sm font-semibold text-foreground">
                                                                {tierName}
                                                            </h3>
                                                            <Badge
                                                                variant="secondary"
                                                                className="text-xs"
                                                            >
                                                                {
                                                                    tierItems.length
                                                                }{" "}
                                                                套
                                                            </Badge>
                                                            {tierConfig ? (
                                                                <span className="text-xs text-muted-foreground">
                                                                    目标 ¥
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
                                                                variant="outline"
                                                                disabled={
                                                                    pending
                                                                }
                                                                loading={regeneratingTierIds?.includes(
                                                                    tierConfig.tier_id,
                                                                )}
                                                                onClick={() =>
                                                                    onRegenerateTier(
                                                                        tierConfig.tier_id,
                                                                    )
                                                                }
                                                                className="h-7 text-xs"
                                                            >
                                                                <RefreshCwIcon
                                                                    className="mr-1 size-3"
                                                                    aria-hidden="true"
                                                                />
                                                                重生成此档
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
