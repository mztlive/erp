"use client"

import {
    WorkspaceTaskPane,
    type WorkspaceTaskPaneProps,
} from "@/components/business/workspace-task-pane"

export type WorkspaceProcessingPaneProps = WorkspaceTaskPaneProps

/** 完整办理页直接复用统一工作台骨架（`WorkspaceTaskPane` / `UnifiedTaskWorkbench`）。 */
export function WorkspaceProcessingPane(props: WorkspaceProcessingPaneProps) {
    return <WorkspaceTaskPane {...props} />
}
