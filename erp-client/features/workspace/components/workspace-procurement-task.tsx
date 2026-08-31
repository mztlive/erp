"use client"

import { WorkspaceTaskPane } from "@/components/business"
import { PurchaseOrderCreatePage } from "@/features/purchase-orders/pages/purchase-order-create-page"
import { SalesOrderPaperPreviewDialog } from "@/features/sales-orders/components/sales-order-paper-preview-dialog"

import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/** W01 供给分配作业面：锁定当前销售单和正式任务，在工作台内完成供给分配。 */
export function WorkspaceProcurementTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    return (
        <WorkspaceTaskPane
            header={
                <WorkspaceTaskIdentityHeader
                    item={item}
                    title="供给分配"
                    subtitle="系统优先推荐现有库存，不足部分再推荐采购；确认后一次完成库存预留和采购缺口建单。"
                />
            }
            aria-label="当前供给分配任务"
        >
            <PurchaseOrderCreatePage
                initialSalesOrderId={item.businessObjectId}
                initialWorkItemId={item.workItemId}
                embedded
                onTaskCompleted={onTaskCompleted}
                renderSalesOrderPreview={(props) => (
                    <SalesOrderPaperPreviewDialog {...props} />
                )}
            />
        </WorkspaceTaskPane>
    )
}
