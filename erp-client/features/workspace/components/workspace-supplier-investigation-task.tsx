"use client"

import { WorkspaceTaskPane } from "@/components/business"
import { SupplierOrderCenterPage } from "@/features/supplier-orders/pages/supplier-order-center-page"

import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/** W01 供应商履约异常调查：锁定供应商订单与任务并在当前详情完成核实。 */
export function WorkspaceSupplierInvestigationTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    return (
        <WorkspaceTaskPane
            header={<WorkspaceTaskIdentityHeader item={item} />}
            aria-label="当前供应商履约异常调查任务"
        >
            <SupplierOrderCenterPage
                supplierOrderId={item.businessObjectId}
                section="overview"
                workItemId={item.workItemId}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
        </WorkspaceTaskPane>
    )
}
