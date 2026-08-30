"use client"

import { MallSyncPage } from "@/features/mall-sync/pages/mall-sync-page"

import type { WorkspaceWorkItem } from "../types"

/** W01 主数据映射：固定当前 W17 任务，在工作台内完成身份关系确认。 */
export function WorkspaceMasterMappingTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    return (
        <section
            className="h-full min-h-0 overflow-auto"
            aria-label="当前主数据映射任务"
        >
            <MallSyncPage
                forcedMappingTaskId={item.businessObjectId}
                forcedWorkItemId={item.workItemId}
                forcedQueueContextId={item.queueContextId}
                embedded
                onTaskCompleted={onTaskCompleted}
            />
        </section>
    )
}
