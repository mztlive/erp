"use client"

import { DocumentSection, surfaceInsetClassName } from "@/components/business"
import { cn } from "@/lib/utils"

import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

export function PurchaseOrderDetailAuditSection({
    order,
}: {
    order: PurchaseOrderCenterView
}) {
    return (
        <DocumentSection title="审计">
            <ul className="space-y-2">
                {order.workflow.map((item) => (
                    <li
                        key={item.id}
                        className={cn(
                            surfaceInsetClassName,
                            "px-3 py-2 text-sm",
                        )}
                    >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <span className="font-medium">
                                {item.actionLabel}
                            </span>
                            <span className="num text-xs text-muted-foreground">
                                {item.at}
                            </span>
                        </div>
                        <div className="mt-0.5 text-xs text-muted-foreground">
                            {item.actorLabel}
                            {item.comment ? ` · ${item.comment}` : ""}
                        </div>
                    </li>
                ))}
            </ul>
        </DocumentSection>
    )
}
