"use client"

import { ScrollArea } from "@/components/ui/scroll-area"
import { PurchaseOrderPreviewLines } from "@/features/purchase-orders/components/purchase-order-preview-lines"
import { PurchaseOrderPreviewOverview } from "@/features/purchase-orders/components/purchase-order-preview-overview"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

type PurchaseOrderPreviewPanelProps = {
    order: PurchaseOrderCenterView
}

/** detail 半屏：左进度/商务/门禁，右明细与金额合计。 */
export function PurchaseOrderPreviewPanel({
    order,
}: PurchaseOrderPreviewPanelProps) {
    return (
        <div
            data-slot="purchase-order-detail-preview"
            className="flex min-h-0 flex-1 flex-col lg:flex-row"
        >
            <ScrollArea className="min-h-0 max-h-[40vh] border-b border-border lg:max-h-none lg:w-[min(20rem,38%)] lg:shrink-0 lg:border-r lg:border-b-0">
                <PurchaseOrderPreviewOverview order={order} />
            </ScrollArea>

            <ScrollArea className="min-h-0 flex-1">
                <PurchaseOrderPreviewLines order={order} />
            </ScrollArea>
        </div>
    )
}
