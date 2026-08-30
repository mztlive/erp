"use client"

import { SupplierOrderCenterPage } from "@/features/supplier-orders/pages/supplier-order-center-page"

import type { WorkspaceWorkItem } from "../types"

/** W01 供应商履约异常调查：锁定供应商订单与任务并在当前详情完成核实。 */
export function WorkspaceSupplierInvestigationTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    return (
        <section
            className="h-full min-h-0 overflow-auto"
            aria-label="当前供应商履约异常调查任务"
        >
            <SupplierOrderCenterPage
                supplierOrderId={item.businessObjectId}
                section="overview"
                workItemId={item.workItemId}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
        </section>
    )
}
