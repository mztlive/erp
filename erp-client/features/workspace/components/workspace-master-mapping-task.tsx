"use client"

import { WorkspaceTaskPane } from "@/components/business"
import { MallSyncPage } from "@/features/mall-sync/pages/mall-sync-page"

import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/** W01 主数据映射：固定当前 W17 任务，在工作台内完成身份关系确认。 */
export function WorkspaceMasterMappingTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    return (
        <WorkspaceTaskPane
            header={<WorkspaceTaskIdentityHeader item={item} />}
            aria-label="当前主数据映射任务"
        >
            <MallSyncPage
                forcedMappingTaskId={item.businessObjectId}
                forcedWorkItemId={item.workItemId}
                forcedQueueContextId={item.queueContextId}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
        </WorkspaceTaskPane>
    )
}
