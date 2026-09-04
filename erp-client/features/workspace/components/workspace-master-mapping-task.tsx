"use client"

import { WorkspaceTaskPane } from "@/components/business"

import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/** W01 主数据映射：商城移除后显示占位，待未来重做时恢复。 */
export function WorkspaceMasterMappingTask({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    void onTaskCompleted;
    // TODO(商城重做): 主数据映射重做后恢复任务内容。
    return (
        <WorkspaceTaskPane
            header={<WorkspaceTaskIdentityHeader item={item} />}
            aria-label="当前主数据映射任务"
        >
            <p className="text-sm text-muted-foreground">
                主数据映射已随商城对接移除。
            </p>
        </WorkspaceTaskPane>
    )
}
