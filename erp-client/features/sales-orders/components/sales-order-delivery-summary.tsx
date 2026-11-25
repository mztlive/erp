"use client"

import { useState } from "react"
import { ChevronRightIcon, MapPinIcon } from "lucide-react"

import { QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { SheetClose, SheetTrigger } from "@/components/ui/sheet"
import { RegionName } from "@/features/regions"
import type { SalesDeliveryAllocation, SalesOrderLineItem } from "../types"
import { addressKey } from "../lib/sales-delivery-import"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import { compactFixed, sumFixed } from "@/lib/fixed-decimal"

function groupAllocationsByAddress(rows: readonly SalesDeliveryAllocation[]) {
    const addresses = new Map<string, SalesDeliveryAllocation>()
    for (const row of rows) {
        const key = addressKey(row)
        const existing = addresses.get(key)
        addresses.set(
            key,
            existing
                ? {
                      ...existing,
                      region_name: existing.region_name || row.region_name,
                      quantity: sumFixed([existing.quantity, row.quantity], {
                          maxScale: 6,
                          outputScale: 6,
                      }),
                  }
                : row,
        )
    }
    return [...addresses.values()]
}

/** 销售行只展示地址摘要，完整交付安排在详情抽屉中读取。 */
export function SalesOrderDeliverySummary({
    line,
    variant = "detail",
}: {
    line: Pick<
        SalesOrderLineItem,
        "id" | "name" | "sku" | "unitName" | "deliveryAllocations"
    >
    variant?: "detail" | "submit"
}) {
    const [open, setOpen] = useState(false)
    const isSubmit = variant === "submit"
    const rows = line.deliveryAllocations ?? []
    const allocations = isSubmit ? groupAllocationsByAddress(rows) : rows
    const first = allocations[0]

    if (!first) {
        if (isSubmit) return null
        return (
            <span className="mt-1 inline-flex items-center gap-1 text-xs text-muted-foreground">
                <MapPinIcon aria-hidden="true" className="size-3 shrink-0" />
                未指定交付地址
            </span>
        )
    }

    const surface = isSubmit ? "sales-order-submit" : "sales-order"
    const prefix = `${surface}-line-${toAutomationIdSegment(line.id)}-delivery`
    const total = compactFixed(
        sumFixed(
            allocations.map((allocation) => allocation.quantity),
            {
                maxScale: 6,
                outputScale: 6,
            },
        ),
    )
    const summary = `分送 ${allocations.length} 个地址 · 共 ${total} ${line.unitName ?? ""}`
    const singleAddress = allocations.length === 1
    const triggerText = isSubmit
        ? singleAddress
            ? "查看配送"
            : `查看 ${allocations.length} 个收货地址`
        : singleAddress
          ? [first.recipient, first.region_name].filter(Boolean).join(" · ")
          : summary
    const trigger = (
        <SheetTrigger
            id={`${prefix}-trigger`}
            aria-label={
                isSubmit
                    ? `${triggerText}（${line.name}）`
                    : `查看${line.name}的交付地址，共${allocations.length}个地址`
            }
            render={
                <Button
                    type="button"
                    variant={isSubmit ? "outline" : "link"}
                    size={isSubmit ? "xs" : "sm"}
                    className={
                        isSubmit
                            ? "max-w-full cursor-pointer"
                            : "mt-1 h-auto max-w-64 justify-start gap-1 px-0 py-0 text-left text-xs font-normal text-primary/90 hover:text-primary"
                    }
                />
            }
        >
            {!isSubmit && (
                <MapPinIcon aria-hidden="true" className="size-3 shrink-0" />
            )}
            <span className="min-w-0 truncate">{triggerText}</span>
            <ChevronRightIcon
                aria-hidden="true"
                className="size-3 shrink-0 opacity-75"
            />
        </SheetTrigger>
    )

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
            summary={<p className="text-xs text-muted-foreground">{summary}</p>}
            trigger={
                isSubmit ? (
                    <div className="mt-1 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
                        {singleAddress && (
                            <span className="min-w-0 truncate">
                                {first.recipient}
                                {(first.region_name || first.region_code) && (
                                    <>
                                        {" · "}
                                        {first.region_name || (
                                            <RegionName
                                                code={first.region_code}
                                            />
                                        )}
                                    </>
                                )}
                            </span>
                        )}
                        {trigger}
                    </div>
                ) : (
                    trigger
                )
            }
            footer={
                <SheetClose
                    id={`${prefix}-footer-close`}
                    render={
                        <Button type="button" variant="outline" size="sm" />
                    }
                >
                    关闭
                </SheetClose>
            }
        >
            <ol className="divide-y divide-border/60">
                {allocations.map((allocation, index) => (
                    <li
                        key={index}
                        className="space-y-3 py-5 first:pt-0 last:pb-0"
                    >
                        <div className="flex flex-wrap items-baseline justify-between gap-2">
                            <h3 className="font-medium">地址 {index + 1}</h3>
                            <span className="num text-body-compact">
                                分配数量：
                                {compactFixed(allocation.quantity)}{" "}
                                {line.unitName ?? ""}
                            </span>
                        </div>
                        <dl className="grid grid-cols-[4rem_minmax(0,1fr)] gap-x-4 gap-y-2 text-body-compact">
                            <dt className="text-muted-foreground">收货人</dt>
                            <dd className="break-words">
                                {allocation.recipient}
                            </dd>
                            <dt className="text-muted-foreground">电话</dt>
                            <dd className="num break-words">
                                {allocation.phone}
                            </dd>
                            <dt className="text-muted-foreground">完整地址</dt>
                            <dd className="break-words">
                                {isSubmit &&
                                    !allocation.region_name &&
                                    allocation.region_code && (
                                        <RegionName
                                            code={allocation.region_code}
                                        />
                                    )}
                                {fullDeliveryAddress(allocation)}
                            </dd>
                        </dl>
                    </li>
                ))}
            </ol>
        </QuickPreviewSheet>
    )
}
