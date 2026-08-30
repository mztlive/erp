"use client"

import { PurchaseOrderCreatePage } from "@/features/purchase-orders/pages/purchase-order-create-page"

import type { WorkspaceWorkItem } from "../types"

/** W01 供给分配作业面：锁定当前销售单和正式任务，在工作台内完成供给分配。 */
export function WorkspaceProcurementTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    return (
        <section
            className="h-full min-h-0 overflow-auto"
            aria-label="当前供给分配任务"
        >
            <PurchaseOrderCreatePage
                initialSalesOrderId={item.businessObjectId}
                initialWorkItemId={item.workItemId}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
        </section>
    )
}
