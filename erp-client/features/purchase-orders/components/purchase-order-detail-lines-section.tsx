"use client"

import { DocumentSection } from "@/components/business"

import { LinesTable } from "@/features/purchase-orders/components/purchase-order-surfaces"
import { PurchaseOrderDetailTotals } from "@/features/purchase-orders/components/purchase-order-detail-totals"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

export function PurchaseOrderDetailLinesSection({
    order,
    costMasked,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
}) {
    return (
        <DocumentSection title="明细与分配">
            <LinesTable order={order} costMasked={costMasked} />
            <PurchaseOrderDetailTotals
                className="mt-4 max-w-md ml-auto"
                order={order}
                costMasked={costMasked}
            />
        </DocumentSection>
    )
}
