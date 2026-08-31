"use client"

import { WorkspaceTaskPane } from "@/components/business"
import { SupplyExceptionTaskPanel } from "@/features/supplier-offerings/components/supply-exception-task-panel"
import { useSupplierSupplyExceptionWorkItemQuery } from "@/features/supplier-offerings/hooks/queries"

import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/** W01 供应停止异常：固定当前 W21 任务，原地登记证据并完成核对责任。 */
export function WorkspaceSupplyExceptionTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const taskQuery = useSupplierSupplyExceptionWorkItemQuery(item.workItemId)

    return (
        <WorkspaceTaskPane
            header={<WorkspaceTaskIdentityHeader item={item} />}
            aria-label="当前供应停止核对任务"
        >
            <SupplyExceptionTaskPanel
                workItemId={item.workItemId}
                task={taskQuery.data}
                isPending={taskQuery.isPending}
                error={taskQuery.error}
                onRetry={() => void taskQuery.refetch()}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
        </WorkspaceTaskPane>
    )
}
