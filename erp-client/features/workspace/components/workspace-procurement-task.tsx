"use client"

import { WorkspaceTaskPane } from "@/components/business"
import { PurchaseOrderCreatePage } from "@/features/purchase-orders/pages/purchase-order-create-page"
import { useCreationBasesQuery } from "@/features/purchase-orders/hooks/queries"
import { StatusBadge } from "@/components/ui/status-badge"
import { isBlockedWorkItem } from "../lib/work-item"
import type { WorkspaceWorkItem } from "../types"
import { stripDocumentNumberPrefix } from "../lib/stable-number"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"

/** W01 供给分配作业面：锁定当前任务及销售责任行，正文展开供给操作。 */
export function WorkspaceProcurementTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const basesQuery = useCreationBasesQuery({
        salesOrderId: item.businessObjectId,
        workItemId: item.workItemId,
    })
    const order = basesQuery.data?.find(
        (basis) => basis.salesOrderId === item.businessObjectId,
    )
    return (
        <WorkspaceTaskPane
            className="@container/document"
            header={
                <>
                    <div className="min-w-0 space-y-2">
                        <div className="flex flex-wrap items-center gap-2">
                            <h2 className="text-xl font-medium tracking-tight">
                                供给分配
                            </h2>
                            {isBlockedWorkItem(item) ? (
                                <StatusBadge label="受阻" tone="warning" />
                            ) : item.dueBucket === "overdue" ? (
                                <StatusBadge
                                    label="已超期"
                                    tone="destructive"
                                />
                            ) : null}
                        </div>
                        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                            {item.counterpartyName ? (
                                <span className="text-sm font-medium">
                                    {item.counterpartyName}
                                </span>
                            ) : null}
                            <span className="num text-xs text-muted-foreground">
                                {stripDocumentNumberPrefix(item.stableNumber)}
                            </span>
                            {order?.contractNumber ? (
                                <span className="text-xs text-muted-foreground">
                                    合同{" "}
                                    <span className="text-foreground">
                                        {order.contractNumber}
                                    </span>
                                </span>
                            ) : null}
                            {order?.salesOwnerName ? (
                                <span className="text-xs text-muted-foreground">
                                    销售负责人{" "}
                                    <span className="text-foreground">
                                        {order.salesOwnerName}
                                    </span>
                                </span>
                            ) : null}
                        </div>
                    </div>
                    <WorkspaceTaskHeaderActions item={item} />
                </>
            }
            aria-label="当前供给分配任务"
        >
            <PurchaseOrderCreatePage
                initialSalesOrderId={item.businessObjectId}
                initialWorkItemId={item.workItemId}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
        </WorkspaceTaskPane>
    )
}
