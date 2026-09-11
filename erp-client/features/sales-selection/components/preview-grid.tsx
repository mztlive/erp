/**
 * 陈列预览网格：销售发布前核对陈列项。
 * 单品平铺，套餐按档分组；封面只读后端字段，不拼图。
 */

"use client"

import * as React from "react"
import { PackageIcon, Trash2Icon } from "lucide-react"

import { SnapshotImage } from "./snapshot-image"
import { BusinessEmptyState, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DisplayItem } from "@/features/sales-selection/types"

const previewGridClassName =
    "grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-4 min-[90rem]:grid-cols-5"

/**
 * 单张陈列卡片，视觉对齐公司商品池画廊卡。
 */
const PreviewCard = ({
    item,
    onDelete,
    canDelete,
}: {
    item: DisplayItem
    onDelete?: (itemId: string) => void
    canDelete: boolean
}) => {
    const cardKey = toAutomationIdSegment(item.item_id)
    const members = item.kind === "PACKAGE" ? (item.members ?? []) : []
    const extraMembers = Math.max(0, members.length - 3)
    return (
        <article className="min-w-0">
            <Card size="sm" className="h-full gap-0 overflow-hidden py-0">
                <div className="relative aspect-square w-full bg-muted">
                    {item.cover_image ? (
                        <SnapshotImage
                            path={item.cover_image}
                            alt={item.name}
                        />
                    ) : (
                        <div className="flex h-full flex-col items-center justify-center gap-1.5 text-muted-foreground">
                            <PackageIcon
                                className="size-8"
                                aria-hidden="true"
                            />
                            <span className="text-xs">暂无图片</span>
                        </div>
                    )}
                    {item.missing_image ? (
                        <Badge
                            variant="warning"
                            className="absolute top-2 left-2"
                        >
                            缺图
                        </Badge>
                    ) : null}
                </div>
                <CardHeader className="gap-1.5 px-3 pt-3 pb-0">
                    <div className="flex items-baseline gap-1.5">
                        <MoneyValue
                            className="font-semibold [&>span:first-child]:text-lg"
                            value={item.price_gross || item.price}
                        />
                        <span className="text-xs text-muted-foreground">
                            含税
                        </span>
                    </div>
                    <CardTitle
                        className="line-clamp-2 text-sm leading-5 font-medium"
                        title={item.name}
                    >
                        {item.name}
                    </CardTitle>
                </CardHeader>
                <CardContent className="flex flex-col gap-1 px-3 pt-2 pb-3 text-xs text-muted-foreground">
                    {item.spec_label ? (
                        <p className="truncate" title={item.spec_label}>
                            {item.spec_label}
                        </p>
                    ) : null}
                    {members.length > 0 ? (
                        <ul className="flex flex-col gap-0.5">
                            {members.slice(0, 3).map((member) => (
                                <li
                                    key={member.sku_id}
                                    className="truncate"
                                    title={`${member.name} · ${member.unit}`}
                                >
                                    {member.name} · {member.unit}
                                </li>
                            ))}
                            {extraMembers > 0 ? (
                                <li className="num">另有 {extraMembers} 件</li>
                            ) : null}
                        </ul>
                    ) : item.unit ? (
                        <p>单位：{item.unit}</p>
                    ) : null}
                    {canDelete && onDelete ? (
                        <Button
                            id={`sales-selection-preview-${cardKey}-delete`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="mt-1 self-start text-destructive"
                            onClick={() => onDelete(item.item_id)}
                        >
                            <Trash2Icon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            删除该项
                        </Button>
                    ) : null}
                </CardContent>
            </Card>
        </article>
    )
}

/**
 * 陈列预览网格。
 * @param items 后端陈列结果
 * @param selectionForm 选品形态（套餐按档分组）
 * @param onDelete 待发布态删减回调
 */
export const PreviewGrid = ({
    items,
    selectionForm,
    onDelete,
}: {
    items: readonly DisplayItem[]
    selectionForm: "SINGLE_SKU" | "PACKAGE"
    onDelete?: (itemId: string) => void
}) => {
    const canDelete = typeof onDelete === "function"

    const groups = React.useMemo(() => {
        if (selectionForm !== "PACKAGE") return null
        const map = new Map<string, DisplayItem[]>()
        for (const item of items) {
            const key = item.tier_name ?? item.tier_id ?? "未分组"
            const bucket = map.get(key) ?? []
            bucket.push(item)
            map.set(key, bucket)
        }
        return [...map.entries()]
    }, [items, selectionForm])

    if (items.length === 0) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="暂无可预览陈列"
                description="开始准备后，生成的商品会显示在这里。"
            />
        )
    }

    if (!groups) {
        return (
            <div className={previewGridClassName}>
                {items.map((item) => (
                    <PreviewCard
                        key={item.item_id}
                        item={item}
                        onDelete={onDelete}
                        canDelete={canDelete}
                    />
                ))}
            </div>
        )
    }

    return (
        <div className="flex flex-col gap-6">
            {groups.map(([tierName, tierItems]) => (
                <section
                    key={tierName}
                    aria-label={tierName}
                    className="flex flex-col gap-3"
                >
                    <div className="flex items-center gap-2">
                        <h3 className="text-sm font-semibold">{tierName}</h3>
                        <Badge variant="secondary">{tierItems.length} 套</Badge>
                    </div>
                    <div className={previewGridClassName}>
                        {tierItems.map((item) => (
                            <PreviewCard
                                key={item.item_id}
                                item={item}
                                onDelete={onDelete}
                                canDelete={canDelete}
                            />
                        ))}
                    </div>
                </section>
            ))}
        </div>
    )
}
