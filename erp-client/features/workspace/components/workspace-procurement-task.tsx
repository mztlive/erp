"use client"

import { PurchaseOrderCreatePage } from "@/features/purchase-orders/pages/purchase-order-create-page"
import { SalesOrderPaperPreviewDialog } from "@/features/sales-orders/components/sales-order-paper-preview-dialog"

import type { WorkspaceWorkItem } from "../types"
import { WorkspacePaneActions } from "./workspace-pane-actions"
import { WorkspaceTaskContextHelp } from "./workspace-task-context"

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
            className="flex h-full min-h-0 flex-col"
            aria-label="当前供给分配任务"
        >
            <PurchaseOrderCreatePage
                initialSalesOrderId={item.businessObjectId}
                initialWorkItemId={item.workItemId}
                embedded
                onTaskCompleted={onTaskCompleted}
                headerActions={
                    <div className="flex items-center gap-1">
                        <WorkspaceTaskContextHelp item={item} />
                        <WorkspacePaneActions />
                    </div>
                }
                renderSalesOrderPreview={(props) => (
                    <SalesOrderPaperPreviewDialog {...props} />
                )}
            />
        </section>
    )
}
