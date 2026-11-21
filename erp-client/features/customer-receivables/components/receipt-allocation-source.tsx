"use client"

import { ArrowUpRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import type { AllocationSource } from "../api/allocation-source"
import type { AllocationTarget } from "../types"
import { salesOrderHref } from "../lib/source-documents"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function ReceiptAllocationSource({
    target,
    source,
    canRead,
    pending,
    failed,
    onRetry,
    idPrefix = "customer-receivables-allocation-source",
}: {
    target: AllocationTarget
    source?: AllocationSource
    canRead: boolean
    pending: boolean
    failed: boolean
    onRetry: () => void
    idPrefix?: string
}) {
    const prefix = `${idPrefix}-${toAutomationIdSegment(target.targetId)}`
    const customerName = source?.customerName || target.customerName
    const entryLabel = target.label.startsWith(`${target.salesOrderNo} · `)
        ? target.label.slice(`${target.salesOrderNo} · `.length)
        : target.label
    return (
        <div className="min-w-64 space-y-1.5 whitespace-normal py-1">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                {canRead ? (
                    <a
                        id={`${prefix}-open`}
                        href={salesOrderHref(target.salesOrderId)}
                        target="_blank"
                        rel="noopener noreferrer"
                        aria-label={`打开销售单 ${target.salesOrderNo}（新标签页）`}
                        className="num inline-flex items-center gap-1 font-medium underline decoration-muted-foreground/40 underline-offset-4 hover:decoration-foreground focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-ring"
                    >
                        {target.salesOrderNo}
                        <ArrowUpRightIcon className="size-3.5 shrink-0" />
                    </a>
                ) : (
                    <span className="num font-medium">
                        {target.salesOrderNo}
                    </span>
                )}
                <span className="text-xs text-muted-foreground">
                    {entryLabel}
                </span>
            </div>
            {customerName ? (
                <p className="font-medium">
                    {customerName}
                    {source?.projectName ? ` · ${source.projectName}` : ""}
                </p>
            ) : null}
            {source ? (
                <p
                    className="text-xs text-muted-foreground"
                    title={source.products.join("；")}
                >
                    {source.products.length > 0
                        ? `共 ${source.products.length} 项商品`
                        : "商品摘要未提供"}
                    {source.ownerName ? ` · 负责销售：${source.ownerName}` : ""}
                </p>
            ) : canRead && pending ? (
                <p className="text-xs text-muted-foreground">
                    正在读取商品与负责销售…
                </p>
            ) : failed ? (
                <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                    <span>销售单摘要暂不可用</span>
                    <Button
                        id={`${prefix}-retry`}
                        size="xs"
                        variant="ghost"
                        type="button"
                        onClick={onRetry}
                    >
                        重试
                    </Button>
                </div>
            ) : (
                <p className="text-xs text-muted-foreground">
                    当前账号无销售单详情查看权限
                </p>
            )}
        </div>
    )
}
