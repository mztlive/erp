"use client"

import { surfaceInsetClassName } from "@/components/business"
import { AcceptanceWorkspace } from "@/features/sales-orders/components/acceptance-workspace"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { cn } from "@/lib/utils"

export function AcceptancePanel({ order }: { order: SalesOrderDetailView }) {
    const latest = order.acceptance

    return (
        <div className="flex flex-col gap-4">
            <div className={cn(surfaceInsetClassName, "px-3 py-3")}>
                <h3 className="text-sm font-medium">客户验收</h3>
                <p className="mt-1 text-xs text-muted-foreground">
                    {latest
                        ? `最近 ${latest.reference} · ${latest.postedAt}${latest.note ? ` · ${latest.note}` : ""}`
                        : "还没有验收记录。客户确认完成后，本单才算交付完毕。"}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                    交付进度：{order.fulfillment.label}
                </p>
            </div>
            <AcceptanceWorkspace salesOrderId={order.id} />
        </div>
    )
}
