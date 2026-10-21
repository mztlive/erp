"use client"

import { useState } from "react"
import { ChevronRightIcon } from "lucide-react"

import { QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { SheetClose, SheetTrigger } from "@/components/ui/sheet"
import type { SalesOrderLineItem } from "@/features/sales-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import { compactFixed, sumFixed } from "@/lib/fixed-decimal"

/** 销售行只展示地址摘要，完整交付安排在详情抽屉中读取。 */
export function SalesOrderDeliverySummary({
    line,
}: {
    line: SalesOrderLineItem
}) {
    const [open, setOpen] = useState(false)
    const allocations = line.deliveryAllocations ?? []
    const first = allocations[0]

    if (!first) {
        return (
            <span className="text-xs text-muted-foreground">
                未指定交付地址
            </span>
        )
    }

    const prefix = `sales-order-line-${toAutomationIdSegment(line.id)}-delivery`
    const total = compactFixed(
        sumFixed(
            allocations.map((allocation) => allocation.quantity),
            {
                maxScale: 6,
                outputScale: 6,
            },
        ),
    )
    const summary = `分送 ${allocations.length} 个地址 · 共 ${total} ${line.unit}`

    return (
        <QuickPreviewSheet
            idPrefix={`${prefix}-sheet`}
            open={open}
            onOpenChange={setOpen}
            size="preview"
            title={line.name}
            description={
                line.sku && line.sku !== "无规格" ? line.sku : undefined
            }
            identity="交付地址"
            summary={<p className="text-sm text-muted-foreground">{summary}</p>}
            trigger={
                <SheetTrigger
                    id={`${prefix}-trigger`}
                    aria-label={`查看${line.name}的交付地址，共${allocations.length}个地址`}
                    render={
                        <Button
                            type="button"
                            variant="link"
                            size="sm"
                            className="h-auto max-w-60 justify-start px-0 py-1 text-left whitespace-normal"
                        />
                    }
                >
                    <span className="min-w-0 break-words">
                        {allocations.length === 1
                            ? [first.recipient, first.region_name]
                                  .filter(Boolean)
                                  .join(" · ")
                            : summary}
                    </span>
                    <ChevronRightIcon aria-hidden data-icon="inline-end" />
                </SheetTrigger>
            }
            footer={
                <SheetClose
                    id={`${prefix}-footer-close`}
                    render={<Button type="button" variant="outline" />}
                >
                    关闭
                </SheetClose>
            }
        >
            <ol className="divide-y divide-border">
                {allocations.map((allocation, index) => (
                    <li
                        key={index}
                        className="space-y-3 py-5 first:pt-0 last:pb-0"
                    >
                        <div className="flex flex-wrap items-baseline justify-between gap-2">
                            <h3 className="font-medium">地址 {index + 1}</h3>
                            <span className="num text-sm">
                                分配数量：
                                {compactFixed(allocation.quantity)} {line.unit}
                            </span>
                        </div>
                        <dl className="grid grid-cols-[4rem_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm">
                            <dt className="text-muted-foreground">收货人</dt>
                            <dd className="break-words">
                                {allocation.recipient}
                            </dd>
                            <dt className="text-muted-foreground">电话</dt>
                            <dd className="num break-words">
                                {allocation.phone}
                            </dd>
                            <dt className="text-muted-foreground">完整地址</dt>
                            <dd className="break-words leading-6">
                                {fullDeliveryAddress(allocation)}
                            </dd>
                        </dl>
                    </li>
                ))}
            </ol>
        </QuickPreviewSheet>
    )
}
