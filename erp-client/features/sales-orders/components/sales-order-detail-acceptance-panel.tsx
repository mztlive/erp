"use client"

import { AcceptanceWorkspace } from "@/features/sales-orders/components/acceptance-workspace"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import type { WorkItemProjection } from "@/features/work-items/types"

export function AcceptancePanel({
    order,
    workItem,
    id,
    idPrefix,
}: {
    order: SalesOrderDetailView
    workItem?: WorkItemProjection
    id?: string
    idPrefix?: string
}) {
    return (
        <AcceptanceWorkspace
            id={idPrefix ?? id ?? "sales-orders-detail-acceptance-workspace"}
            salesOrderId={order.id}
            ownerUserId={order.ownerUserId}
            ownerName={order.ownerName}
            workItem={workItem}
        />
    )
}
